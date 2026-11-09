//! Checks on the DSI factory project files (`61` messages), which are proprietary and not in the repository.
//! Run with `TEMPESTER_FACTORY_PROJECTS=<folder of the .syx project files> cargo test -- --ignored`.
//!
//! A project file holds what the menu's Send Project sends (same serializers, `refs/firmware-main.md` 9.6): the
//! parameters and Play Lists, then 16 beats. The test cuts a file into a `5E` and 16 `5C n` messages the way the
//! machine would send them and checks that the assembler takes them, with every sound readable.

#![allow(clippy::expect_used)] // integration tests: a panic with a message is the failure report

use std::fs;
use std::path::PathBuf;

use tempest_proto::beat::{self, PADS};
use tempest_proto::pack7::{is_standard_for, unpack};
use tempest_proto::project::{self, ProjectAssembler, BEATS};
use tempest_proto::sound_message;

fn project_files() -> Vec<PathBuf> {
    let folder = std::env::var_os("TEMPESTER_FACTORY_PROJECTS")
        .expect("set TEMPESTER_FACTORY_PROJECTS to the folder of the factory project .syx files");
    let mut files: Vec<PathBuf> = fs::read_dir(folder)
        .expect("folder readable")
        .map(|entry| entry.expect("entry readable").path())
        .filter(|path| path.extension().is_some_and(|e| e == "syx"))
        .collect();
    files.sort();
    files
}

/// The file content of a `F0 01 28 61 <len> <pack(name, 3-byte word count, file)> F7` message.
fn project_file_bytes(message: &[u8]) -> Vec<u8> {
    assert_eq!(&message[..4], &[0xF0, 0x01, 0x28, 0x61]);
    let name_len = usize::from(message[4]);
    let body = unpack(&message[5..message.len() - 1]);
    let words = body[name_len..name_len + 3]
        .iter()
        .fold(0usize, |acc, byte| acc << 8 | usize::from(*byte));
    body[name_len + 3..name_len + 3 + 2 * words].to_vec()
}

#[test]
#[ignore = "needs the factory project files (TEMPESTER_FACTORY_PROJECTS)"]
fn test_assembler_when_factory_projects_are_cut_like_a_send_project_returns_16_readable_beats() {
    let files = project_files();
    assert!(!files.is_empty(), "no .syx file in the folder");
    for path in files {
        let label = path.display().to_string();
        let data = project_file_bytes(&fs::read(&path).expect("file readable"));
        let (header, header_len) = project::read_header(&data).expect("header reads");
        let mut head = vec![0xF0, 0x01, 0x28, project::CMD_PROJECT_HEADER];
        head.extend(tempest_proto::pack7::pack_flushed(&data[..header_len]));
        head.push(0xF7);
        let mut messages = vec![head];
        let mut at = header_len;
        for n in 0..BEATS {
            let (_, used) = beat::read_unpacked_prefix(&data[at..])
                .unwrap_or_else(|e| panic!("{label}: beat {n}: {e}"));
            messages.push(project::beat_message(n as u8, &data[at..at + used]));
            at += used;
        }
        assert_eq!(at, data.len(), "{label}: bytes left after beat 16");

        let mut assembler = ProjectAssembler::new();
        for message in &messages {
            assembler
                .accept(message)
                .unwrap_or_else(|e| panic!("{label}: {e}"));
        }
        let project = assembler.finish().expect("header came");
        assert!(project.is_complete(), "{label}");

        assert_eq!(project.header, header, "{label}");
        for beat in &project.beats {
            assert_eq!(beat.dump.sounds.iter().flatten().count(), PADS, "{label}");
            let file = beat.beat_file();
            let body = &file[4..file.len() - 1];
            let data = beat::read_body(body).expect("5F reads");
            assert!(is_standard_for(body, &data), "{label}: standard layout");
            assert_eq!(beat::read_unpacked(&data).expect("5F parses"), beat.dump);
            for sound in beat.dump.sounds.iter().flatten() {
                let message = sound_message::encode_file("x", sound).expect("63 encodes");
                assert!(sound_message::decode(&message).is_ok());
            }
        }
    }
}
