//! The manager through the worker's commands, on the fake machine and a disposable data folder: saving through the
//! duplicate check, the review, and library -> machine (identical, known, unknown, no answer, changed meanwhile).

use super::*;
use crate::library::command::LibCommand;
use crate::library::plan::{OnIdentical, OnModified};
use crate::machine::testkit::TestRig;
use crate::storage::Folder;
use tempest_proto::sound::Sound;
use tempest_proto::sound_message::{encode_current, encode_file, REQUEST_BEAT, REQUEST_SOUND};

fn lib(rig: &mut TestRig, command: LibCommand) {
    rig.machine.handle(Command::Lib(command));
}

fn text(event: &Event, key: &str) -> String {
    match event.field(key) {
        Some(Json::Str(text)) => text.clone(),
        other => panic!("{key}: {other:?}"),
    }
}

fn lines(event: &Event) -> String {
    match event.field("lines") {
        Some(Json::Arr(items)) => items
            .iter()
            .map(|i| match i {
                Json::Str(s) => s.clone(),
                other => panic!("{other:?}"),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        other => panic!("lines: {other:?}"),
    }
}

fn sound(name: &str, freq: u16) -> Sound {
    let mut sound = Sound::with_defaults();
    sound.set(0, freq).unwrap();
    sound.set_name(name).unwrap();
    sound
}

/// Puts a sound in the library through the plain import command and returns its id.
fn import_sound(rig: &mut TestRig, name: &str, freq: u16) -> String {
    let file = encode_file(name, &sound(name, freq)).unwrap();
    lib(
        rig,
        LibCommand::Import {
            name: format!("{name}.syx"),
            data: file,
        },
    );
    rig.events();
    let (items, _) = rig.machine.library().list(Kind::Sounds).unwrap();
    items
        .into_iter()
        .find(|i| i.name == name)
        .expect("imported")
        .id
}

/// Saves the machine's own current sound (the fake's "Fake Kick") as `Kick`.
fn save_machine_sound(rig: &mut TestRig) -> String {
    rig.machine.handle(Command::FetchSound);
    rig.machine.handle(Command::SaveSound("Kick".into()));
    rig.events();
    let (items, _) = rig.machine.library().list(Kind::Sounds).unwrap();
    items
        .into_iter()
        .find(|i| i.path == "Kick.syx")
        .expect("saved")
        .id
}

fn check(rig: &mut TestRig, kind: Kind, id: &str) -> Event {
    lib(
        rig,
        LibCommand::SendCheck {
            kind,
            id: id.into(),
        },
    );
    rig.expect("lib_send_plan")
}

fn apply_send(rig: &mut TestRig, kind: Kind, id: &str, token: &str) {
    lib(
        rig,
        LibCommand::SendApply {
            kind,
            id: id.into(),
            machine: token.into(),
        },
    );
}

#[test]
fn test_send_check_when_the_machine_holds_the_same_sound_returns_identical_and_sends_nothing() {
    let mut rig = TestRig::connected("send-identical");
    let id = save_machine_sound(&mut rig);

    let plan = check(&mut rig, Kind::Sounds, &id);

    assert_eq!(text(&plan, "status"), "identical");
    assert!(lines(&plan).contains("nothing to send"));
    assert_eq!(
        rig.transport().sent_bytes(),
        [REQUEST_SOUND, REQUEST_SOUND].concat(),
        "only the two requests (save, check) went out"
    );
}

#[test]
fn test_send_apply_when_the_machines_sound_is_in_the_library_returns_it_sent_without_a_backup() {
    let mut rig = TestRig::connected("send-known");
    save_machine_sound(&mut rig);
    let other = import_sound(&mut rig, "Other", 77);
    let plan = check(&mut rig, Kind::Sounds, &other);
    assert_eq!(text(&plan, "status"), "known");
    assert!(lines(&plan).contains("nothing is lost"), "{}", lines(&plan));

    apply_send(&mut rig, Kind::Sounds, &other, &text(&plan, "token"));

    let sent = rig.transport().sent_bytes();
    assert!(sent.ends_with(&encode_current(&sound("Other", 77))));
    let index = rig.machine.library().snapshot().unwrap();
    assert!(
        index.item("machine-sounds").is_none(),
        "known: no backup was needed"
    );
    assert!(rig.events().iter().any(|e| e.name == "sent"));
}

#[test]
fn test_send_apply_when_the_machines_sound_is_unknown_returns_an_auto_backup_then_the_send() {
    let mut rig = TestRig::connected("send-unknown");
    let other = import_sound(&mut rig, "Other", 77);
    let plan = check(&mut rig, Kind::Sounds, &other);
    assert_eq!(text(&plan, "status"), "unknown");
    let said = lines(&plan);
    assert!(
        said.contains("NOT in your library") && said.contains("auto backup"),
        "{said}"
    );
    assert!(
        said.contains("\"Fake Kick\""),
        "it names what is replaced: {said}"
    );
    rig.events();

    apply_send(&mut rig, Kind::Sounds, &other, &text(&plan, "token"));

    let index = rig.machine.library().snapshot().unwrap();
    let slot = index
        .item("machine-sounds")
        .expect("the slot of auto backups");
    assert_eq!(slot.history.len(), 1);
    assert!(slot.history[0].label.starts_with("auto backup "));
    let kept = std::fs::read(rig.machine.storage.root().join(&slot.history[0].file)).unwrap();
    let Ok(SoundMessage::File {
        sound: backed_up, ..
    }) = sound_message::decode(&kept)
    else {
        panic!("the backup is a 63 file");
    };
    assert_eq!(backed_up.name(), "Fake Kick");
    assert!(rig
        .transport()
        .sent_bytes()
        .ends_with(&encode_current(&sound("Other", 77))));
    let done = rig
        .events()
        .into_iter()
        .find(|e| e.name == "lib_done")
        .expect("a message");
    assert!(
        lines(&done).contains("saved as an auto backup first"),
        "{}",
        lines(&done)
    );
    // The machine now holds Other, which is in the library: sending another item needs no backup.
    let third = import_sound(&mut rig, "Third", 5);
    let again = check(&mut rig, Kind::Sounds, &third);
    assert_eq!(text(&again, "status"), "known");
}

#[test]
fn test_send_apply_when_the_machine_changed_since_the_check_returns_a_new_plan_and_sends_nothing() {
    let mut rig = TestRig::connected("send-changed");
    let other = import_sound(&mut rig, "Other", 77);
    let plan = check(&mut rig, Kind::Sounds, &other);
    // Someone changes the sound on the machine between the check and the click.
    let hash_before = text(&plan, "token");
    rig.machine.handle(Command::OpenSound {
        name: "x.syx".into(),
        data: encode_current(&sound("Moved", 3)),
    });
    rig.machine.handle(Command::SendSound);
    rig.events();
    let before = rig.transport().sent_bytes().len();

    apply_send(&mut rig, Kind::Sounds, &other, &hash_before);

    let replan = rig.expect("lib_send_plan");
    assert_eq!(replan.field("changed"), Some(&Json::Bool(true)));
    let sent = rig.transport().sent_bytes();
    assert!(
        !sent[before..].ends_with(&encode_current(&sound("Other", 77))),
        "nothing was sent after the change"
    );
    assert!(rig
        .machine
        .library()
        .snapshot()
        .unwrap()
        .item("machine-sounds")
        .is_none());
}

#[test]
fn test_send_check_when_the_machine_does_not_answer_returns_no_answer_and_apply_sends_only_on_that_token(
) {
    let mut rig = TestRig::connected("send-silent");
    let other = import_sound(&mut rig, "Other", 77);
    rig.transport().set_responder(Box::new(|_| vec![]));

    let plan = check(&mut rig, Kind::Sounds, &other);

    assert_eq!(text(&plan, "status"), "no_answer");
    assert_eq!(text(&plan, "token"), "none");
    assert!(
        lines(&plan).contains("cannot be backed up"),
        "{}",
        lines(&plan)
    );
    assert!(!rig
        .transport()
        .sent_bytes()
        .ends_with(&encode_current(&sound("Other", 77))));
    // Counter-proof: a hash token is refused when the machine stays silent (no confirmation of « no backup »).
    apply_send(&mut rig, Kind::Sounds, &other, &"a".repeat(64));
    assert_eq!(rig.expect("error").field("code"), Some(&"invalid".into()));
    assert!(!rig
        .transport()
        .sent_bytes()
        .ends_with(&encode_current(&sound("Other", 77))));
    rig.events();

    apply_send(&mut rig, Kind::Sounds, &other, "none");

    assert!(rig
        .transport()
        .sent_bytes()
        .ends_with(&encode_current(&sound("Other", 77))));
    assert!(rig
        .machine
        .library()
        .snapshot()
        .unwrap()
        .item("machine-sounds")
        .is_none());
}

#[test]
fn test_send_check_when_not_connected_returns_not_connected_error_and_nothing_to_open_returns_a_precondition_error(
) {
    let mut rig = TestRig::new("send-off");
    let id = import_sound(&mut rig, "Other", 77);

    lib(
        &mut rig,
        LibCommand::SendCheck {
            kind: Kind::Sounds,
            id,
        },
    );
    assert_eq!(
        rig.expect("error").field("code"),
        Some(&"not_connected".into())
    );
    lib(
        &mut rig,
        LibCommand::SendCheck {
            kind: Kind::Sounds,
            id: String::new(),
        },
    );
    assert_eq!(
        rig.expect("error").field("code"),
        Some(&"precondition".into())
    );
}

#[test]
fn test_send_apply_when_a_beat_is_unknown_returns_the_machines_beat_backed_up_then_the_beat_sent() {
    let mut rig = TestRig::connected("send-beat");
    rig.machine.handle(Command::FetchBeat);
    rig.machine.handle(Command::SaveBeat("Mine".into()));
    // Replace the machine's beat by another one, then look for what would replace *that*.
    let data = tempest_proto::beat::synthetic_tail_beat(2);
    rig.machine.handle(Command::OpenBeat {
        name: "x.syx".into(),
        data: tempest_proto::beat::standard_dump(&data),
    });
    rig.machine.handle(Command::SendBeat);
    rig.events();
    let (items, _) = rig.machine.library().list(Kind::Beats).unwrap();
    let mine = items
        .iter()
        .find(|i| i.path == "Mine.syx")
        .expect("saved beat")
        .id
        .clone();

    let plan = check(&mut rig, Kind::Beats, &mine);

    assert_eq!(text(&plan, "status"), "unknown");
    apply_send(&mut rig, Kind::Beats, &mine, &text(&plan, "token"));
    let index = rig.machine.library().snapshot().unwrap();
    assert_eq!(
        index.item("machine-beats").map(|s| s.history.len()),
        Some(1)
    );
    assert!(rig.transport().sent_bytes().len() > REQUEST_BEAT.len());
}

// ---- saving through the duplicate check ----------------------------------------------------------------------------

#[test]
fn test_save_sound_when_the_same_sound_is_saved_twice_returns_a_review_and_no_second_file() {
    let mut rig = TestRig::connected("save-twice");
    save_machine_sound(&mut rig);

    rig.machine.handle(Command::SaveSound("Another".into()));

    let plan = rig.expect("lib_plan");
    assert_eq!(plan.field("identical"), Some(&1usize.into()));
    assert_eq!(rig.machine.storage.list(Folder::Sounds).unwrap().len(), 1);
    lib(&mut rig, LibCommand::Apply(Policy::default()));
    assert_eq!(
        rig.machine.storage.list(Folder::Sounds).unwrap().len(),
        1,
        "skipped by default"
    );
    let done = rig
        .events()
        .into_iter()
        .find(|e| e.name == "lib_done")
        .expect("message");
    assert!(
        lines(&done).contains("already in the library as \"Fake Kick\""),
        "{}",
        lines(&done)
    );
}

#[test]
fn test_save_sound_when_the_name_exists_with_other_content_returns_a_review_with_both_dates_and_updates_it(
) {
    let mut rig = TestRig::connected("save-modified");
    save_machine_sound(&mut rig);
    rig.machine.handle(Command::OpenSound {
        name: "x.syx".into(),
        data: encode_current(&sound("Fake Kick", 99)),
    });
    rig.machine.handle(Command::SendSound);
    rig.machine.handle(Command::FetchSound);
    rig.events();

    rig.machine.handle(Command::SaveSound("Kick".into()));

    let plan = rig.expect("lib_plan");
    assert_eq!(plan.field("modified"), Some(&1usize.into()));
    let Some(Json::Arr(entries)) = plan.field("entries") else {
        panic!("no entries");
    };
    let Json::Obj(fields) = &entries[0] else {
        panic!("not an object")
    };
    let get = |key: &str| {
        fields
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.clone())
    };
    assert!(matches!(get("existing_date"), Some(Json::Int(_))));
    assert!(matches!(get("candidate_date"), Some(Json::Int(_))));
    assert_eq!(get("newer"), Some(Json::Bool(true)));
    lib(&mut rig, LibCommand::Apply(Policy::default()));
    let (items, _) = rig.machine.library().list(Kind::Sounds).unwrap();
    assert_eq!((items.len(), items[0].history.len()), (1, 1));
}

#[test]
fn test_cancel_when_a_review_is_waiting_returns_nothing_written_and_a_message() {
    let mut rig = TestRig::connected("save-cancel");
    save_machine_sound(&mut rig);
    rig.machine.handle(Command::SaveSound("Again".into()));
    rig.events();

    lib(&mut rig, LibCommand::CancelPlan);

    let done = rig.expect("lib_done");
    assert!(lines(&done).contains("nothing was written"));
    assert!(rig.machine.plan.is_none());
    lib(&mut rig, LibCommand::Apply(Policy::default()));
    assert_eq!(
        rig.expect("error").field("code"),
        Some(&"precondition".into()),
        "nothing waits any more"
    );
}

#[test]
fn test_save_sound_when_a_review_is_already_waiting_returns_the_old_one_dropped_with_a_message() {
    let mut rig = TestRig::connected("save-replace-review");
    save_machine_sound(&mut rig);
    rig.machine.handle(Command::SaveSound("First".into()));
    rig.events();

    rig.machine.handle(Command::SaveSound("Second".into()));

    let events = rig.events();
    let dropped = events
        .iter()
        .find(|e| e.name == "lib_done")
        .expect("the dropped review is said");
    assert!(
        lines(dropped).contains("nothing was written"),
        "{}",
        lines(dropped)
    );
    assert!(events.iter().any(|e| e.name == "lib_plan"));
    assert_eq!(rig.machine.storage.list(Folder::Sounds).unwrap().len(), 1);
}

#[test]
fn test_apply_when_choices_are_copy_and_new_returns_the_files_written() {
    let mut rig = TestRig::connected("save-choices");
    save_machine_sound(&mut rig);
    rig.machine.handle(Command::SaveSound("Again".into()));

    lib(
        &mut rig,
        LibCommand::Apply(Policy {
            identical: OnIdentical::KeepCopy,
            modified: OnModified::SaveAsNew,
            ..Policy::default()
        }),
    );

    assert_eq!(rig.machine.storage.list(Folder::Sounds).unwrap().len(), 2);
}

#[test]
fn test_import_when_the_file_is_not_a_tempest_file_returns_an_error_and_writes_nothing() {
    let mut rig = TestRig::connected("import-bad");

    lib(
        &mut rig,
        LibCommand::Import {
            name: "x.syx".into(),
            data: vec![1, 2, 3],
        },
    );

    assert_eq!(rig.expect("error").field("during"), Some(&"lib".into()));
    assert!(rig.machine.storage.list(Folder::Sounds).unwrap().is_empty());
}

// ---- actions ---------------------------------------------------------------------------------------------------------

#[test]
fn test_edit_when_rename_tag_and_delete_are_asked_returns_each_one_said_in_a_message_and_the_list_refreshed(
) {
    let mut rig = TestRig::connected("edit");
    let id = save_machine_sound(&mut rig);

    lib(
        &mut rig,
        LibCommand::Rename {
            id: id.clone(),
            name: "Big Kick".into(),
        },
    );
    let done = rig
        .events()
        .into_iter()
        .find(|e| e.name == "lib_done")
        .expect("rename message");
    assert!(lines(&done).contains("To undo"), "{}", lines(&done));
    lib(
        &mut rig,
        LibCommand::Tag {
            ids: vec![id.clone()],
            add: true,
            tag: "drums".into(),
        },
    );
    lib(
        &mut rig,
        LibCommand::Delete {
            ids: vec![id.clone()],
        },
    );
    let events = rig.events();
    assert!(events.iter().any(|e| e.name == "library"));
    let trash = events
        .iter()
        .rev()
        .find(|e| e.name == "lib_trash")
        .expect("trash list");
    assert!(matches!(trash.field("items"), Some(Json::Arr(items)) if items.len() == 1));

    lib(&mut rig, LibCommand::EmptyTrash { expected: 5 });
    assert_eq!(
        rig.expect("error").field("code"),
        Some(&"invalid".into()),
        "wrong count refused"
    );
    assert_eq!(rig.machine.library().trash_list().unwrap().len(), 1);
    lib(&mut rig, LibCommand::EmptyTrash { expected: 1 });
    assert!(rig.machine.library().trash_list().unwrap().is_empty());
}

#[test]
fn test_history_when_an_item_was_updated_returns_its_versions_and_the_comparison_names_the_difference(
) {
    let mut rig = TestRig::connected("history");
    let id = save_machine_sound(&mut rig);
    rig.machine.handle(Command::OpenSound {
        name: "x.syx".into(),
        data: encode_current(&sound("Fake Kick", 99)),
    });
    rig.machine.handle(Command::SendSound);
    rig.machine.handle(Command::FetchSound);
    rig.machine.handle(Command::SaveSound("Kick".into()));
    lib(&mut rig, LibCommand::Apply(Policy::default()));
    rig.events();

    lib(&mut rig, LibCommand::History { id: id.clone() });
    let history = rig.expect("lib_history");
    assert!(matches!(history.field("versions"), Some(Json::Arr(v)) if v.len() == 1));
    lib(
        &mut rig,
        LibCommand::Compare {
            id: id.clone(),
            version: 0,
        },
    );
    let compare = rig.expect("lib_compare");
    assert!(
        lines(&compare).contains("Osc 1 Frequency: 36 -> 99"),
        "{}",
        lines(&compare)
    );

    lib(&mut rig, LibCommand::RestoreVersion { id, version: 0 });
    let done = rig
        .events()
        .into_iter()
        .find(|e| e.name == "lib_done")
        .expect("message");
    assert!(lines(&done).contains("Restored"), "{}", lines(&done));
}

#[test]
fn test_open_when_an_item_is_asked_returns_the_card_event_and_a_project_is_refused() {
    let mut rig = TestRig::connected("open-item");
    let id = save_machine_sound(&mut rig);
    rig.events();

    lib(&mut rig, LibCommand::Open { id });

    assert_eq!(
        rig.expect("sound_opened").field("file"),
        Some(&"Kick.syx".into())
    );
}

/// The project captured from the author's machine (`TEMPESTER_PROJECT_CAPTURE`; their data, read only, never
/// copied). The batch summary is checked on real data: a first fetch is all new, a second all identical.
#[test]
#[ignore = "needs the captured project (TEMPESTER_PROJECT_CAPTURE)"]
fn test_real_project_capture_library_summary_when_fetched_twice_returns_new_then_identical() {
    let path = std::env::var_os("TEMPESTER_PROJECT_CAPTURE").expect("TEMPESTER_PROJECT_CAPTURE");
    let capture = std::fs::read(path).expect("the capture file");
    let messages: Vec<&[u8]> = capture.split_inclusive(|b| *b == 0xF7).collect();
    let mut rig = TestRig::connected("real-capture-summary");
    let mut counts = Vec::new();
    for round in 0..2 {
        rig.machine.handle(Command::ReceiveProject);
        for message in &messages {
            rig.transport().push_incoming(message);
        }
        rig.machine.poll_incoming();
        let plan = rig.expect("lib_plan");
        let count = |key: &str| match plan.field(key) {
            Some(Json::Int(n)) => *n,
            other => panic!("{key}: {other:?}"),
        };
        counts.push((count("new"), count("identical"), count("modified")));
        println!(
            "round {}: {:?} (new, identical, modified)",
            round + 1,
            counts[round]
        );
        lib(&mut rig, LibCommand::Apply(Policy::default()));
        rig.events();
    }
    let (new, identical, modified) = counts[0];
    assert!(new > 0 && identical == 0 && modified == 0, "{counts:?}");
    assert_eq!(counts[1], (0, new + identical + modified, 0), "{counts:?}");
}
