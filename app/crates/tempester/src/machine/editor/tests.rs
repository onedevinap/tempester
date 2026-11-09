//! The editing tools on the worker, against the fake machine and the fake controller: what reaches the machine,
//! what is saved, what never goes back to the controller.

use tempest_proto::nrpn;
use tempest_proto::sound::Sound;
use tempest_proto::sound_message;

use super::*;
use crate::edit::variation::Group;
use crate::json::Json;
use crate::machine::connector::FAKE_CONTROLLER;
use crate::machine::testkit::TestRig;

const LP_FREQ: u16 = 34;

/// Connected, the sound fetched, sends not paced (tests do not wait), events consumed.
fn synced(label: &str) -> TestRig {
    let mut rig = TestRig::connected(label);
    rig.machine.rate = 100_000_000;
    rig.machine.handle(Command::FetchSound);
    rig.events();
    rig
}

fn edit(rig: &mut TestRig, command: EditCommand) {
    rig.machine.handle(Command::Edit(command));
}

/// The packets sent from `from` on.
fn sent_since(rig: &TestRig, from: usize) -> Vec<Vec<u8>> {
    rig.transport().sent()[from..].to_vec()
}

fn nrpn_packet(number: u16, value: u16) -> Vec<u8> {
    nrpn::encode(0, number, value).unwrap().to_vec()
}

/// A value of the fake machine's sound ("Fake Kick"), read the way the app reads it.
fn fetched_value(index: usize) -> u16 {
    let rig = synced("fetched-value");
    rig.machine.edit.history.values()[index]
}

#[test]
fn test_undo_when_a_knob_was_dragged_returns_one_nrpn_back_to_the_value_before_the_drag() {
    let mut rig = synced("edit-undo");
    let before = fetched_value(34);
    for value in [100, 105, 110, 120] {
        rig.machine.handle(Command::Nrpn {
            number: LP_FREQ,
            value,
        });
    }
    let from = rig.transport().sent().len();

    edit(&mut rig, EditCommand::Undo);

    assert_eq!(sent_since(&rig, from), vec![nrpn_packet(LP_FREQ, before)]);
    let from = rig.transport().sent().len();
    edit(&mut rig, EditCommand::Redo);
    assert_eq!(sent_since(&rig, from), vec![nrpn_packet(LP_FREQ, 120)]);
    let event = rig.expect("edit");
    assert_eq!(event.field("depth"), Some(&Json::from(1usize)));
}

#[test]
fn test_undo_when_nothing_was_changed_returns_a_precondition_error_and_sends_nothing() {
    let mut rig = synced("edit-undo-empty");
    let from = rig.transport().sent().len();

    edit(&mut rig, EditCommand::Undo);

    assert_eq!(
        rig.expect("error").field("code"),
        Some(&"precondition".into())
    );
    assert!(sent_since(&rig, from).is_empty());
}

#[test]
fn test_recall_when_a_and_b_differ_returns_only_the_differing_parameters_as_nrpn() {
    let mut rig = synced("edit-ab");
    edit(&mut rig, EditCommand::Store(Slot::A));
    rig.machine.handle(Command::Nrpn {
        number: LP_FREQ,
        value: 20,
    });
    rig.machine.handle(Command::Nrpn {
        number: 72,
        value: 99,
    });
    edit(&mut rig, EditCommand::Store(Slot::B));
    let from = rig.transport().sent().len();

    edit(&mut rig, EditCommand::Recall(Slot::A));

    assert_eq!(
        sent_since(&rig, from),
        vec![
            nrpn_packet(LP_FREQ, fetched_value(34)),
            nrpn_packet(72, fetched_value(72))
        ]
    );
    let from = rig.transport().sent().len();
    edit(&mut rig, EditCommand::Recall(Slot::B));
    assert_eq!(
        sent_since(&rig, from),
        vec![nrpn_packet(LP_FREQ, 20), nrpn_packet(72, 99)]
    );
    // every packet of the editing tools is an NRPN: no SysEx, nothing that could reach the flash
    for packet in rig.transport().sent().iter().skip(1) {
        assert_eq!(
            (packet.len(), packet[0] & 0xF0),
            (12, 0xB0),
            "{packet:02x?}"
        );
    }
    let last = rig.events().into_iter().rev().find(|e| e.name == "edit");
    assert_eq!(
        last.and_then(|e| e.field("active").cloned()),
        Some("B".into())
    );
}

#[test]
fn test_store_and_vary_when_no_sound_was_fetched_returns_a_precondition_error() {
    let mut rig = TestRig::connected("edit-unsynced");

    edit(&mut rig, EditCommand::Store(Slot::A));
    edit(
        &mut rig,
        EditCommand::Vary {
            group: Group::All,
            amount: 50,
            seed: 1,
        },
    );

    let codes: Vec<_> = rig
        .events()
        .into_iter()
        .filter(|e| e.name == "error")
        .map(|e| e.field("code").cloned())
        .collect();
    assert_eq!(codes, vec![Some("precondition".into()); 2]);
    assert_eq!(rig.transport().sent().len(), 0);
}

#[test]
fn test_vary_when_the_same_seed_is_used_on_the_same_sound_sends_the_same_bytes_and_undo_restores() {
    let vary_once = |label: &str, seed: u32| {
        let mut rig = synced(label);
        let from = rig.transport().sent().len();
        edit(
            &mut rig,
            EditCommand::Vary {
                group: Group::Filter,
                amount: 60,
                seed,
            },
        );
        let sent = sent_since(&rig, from);
        let from = rig.transport().sent().len();
        edit(&mut rig, EditCommand::Undo);
        (sent, sent_since(&rig, from))
    };

    let (first, undone) = vary_once("vary-1", 4242);
    let (again, _) = vary_once("vary-2", 4242);
    let (other, _) = vary_once("vary-3", 4243);

    assert!(!first.is_empty());
    assert_eq!(first, again, "same sound, group, amount and seed");
    assert_ne!(first, other, "counter-proof: another seed");
    assert_eq!(
        undone.len(),
        first.len(),
        "undo puts back every varied parameter"
    );
    for packet in &first {
        let number = (u16::from(packet[2]) << 7) | u16::from(packet[5]);
        assert!(Group::Filter.params().contains(&(number as u8)), "{number}");
    }
}

#[test]
fn test_a_sysex_send_when_a_sound_was_synced_returns_the_tools_locked_until_the_next_sync() {
    let mut rig = synced("edit-forget");
    let file = sound_message::encode_current(&Sound::with_defaults());
    rig.machine.handle(Command::OpenSound {
        name: "x.syx".into(),
        data: file,
    });
    rig.machine.handle(Command::SendSound);
    rig.events();

    edit(&mut rig, EditCommand::Store(Slot::A));

    assert_eq!(
        rig.expect("error").field("code"),
        Some(&"precondition".into())
    );
    rig.machine.handle(Command::FetchSound);
    edit(&mut rig, EditCommand::Store(Slot::A));
    assert!(rig.events().iter().all(|e| e.name != "error"));
}

/// The fake controller opened, events consumed.
fn with_controller(label: &str) -> TestRig {
    let mut rig = synced(label);
    edit(
        &mut rig,
        EditCommand::LearnOpen(Some(FAKE_CONTROLLER.into())),
    );
    rig.events();
    rig
}

fn controller(rig: &TestRig) -> Arc<tempest_midi::FakeTransport> {
    rig.machine.connector.fake_controller().expect("opened")
}

#[test]
fn test_learn_when_armed_and_a_cc_arrives_maps_it_saves_it_and_then_drives_the_target() {
    let mut rig = with_controller("learn");
    let tempest_before = rig.transport().sent().len();
    edit(&mut rig, EditCommand::LearnArm(Target::Param(34)));

    controller(&rig).push_incoming(&[0xB3, 21, 5]);
    rig.machine.poll_incoming();

    let learn = rig
        .events()
        .into_iter()
        .rev()
        .find(|e| e.name == "learn")
        .expect("learn event");
    assert_eq!(learn.field("learned_cc"), Some(&Json::from(21u8)));
    assert_eq!(learn.field("armed"), Some(&Json::Null));
    assert_eq!(
        rig.machine.storage.load_settings().get(KEY_LEARN_MAP),
        Some("21=n34")
    );

    controller(&rig).push_incoming(&[0xB0, 21, 10, 0xB0, 21, 64]);
    rig.machine.poll_incoming();
    let learned: Vec<_> = rig
        .events()
        .into_iter()
        .filter(|e| e.name == "learned")
        .collect();
    assert_eq!(learned.len(), 1, "the last value of a batch only");
    assert_eq!(learned[0].field("target"), Some(&"n34".into()));
    assert_eq!(learned[0].field("value"), Some(&Json::from(83u16)));
    // no echo: nothing goes back to the controller, and the controller's bytes are not passed on to the machine
    assert!(controller(&rig).sent().is_empty());
    assert_eq!(rig.transport().sent().len(), tempest_before);
}

#[test]
fn test_learn_when_not_armed_or_reserved_cc_or_unmapped_returns_nothing_learned() {
    // counter-proofs of the learn test
    let mut rig = with_controller("learn-not");
    controller(&rig).push_incoming(&[0xB0, 21, 5]);
    rig.machine.poll_incoming();
    edit(&mut rig, EditCommand::LearnArm(Target::Param(34)));
    controller(&rig).push_incoming(&[0xB0, 6, 5, 0xB0, 99, 1, 0x90, 40, 100]);
    rig.machine.poll_incoming();

    let events = rig.events();
    assert!(events.iter().all(|e| e.name != "learned"));
    assert!(events.iter().all(|e| e.field("learned_cc").is_none()));
    assert_eq!(rig.machine.storage.load_settings().get(KEY_LEARN_MAP), None);
}

#[test]
fn test_learn_when_the_file_changed_meanwhile_returns_both_mappings_kept() {
    let mut rig = with_controller("learn-merge");
    rig.machine
        .storage
        .update_settings(|s| s.set(KEY_LEARN_MAP, "30=distort"))
        .unwrap();
    edit(&mut rig, EditCommand::LearnArm(Target::Param(35)));

    controller(&rig).push_incoming(&[0xB0, 22, 1]);
    rig.machine.poll_incoming();

    assert_eq!(
        rig.machine.storage.load_settings().get(KEY_LEARN_MAP),
        Some("22=n35,30=distort")
    );
}

#[test]
fn test_learn_clear_when_one_then_all_returns_the_table_emptied_step_by_step() {
    let mut rig = with_controller("learn-clear");
    rig.machine
        .storage
        .update_settings(|s| s.set(KEY_LEARN_MAP, "21=n34,22=volume"))
        .unwrap();

    edit(&mut rig, EditCommand::LearnClear(Target::Param(34)));
    assert_eq!(
        rig.machine.storage.load_settings().get(KEY_LEARN_MAP),
        Some("22=volume")
    );
    edit(&mut rig, EditCommand::LearnClearAll);

    assert_eq!(
        rig.machine.storage.load_settings().get(KEY_LEARN_MAP),
        Some("")
    );
    controller(&rig).push_incoming(&[0xB0, 22, 1]);
    rig.machine.poll_incoming();
    assert!(rig.events().iter().all(|e| e.name != "learned"));
}

#[test]
fn test_learn_open_when_the_tempest_port_is_chosen_returns_an_error_and_opens_nothing() {
    let mut rig = synced("learn-own-port");

    edit(&mut rig, EditCommand::LearnOpen(Some("Tempest".into())));

    assert_eq!(
        rig.expect("error").field("code"),
        Some(&"precondition".into())
    );
    assert!(rig.machine.connector.fake_controller().is_none());
}

#[test]
fn test_learn_port_when_saved_returns_it_reopened_by_a_new_worker() {
    let mut rig = with_controller("learn-reopen");
    let storage = crate::storage::Storage::at(rig.machine.storage.root().to_path_buf());
    let (sender, events) = std::sync::mpsc::channel();
    let emit: crate::event::Emitter = Arc::new(move |event| {
        let _ = sender.send(event);
    });
    let mut again = Machine::new(
        Box::new(super::super::connector::FakeConnector::new()),
        storage,
        emit,
    );

    again.handle(Command::Edit(EditCommand::State));

    let learn = events
        .try_iter()
        .find(|e| e.name == "learn")
        .expect("learn event");
    assert_eq!(learn.field("port"), Some(&FAKE_CONTROLLER.into()));
    rig.events();
}
