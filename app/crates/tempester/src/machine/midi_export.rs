//! A beat as a Standard MIDI File: a library beat, or the beat fetched in the card, written to the `exports` folder.
//! The conversion (resolution, swing, durations, what is left out) is `tempest_proto::midi_file`; this file reads the
//! beat and says what was written and where.

use std::fs;

use tempest_proto::beat;
use tempest_proto::beat_sequence::{read_sequence, BeatSequence};
use tempest_proto::midi_file::{self, Export, PPQN};

use super::manager::done_event;
use super::Machine;
use crate::command::Kind;
use crate::error::AppError;
use crate::library::model::Item;
use crate::storage::Folder;

/// The name of a file for a beat that has none.
const FALLBACK_NAME: &str = "Beat";

/// The unpacked data of a beat file or message, whichever 7-bit layout it came in.
pub(super) fn beat_data(message: &[u8]) -> Result<Vec<u8>, AppError> {
    let invalid = |e: beat::BeatError| AppError::Invalid(format!("not a beat dump: {e}"));
    let message = beat::standard_message(message).map_err(invalid)?;
    beat::read_body(&message[4..message.len() - 1]).map_err(invalid)
}

impl Machine {
    /// The library item `id` of `kind` that is a plain file (not an auto backup slot) and its bytes.
    pub(super) fn item_bytes(&self, kind: Kind, id: &str) -> Result<(Item, Vec<u8>), AppError> {
        let library = self.library();
        let item = library.item(id)?;
        if item.kind != kind || item.slot {
            return Err(AppError::Invalid("this item cannot be used here".into()));
        }
        let bytes = fs::read(library.file_of(kind, &item.path)?)?;
        Ok((item, bytes))
    }

    /// Writes the beat `id` (empty: the one fetched from the machine) as a `.mid` file and reports it.
    pub(super) fn export_midi(&mut self, id: &str) -> Result<(), AppError> {
        let (name, message) = if id.is_empty() {
            let fetched = self.fetched_beat.as_ref().ok_or(AppError::Precondition(
                "fetch the beat from the machine first",
            ))?;
            (fetched.summary.name.clone(), fetched.message.clone())
        } else {
            let (item, bytes) = self.item_bytes(Kind::Beats, id)?;
            (item.name, bytes)
        };
        let sequence = read_sequence(&beat_data(&message)?)
            .map_err(|e| AppError::Invalid(format!("cannot read the beat's sequence: {e}")))?;
        let export = midi_file::export(&sequence, &self.pad_notes);
        let stem = if name.trim().is_empty() {
            FALLBACK_NAME
        } else {
            name.trim()
        };
        let dir = self.storage.folder(Folder::Exports)?;
        let path = self.storage.save_in(&dir, stem, "mid", &export.bytes)?;
        let mut lines = vec![format!("Wrote {}.", path.display())];
        lines.extend(describe(&sequence, &export));
        self.emit(done_event("Export as MIDI file", &lines));
        Ok(())
    }
}

fn describe(sequence: &BeatSequence, export: &Export) -> Vec<String> {
    let mut lines = vec![format!(
        "{} notes on {} pad tracks (each pad on its note in the pad note table of the Machine page, channel 10), {PPQN} PPQN, {:.1} BPM, {}/{} for {} measure(s); swing ({} ticks) and Time Shift are in the note positions.",
        export.notes,
        export.tracks - 1,
        f64::from(sequence.tempo_tenths) / 10.0,
        sequence.beats_per_measure,
        denominator_text(sequence),
        sequence.measures,
        sequence.swing,
    )];
    let left_out = [
        (export.silent, "silent (velocity or duration 0)"),
        (export.beyond_end, "past the end of the beat"),
        (
            export.overlapped,
            "on the same tick as another of their pad",
        ),
    ];
    for (count, why) in left_out.iter().filter(|(count, _)| *count > 0) {
        lines.push(format!("Left out: {count} event(s) {why}."));
    }
    lines
}

/// The time signature's denominator for people: a note value, or the machine's one-step unit.
fn denominator_text(sequence: &BeatSequence) -> String {
    sequence.denominator_power().map_or_else(
        || "(1 step)".to_string(),
        |power| (1u32 << power).to_string(),
    )
}

#[cfg(test)]
mod tests;
