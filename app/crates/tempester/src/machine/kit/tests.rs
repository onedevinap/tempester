//! The kit builder through the worker's commands, on the fake machine and a disposable data folder: the plan, the bytes
//! sent (only the chosen sounds differ, byte for byte), the auto backup, and what is refused.

use super::*;
use crate::command::Command;
use crate::json::Json;
use crate::library::command::LibCommand;
use crate::machine::testkit::TestRig;
use tempest_proto::beat::{synthetic_beat_data, with_sounds};
use tempest_proto::sound::SOUND_BYTES;
use tempest_proto::sound_message::{encode_file, REQUEST_BEAT};

fn lib(rig: &mut TestRig, command: LibCommand) {
    rig.machine.handle(Command::Lib(command));
}

fn sound(name: &str, freq: u16) -> Sound {
    let mut sound = Sound::with_defaults();
    sound.set(0, freq).expect("in range");
    sound.set_name(name).expect("ascii");
    sound
}

fn import_sound(rig: &mut TestRig, name: &str, freq: u16) -> String {
    let file = encode_file(name, &sound(name, freq)).expect("encodes");
    lib(
        rig,
        LibCommand::Import {
            name: format!("{name}.syx"),
            data: file,
        },
    );
    rig.events();
    let (items, _) = rig.machine.library().list(Kind::Sounds).expect("lists");
    items
        .into_iter()
        .find(|i| i.name == name)
        .expect("imported")
        .id
}

fn kit_of(choices: &[(usize, &str)]) -> Vec<Option<String>> {
    let mut ids = vec![None; PADS];
    for (pad, id) in choices {
        ids[*pad] = Some((*id).to_string());
    }
    ids
}

fn text(event: &Event, key: &str) -> String {
    match event.field(key) {
        Some(Json::Str(text)) => text.clone(),
        other => panic!("{key}: {other:?}"),
    }
}

/// The machine's beat as the fake holds it now, unpacked.
fn machine_data(rig: &mut TestRig) -> Vec<u8> {
    rig.machine.handle(Command::FetchBeat);
    let fetched = rig.machine.fetched_beat.as_ref().expect("fetched");
    beat_data(&fetched.message).expect("decodes")
}

/// Fails unless `new` is `old` with only the sound records of `pads` changed (and each of those really changed).
fn assert_only_sounds_differ(old: &[u8], new: &[u8], pads: &[usize]) {
    assert_eq!(old.len(), new.len(), "same length");
    let first = old.len() - PADS * SOUND_BYTES;
    for at in 0..old.len() {
        let allowed = at >= first && pads.contains(&((at - first) / SOUND_BYTES));
        if !allowed {
            assert_eq!(old[at], new[at], "byte {at} changed outside the sounds");
        }
    }
    for pad in pads {
        let range = first + pad * SOUND_BYTES..first + (pad + 1) * SOUND_BYTES;
        assert_ne!(old[range.clone()], new[range], "pad {} unchanged", pad + 1);
    }
}

#[test]
fn test_kit_check_when_two_sounds_are_chosen_returns_a_plan_naming_them_and_sends_only_the_request()
{
    let mut rig = TestRig::connected("kit-check");
    let a = import_sound(&mut rig, "Kick A", 10);
    let b = import_sound(&mut rig, "Snare B", 20);

    lib(
        &mut rig,
        LibCommand::KitCheck {
            sounds: kit_of(&[(0, &a), (31, &b)]),
        },
    );

    let plan = rig.expect("lib_kit_plan");
    assert_eq!(text(&plan, "status"), "unknown");
    assert_eq!(text(&plan, "machine"), "Fake beat 01");
    let said = format!("{:?}", plan.field("lines"));
    assert!(
        said.contains("2 pad(s)") && said.contains("1 Kick A") && said.contains("32 Snare B"),
        "{said}"
    );
    assert!(said.contains("byte for byte"), "{said}");
    assert_eq!(
        rig.transport().sent_bytes(),
        REQUEST_BEAT,
        "nothing but the request"
    );
}

#[test]
fn test_kit_apply_when_the_beat_is_unknown_returns_the_backup_and_a_beat_where_only_those_sounds_differ(
) {
    let mut rig = TestRig::connected("kit-apply");
    let a = import_sound(&mut rig, "Kick A", 10);
    let b = import_sound(&mut rig, "Snare B", 20);
    let original = machine_data(&mut rig);
    let ids = kit_of(&[(0, &a), (31, &b)]);
    lib(
        &mut rig,
        LibCommand::KitCheck {
            sounds: ids.clone(),
        },
    );
    let token = text(&rig.expect("lib_kit_plan"), "token");
    let before = rig.transport().sent_bytes().len();

    lib(
        &mut rig,
        LibCommand::KitApply {
            sounds: ids,
            machine: token,
        },
    );

    let sent = rig.transport().sent_bytes()[before..].to_vec();
    let mut chosen: [Option<Sound>; PADS] = std::array::from_fn(|_| None);
    chosen[0] = Some(sound("Kick A", 10));
    chosen[31] = Some(sound("Snare B", 20));
    let expected = beat::standard_dump(&with_sounds(&original, &chosen).expect("kit"));
    assert_eq!(
        sent,
        [REQUEST_BEAT.to_vec(), expected].concat(),
        "the request, then exactly the kit"
    );
    let now = machine_data(&mut rig);
    assert_only_sounds_differ(&original, &now, &[0, 31]);
    let index = rig.machine.library().snapshot().expect("index");
    let slot = index
        .item("machine-beats")
        .expect("the slot of auto backups");
    assert_eq!(slot.history.len(), 1);
    let kept =
        std::fs::read(rig.machine.storage.root().join(&slot.history[0].file)).expect("backup file");
    assert_eq!(
        beat_data(&kept).expect("a beat"),
        original,
        "the backup is the machine's beat before"
    );
    let done = rig
        .events()
        .into_iter()
        .find(|e| e.name == "lib_done")
        .expect("a message");
    assert!(format!("{:?}", done.field("lines")).contains("saved as an auto backup first"));
}

#[test]
fn test_kit_apply_when_the_machines_beat_is_in_the_library_returns_the_kit_sent_without_a_new_backup(
) {
    let mut rig = TestRig::connected("kit-known");
    let a = import_sound(&mut rig, "Kick A", 10);
    rig.machine.handle(Command::FetchBeat);
    rig.machine.handle(Command::SaveBeat("Mine".into()));
    rig.events();
    let ids = kit_of(&[(5, &a)]);
    lib(
        &mut rig,
        LibCommand::KitCheck {
            sounds: ids.clone(),
        },
    );
    let plan = rig.expect("lib_kit_plan");
    assert_eq!(text(&plan, "status"), "known");

    lib(
        &mut rig,
        LibCommand::KitApply {
            sounds: ids,
            machine: text(&plan, "token"),
        },
    );

    let index = rig.machine.library().snapshot().expect("index");
    assert!(
        index.item("machine-beats").is_none(),
        "no backup was needed"
    );
    assert!(rig.transport().sent_bytes().len() > 2 * REQUEST_BEAT.len());
}

#[test]
fn test_kit_apply_when_the_machines_beat_changed_since_the_check_returns_a_new_plan_and_sends_nothing(
) {
    // Counter-proof of the token: a kit built for one beat must not land on another.
    let mut rig = TestRig::connected("kit-changed");
    let a = import_sound(&mut rig, "Kick A", 10);
    let ids = kit_of(&[(0, &a)]);
    lib(
        &mut rig,
        LibCommand::KitCheck {
            sounds: ids.clone(),
        },
    );
    let token = text(&rig.expect("lib_kit_plan"), "token");
    rig.machine.handle(Command::OpenBeat {
        name: "x.syx".into(),
        data: beat::standard_dump(&synthetic_beat_data(
            "Moved",
            &[],
            0,
            &std::array::from_fn(|_| Sound::with_defaults()),
        )),
    });
    rig.machine.handle(Command::SendBeat);
    rig.events();
    let before = rig.transport().sent_bytes().len();

    lib(
        &mut rig,
        LibCommand::KitApply {
            sounds: ids,
            machine: token.clone(),
        },
    );

    let plan = rig.expect("lib_kit_plan");
    assert_eq!(plan.field("changed"), Some(&true.into()));
    assert_ne!(text(&plan, "token"), token);
    assert_eq!(
        rig.transport().sent_bytes()[before..],
        REQUEST_BEAT,
        "only the request went out"
    );
    assert!(rig
        .machine
        .library()
        .snapshot()
        .expect("index")
        .item("machine-beats")
        .is_none());
}

#[test]
fn test_kit_check_when_no_pad_has_a_sound_returns_precondition_error() {
    let mut rig = TestRig::connected("kit-empty");

    lib(
        &mut rig,
        LibCommand::KitCheck {
            sounds: kit_of(&[]),
        },
    );

    assert_eq!(
        rig.expect("error").field("code"),
        Some(&"precondition".into())
    );
    assert!(rig.transport().sent_bytes().is_empty());
}

#[test]
fn test_kit_check_when_the_machine_is_silent_returns_timeout_and_sends_no_beat() {
    let mut rig = TestRig::connected("kit-silent");
    let a = import_sound(&mut rig, "Kick A", 10);
    rig.transport().set_responder(Box::new(|_| vec![]));

    lib(
        &mut rig,
        LibCommand::KitCheck {
            sounds: kit_of(&[(0, &a)]),
        },
    );

    assert_eq!(rig.expect("error").field("code"), Some(&"timeout".into()));
    assert_eq!(rig.transport().sent_bytes(), REQUEST_BEAT);
}

#[test]
fn test_kit_check_when_an_id_is_not_a_sound_returns_invalid_error() {
    let mut rig = TestRig::connected("kit-bad-id");
    rig.machine.handle(Command::FetchBeat);
    rig.machine.handle(Command::SaveBeat("Mine".into()));
    rig.events();
    let (beats, _) = rig.machine.library().list(Kind::Beats).expect("lists");

    lib(
        &mut rig,
        LibCommand::KitCheck {
            sounds: kit_of(&[(0, &beats[0].id)]),
        },
    );

    assert_eq!(rig.expect("error").field("code"), Some(&"invalid".into()));
}

#[test]
#[should_panic(expected = "changed outside the sounds")]
fn test_assert_only_sounds_differ_when_an_event_byte_changes_returns_a_failure() {
    // Counter-proof of the byte-for-byte check itself: one event byte altered must be caught.
    let sounds: [Sound; PADS] = std::array::from_fn(|_| Sound::with_defaults());
    let old = synthetic_beat_data("x", &[], 3, &sounds);
    let mut new = old.clone();
    new[538 * 2 + 4] ^= 1;
    let at = old.len() - SOUND_BYTES;
    new[at] ^= 1;

    assert_only_sounds_differ(&old, &new, &[31]);
}
