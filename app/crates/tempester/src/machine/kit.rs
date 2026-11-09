//! Kit builder: put sounds of the library on the pads of the machine's current beat. The beat is fetched from the
//! machine (`41`), only the 128-byte sound records of the chosen pads are replaced in its data
//! (`tempest_proto::beat::with_sounds`: the header, the events and every other sound stay byte for byte), and the
//! result goes back as a `5F`. Like every send, it is in two steps: the check looks at the machine's beat and says
//! what would be replaced; the apply looks again, keeps the machine's beat as an auto backup when the library does
//! not hold it, and only then sends.

use tempest_proto::beat::{self, PADS};
use tempest_proto::sound::Sound;
use tempest_proto::sound_message::{self, SoundMessage};

use super::manager::{done_event, known_as};
use super::midi_export::beat_data;
use super::{BeatData, Machine};
use crate::command::Kind;
use crate::error::AppError;
use crate::event::Event;
use crate::library::inspect::{inspect, Facts};
use crate::library::model::{kind_name, Index};

/// The sounds chosen for the pads: `None` keeps what the pad has.
struct Kit {
    sounds: [Option<Sound>; PADS],
    /// `(pad, sound name)` of every replaced pad, in pad order.
    replaced: Vec<(usize, String)>,
}

/// The `5F` message of `machine` with the kit's sounds in place, in the standard layout.
fn rebuilt(machine: &BeatData, kit: &Kit) -> Result<Vec<u8>, AppError> {
    let data = beat_data(&machine.message)?;
    let kitted = beat::with_sounds(&data, &kit.sounds)
        .map_err(|e| AppError::Invalid(format!("cannot put the sounds in the beat: {e}")))?;
    Ok(beat::standard_dump(&kitted))
}

fn plan_event(kit: &Kit, machine: &BeatData, facts: &Facts, index: &Index, changed: bool) -> Event {
    let (status, backup) = match known_as(index, &facts.hash) {
        Some(place) => (
            "known",
            format!("The machine's beat \"{}\" is already in your library as {place}: nothing is lost.", machine.summary.name),
        ),
        None => (
            "unknown",
            format!("The machine's beat \"{}\" is NOT in your library: it is first saved as \"auto backup <date>\" (Library > Auto backups), then the kit is sent.", machine.summary.name),
        ),
    };
    let pads: Vec<String> = kit
        .replaced
        .iter()
        .map(|(pad, name)| format!("{} {name}", pad + 1))
        .collect();
    let mut lines = vec![
        format!(
            "{} pad(s) get a new sound: {}.",
            kit.replaced.len(),
            pads.join(", ")
        ),
        format!(
            "Only those sounds change: the beat's {} events, tempo, swing, effects and the other sounds are kept byte for byte.",
            machine.summary.events
        ),
        backup,
    ];
    if changed {
        lines.insert(
            0,
            "The machine's beat changed since you looked: check again.".to_string(),
        );
    }
    Event::new("lib_kit_plan")
        .with("kind", kind_name(Kind::Beats))
        .with("status", status)
        .with("changed", changed)
        .with("item", format!("Kit of {} sound(s)", kit.replaced.len()))
        .with("from", "the library")
        .with("machine", machine.summary.name.clone())
        .with("token", facts.hash.clone())
        .with("lines", lines)
}

impl Machine {
    /// The sounds of the library items chosen for the pads.
    fn load_kit(&self, ids: &[Option<String>]) -> Result<Kit, AppError> {
        let mut sounds: [Option<Sound>; PADS] = std::array::from_fn(|_| None);
        let mut replaced = Vec::new();
        for (pad, id) in ids.iter().enumerate().take(PADS) {
            let Some(id) = id else { continue };
            let (_, bytes) = self.item_bytes(Kind::Sounds, id)?;
            let sound = match sound_message::decode(&bytes) {
                Ok(SoundMessage::File { sound, .. } | SoundMessage::Current(sound)) => sound,
                Err(e) => return Err(AppError::Invalid(format!("not a sound file: {e}"))),
            };
            replaced.push((pad, sound.name().trim().to_string()));
            sounds[pad] = Some(sound);
        }
        if replaced.is_empty() {
            return Err(AppError::Precondition(
                "choose a sound for at least one pad",
            ));
        }
        Ok(Kit { sounds, replaced })
    }

    /// The machine's current beat, with its facts (the hash is the token of the apply step).
    fn machine_beat(&mut self) -> Result<(BeatData, Facts), AppError> {
        self.idle_session()?;
        let beat = self.query_beat()?;
        let facts = inspect(Kind::Beats, &beat.message, "machine")?;
        Ok((beat, facts))
    }

    pub(super) fn kit_check(&mut self, ids: &[Option<String>]) -> Result<(), AppError> {
        let kit = self.load_kit(ids)?;
        let (machine, facts) = self.machine_beat()?;
        rebuilt(&machine, &kit)?;
        let index = self.library().snapshot()?;
        self.emit(plan_event(&kit, &machine, &facts, &index, false));
        Ok(())
    }

    pub(super) fn kit_apply(
        &mut self,
        ids: &[Option<String>],
        token: &str,
    ) -> Result<(), AppError> {
        let kit = self.load_kit(ids)?;
        let (machine, facts) = self.machine_beat()?;
        let message = rebuilt(&machine, &kit)?;
        let library = self.library();
        let index = library.snapshot()?;
        if facts.hash != token {
            self.emit(plan_event(&kit, &machine, &facts, &index, true));
            return Ok(());
        }
        let mut lines = Vec::new();
        if known_as(&index, &facts.hash).is_none() {
            let kept = library.auto_backup(&facts)?;
            lines.push(format!(
                "The machine's beat \"{}\" was not in the library: it was saved as an auto backup first ({kept}; see Library > Auto backups, it can be opened and sent back).",
                machine.summary.name
            ));
        }
        self.send_sysex("beat", &message)?;
        self.emit(Event::new("sent").with("kind", "beat"));
        lines.insert(
            0,
            format!(
                "Sent the kit of {} sound(s) to the machine as its current beat \"{}\". To undo: open the auto backup or the beat you replaced and send it.",
                kit.replaced.len(),
                machine.summary.name
            ),
        );
        self.emit(done_event("Kit to the machine", &lines));
        self.publish_library(Kind::Beats)
    }
}

#[cfg(test)]
mod tests;
