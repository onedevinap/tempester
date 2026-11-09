use std::path::PathBuf;
use std::time::Duration;

use super::*;
use crate::machine::fake_machine::debug_send_messages;
use crate::machine::testkit::TestRig;
use tempest_midi::paced::CHUNK_SIZE;
use tempest_proto::sound_message::REQUEST_SOUND;

/// A valid SAM file (the smallest OS image: 32 blocks) made of a pattern.
fn sam_file() -> Vec<u8> {
    let image: Vec<u8> = (0..OsId::Sam.blocks() * 1024)
        .map(|i| (i * 13 % 251) as u8)
        .collect();
    os_image::encode(OsId::Sam, &image).unwrap()
}

fn fast(rig: &mut TestRig) {
    rig.machine.rate = 100_000_000;
}

fn ready(rig: &mut TestRig, file: &[u8]) {
    rig.machine.handle(Command::OsValidate {
        name: "sam.syx".into(),
        data: file.to_vec(),
    });
    rig.expect("os_ready");
}

#[test]
fn test_new_when_machine_created_returns_the_cable_rate_of_3125_bytes_per_second() {
    let rig = TestRig::new("os-rate");

    assert_eq!(rig.machine.rate, 3125);
}

#[test]
fn test_os_versions_when_asked_returns_newest_first_with_descriptions() {
    let mut rig = TestRig::new("os-versions");

    rig.machine.handle(Command::OsVersions);

    let event = rig.expect("os_versions");
    let Some(Json::Arr(list)) = event.field("versions") else {
        panic!("no list")
    };
    let numbers: Vec<String> = list.iter().map(|v| v.render()).collect();
    let shown: Vec<Vec<u32>> = numbers
        .iter()
        .filter_map(|v| v.split(r#""version":""#).nth(1)?.split('"').next())
        .map(version_key)
        .collect();
    let mut expected: Vec<Vec<u32>> = versions::versions()
        .iter()
        .map(|v| version_key(v.version))
        .collect();
    expected.sort_by(|a, b| b.cmp(a));
    assert_eq!(shown, expected, "{numbers:?}");
    for field in ["kind", "date", "summary", "changes", "fixes", "description"] {
        let key = format!(r#""{field}":"#);
        assert!(numbers.iter().all(|v| v.contains(&key)), "{field}");
    }
}

#[test]
fn test_version_key_when_two_digit_part_returns_numeric_order() {
    assert!(version_key("1.10.0.0") > version_key("1.9.0.0"));
    assert!(version_key("1.5.4.1") > version_key("1.5.4.0"));
}

#[test]
fn test_os_validate_when_valid_file_returns_ready_with_chip_blocks_checksum_and_duration() {
    let mut rig = TestRig::new("os-validate");
    let file = sam_file();

    rig.machine.handle(Command::OsValidate {
        name: "sam.syx".into(),
        data: file.clone(),
    });

    let event = rig.expect("os_ready");
    assert_eq!(event.field("chip"), Some(&"SAM".into()));
    assert_eq!(event.field("blocks"), Some(&32usize.into()));
    assert_eq!(event.field("size"), Some(&file.len().into()));
    assert_eq!(
        event.field("seconds"),
        Some(&file.len().div_ceil(3125).into())
    );
    assert!(rig.machine.pending_os.is_some());
}

#[test]
fn test_os_validate_when_checksum_wrong_returns_error_and_nothing_pending_or_sent() {
    let mut rig = TestRig::connected("os-bad");
    let mut file = sam_file();
    let at = file.len() - 3;
    file[at] ^= 0x01;

    rig.machine.handle(Command::OsValidate {
        name: "bad.syx".into(),
        data: file,
    });
    rig.machine.handle(Command::OsSendUnknown);

    let codes: Vec<_> = rig
        .events()
        .iter()
        .filter(|e| e.name == "error")
        .map(|e| e.field("code").cloned())
        .collect();
    assert_eq!(
        codes,
        vec![Some("invalid".into()), Some("precondition".into())]
    );
    assert!(rig.machine.pending_os.is_none());
    assert!(rig.transport().sent().is_empty());
}

#[test]
fn test_os_validate_when_not_an_os_file_returns_error() {
    let mut rig = TestRig::new("os-garbage");

    rig.machine.handle(Command::OsValidate {
        name: "x".into(),
        data: vec![1, 2, 3, 4, 5, 6, 7, 8],
    });

    assert_eq!(rig.expect("error").field("code"), Some(&"invalid".into()));
}

#[test]
fn test_os_build_when_file_is_not_the_dsi_main_returns_error_before_anything_is_ready() {
    let mut rig = TestRig::connected("os-build-wrong");
    let main = debug_send_messages().remove(0);

    rig.machine.handle(Command::OsBuild {
        version: "1.5.1.0".into(),
        data: main,
    });

    let error = rig.expect("error");
    assert!(
        error.render().contains("origin") || error.render().contains("SHA"),
        "{}",
        error.render()
    );
    assert!(rig.machine.pending_os.is_none());
    assert!(rig.transport().sent().is_empty());
}

#[test]
fn test_os_build_when_version_unknown_returns_error() {
    let mut rig = TestRig::new("os-build-unknown");

    rig.machine.handle(Command::OsBuild {
        version: "9.9.9.9".into(),
        data: vec![],
    });

    assert_eq!(rig.expect("error").field("code"), Some(&"invalid".into()));
}

#[test]
fn test_os_original_when_file_is_not_the_dsi_original_returns_error() {
    let mut rig = TestRig::new("os-original-wrong");

    rig.machine.handle(Command::OsOriginal {
        data: debug_send_messages().remove(0),
    });

    assert_eq!(rig.expect("error").field("code"), Some(&"invalid".into()));
    assert!(rig.machine.pending_os.is_none());
}

#[test]
fn test_os_send_when_ready_returns_exactly_the_file_bytes_in_128_byte_packets() {
    let mut rig = TestRig::connected("os-send");
    fast(&mut rig);
    let file = sam_file();
    ready(&mut rig, &file);

    rig.machine.handle(Command::OsSendUnknown);
    let finished = rig.wait_for("os_finished");

    assert_eq!(finished.field("state"), Some(&"done".into()));
    let sent = rig.transport().sent();
    assert_eq!(sent.concat(), file);
    assert_eq!(sent.len(), file.len().div_ceil(CHUNK_SIZE));
    assert!(
        rig.machine.pending_os.is_none(),
        "a file is sent once, then must be confirmed again"
    );
    assert!(!rig.machine.os_active.load(Ordering::SeqCst));
}

#[test]
fn test_os_send_when_a_remote_key_is_held_releases_it_before_the_first_byte_of_the_file() {
    let mut rig = TestRig::connected("os-send-held");
    fast(&mut rig);
    let file = sam_file();
    ready(&mut rig, &file);
    rig.machine.handle(Command::Key {
        down: true,
        code: 9,
    });

    rig.machine.handle(Command::OsSendUnknown);
    rig.wait_for("os_finished");

    let sent = rig.transport().sent();
    assert_eq!(sent[..2], [vec![0xB0, 102, 9], vec![0xB0, 103, 9]]);
    assert_eq!(sent[2..].concat(), file);
    assert!(rig.machine.held_keys.is_empty());
}

#[test]
fn test_os_send_when_not_connected_returns_not_connected_and_keeps_the_file_ready() {
    let mut rig = TestRig::new("os-send-off");
    ready(&mut rig, &sam_file());

    rig.machine.handle(Command::OsSendUnknown);

    assert_eq!(
        rig.expect("error").field("code"),
        Some(&"not_connected".into())
    );
    assert!(rig.machine.pending_os.is_some());
    assert!(!rig.machine.os_active.load(Ordering::SeqCst));
}

#[test]
fn test_os_send_when_nothing_ready_returns_precondition_error_and_sends_nothing() {
    let mut rig = TestRig::connected("os-send-none");

    rig.machine.handle(Command::OsSendUnknown);

    assert_eq!(
        rig.expect("error").field("code"),
        Some(&"precondition".into())
    );
    assert!(rig.transport().sent().is_empty());
}

#[test]
fn test_os_cancel_when_send_is_running_returns_cancelled_with_less_than_the_whole_file_sent() {
    let mut rig = TestRig::connected("os-cancel");
    rig.machine.rate = 20_000; // about two seconds for the whole file
    let file = sam_file();
    ready(&mut rig, &file);

    rig.machine.handle(Command::OsSendUnknown);
    rig.wait_for("os_progress");
    rig.machine.handle(Command::OsCancel);
    let finished = rig.wait_for("os_finished");

    assert_eq!(finished.field("state"), Some(&"cancelled".into()));
    let sent = rig.transport().sent_bytes();
    assert!(
        !sent.is_empty() && sent.len() < file.len(),
        "{} of {}",
        sent.len(),
        file.len()
    );
    assert_eq!(
        sent,
        file[..sent.len()],
        "what was sent is a prefix of the file"
    );
    assert!(!rig.machine.os_active.load(Ordering::SeqCst));
}

#[test]
fn test_os_cancel_when_nothing_runs_returns_precondition_error() {
    let mut rig = TestRig::connected("os-cancel-none");

    rig.machine.handle(Command::OsCancel);

    assert_eq!(
        rig.expect("error").field("code"),
        Some(&"precondition".into())
    );
}

#[test]
fn test_other_commands_when_send_is_running_return_busy_and_send_nothing() {
    let mut rig = TestRig::connected("os-busy");
    rig.machine.rate = 20_000;
    ready(&mut rig, &sam_file());
    rig.machine.handle(Command::OsSendUnknown);
    rig.wait_for("os_started");
    let before = rig.transport().sent().len();

    rig.machine.handle(Command::FetchSound);
    rig.machine.handle(Command::Nrpn {
        number: 1,
        value: 1,
    });
    rig.machine.handle(Command::Disconnect);
    let busy = rig
        .events()
        .iter()
        .filter(|e| e.field("code") == Some(&"busy".into()))
        .count();
    rig.machine.handle(Command::OsCancel);
    rig.wait_for("os_finished");

    assert_eq!(busy, 3);
    assert!(!rig
        .transport()
        .sent()
        .iter()
        .any(|packet| *packet == REQUEST_SOUND));
    assert!(rig.transport().sent().len() >= before);
}

#[test]
fn test_cc_and_transport_when_send_is_running_return_busy_and_send_nothing() {
    let mut rig = TestRig::connected("os-busy-cc");
    rig.machine.rate = 20_000;
    ready(&mut rig, &sam_file());
    rig.machine.handle(Command::OsSendUnknown);
    rig.wait_for("os_started");

    rig.machine.handle(Command::Cc {
        controller: 12,
        value: 1,
    });
    rig.machine.handle(Command::Transport { start: true });
    rig.machine.handle(Command::Transport { start: false });
    let busy = rig
        .events()
        .iter()
        .filter(|e| e.field("code") == Some(&"busy".into()))
        .count();
    rig.machine.handle(Command::OsCancel);
    rig.wait_for("os_finished");

    assert_eq!(busy, 3);
    let own = [vec![0xB0, 12, 1], vec![0xFA], vec![0xFC]];
    assert!(!rig.transport().sent().iter().any(|p| own.contains(p)));
}

#[test]
fn test_os_send_when_port_fails_returns_failed_state() {
    let mut rig = TestRig::connected("os-fail");
    fast(&mut rig);
    ready(&mut rig, &sam_file());
    rig.transport().fail_sends();

    rig.machine.handle(Command::OsSendUnknown);

    let finished = rig.wait_for("os_finished");
    assert_eq!(finished.field("state"), Some(&"failed".into()));
    assert!(!rig.machine.os_active.load(Ordering::SeqCst));
}

#[test]
fn test_os_send_when_second_one_started_while_running_returns_busy() {
    let mut rig = TestRig::connected("os-twice");
    rig.machine.rate = 20_000;
    let file = sam_file();
    ready(&mut rig, &file);
    rig.machine.handle(Command::OsSendUnknown);
    rig.wait_for("os_started");
    ready_again(&mut rig, &file);

    rig.machine.handle(Command::OsSendUnknown);

    assert_eq!(rig.expect("error").field("code"), Some(&"busy".into()));
    rig.machine.handle(Command::OsCancel);
    rig.wait_for("os_finished");
}

/// Validates again while a send runs (allowed: it only prepares a file).
fn ready_again(rig: &mut TestRig, file: &[u8]) {
    rig.machine.handle(Command::OsValidate {
        name: "again.syx".into(),
        data: file.to_vec(),
    });
    std::thread::sleep(Duration::from_millis(1));
}

fn dsi_dir() -> Option<PathBuf> {
    std::env::var_os("TEMPESTER_DSI_SYX").map(PathBuf::from)
}

#[test]
#[ignore = "needs the real DSI files (TEMPESTER_DSI_SYX)"]
fn test_os_build_when_real_dsi_main_returns_the_built_bytes_over_the_wire_and_original_is_accepted()
{
    let dir = dsi_dir().expect("set TEMPESTER_DSI_SYX");
    let dsi = std::fs::read(dir.join("Tempest_Main_1.5.0.2.syx")).expect("DSI Main file");
    let expected = versions::build(versions::find_version("1.5.1.0").expect("version"), &dsi)
        .expect("builds")
        .syx;
    let mut rig = TestRig::connected("os-real");
    fast(&mut rig);

    rig.machine.handle(Command::OsBuild {
        version: "1.5.1.0".into(),
        data: dsi.clone(),
    });
    let ready = rig.expect("os_ready");
    assert_eq!(ready.field("known"), Some(&true.into()));
    // A built, hash-checked version needs no extra confirmation.
    rig.machine.handle(Command::OsSend);
    rig.wait_for("os_finished");

    assert_eq!(ready.field("size"), Some(&expected.len().into()));
    assert_eq!(rig.transport().sent_bytes(), expected);
    rig.machine.handle(Command::OsOriginal { data: dsi });
    assert_eq!(
        rig.expect("os_ready").field("label"),
        Some(&"DSI 1.5.0.2 (original)".into())
    );
}
