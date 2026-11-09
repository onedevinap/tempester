//! A beat as a Standard MIDI File (type 1), standard library only.
//!
//! The resolution is **96 pulses per quarter note**, which is the machine's own position tick (`refs/firmware-main.md`
//! 7.1: 96 ticks per quarter note, 8 per step, 12 steps per quarter), so every event position converts without
//! rounding. The MIDI clock (24 per quarter) is a quarter of it, and a stored note duration is in 24 ppq units: it is
//! 4 ticks each, exactly. 960 PPQN would only multiply every number by ten.
//!
//! What the file holds:
//! - track 0: the beat's name, tempo, time signature, and the beat's length (the end of every track, so a player
//!   loops it as the machine does);
//! - one track per pad that plays something, named `Pad NN <sound name>`, on channel 10, with the note the machine
//!   plays that pad with (its own table, `pad_notes`, as the app knows it: the beat holds no MIDI note field);
//! - velocity and duration straight from the events; the swing, the Time Shift and the half step are already in the
//!   note positions, exactly as the machine computes them (`BeatSequence::event_tick`), not kept as separate data;
//! - the tempo is stored in microseconds per quarter note, rounded to the nearest: 109.0 BPM is 550459 us, which is
//!   109.00 BPM to two decimals. This is the one approximation, forced by the format.
//!
//! A note that would still sound at the next note of the same pad is cut there (two overlapping notes of one key are
//! ambiguous in a file); a note is also cut at the beat's end. Events the machine never plays (velocity 0, free
//! slot, step past the beat's end) are left out and counted in `Export`.

use crate::beat::PADS;
use crate::beat_sequence::BeatSequence;
use crate::pad_notes::{PadNotes, PAD_COUNT};

pub const PPQN: u16 = 96;
// A beat's pads are the pads of the note table: one note for each.
const _: () = assert!(PADS == PAD_COUNT);
const CHANNEL: u8 = 9;
const TICKS_PER_DURATION_UNIT: u32 = 4;
const MAX_TEMPO: u32 = 0x00FF_FFFF;

/// The file and what went into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Export {
    pub bytes: Vec<u8>,
    /// Notes written.
    pub notes: usize,
    /// Tracks written, the tempo track included.
    pub tracks: usize,
    /// Events not played by the machine: velocity 0 or duration 0.
    pub silent: usize,
    /// Events whose step is past the end of the beat.
    pub beyond_end: usize,
    /// Events dropped because another event of the same pad lands on the same tick.
    pub overlapped: usize,
}

struct Note {
    start: u32,
    end: u32,
    velocity: u8,
}

/// Writes the Standard MIDI File of `sequence`, each pad on its note in `pad_map`.
pub fn export(sequence: &BeatSequence, pad_map: &PadNotes) -> Export {
    let length = sequence.length_ticks();
    let mut counts = Export {
        bytes: Vec::new(),
        notes: 0,
        tracks: 1,
        silent: 0,
        beyond_end: 0,
        overlapped: 0,
    };
    let mut tracks = vec![tempo_track(sequence, length)];
    for (pad, note) in pad_map.notes().into_iter().enumerate().take(PADS) {
        let notes = pad_notes(sequence, pad, &mut counts);
        if !notes.is_empty() {
            counts.notes += notes.len();
            tracks.push(pad_track(sequence, pad, note, &notes, length));
        }
    }
    counts.tracks = tracks.len();
    counts.bytes = assemble(&tracks);
    counts
}

fn pad_notes(sequence: &BeatSequence, pad: usize, counts: &mut Export) -> Vec<Note> {
    let length = sequence.length_ticks();
    let mut starts: Vec<(u32, u32, u8)> = Vec::new();
    for event in sequence.events.iter().filter(|e| usize::from(e.pad) == pad) {
        if !event.is_played() {
            counts.silent += 1;
        } else if let Some(tick) = sequence.event_tick(event) {
            let end = tick + u32::from(event.duration) * TICKS_PER_DURATION_UNIT;
            starts.push((tick, end.min(length), event.velocity));
        } else {
            counts.beyond_end += 1;
        }
    }
    starts.sort_by_key(|(tick, _, _)| *tick);
    let mut notes: Vec<Note> = Vec::new();
    for (start, end, velocity) in starts {
        if let Some(last) = notes.last_mut() {
            if last.start == start {
                counts.overlapped += 1;
                continue;
            }
            last.end = last.end.min(start);
        }
        notes.push(Note {
            start,
            end,
            velocity,
        });
    }
    // A note that starts on the very last tick has no room to sound before the end: it keeps one tick.
    for note in &mut notes {
        note.end = note.end.max(note.start + 1);
    }
    notes
}

fn tempo_track(sequence: &BeatSequence, length: u32) -> Vec<u8> {
    let mut track = Vec::new();
    meta(&mut track, 0, 0x03, sequence.name.as_bytes());
    let tenths = sequence.tempo_tenths;
    if let Some(micros) = (600_000_000 + tenths / 2).checked_div(tenths) {
        let micros = micros.min(MAX_TEMPO).to_be_bytes();
        meta(&mut track, 0, 0x51, &micros[1..]);
    }
    if let Some(power) = sequence.denominator_power() {
        // Clocks per metronome click: a unit lasts 96 / denominator MIDI clocks (384 ticks of the whole note / 4).
        let clocks = (96 >> power) as u8;
        let signature = [sequence.beats_per_measure, power, clocks, 8];
        meta(&mut track, 0, 0x58, &signature);
    }
    meta(&mut track, length, 0x2F, &[]);
    track
}

fn pad_track(
    sequence: &BeatSequence,
    pad: usize,
    note: u8,
    notes: &[Note],
    length: u32,
) -> Vec<u8> {
    let mut track = Vec::new();
    let sound = sequence.sound_names[pad].as_deref().unwrap_or("").trim();
    let name = format!("Pad {:02} {sound}", pad + 1);
    meta(&mut track, 0, 0x03, name.trim_end().as_bytes());
    // (tick, off before on at the same tick, status byte, velocity)
    let mut messages: Vec<(u32, u8, u8, u8)> = Vec::with_capacity(notes.len() * 2);
    for n in notes {
        messages.push((n.start, 1, 0x90 | CHANNEL, n.velocity));
        messages.push((n.end, 0, 0x80 | CHANNEL, 0));
    }
    messages.sort_by_key(|(tick, order, _, _)| (*tick, *order));
    let mut at = 0;
    for (tick, _, status, velocity) in messages {
        variable_length(&mut track, tick - at);
        track.extend_from_slice(&[status, note, velocity]);
        at = tick;
    }
    meta(&mut track, length - at, 0x2F, &[]);
    track
}

fn meta(track: &mut Vec<u8>, delta: u32, kind: u8, data: &[u8]) {
    variable_length(track, delta);
    track.extend_from_slice(&[0xFF, kind]);
    variable_length(track, data.len() as u32);
    track.extend_from_slice(data);
}

/// A number as the format's variable-length quantity: 7 bits at a time, high group first, bit 7 set but on the last.
fn variable_length(out: &mut Vec<u8>, value: u32) {
    let mut groups = vec![(value & 0x7F) as u8];
    let mut rest = value >> 7;
    while rest > 0 {
        groups.push((rest & 0x7F) as u8 | 0x80);
        rest >>= 7;
    }
    out.extend(groups.iter().rev());
}

fn assemble(tracks: &[Vec<u8>]) -> Vec<u8> {
    let mut out = b"MThd".to_vec();
    out.extend_from_slice(&6u32.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&(tracks.len() as u16).to_be_bytes());
    out.extend_from_slice(&PPQN.to_be_bytes());
    for track in tracks {
        out.extend_from_slice(b"MTrk");
        out.extend_from_slice(&(track.len() as u32).to_be_bytes());
        out.extend_from_slice(track);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beat_sequence::BeatEvent;

    fn event(pad: u8, step: u16, velocity: u8, duration: u8) -> BeatEvent {
        BeatEvent {
            step,
            pad,
            velocity,
            duration,
            time_shift: 7,
            half_step: false,
        }
    }

    fn sequence(events: Vec<BeatEvent>) -> BeatSequence {
        BeatSequence {
            name: "t".into(),
            tempo_tenths: 1200,
            swing: 0,
            beats_per_measure: 4,
            unit_index: 2,
            measures: 1,
            events,
            sound_names: std::array::from_fn(|_| None),
        }
    }

    #[test]
    fn test_variable_length_when_known_values_returns_the_specification_bytes() {
        let encode = |value| {
            let mut out = Vec::new();
            variable_length(&mut out, value);
            out
        };
        assert_eq!(encode(0), [0x00]);
        assert_eq!(encode(0x7F), [0x7F]);
        assert_eq!(encode(0x80), [0x81, 0x00]);
        assert_eq!(encode(384), [0x83, 0x00]);
        assert_eq!(encode(0x0FFF_FFFF), [0xFF, 0xFF, 0xFF, 0x7F]);
    }

    #[test]
    fn test_export_when_synthetic_beat_returns_the_exact_bytes() {
        let beat = sequence(vec![
            event(0, 0, 100, 3),
            event(0, 12, 90, 1),
            event(2, 6, 64, 24),
        ]);

        let export = export(&beat, &PadNotes::default());

        #[rustfmt::skip]
        let expected: Vec<u8> = vec![
            0x4D, 0x54, 0x68, 0x64, 0, 0, 0, 6, 0, 1, 0, 3, 0, 0x60,
            0x4D, 0x54, 0x72, 0x6B, 0, 0, 0, 25,
            0x00, 0xFF, 0x03, 0x01, b't',
            0x00, 0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20,
            0x00, 0xFF, 0x58, 0x04, 4, 2, 24, 8,
            0x83, 0x00, 0xFF, 0x2F, 0x00,
            0x4D, 0x54, 0x72, 0x6B, 0, 0, 0, 31,
            0x00, 0xFF, 0x03, 0x06, b'P', b'a', b'd', b' ', b'0', b'1',
            0x00, 0x99, 36, 100,
            0x0C, 0x89, 36, 0,
            0x54, 0x99, 36, 90,
            0x04, 0x89, 36, 0,
            0x82, 0x1C, 0xFF, 0x2F, 0x00,
            0x4D, 0x54, 0x72, 0x6B, 0, 0, 0, 23,
            0x00, 0xFF, 0x03, 0x06, b'P', b'a', b'd', b' ', b'0', b'3',
            0x30, 0x99, 40, 64,
            0x60, 0x89, 40, 0,
            0x81, 0x70, 0xFF, 0x2F, 0x00,
        ];
        assert_eq!(export.bytes, expected);
        assert_eq!((export.notes, export.tracks), (3, 3));
    }

    #[test]
    fn test_export_when_pad_notes_are_learned_returns_each_pad_on_its_learned_note() {
        let beat = sequence(vec![event(0, 0, 100, 3), event(17, 2, 90, 3)]);
        let mut map = PadNotes::default();
        map.learn(0, 100);
        map.learn(17, 30);

        let bytes = export(&beat, &map).bytes;

        let on = |note: u8| bytes.windows(2).any(|w| w == [0x99, note]);
        assert!(on(100) && on(30), "pads A1 and B2 on their learned notes");
        assert!(!on(36) && !on(53), "not 36 + pad");
    }

    #[test]
    fn test_export_when_notes_overlap_on_one_pad_returns_the_first_cut_at_the_second() {
        // Step 0 lasts 100 x 4 ticks but the next note of the pad starts at tick 8.
        let beat = sequence(vec![event(1, 0, 100, 100), event(1, 1, 100, 1)]);

        let notes = pad_notes(
            &beat,
            1,
            &mut export(&sequence(Vec::new()), &PadNotes::default()),
        );

        assert_eq!(
            notes.iter().map(|n| (n.start, n.end)).collect::<Vec<_>>(),
            vec![(0, 8), (8, 12)]
        );
    }

    #[test]
    fn test_export_when_events_are_silent_beyond_the_end_or_collide_returns_them_counted_not_written(
    ) {
        let beat = sequence(vec![
            event(0, 0, 0, 3),    // velocity 0
            event(0, 1, 100, 0),  // free slot
            event(0, 48, 100, 3), // step 48 = tick 384 = the end of a 1-measure beat
            event(0, 2, 100, 3),
            event(0, 2, 90, 3), // same tick as the previous one
        ]);

        let export = export(&beat, &PadNotes::default());

        assert_eq!(
            (
                export.notes,
                export.silent,
                export.beyond_end,
                export.overlapped
            ),
            (1, 2, 1, 1)
        );
    }

    #[test]
    fn test_export_when_the_note_would_pass_the_end_returns_it_cut_at_the_beats_length() {
        let beat = sequence(vec![event(0, 47, 100, 255)]);

        let notes = pad_notes(
            &beat,
            0,
            &mut export(&sequence(Vec::new()), &PadNotes::default()),
        );

        assert_eq!((notes[0].start, notes[0].end), (376, 384));
    }
}
