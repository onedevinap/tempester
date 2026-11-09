//! Reads a project as the real machine sent it (Tempester 1.6.1.0 answering `F0 01 28 42 F7` over USB, 2026-11-08):
//! one `5E` and 16 `5C n`, with the partial last groups of the firmware's packer (`refs/firmware-main.md` 9.1).
//! The capture is the author's own data and stays out of the repository: set `TEMPESTER_PROJECT_CAPTURE` to its path
//! (`tools/test_app.sh` does when `work/captures/projet_machine_2026-11-08.syx` exists, and says so when it does not).
//! Run with `TEMPESTER_PROJECT_CAPTURE=<file> cargo test --test real_capture -- --ignored`.

#![allow(clippy::expect_used)] // integration tests: a panic with a message is the failure report

use std::fs;

use tempest_proto::beat;
use tempest_proto::pack7::{unpack, unpack_flushed_with};
use tempest_proto::project::{read_header, Accepted, ProjectAssembler, BEATS};

const HEADER_BODY_FROM: usize = 4;

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
    assert!(start.is_none(), "the capture ends inside a message");
    messages
}

#[test]
#[ignore = "needs the captured project (TEMPESTER_PROJECT_CAPTURE)"]
fn test_assembler_when_real_project_capture_returns_header_and_16_beats_with_nothing_left() {
    let bytes = capture();
    let messages = split_messages(&bytes);
    assert_eq!(messages.len(), 1 + BEATS);

    let mut assembler = ProjectAssembler::new();
    for (n, message) in messages.iter().enumerate() {
        let accepted = assembler
            .accept(message)
            .unwrap_or_else(|e| panic!("message {n}: {e}"));
        let expected = if n == 0 {
            Accepted::Header
        } else {
            Accepted::Beat(n as u8 - 1)
        };
        assert_eq!(accepted, expected, "message {n}");
    }
    let project = assembler.finish().expect("header came");

    assert_eq!(project.header.params_version, 5);
    assert_eq!(project.header.play_list_version, Some(2));
    assert_eq!(
        project.header.play_list_names,
        [Some("Play List 1".to_string()), None, None, None]
    );
    assert!(project.is_complete());
    for beat in &project.beats {
        let name = &beat.dump.summary.name;
        assert!(
            !name.is_empty() && !name.contains('?'),
            "beat {} name {name:?}",
            beat.index + 1
        );
        assert_eq!(
            beat.dump.sounds.iter().flatten().count(),
            beat::PADS,
            "beat {}",
            beat.index + 1
        );
    }
    // Every byte of the capture is kept, so the project can be saved and sent back as it came.
    assert_eq!(project.raw(), bytes);
}

/// The packed body of the capture's `5E`.
fn header_body(messages: &[Vec<u8>]) -> &[u8] {
    let message = &messages[0];
    assert_eq!(message[3], 0x5E);
    &message[HEADER_BODY_FROM..message.len() - 1]
}

#[test]
#[ignore = "needs the captured project (TEMPESTER_PROJECT_CAPTURE)"]
fn test_read_header_when_real_capture_is_read_with_the_old_continuous_rule_returns_wrong_header() {
    let messages = split_messages(&capture());
    let continuous = unpack(header_body(&messages));

    let outcome = read_header(&continuous);

    // 3 empty lists end the real data at 318 bytes: the continuous rule reads 322 bytes, the last ones with the
    // MSBs of the flush left out, so the empty lists come out as 0x7E7E words.
    assert!(
        !matches!(&outcome, Ok((header, used))
            if *used == 318 && header.play_list_names[1..].iter().all(Option::is_none)),
        "{outcome:?}"
    );
}

/// The last group of the 5E body read with a variant of the rule, for every length the last group can hold: true when
/// some length makes `read_header` consume exactly the data.
fn variant_reads_header(body: &[u8], reversed_bits: bool, shift_error: isize) -> bool {
    let groups = body.len() / 8;
    (1..=7usize).any(|partial| {
        let flush_steps = (1..=3usize)
            .find(|steps| partial + 2 * steps > 6)
            .expect("steps");
        let shift = 7 - partial as isize - flush_steps as isize + shift_error;
        let mut data = Vec::new();
        for (g, group) in body.chunks(8).enumerate() {
            let last = g + 1 == groups;
            let take = if last { partial } else { 7 };
            for j in 0..take {
                let bit = if last && partial < 7 {
                    shift + j as isize
                } else {
                    j as isize
                };
                let bit = if reversed_bits { 6 - bit } else { bit };
                let msb = u8::from((0..8).contains(&bit) && (group[0] >> bit) & 1 == 1);
                data.push(group[1 + j] | (msb << 7));
            }
        }
        matches!(read_header(&data), Ok((_, used)) if used == data.len())
    })
}

#[test]
#[ignore = "needs the captured project (TEMPESTER_PROJECT_CAPTURE)"]
fn test_read_header_when_real_capture_is_read_with_the_right_rule_returns_header_and_wrong_variants_fail(
) {
    let messages = split_messages(&capture());
    let body = header_body(&messages);

    assert!(variant_reads_header(body, false, 0), "the rule itself");
    assert!(!variant_reads_header(body, true, 0), "reversed bit order");
    assert!(!variant_reads_header(body, false, 1), "mask shifted up");
    assert!(!variant_reads_header(body, false, -1), "mask shifted down");
    // The same decoder, but unpack_flushed_with gives the exact answer.
    let header = unpack_flushed_with(body, |data| {
        read_header(data).and_then(|(header, used)| {
            if used == data.len() {
                Ok(header)
            } else {
                Err(tempest_proto::project::ProjectError::HeaderTrailing(
                    data.len() - used,
                ))
            }
        })
    })
    .expect("header reads");
    assert_eq!(header.params_version, 5);
}
