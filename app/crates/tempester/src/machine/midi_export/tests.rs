//! Exporting a beat as a MIDI file through the worker: the fetched beat and a library beat, on the fake machine and a
//! disposable data folder.

use super::*;
use crate::command::Command;
use crate::json::Json;
use crate::library::command::LibCommand;
use crate::machine::testkit::TestRig;

fn export_files(rig: &TestRig) -> Vec<String> {
    let dir = rig.machine.storage.folder(Folder::Exports).expect("folder");
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("readable")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn lines_of(event: &crate::event::Event) -> String {
    match event.field("lines") {
        Some(Json::Arr(items)) => format!("{items:?}"),
        other => panic!("lines: {other:?}"),
    }
}

#[test]
fn test_export_midi_when_the_fetched_beat_is_exported_returns_a_mid_file_of_its_24_events() {
    let mut rig = TestRig::connected("midi-fetched");
    rig.machine.handle(Command::FetchBeat);
    rig.events();

    rig.machine
        .handle(Command::Lib(LibCommand::ExportMidi { id: String::new() }));

    assert_eq!(export_files(&rig), ["Fake beat 01.mid"]);
    let bytes = fs::read(
        rig.machine
            .storage
            .folder(Folder::Exports)
            .expect("folder")
            .join("Fake beat 01.mid"),
    )
    .expect("file");
    assert_eq!(&bytes[..4], b"MThd");
    assert_eq!(&bytes[12..14], &[0, 96], "96 PPQN");
    let said = lines_of(&rig.expect("lib_done"));
    assert!(said.contains("24 notes on 4 pad tracks"), "{said}");
    assert!(
        said.contains("120.0 BPM") && said.contains("4/4 for 4 measure(s)"),
        "{said}"
    );
}

#[test]
fn test_export_midi_when_a_library_beat_is_exported_twice_returns_two_files_and_none_overwritten() {
    let mut rig = TestRig::connected("midi-library");
    rig.machine.handle(Command::FetchBeat);
    rig.machine.handle(Command::SaveBeat("Groove".into()));
    rig.events();
    let (beats, _) = rig.machine.library().list(Kind::Beats).expect("lists");
    let id = beats[0].id.clone();

    for _ in 0..2 {
        rig.machine
            .handle(Command::Lib(LibCommand::ExportMidi { id: id.clone() }));
    }

    assert_eq!(
        export_files(&rig),
        ["Fake beat 01 (2).mid", "Fake beat 01.mid"]
    );
}

#[test]
fn test_export_midi_when_nothing_is_fetched_returns_precondition_error_and_writes_nothing() {
    let mut rig = TestRig::connected("midi-none");

    rig.machine
        .handle(Command::Lib(LibCommand::ExportMidi { id: String::new() }));

    assert_eq!(
        rig.expect("error").field("code"),
        Some(&"precondition".into())
    );
    assert!(export_files(&rig).is_empty());
}

#[test]
fn test_export_midi_when_the_item_is_a_sound_returns_invalid_error_and_writes_nothing() {
    let mut rig = TestRig::connected("midi-sound");
    rig.machine.handle(Command::FetchSound);
    rig.machine.handle(Command::SaveSound("Kick".into()));
    rig.events();
    let (sounds, _) = rig.machine.library().list(Kind::Sounds).expect("lists");

    rig.machine.handle(Command::Lib(LibCommand::ExportMidi {
        id: sounds[0].id.clone(),
    }));

    assert_eq!(rig.expect("error").field("code"), Some(&"invalid".into()));
    assert!(export_files(&rig).is_empty());
}
