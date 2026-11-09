//! Reads what the firmware's own senders write, simulated instruction by instruction on two images with the same
//! project (`tools/sim_pack7_fix.py --dump`, a firmware simulation, not in the public tree): Tempester 1.6.1.0
//! (factory flush, last partial group with shifted head bits and stale padding) and 1.6.1.2 (n°27: standard last
//! group). Each file holds
//! 6 sounds `60` (last group k = 2 bytes), 7 beats `5F` and a project `5E` + 7 x `5C n` (k = 0..6 for the beats),
//! then 4 projects `5E` alone with 1 to 4 Play Lists (k = 3, 6, 2, 5).
//! Both exports must read to the same values, and the 1.6.1.2 one must already be what the app writes back (the
//! standard layout): its messages equal the app's re-encoding byte for byte.
//! The exports are built from the author's DSI image and stay out of the repository: `tools/verify.sh --long` makes
//! them and runs `TEMPESTER_FIXED_EXPORT=<1.6.1.2 file> TEMPESTER_STOCK_EXPORT=<1.6.1.0 file>
//! cargo test --test fixed_export -- --ignored` through `tools/test_app.sh`.

#![allow(clippy::expect_used)] // integration tests: a panic with a message is the failure report

use std::fs;

use tempest_proto::pack7::unpack_flushed_with;
use tempest_proto::project::{Accepted, Project, ProjectAssembler};
use tempest_proto::{beat, sound_message};

const SOUNDS: usize = 6;
const BEATS: usize = 7;
const PROJECTS: usize = 5;

fn export(variable: &str) -> Vec<Vec<u8>> {
    let path = std::env::var_os(variable).unwrap_or_else(|| panic!("set {variable}"));
    let bytes = fs::read(path).expect("export readable");
    let mut messages = Vec::new();
    let mut start = 0;
    for (at, byte) in bytes.iter().enumerate() {
        if *byte == 0xF7 {
            messages.push(bytes[start..=at].to_vec());
            start = at + 1;
        }
    }
    assert_eq!(
        start,
        bytes.len(),
        "{variable}: the export ends inside a message"
    );
    messages
}

/// What the app reads out of one export: the sounds, the beats as the app saves them (standard `5F`), the projects.
struct Read {
    sounds: Vec<sound_message::SoundMessage>,
    beats: Vec<Vec<u8>>,
    projects: Vec<Project>,
}

fn read_all(label: &str, messages: &[Vec<u8>]) -> Read {
    let mut read = Read {
        sounds: Vec::new(),
        beats: Vec::new(),
        projects: Vec::new(),
    };
    let mut assembler: Option<ProjectAssembler> = None;
    for (n, message) in messages.iter().enumerate() {
        let at = format!("{label}, message {n} ({:02X})", message[3]);
        match message[3] {
            0x60 => read
                .sounds
                .push(sound_message::decode(message).unwrap_or_else(|e| panic!("{at}: {e}"))),
            0x5F => read
                .beats
                .push(beat::standard_message(message).unwrap_or_else(|e| panic!("{at}: {e}"))),
            command => {
                if command == 0x5E {
                    if let Some(done) = assembler.take() {
                        read.projects.push(done.finish().expect("header came"));
                    }
                    assembler = Some(ProjectAssembler::new());
                }
                let accepted = assembler
                    .as_mut()
                    .expect("5E first")
                    .accept(message)
                    .unwrap_or_else(|e| panic!("{at}: {e}"));
                assert_ne!(accepted, Accepted::Ignored, "{label}, message {n}");
            }
        }
    }
    read.projects
        .push(assembler.expect("a project").finish().expect("header came"));
    read
}

#[test]
#[ignore = "needs the simulated exports (TEMPESTER_FIXED_EXPORT, TEMPESTER_STOCK_EXPORT)"]
fn test_read_when_simulated_exports_of_both_versions_returns_the_same_values() {
    let stock_messages = export("TEMPESTER_STOCK_EXPORT");
    let fixed_messages = export("TEMPESTER_FIXED_EXPORT");

    let stock = read_all("1.6.1.0", &stock_messages);
    let fixed = read_all("1.6.1.2", &fixed_messages);

    assert_eq!(fixed_messages.len(), stock_messages.len());
    assert_eq!(
        (fixed.sounds.len(), fixed.beats.len(), fixed.projects.len()),
        (SOUNDS, BEATS, PROJECTS)
    );
    assert_eq!(fixed.sounds, stock.sounds);
    assert_eq!(fixed.beats, stock.beats);
    assert_eq!(fixed.projects[0].beats.len(), BEATS);
    for (f, s) in fixed.projects.iter().zip(&stock.projects) {
        assert_eq!(f.header, s.header);
        assert_eq!(f.beats.len(), s.beats.len());
        for (fb, sb) in f.beats.iter().zip(&s.beats) {
            assert_eq!((fb.index, &fb.dump), (sb.index, &sb.dump));
        }
        assert_eq!(f.standard(), s.standard());
    }
}

#[test]
#[ignore = "needs the simulated exports (TEMPESTER_FIXED_EXPORT, TEMPESTER_STOCK_EXPORT)"]
fn test_standard_when_simulated_export_of_1_6_1_2_returns_the_messages_as_sent() {
    let messages = export("TEMPESTER_FIXED_EXPORT");

    let fixed = read_all("1.6.1.2", &messages);

    for (n, sound) in fixed.sounds.iter().enumerate() {
        let sound_message::SoundMessage::Current(sound) = sound else {
            panic!("sound {n} is not a 60")
        };
        assert_eq!(
            sound_message::encode_current(sound),
            messages[n],
            "sound {n}"
        );
    }
    for (n, beat) in fixed.beats.iter().enumerate() {
        assert_eq!(*beat, messages[SOUNDS + n], "beat {n}");
    }
    for (n, project) in fixed.projects.iter().enumerate() {
        assert_eq!(project.standard(), project.raw(), "project {n}");
    }
}

#[test]
#[ignore = "needs the simulated exports (TEMPESTER_FIXED_EXPORT, TEMPESTER_STOCK_EXPORT)"]
fn test_standard_when_simulated_export_of_1_6_1_0_returns_other_bytes_than_sent() {
    // Counter-proof of the test above: the factory flush is not the standard layout, so the app's re-encoding of the
    // 1.6.1.0 export differs from what that machine sent (beats and projects whose data has an MSB in the last group).
    let messages = export("TEMPESTER_STOCK_EXPORT");

    let stock = read_all("1.6.1.0", &messages);

    let beats_differ = (0..BEATS)
        .filter(|n| stock.beats[*n] != messages[SOUNDS + n])
        .count();
    let projects_differ = stock
        .projects
        .iter()
        .filter(|p| p.standard() != p.raw())
        .count();
    assert!(
        beats_differ > 0 && projects_differ > 0,
        "{beats_differ} {projects_differ}"
    );
}

#[test]
#[ignore = "needs the simulated exports (TEMPESTER_FIXED_EXPORT, TEMPESTER_STOCK_EXPORT)"]
fn test_flushed_reader_when_simulated_export_of_1_6_1_2_refuses_or_misreads_beats() {
    // Counter-proof of the switch to `unpack_tolerant_with`: the reader the project assembler used before n°27
    // (`unpack_flushed_with`, factory layout only) refuses or misreads some of the fixed machine's 5C.
    let messages = export("TEMPESTER_FIXED_EXPORT");
    let fixed = read_all("1.6.1.2", &messages);

    let wrong = messages
        .iter()
        .filter(|m| m[3] == 0x5C)
        .zip(&fixed.projects[0].beats)
        .filter(|(m, beat)| {
            let old = unpack_flushed_with(&m[5..m.len() - 1], beat::read_unpacked);
            old.as_ref() != Ok(&beat.dump)
        })
        .count();

    assert!(wrong > 0, "the flushed-only reader read every fixed 5C");
}
