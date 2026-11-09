//! A beat exported as a Standard MIDI File and read back by a small independent reader (running status, meta events,
//! variable-length quantities), so the writer is not only compared with itself.
//! The real-capture test needs the author's project capture (`TEMPESTER_PROJECT_CAPTURE`, see `real_capture.rs`):
//! `TEMPESTER_PROJECT_CAPTURE=<file> cargo test --test midi_export -- --ignored`.

#![allow(clippy::expect_used)] // integration tests: a panic with a message is the failure report

use std::fs;

use tempest_proto::beat_sequence::{read_sequence, BeatEvent, BeatSequence};
use tempest_proto::midi_file::{export, PPQN};
use tempest_proto::pad_notes::PadNotes;
use tempest_proto::project::ProjectAssembler;

#[derive(Debug, PartialEq, Eq)]
enum Item {
    NoteOn { tick: u32, key: u8, velocity: u8 },
    NoteOff { tick: u32, key: u8 },
    Tempo(u32),
    Signature { numerator: u8, power: u8 },
    Name(String),
    End { tick: u32 },
}

struct Smf {
    format: u16,
    division: u16,
    tracks: Vec<Vec<Item>>,
}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn take(&mut self, count: usize) -> &[u8] {
        let slice = &self.bytes[self.at..self.at + count];
        self.at += count;
        slice
    }

    fn byte(&mut self) -> u8 {
        self.take(1)[0]
    }

    fn u16(&mut self) -> u16 {
        u16::from_be_bytes(self.take(2).try_into().expect("two bytes"))
    }

    fn u32(&mut self) -> u32 {
        u32::from_be_bytes(self.take(4).try_into().expect("four bytes"))
    }

    fn variable(&mut self) -> u32 {
        let mut value = 0u32;
        loop {
            let byte = self.byte();
            value = value << 7 | u32::from(byte & 0x7F);
            if byte & 0x80 == 0 {
                return value;
            }
        }
    }
}

fn read_track(cursor: &mut Cursor) -> Vec<Item> {
    assert_eq!(cursor.take(4), b"MTrk");
    let size = cursor.u32() as usize;
    let end = cursor.at + size;
    let (mut items, mut tick, mut status) = (Vec::new(), 0u32, 0u8);
    while cursor.at < end {
        tick += cursor.variable();
        let first = cursor.byte();
        if first == 0xFF {
            let kind = cursor.byte();
            let length = cursor.variable() as usize;
            let data = cursor.take(length).to_vec();
            items.push(match (kind, data.as_slice()) {
                (0x03, name) => Item::Name(String::from_utf8_lossy(name).into_owned()),
                (0x51, [a, b, c]) => Item::Tempo(u32::from_be_bytes([0, *a, *b, *c])),
                (0x58, [numerator, power, _, _]) => Item::Signature {
                    numerator: *numerator,
                    power: *power,
                },
                (0x2F, []) => Item::End { tick },
                other => panic!("unexpected meta event {other:?}"),
            });
            continue;
        }
        let data1 = if first & 0x80 == 0 {
            first
        } else {
            status = first;
            cursor.byte()
        };
        let velocity = cursor.byte();
        let key = data1;
        match status & 0xF0 {
            0x90 if velocity > 0 => items.push(Item::NoteOn {
                tick,
                key,
                velocity,
            }),
            0x80 | 0x90 => items.push(Item::NoteOff { tick, key }),
            other => panic!("unexpected channel message {other:#x}"),
        }
    }
    assert_eq!(cursor.at, end, "track size matches its content");
    items
}

fn read_smf(bytes: &[u8]) -> Smf {
    let mut cursor = Cursor { bytes, at: 0 };
    assert_eq!(cursor.take(4), b"MThd");
    assert_eq!(cursor.u32(), 6);
    let (format, count, division) = (cursor.u16(), cursor.u16(), cursor.u16());
    let tracks = (0..count).map(|_| read_track(&mut cursor)).collect();
    assert_eq!(cursor.at, bytes.len(), "nothing after the last track");
    Smf {
        format,
        division,
        tracks,
    }
}

fn note_ons(track: &[Item]) -> Vec<(u32, u8, u8)> {
    track
        .iter()
        .filter_map(|item| match item {
            Item::NoteOn {
                tick,
                key,
                velocity,
            } => Some((*tick, *key, *velocity)),
            _ => None,
        })
        .collect()
}

fn note_offs(track: &[Item]) -> Vec<u32> {
    track
        .iter()
        .filter_map(|item| match item {
            Item::NoteOff { tick, .. } => Some(*tick),
            _ => None,
        })
        .collect()
}

#[test]
fn test_read_smf_when_synthetic_beat_with_swing_is_exported_returns_the_same_notes_at_the_machines_ticks(
) {
    let event = |pad, step, velocity, duration| BeatEvent {
        step,
        pad,
        velocity,
        duration,
        time_shift: 7,
        half_step: false,
    };
    let sequence = BeatSequence {
        name: "Swung".into(),
        tempo_tenths: 1205,
        swing: 6,
        beats_per_measure: 3,
        unit_index: 3,
        measures: 2,
        events: vec![
            event(15, 3, 99, 2),
            event(15, 0, 50, 1),
            event(31, 20, 127, 9),
        ],
        sound_names: std::array::from_fn(|_| None),
    };

    let result = export(&sequence, &PadNotes::default());
    let smf = read_smf(&result.bytes);

    assert_eq!((smf.format, smf.division, smf.tracks.len()), (1, PPQN, 3));
    assert_eq!(smf.tracks[0][1], Item::Tempo(497_925)); // 600e6 / 1205 = 497925.3
    assert_eq!(
        smf.tracks[0][2],
        Item::Signature {
            numerator: 3,
            power: 3
        }
    );
    // 3 x 6 steps x 2 measures x 8 ticks = 288: every track ends there.
    assert!(smf
        .tracks
        .iter()
        .all(|t| t.last() == Some(&Item::End { tick: 288 })));
    // Step 0 at tick 0; step 3 is the off 16th (tick 24), delayed by the swing of 6 ticks.
    // The pad is A16: note 61 in the default pad note table.
    assert_eq!(note_ons(&smf.tracks[1]), vec![(0, 61, 50), (30, 61, 99)]);
    assert_eq!(note_offs(&smf.tracks[1]), vec![4, 38]);
    // Step 20 = tick 160, on the first half of its 8th (160 mod 48 = 16): no swing.
    assert_eq!(note_ons(&smf.tracks[2]), vec![(160, 77, 127)]);
    assert_eq!(note_offs(&smf.tracks[2]), vec![196]);
}

fn capture() -> Vec<u8> {
    let path = std::env::var_os("TEMPESTER_PROJECT_CAPTURE")
        .expect("set TEMPESTER_PROJECT_CAPTURE to the captured project .syx file");
    fs::read(path).expect("capture readable")
}

fn split_messages(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut messages = Vec::new();
    let mut start = None;
    for (at, byte) in bytes.iter().enumerate() {
        match (*byte, start) {
            (0xF0, _) => start = Some(at),
            (0xF7, Some(from)) => {
                messages.push(bytes[from..=at].to_vec());
                start = None;
            }
            _ => {}
        }
    }
    messages
}

#[test]
#[ignore = "needs the captured project (TEMPESTER_PROJECT_CAPTURE)"]
fn test_export_when_real_capture_beats_are_exported_returns_16_files_whose_notes_match_the_events()
{
    let mut assembler = ProjectAssembler::new();
    for message in split_messages(&capture()) {
        assembler.accept(&message).expect("message accepted");
    }
    let project = assembler.finish().expect("project complete");
    assert_eq!(project.beats.len(), 16);

    let mut total_notes = 0;
    for beat in &project.beats {
        let sequence = read_sequence(beat.data()).expect("sequence reads");
        let map = PadNotes::default();
        let result = export(&sequence, &map);
        let smf = read_smf(&result.bytes);
        let events = beat.dump.summary.events;
        let label = format!("beat {} {:?}", beat.index + 1, sequence.name);

        assert_eq!(sequence.events.len(), events, "{label}");
        assert_eq!(
            result.notes + result.silent + result.beyond_end + result.overlapped,
            events,
            "{label}: every event is a note or counted"
        );
        let on_count: usize = smf.tracks.iter().map(|t| note_ons(t).len()).sum();
        let off_count: usize = smf.tracks.iter().map(|t| note_offs(t).len()).sum();
        assert_eq!(
            (on_count, off_count),
            (result.notes, result.notes),
            "{label}"
        );
        assert_eq!(smf.tracks.len(), result.tracks, "{label}");
        assert_eq!(smf.division, 96, "{label}");
        assert_eq!(
            smf.tracks[0][1],
            Item::Tempo((600_000_000 + sequence.tempo_tenths / 2) / sequence.tempo_tenths),
            "{label}"
        );
        let length = sequence.length_ticks();
        for track in &smf.tracks {
            assert_eq!(track.last(), Some(&Item::End { tick: length }), "{label}");
        }
        for track in &smf.tracks[1..] {
            for (tick, key, _) in note_ons(track) {
                assert!(tick < length && map.pad_of(key).is_some(), "{label}");
            }
        }
        total_notes += result.notes;
        println!(
            "{label}: {events} events -> {} notes on {} pad tracks, {:.1} BPM, swing {}, {} ticks, {} bytes",
            result.notes,
            result.tracks - 1,
            f64::from(sequence.tempo_tenths) / 10.0,
            sequence.swing,
            length,
            result.bytes.len()
        );
    }
    assert!(total_notes > 1000, "{total_notes}");
}
