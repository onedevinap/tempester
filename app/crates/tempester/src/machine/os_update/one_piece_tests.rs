//! A send in one piece (Windows, Linux: the port takes no part of a SysEx message) keeps the app alive. The fake's
//! `hold_sends` keeps the file's one `send` waiting, as WinMM keeps the call for the whole file (about 190 s for a
//! Main): meanwhile the worker answers the page, the window can stop the worker at once, and quitting forgets the record
//! of the chip that now holds part of a file.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use super::*;
use crate::machine::testkit::TestRig;
use crate::machine::Worker;

const SOON: Duration = Duration::from_secs(5);

/// A valid OS file for `id`, made of a pattern (unknown to the app: sent with `OsSendUnknown`).
fn os_file(id: OsId) -> Vec<u8> {
    let image: Vec<u8> = (0..id.blocks() * 1024)
        .map(|i| (i * 13 % 251) as u8)
        .collect();
    os_image::encode(id, &image).expect("a valid image")
}

/// A connected machine whose port takes SysEx only whole, with `file` ready to send.
fn whole_message_rig(label: &str, file: &[u8]) -> TestRig {
    let mut rig = TestRig::connected(label);
    rig.transport().refuse_sysex_fragments();
    rig.machine.rate = 100_000_000;
    rig.machine.handle(Command::OsValidate {
        name: "os.syx".into(),
        data: file.to_vec(),
    });
    rig.expect("os_ready");
    rig
}

/// A Main the app believes installed, to see whether a quit keeps or forgets it.
fn installed_record() -> InstalledOs {
    InstalledOs {
        label: "Tempester 1.6.9.1".into(),
        version: Some("1.6.9.1".into()),
        at: "2026-11-21T10:00:00Z".into(),
        sha256_image: "ab".repeat(32),
        sha256_image_written: Some("cd".repeat(32)),
        verified_at: None,
    }
}

/// `Worker::stop` on another thread: true when it returned within `SOON`.
fn stops_soon(worker: Worker) -> bool {
    let (done, finished) = mpsc::channel();
    thread::spawn(move || {
        worker.stop();
        let _ = done.send(());
    });
    finished.recv_timeout(SOON).is_ok()
}

#[test]
fn test_os_send_in_one_piece_when_the_driver_holds_the_file_returns_answers_to_the_page_meanwhile()
{
    let file = os_file(OsId::Sam);
    let rig = whole_message_rig("one-piece-alive", &file);
    let transport = rig.transport();
    let hold = transport.hold_sends();
    let rate = rig.machine.rate;
    let (worker, events) = rig.into_worker();

    worker
        .commands
        .send(Command::OsSendUnknown)
        .expect("worker alive");
    let started = events.wait_for("os_started");
    assert!(
        hold.wait_for_a_blocked_send(SOON),
        "the file is in the driver"
    );
    worker.commands.send(Command::State).expect("worker alive");
    events.wait_for("state");
    worker
        .commands
        .send(Command::Identity)
        .expect("worker alive");
    let refused = events.wait_for("error");
    assert!(worker.os_active.load(Ordering::SeqCst));
    hold.release();
    let finished = events.wait_for("os_finished");

    assert_eq!(started.field("in_one_piece"), Some(&Json::Bool(true)));
    assert_eq!(
        started.field("seconds"),
        Some(&seconds_for(file.len(), rate).into())
    );
    assert_eq!(refused.field("code"), Some(&"busy".into()));
    assert_eq!(finished.field("state"), Some(&"done".into()));
    assert_eq!(transport.sent(), vec![file]);
    assert!(stops_soon(worker));
}

#[test]
fn test_stop_when_a_main_is_sent_in_one_piece_returns_at_once_without_touching_the_port_and_forgets_the_record(
) {
    let file = os_file(OsId::Main);
    let mut rig = whole_message_rig("one-piece-quit", &file);
    let storage = rig.machine.storage.clone();
    storage
        .set_installed_os(Some(&installed_record()))
        .expect("record written");
    let hold = rig.transport().hold_sends();
    // Off the test thread: a send run by the worker itself would never hand the machine back.
    let (back, machine_back) = mpsc::channel();
    thread::spawn(move || {
        rig.machine.handle(Command::OsSendUnknown);
        // A key the worker still believes held: its release would wait behind the file on a real port.
        rig.machine.held_keys.insert(9);
        let _ = back.send(rig);
    });
    let rig = machine_back
        .recv_timeout(SOON)
        .expect("os_send hands the file to its own thread and returns");
    assert!(
        hold.wait_for_a_blocked_send(SOON),
        "the file is in the driver"
    );
    let (worker, _events) = rig.into_worker();

    let stopped = stops_soon(worker);
    let record = storage.installed_os();
    hold.release();

    assert!(stopped, "the window waited on the port");
    assert_eq!(record, None, "the Main now holds part of a file");
}

#[test]
fn test_stop_when_no_send_runs_returns_the_held_keys_released_and_the_record_kept() {
    let mut rig = TestRig::connected("one-piece-idle-quit");
    let storage = rig.machine.storage.clone();
    let record = installed_record();
    storage
        .set_installed_os(Some(&record))
        .expect("record written");
    rig.machine.handle(Command::Key {
        down: true,
        code: 9,
    });
    let transport = rig.transport();
    let (worker, _events) = rig.into_worker();

    assert!(stops_soon(worker));

    assert_eq!(transport.sent().last(), Some(&vec![0xB0, 103, 9]));
    assert_eq!(storage.installed_os(), Some(record));
}
