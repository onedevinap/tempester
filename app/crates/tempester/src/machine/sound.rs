//! Sounds: fetch the current sound (`40` -> `60`), save it as a named `63` file, open a `.syx` and send it back
//! to the machine as the current sound (`60`). Sending needs a file opened first: the page asks the user to confirm
//! between the two.

use std::time::Duration;

use tempest_proto::sound::Sound;
use tempest_proto::sound_message::{self, SoundMessage, CMD_CURRENT_SOUND, REQUEST_SOUND};

use super::Machine;
use crate::command::{Command, Kind};
use crate::error::AppError;
use crate::event::Event;
use crate::library::now_secs;

const SOUND_TIMEOUT: Duration = Duration::from_secs(3);

/// The event describing a sound: its 127 raw values and its internal name.
pub(super) fn sound_event(name: &'static str, sound: &Sound) -> Event {
    Event::new(name)
        .with("values", sound.values().to_vec())
        .with("name", sound.name())
}

impl Machine {
    pub(super) fn handle_sound(&mut self, command: Command) -> Option<Command> {
        match command {
            Command::FetchSound => self.guard("fetch_sound", Machine::fetch_sound),
            Command::SaveSound(name) => self.guard("save_sound", |m| m.save_sound(&name)),
            Command::OpenSound { name, data } => {
                self.guard("open_sound", |m| m.open_sound(&name, &data));
            }
            Command::SendSound => self.guard("send_sound", Machine::send_sound),
            other => return Some(other),
        }
        None
    }

    /// Asks the machine for its current sound (`40` -> `60`).
    pub(super) fn query_sound(&mut self) -> Result<Sound, AppError> {
        let reply = self.ask(&REQUEST_SOUND, CMD_CURRENT_SOUND, SOUND_TIMEOUT)?;
        match sound_message::decode(&reply) {
            Ok(SoundMessage::Current(sound)) => Ok(sound),
            _ => Err(AppError::Invalid(
                "the machine's answer is not a sound".into(),
            )),
        }
    }

    fn fetch_sound(&mut self) -> Result<(), AppError> {
        let sound = self.query_sound()?;
        self.emit(sound_event("sound", &sound));
        self.edit_synced(&sound);
        self.fetched_sound = Some(sound);
        self.fetched_sound_at = now_secs();
        Ok(())
    }

    fn save_sound(&mut self, name: &str) -> Result<(), AppError> {
        let sound = self.fetched_sound.as_ref().ok_or(AppError::Precondition(
            "fetch the sound from the machine first",
        ))?;
        let file = sound_message::encode_file(name.trim(), sound)
            .map_err(|e| AppError::Invalid(e.to_string()))?;
        let candidate = Self::candidate(
            Kind::Sounds,
            name.trim(),
            &file,
            "machine",
            self.fetched_sound_at,
            true,
        )?;
        let title = format!("Save sound \"{}\"", name.trim());
        self.submit(vec![candidate], &title, "sound", false)
    }

    fn open_sound(&mut self, file_name: &str, data: &[u8]) -> Result<(), AppError> {
        let (sound, kind) = match sound_message::decode(data) {
            Ok(SoundMessage::Current(sound)) => (sound, "current"),
            Ok(SoundMessage::File { sound, .. }) => (sound, "file"),
            Err(e) => return Err(AppError::Invalid(format!("not a sound file: {e}"))),
        };
        self.emit(
            sound_event("sound_opened", &sound)
                .with("file", file_name)
                .with("kind", kind)
                .with("size", data.len()),
        );
        self.opened_sound = Some(sound);
        Ok(())
    }

    fn send_sound(&mut self) -> Result<(), AppError> {
        let sound = self
            .opened_sound
            .as_ref()
            .ok_or(AppError::Precondition("open a sound file first"))?;
        let message = sound_message::encode_current(sound);
        self.send_sysex("sound", &message)?;
        self.emit(Event::new("sent").with("kind", "sound"));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::testkit::TestRig;
    use crate::storage::Folder;
    use tempest_proto::pack7::{is_standard_for, unpack};
    use tempest_proto::sound::Sound;

    fn fetched(label: &str) -> TestRig {
        let mut rig = TestRig::connected(label);
        rig.machine.handle(Command::FetchSound);
        rig
    }

    #[test]
    fn test_fetch_sound_when_machine_answers_returns_127_values_and_the_name() {
        let mut rig = fetched("sound-fetch");

        let event = rig.expect("sound");

        assert_eq!(event.field("name"), Some(&"Fake Kick".into()));
        assert!(matches!(event.field("values"), Some(crate::json::Json::Arr(v)) if v.len() == 127));
    }

    #[test]
    fn test_fetch_sound_when_machine_silent_returns_timeout_error_and_keeps_no_sound() {
        let mut rig = TestRig::connected("sound-silent");
        rig.transport().set_responder(Box::new(|_| vec![]));

        rig.machine.handle(Command::FetchSound);

        assert_eq!(rig.expect("error").field("code"), Some(&"timeout".into()));
        assert!(rig.machine.fetched_sound.is_none());
    }

    #[test]
    fn test_save_sound_when_fetched_returns_a_63_file_that_decodes_to_the_same_sound() {
        let mut rig = fetched("sound-save");

        rig.machine.handle(Command::SaveSound("My Kick".into()));

        let saved = rig.expect("saved");
        assert_eq!(saved.field("file"), Some(&"My Kick.syx".into()));
        let bytes = rig
            .machine
            .storage
            .read(Folder::Sounds, "My Kick.syx")
            .unwrap();
        match sound_message::decode(&bytes).unwrap() {
            SoundMessage::File { name, sound } => {
                assert_eq!(name, "My Kick");
                assert_eq!(Some(&sound), rig.machine.fetched_sound.as_ref());
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn test_save_sound_when_nothing_fetched_returns_precondition_error_and_writes_nothing() {
        let mut rig = TestRig::connected("sound-save-none");

        rig.machine.handle(Command::SaveSound("x".into()));

        assert_eq!(
            rig.expect("error").field("code"),
            Some(&"precondition".into())
        );
        assert!(rig.machine.storage.list(Folder::Sounds).unwrap().is_empty());
    }

    #[test]
    fn test_save_sound_when_name_not_ascii_returns_invalid_error() {
        let mut rig = fetched("sound-save-name");

        rig.machine
            .handle(Command::SaveSound("Grosse caisse é".into()));

        assert_eq!(rig.expect("error").field("code"), Some(&"invalid".into()));
    }

    #[test]
    fn test_open_sound_when_not_a_sound_returns_error_and_keeps_nothing() {
        let mut rig = TestRig::connected("sound-open-bad");

        rig.machine.handle(Command::OpenSound {
            name: "x.syx".into(),
            data: vec![1, 2, 3],
        });

        assert_eq!(rig.expect("error").field("code"), Some(&"invalid".into()));
        assert!(rig.machine.opened_sound.is_none());
    }

    #[test]
    fn test_send_sound_when_nothing_opened_returns_precondition_error_and_sends_nothing() {
        let mut rig = TestRig::connected("sound-send-none");

        rig.machine.handle(Command::SendSound);

        assert_eq!(
            rig.expect("error").field("code"),
            Some(&"precondition".into())
        );
        assert!(rig.transport().sent().is_empty());
    }

    #[test]
    fn test_send_sound_when_file_opened_returns_the_60_message_and_the_machine_keeps_it() {
        let mut rig = TestRig::connected("sound-send");
        let mut sound = Sound::with_defaults();
        sound.set(34, 7).unwrap();
        sound.set_name("From file").unwrap();
        let file = sound_message::encode_file("From file", &sound).unwrap();

        rig.machine.handle(Command::OpenSound {
            name: "From file.syx".into(),
            data: file,
        });
        rig.machine.handle(Command::SendSound);

        assert_eq!(
            rig.transport().sent_bytes(),
            sound_message::encode_current(&sound)
        );
        rig.machine.handle(Command::FetchSound);
        assert_eq!(rig.machine.fetched_sound, Some(sound));
    }

    /// A sound whose last two bytes (name bits) have their MSB set, the case the machine's flushed tail loses.
    fn sound_with_msb_tail() -> Sound {
        let mut bytes = *Sound::with_defaults().bytes();
        bytes[126] = 0xFF;
        bytes[127] = 0xA5;
        Sound::from_bytes(bytes)
    }

    /// The `60` message as the machine sends it: 2 bytes in the last group, flushed layout.
    fn machine_current(sound: &Sound) -> Vec<u8> {
        let mut message = vec![0xF0, 0x01, 0x28, CMD_CURRENT_SOUND];
        message.extend(tempest_proto::pack7::pack_flushed(sound.bytes()));
        message.push(0xF7);
        message
    }

    #[test]
    fn test_save_sound_when_machine_sends_the_flushed_tail_returns_a_standard_63_file_the_receiver_reads_exactly(
    ) {
        let mut rig = TestRig::connected("sound-save-msb");
        let sound = sound_with_msb_tail();
        let reply = machine_current(&sound);
        rig.transport()
            .set_responder(Box::new(move |_| vec![reply.clone()]));
        rig.machine.handle(Command::FetchSound);

        rig.machine.handle(Command::SaveSound("Tail".into()));

        let file = rig
            .machine
            .storage
            .read(Folder::Sounds, "Tail.syx")
            .unwrap();
        let mut payload = b"Tail\0".to_vec();
        payload.extend_from_slice(sound.bytes());
        let body = &file[5..file.len() - 1];
        assert!(is_standard_for(body, &payload));
        assert_eq!(unpack(body)[..payload.len()], payload[..]);
        assert_eq!(
            sound_message::decode(&file),
            Ok(SoundMessage::File {
                name: "Tail".into(),
                sound
            })
        );
    }

    #[test]
    fn test_machine_current_when_kept_as_is_returns_bytes_the_receiver_misreads() {
        // Counter-proof: the machine's own `60` fails the standard check, and the receiver rule loses the MSBs.
        let sound = sound_with_msb_tail();
        let raw = machine_current(&sound);
        let body = &raw[4..raw.len() - 1];

        assert!(!is_standard_for(body, sound.bytes()));
        assert_ne!(unpack(body)[..128], sound.bytes()[..]);
    }

    #[test]
    fn test_send_sound_when_an_old_flushed_60_file_is_opened_returns_the_standard_message() {
        let mut rig = TestRig::connected("sound-send-old");
        let sound = sound_with_msb_tail();

        rig.machine.handle(Command::OpenSound {
            name: "old.syx".into(),
            data: machine_current(&sound),
        });
        rig.machine.handle(Command::SendSound);

        let sent = rig.transport().sent_bytes();
        assert_eq!(sent, sound_message::encode_current(&sound));
        assert!(is_standard_for(&sent[4..sent.len() - 1], sound.bytes()));
        assert_eq!(unpack(&sent[4..sent.len() - 1])[..128], sound.bytes()[..]);
    }
}
