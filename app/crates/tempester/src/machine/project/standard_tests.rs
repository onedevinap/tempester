//! What a saved project writes, checked against the machine's receiver rule: every file meant to be sent back is in the
//! standard 7-bit layout and decodes to exactly the data parsed from the messages as received; the one file kept as
//! evidence equals the capture byte for byte. Includes the real capture of the author's machine
//! (`TEMPESTER_PROJECT_CAPTURE`, never in the repository).

use super::*;
use crate::machine::testkit::TestRig;
use tempest_proto::beat::{self, synthetic_tail_beat};
use tempest_proto::pack7::{is_standard_for, unpack};
use tempest_proto::project::{beat_message, header_message};
use tempest_proto::sound_message::SoundMessage;

/// Splits concatenated SysEx messages.
fn split(bytes: &[u8]) -> Vec<&[u8]> {
    bytes.split_inclusive(|byte| *byte == 0xF7).collect()
}

fn names(rig: &TestRig, folder: Folder) -> Vec<String> {
    rig.machine
        .storage
        .list(folder)
        .unwrap()
        .into_iter()
        .map(|entry| entry.name)
        .collect()
}

fn project_of(messages: &[Vec<u8>]) -> Project {
    let mut assembler = ProjectAssembler::new();
    for message in messages {
        assembler.accept(message).unwrap();
    }
    assembler.finish().unwrap()
}

/// `save_project` into a disposable folder, then every written file checked against `project` and the `capture` bytes.
fn save_and_check(label: &str, project: &Project, capture: &[u8]) -> TestRig {
    let mut rig = TestRig::connected(label);
    let stamp = "2026-11-08_100000Z";
    support::save_project_now(&mut rig, project, stamp);
    let saved = rig.expect("project_saved");
    let text = |key: &str| match saved.field(key) {
        Some(crate::json::Json::Str(text)) => text.clone(),
        other => panic!("{key}: {other:?}"),
    };
    let (received_file, project_file) = (text("received_file"), text("project_file"));
    let stem = project_label(stamp, project);
    let read = |folder: Folder, name: &str| rig.machine.storage.read(folder, name).unwrap();

    // The evidence is the capture itself; the file to send back is the standard re-encoding of the same messages.
    assert_eq!(received_file, format!("{stem}{AS_RECEIVED}.syx"));
    assert_eq!(read(Folder::Projects, &received_file), capture);
    let standard = read(Folder::Projects, &project_file);
    assert_eq!(standard, project.standard());
    check_project_messages(&standard, project);

    for beat in &project.beats {
        let name = format!(
            "{stem}/{:02} {}.syx",
            beat.index + 1,
            beat_name(beat.index, &beat.dump.summary.name)
        );
        let file = read(Folder::Beats, &name);
        let body = &file[4..file.len() - 1];
        let data = unpack(body);
        let (dump, used) = beat::read_unpacked_prefix(&data).unwrap();
        assert_eq!(dump, beat.dump, "{name}");
        assert!(is_standard_for(body, &data[..used]), "{name}");
        assert_eq!(file, beat.beat_file(), "{name}");
    }

    let sounds = names(&rig, Folder::Sounds);
    assert_eq!(
        saved.field("sounds"),
        Some(&sounds.len().into()),
        "the event counts the sound files written"
    );
    let known: Vec<[u8; 128]> = project
        .beats
        .iter()
        .flat_map(|b| b.dump.sounds.iter().flatten().map(|s| *s.bytes()))
        .collect();
    for name in &sounds {
        let file = read(Folder::Sounds, name);
        let Ok(SoundMessage::File { sound, .. }) = sound_message::decode(&file) else {
            panic!("{name}: not a 63 file");
        };
        let body = &file[5..file.len() - 1];
        let payload = &unpack(body)[..usize::from(file[4]) + 128];
        assert!(is_standard_for(body, payload), "{name}");
        assert!(known.contains(sound.bytes()), "{name}");
    }
    rig
}

/// Each message of the standard project file: the receiver rule reads the data of the parsed beat, MSBs included.
fn check_project_messages(standard: &[u8], project: &Project) {
    let messages = split(standard);
    assert_eq!(messages.len(), 1 + project.beats.len());
    let header = messages[0];
    assert!(is_standard_for(
        &header[4..header.len() - 1],
        &unpack(&header[4..header.len() - 1])[..header_len(header)]
    ));
    for (message, beat) in messages[1..].iter().zip(&project.beats) {
        let body = &message[5..message.len() - 1];
        let data = beat::read_body(body).unwrap();
        assert_eq!(message[4], beat.index);
        assert_eq!(beat::read_unpacked(&data).unwrap(), beat.dump);
        assert!(is_standard_for(body, &data));
        assert_eq!(unpack(body)[..data.len()], data[..]);
    }
}

/// Data length of a standard `5E` message (what the header parser consumes).
fn header_len(message: &[u8]) -> usize {
    let data = unpack(&message[4..message.len() - 1]);
    tempest_proto::project::read_header(&data).unwrap().1
}

#[test]
fn test_save_project_when_beats_end_in_every_partial_group_returns_standard_files_and_the_exact_capture(
) {
    let mut messages = vec![header_message(1)];
    messages.extend((0..7).map(|n| beat_message(n as u8, &synthetic_tail_beat(n))));
    let project = project_of(&messages);

    save_and_check("project-standard-tails", &project, &messages.concat());
}

#[test]
fn test_save_project_when_the_fake_machine_sends_a_project_returns_standard_files() {
    let messages = fake_machine::project_messages();
    let project = project_of(&messages);

    save_and_check("project-standard-fake", &project, &messages.concat());
}

#[test]
fn test_save_project_when_the_as_received_file_is_checked_returns_a_failure_for_the_machines_tails()
{
    // Counter-proof: sending back the capture (the flushed tails) would not pass the check the standard file passes.
    let mut messages = vec![header_message(1)];
    messages.extend((1..7).map(|n| beat_message(n as u8, &synthetic_tail_beat(n))));
    let project = project_of(&messages);

    let capture = project.raw();

    assert_ne!(capture, project.standard());
    for (message, beat) in split(&capture)[1..].iter().zip(&project.beats) {
        let body = &message[5..message.len() - 1];
        let data = beat::read_body(body).unwrap();
        assert_eq!(beat::read_unpacked(&data).unwrap(), beat.dump);
        assert!(!is_standard_for(body, &data));
    }
}

#[test]
#[ignore = "needs the captured project (TEMPESTER_PROJECT_CAPTURE)"]
#[allow(clippy::expect_used)] // a test on the author's data: a panic with a message is the failure report
fn test_save_project_when_real_capture_returns_standard_files_and_the_capture_byte_for_byte() {
    let path = std::env::var_os("TEMPESTER_PROJECT_CAPTURE")
        .expect("set TEMPESTER_PROJECT_CAPTURE to the captured project .syx file");
    let capture = std::fs::read(path).expect("capture readable");
    let messages: Vec<Vec<u8>> = split(&capture).iter().map(|m| m.to_vec()).collect();
    let project = project_of(&messages);
    assert!(project.is_complete());

    save_and_check("project-real-capture", &project, &capture);
}
