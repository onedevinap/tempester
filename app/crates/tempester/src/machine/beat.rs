//! Beats: fetch the current beat (`41` -> `5F`), save it, open a `.syx` and restore it (`5F` sent to the machine, after
//! the user's confirmation). The machine ends its dump with a 7-bit group its own receivers misread
//! (`refs/firmware-main.md` 9.1), so a beat is decoded exactly and kept re-encoded in the standard layout: that is
//! what is saved and what is sent, whichever layout the dump or the opened file came in (an old file saved from a
//! machine dump is repaired on the way). A dump whose end cannot be decided is refused, not guessed.

use std::time::Duration;

use tempest_proto::beat::{self, PADS};
use tempest_proto::sound_message::REQUEST_BEAT;

use super::{BeatData, Machine};
use crate::command::{Command, Kind};
use crate::error::AppError;
use crate::event::Event;
use crate::library::now_secs;

/// A beat is a few KB to a few tens of KB; on a DIN cable that takes seconds.
const BEAT_TIMEOUT: Duration = Duration::from_secs(20);
const CMD_BEAT: u8 = 0x5F;

fn beat_event(name: &'static str, data: &BeatData) -> Event {
    let assigned = data
        .summary
        .voice_assign
        .iter()
        .filter(|v| **v != 0)
        .count();
    Event::new(name)
        .with("name", data.summary.name.clone())
        .with("events", data.summary.events)
        .with("voice_assign", data.summary.voice_assign.to_vec())
        .with("play_probability", data.summary.play_probability.to_vec())
        .with("assigned", assigned)
        .with("pads", PADS)
        .with("size", data.message.len())
}

/// The beat of a `5F` message in either layout, kept in the standard one.
fn read_beat(message: &[u8]) -> Result<BeatData, AppError> {
    let invalid = |e: beat::BeatError| AppError::Invalid(format!("not a beat dump: {e}"));
    let message = beat::standard_message(message).map_err(invalid)?;
    let summary = beat::summarize(&message).map_err(invalid)?;
    Ok(BeatData { message, summary })
}

impl Machine {
    pub(super) fn handle_beat(&mut self, command: Command) -> Option<Command> {
        match command {
            Command::FetchBeat => self.guard("fetch_beat", Machine::fetch_beat),
            Command::SaveBeat(name) => self.guard("save_beat", |m| m.save_beat(&name)),
            Command::OpenBeat { name, data } => {
                self.guard("open_beat", |m| m.open_beat(&name, &data));
            }
            Command::SendBeat => self.guard("send_beat", Machine::send_beat),
            other => return Some(other),
        }
        None
    }

    /// Asks the machine for its current beat (`41` -> `5F`).
    pub(super) fn query_beat(&mut self) -> Result<BeatData, AppError> {
        let message = self.ask(&REQUEST_BEAT, CMD_BEAT, BEAT_TIMEOUT)?;
        read_beat(&message)
    }

    fn fetch_beat(&mut self) -> Result<(), AppError> {
        let data = self.query_beat()?;
        self.emit(beat_event("beat", &data));
        self.fetched_beat = Some(data);
        self.fetched_beat_at = now_secs();
        Ok(())
    }

    fn save_beat(&mut self, name: &str) -> Result<(), AppError> {
        let data = self.fetched_beat.as_ref().ok_or(AppError::Precondition(
            "fetch the beat from the machine first",
        ))?;
        let candidate = Self::candidate(
            Kind::Beats,
            name.trim(),
            &data.message,
            "machine",
            self.fetched_beat_at,
            true,
        )?;
        let title = format!("Save beat \"{}\"", name.trim());
        self.submit(vec![candidate], &title, "beat", false)
    }

    fn open_beat(&mut self, file_name: &str, message: &[u8]) -> Result<(), AppError> {
        let data = read_beat(message)?;
        self.emit(beat_event("beat_opened", &data).with("file", file_name));
        self.opened_beat = Some(data);
        Ok(())
    }

    fn send_beat(&mut self) -> Result<(), AppError> {
        let data = self
            .opened_beat
            .as_ref()
            .ok_or(AppError::Precondition("open a beat file first"))?;
        let message = data.message.clone();
        self.send_sysex("beat", &message)?;
        self.emit(Event::new("sent").with("kind", "beat"));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::Json;
    use crate::machine::testkit::TestRig;
    use crate::storage::Folder;
    use tempest_proto::pack7::{is_standard_for, unpack};

    #[test]
    fn test_fetch_beat_when_machine_answers_returns_name_events_and_assignments() {
        let mut rig = TestRig::connected("beat-fetch");

        rig.machine.handle(Command::FetchBeat);

        let event = rig.expect("beat");
        assert_eq!(event.field("name"), Some(&"Fake beat 01".into()));
        assert_eq!(event.field("events"), Some(&24usize.into()));
        assert_eq!(event.field("assigned"), Some(&4usize.into()));
        assert!(matches!(event.field("voice_assign"), Some(Json::Arr(a)) if a.len() == PADS));
    }

    #[test]
    fn test_fetch_beat_when_answer_is_not_a_beat_returns_invalid_error() {
        let mut rig = TestRig::connected("beat-bad");
        rig.transport().set_responder(Box::new(|_| {
            vec![vec![0xF0, 0x01, 0x28, 0x5F, 0x00, 0x01, 0xF7]]
        }));

        rig.machine.handle(Command::FetchBeat);

        assert_eq!(rig.expect("error").field("code"), Some(&"invalid".into()));
        assert!(rig.machine.fetched_beat.is_none());
    }

    #[test]
    fn test_save_beat_when_fetched_returns_the_standard_layout_dump_in_the_library() {
        let mut rig = TestRig::connected("beat-save");
        rig.machine.handle(Command::FetchBeat);

        rig.machine.handle(Command::SaveBeat("Groove".into()));

        let bytes = rig
            .machine
            .storage
            .read(Folder::Beats, "Groove.syx")
            .unwrap();
        assert_eq!(
            Some(&bytes),
            rig.machine.fetched_beat.as_ref().map(|b| &b.message)
        );
        let data = beat::read_body(&bytes[4..bytes.len() - 1]).unwrap();
        assert!(is_standard_for(&bytes[4..bytes.len() - 1], &data));
        assert!(rig.events().iter().any(|e| e.name == "library"));
    }

    /// A machine that answers `41` with a `5F` ending in a partial group of `tail` bytes, in its own (flushed) layout.
    fn rig_with_beat_tail(label: &str, tail: usize) -> (TestRig, Vec<u8>) {
        let rig = TestRig::connected(label);
        let data = beat::synthetic_tail_beat(tail);
        let dump = beat::machine_dump(&data);
        rig.transport()
            .set_responder(Box::new(move |_| vec![dump.clone()]));
        (rig, data)
    }

    #[test]
    fn test_save_beat_when_machine_dump_ends_in_every_partial_group_returns_a_file_the_receiver_reads_exactly(
    ) {
        for tail in 0..=6 {
            let (mut rig, data) = rig_with_beat_tail(&format!("beat-save-tail-{tail}"), tail);
            rig.machine.handle(Command::FetchBeat);

            rig.machine.handle(Command::SaveBeat("Tail".into()));

            let file = rig.machine.storage.read(Folder::Beats, "Tail.syx").unwrap();
            let body = &file[4..file.len() - 1];
            assert_eq!(file, beat::standard_dump(&data), "tail {tail}");
            assert!(is_standard_for(body, &data), "tail {tail}");
            // The machine's receiver rule: MSBs of the final bytes included.
            assert_eq!(unpack(body)[..data.len()], data[..], "tail {tail}");
            assert!(data[data.len() - 1] >= 0x80);
        }
    }

    #[test]
    fn test_save_beat_when_the_machine_dump_is_kept_as_is_returns_bytes_that_fail_the_check() {
        // Counter-proof: the raw flushed dump is not what the app saves, and it fails the standard check.
        for tail in 1..=6 {
            let (mut rig, data) = rig_with_beat_tail(&format!("beat-raw-tail-{tail}"), tail);
            rig.machine.handle(Command::FetchBeat);
            rig.machine.handle(Command::SaveBeat("Tail".into()));
            let raw = beat::machine_dump(&data);

            let file = rig.machine.storage.read(Folder::Beats, "Tail.syx").unwrap();

            assert_ne!(file, raw, "tail {tail}");
            assert!(
                !is_standard_for(&raw[4..raw.len() - 1], &data),
                "tail {tail}"
            );
        }
    }

    #[test]
    fn test_send_beat_when_an_old_flushed_file_is_opened_returns_the_standard_layout_to_the_machine(
    ) {
        for tail in 0..=6 {
            let mut rig = TestRig::connected(&format!("beat-send-old-{tail}"));
            let data = beat::synthetic_tail_beat(tail);

            rig.machine.handle(Command::OpenBeat {
                name: "old.syx".into(),
                data: beat::machine_dump(&data),
            });
            rig.machine.handle(Command::SendBeat);

            let sent = rig.transport().sent_bytes();
            assert_eq!(sent, beat::standard_dump(&data), "tail {tail}");
            assert!(
                is_standard_for(&sent[4..sent.len() - 1], &data),
                "tail {tail}"
            );
            assert_eq!(unpack(&sent[4..sent.len() - 1])[..data.len()], data[..]);
        }
    }

    #[test]
    fn test_send_beat_when_an_ambiguous_or_cut_file_is_opened_returns_error_and_sends_nothing() {
        let mut rig = TestRig::connected("beat-send-cut");
        let mut cut = beat::machine_dump(&beat::synthetic_tail_beat(3));
        cut.drain(2000..2016);

        rig.machine.handle(Command::OpenBeat {
            name: "cut.syx".into(),
            data: cut,
        });
        rig.machine.handle(Command::SendBeat);

        let error = rig.expect("error");
        assert_eq!(error.field("code"), Some(&"invalid".into()));
        assert!(error.render().contains("not a beat dump"));
        assert!(rig.machine.opened_beat.is_none());
        assert!(rig.transport().sent().is_empty());
    }

    #[test]
    fn test_save_beat_when_nothing_fetched_returns_precondition_error() {
        let mut rig = TestRig::connected("beat-save-none");

        rig.machine.handle(Command::SaveBeat("x".into()));

        assert_eq!(
            rig.expect("error").field("code"),
            Some(&"precondition".into())
        );
        assert!(rig.machine.storage.list(Folder::Beats).unwrap().is_empty());
    }

    #[test]
    fn test_send_beat_when_opened_returns_exactly_the_file_bytes_to_the_machine() {
        let mut rig = TestRig::connected("beat-send");
        let dump = beat::standard_dump(&beat::synthetic_tail_beat(3));

        rig.machine.handle(Command::OpenBeat {
            name: "r.syx".into(),
            data: dump.clone(),
        });
        rig.machine.handle(Command::SendBeat);

        assert_eq!(rig.transport().sent_bytes(), dump);
        rig.machine.handle(Command::FetchBeat);
        assert_eq!(
            rig.machine.fetched_beat.map(|b| b.summary.name),
            Some("Tail".to_string())
        );
    }

    #[test]
    fn test_send_beat_when_nothing_opened_returns_precondition_error_and_sends_nothing() {
        let mut rig = TestRig::connected("beat-send-none");

        rig.machine.handle(Command::SendBeat);

        assert_eq!(
            rig.expect("error").field("code"),
            Some(&"precondition".into())
        );
        assert!(rig.transport().sent().is_empty());
    }

    #[test]
    fn test_open_beat_when_file_is_a_sound_returns_error_and_keeps_nothing() {
        let mut rig = TestRig::connected("beat-open-bad");
        let sound = tempest_proto::sound_message::encode_current(
            &tempest_proto::sound::Sound::with_defaults(),
        );

        rig.machine.handle(Command::OpenBeat {
            name: "s.syx".into(),
            data: sound,
        });

        assert_eq!(rig.expect("error").field("code"), Some(&"invalid".into()));
        assert!(rig.machine.opened_beat.is_none());
    }
}
