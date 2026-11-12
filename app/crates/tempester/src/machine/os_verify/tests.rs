use super::*;
use crate::command::Command;
use crate::machine::fake_machine::{debug_send, debug_send_messages};
use crate::machine::testkit::TestRig;
use crate::storage::Folder;
use tempest_proto::identity::TempesterVersion;
use tempest_proto::os_image::{self, OsId};
use tempest_proto::sha256::sha256_hex;
use tempest_proto::versions::find_version;
use tempest_proto::versions::{written_range, BASE, FLASHER_PAGE, WRITTEN_LEN};

fn main_dump_image() -> Vec<u8> {
    let message = debug_send_messages().remove(0);
    os_image::decode(&message).expect("decodes").image
}

/// The SHA-256 of what the fake machine's Main dump holds below the flasher page.
fn main_dump_sha256() -> String {
    sha256_hex(written_range(&main_dump_image()))
}

/// A record whose full-image hash is never the dump's: only the written range may decide.
fn installed(sha256_image_written: String) -> InstalledOs {
    InstalledOs {
        label: "Tempester 1.6.1.1".into(),
        version: Some("1.6.1.1".into()),
        at: "2026-11-08_100000Z".into(),
        sha256_image: "f".repeat(64),
        sha256_image_written: Some(sha256_image_written),
        verified_at: None,
    }
}

fn dump_message(image: &[u8]) -> Vec<u8> {
    os_image::encode(OsId::Main, image).expect("encodes")
}

/// Words of the machine's own flasher page differ from the sent image's: what the real machine does.
fn with_other_flasher(mut image: Vec<u8>) -> Vec<u8> {
    for byte in &mut image[WRITTEN_LEN..] {
        *byte ^= 0x5A;
    }
    image
}

/// A connected machine that last installed `record`, with the check listening.
fn listening(label: &str, record: &InstalledOs) -> TestRig {
    let mut rig = TestRig::connected(label);
    rig.machine
        .storage
        .set_installed_os(Some(record))
        .expect("stored");
    rig.machine.handle(Command::OsVerifyStart);
    rig.events();
    rig
}

fn hear_debug_sends(rig: &mut TestRig) {
    debug_send(&rig.transport());
    rig.machine.poll_incoming();
}

fn backup_folders(rig: &TestRig) -> usize {
    let dir = rig.machine.storage.folder(Folder::Backups).expect("folder");
    std::fs::read_dir(dir).expect("readable").count()
}

#[test]
fn test_judge_when_image_hash_equals_expected_returns_match() {
    let valid = valid_verdict(&"a".repeat(64), "1");

    assert_eq!(
        judge(&"a".repeat(64), &valid),
        Outcome::Match {
            sha256: "a".repeat(64),
            flasher: "1".repeat(64)
        }
    );
}

fn valid_verdict(written: &str, flasher: &str) -> Verdict {
    Verdict::Valid {
        checksum: 1,
        sha256: "c".repeat(64),
        written_sha256: written.into(),
        flasher_sha256: flasher.repeat(64),
    }
}

#[test]
fn test_judge_when_image_hash_differs_returns_mismatch_with_the_found_hash() {
    let valid = valid_verdict(&"b".repeat(64), "1");

    assert_eq!(
        judge(&"a".repeat(64), &valid),
        Outcome::Mismatch {
            found: "b".repeat(64),
            flasher: "1".repeat(64)
        }
    );
}

#[test]
fn test_judge_when_only_the_full_image_hash_equals_expected_returns_mismatch() {
    // The full-image hash ("c") is the expected one, the written range is not: the full hash must not count.
    let valid = valid_verdict(&"b".repeat(64), "1");

    assert!(matches!(
        judge(&"c".repeat(64), &valid),
        Outcome::Mismatch { .. }
    ));
}

#[test]
fn test_judge_when_flasher_pages_differ_and_written_range_is_equal_returns_match() {
    let one = judge(&"a".repeat(64), &valid_verdict(&"a".repeat(64), "1"));
    let other = judge(&"a".repeat(64), &valid_verdict(&"a".repeat(64), "2"));

    assert!(matches!(one, Outcome::Match { .. }));
    assert!(matches!(other, Outcome::Match { .. }));
}

#[test]
fn test_judge_when_image_is_constant_returns_constant_even_if_hash_equals_expected() {
    let constant = Verdict::Constant {
        byte: 0xFF,
        sha256: "a".repeat(64),
    };

    assert!(matches!(
        judge(&"a".repeat(64), &constant),
        Outcome::Constant { byte: 0xFF, .. }
    ));
}

#[test]
fn test_judge_when_message_is_invalid_returns_incomplete() {
    let outcome = judge("x", &Verdict::Invalid("refused: checksum".into()));

    assert_eq!(outcome, Outcome::Incomplete("refused: checksum".into()));
    assert!(!outcome.is_final());
}

#[test]
fn test_verify_when_dump_equals_the_installed_image_returns_match_backup_and_verified_record() {
    let mut rig = listening("verify-match", &installed(main_dump_sha256()));

    hear_debug_sends(&mut rig);

    let events = rig.events();
    let result = events
        .iter()
        .find(|e| e.name == "os_verify")
        .expect("result");
    assert_eq!(result.field("state"), Some(&"match".into()));
    assert_eq!(result.field("version"), Some(&"1.6.1.1".into()));
    assert_eq!(result.field("found"), result.field("expected"));
    assert!(
        result.field("flasher").is_some(),
        "the flasher page is reported apart"
    );
    assert!(result.field("dir").is_some(), "the dump is kept");
    assert_eq!(backup_folders(&rig), 1);
    let record = rig.machine.storage.installed_os().expect("record");
    assert!(record.verified_at.is_some());
    assert!(events.iter().any(|e| e.name == "installed"));
    assert!(rig.machine.verify.is_none(), "one result ends the check");
}

#[test]
fn test_verify_when_dump_is_another_image_returns_mismatch_with_both_hashes_and_stays_unverified() {
    let other = "e".repeat(64);
    let mut rig = listening("verify-mismatch", &installed(other.clone()));

    hear_debug_sends(&mut rig);

    let result = rig.expect("os_verify");
    assert_eq!(result.field("state"), Some(&"mismatch".into()));
    assert_eq!(result.field("expected"), Some(&other.into()));
    assert_eq!(result.field("found"), Some(&main_dump_sha256().into()));
    assert_eq!(backup_folders(&rig), 1, "the dump is kept anyway");
    let record = rig.machine.storage.installed_os().expect("record");
    assert_eq!(record.verified_at, None);
}

#[test]
fn flasher_page_differs_returns_match_and_verified_record() {
    let mut image = main_dump_image();
    let own_flasher = sha256_hex(&image[WRITTEN_LEN..]);
    image = with_other_flasher(image);
    let mut rig = listening("verify-flasher", &installed(main_dump_sha256()));

    rig.transport().push_incoming(&dump_message(&image));
    rig.machine.poll_incoming();

    let result = rig.expect("os_verify");
    assert_eq!(result.field("state"), Some(&"match".into()));
    assert_ne!(
        result.field("flasher"),
        Some(&own_flasher.into()),
        "the flasher page reported is the dump's own"
    );
    let record = rig.machine.storage.installed_os().expect("record");
    assert!(record.verified_at.is_some());
}

#[test]
fn test_verify_when_one_word_below_the_flasher_page_differs_returns_mismatch_even_if_flasher_is_equal(
) {
    let mut image = main_dump_image();
    let last_written_word = (FLASHER_PAGE - BASE) as usize - 4;
    image[last_written_word] ^= 0x01;
    let mut rig = listening("verify-one-word", &installed(main_dump_sha256()));

    rig.transport().push_incoming(&dump_message(&image));
    rig.machine.poll_incoming();

    let result = rig.expect("os_verify");
    assert_eq!(result.field("state"), Some(&"mismatch".into()));
    assert_ne!(result.field("found"), result.field("expected"));
    let record = rig.machine.storage.installed_os().expect("record");
    assert_eq!(record.verified_at, None);
}

#[test]
fn test_verify_when_one_word_differs_and_flasher_differs_too_returns_mismatch() {
    let mut image = with_other_flasher(main_dump_image());
    image[0x100] ^= 0x01;
    let mut rig = listening("verify-both", &installed(main_dump_sha256()));

    rig.transport().push_incoming(&dump_message(&image));
    rig.machine.poll_incoming();

    assert_eq!(
        rig.expect("os_verify").field("state"),
        Some(&"mismatch".into())
    );
}

#[test]
fn test_verify_start_when_record_has_no_written_hash_returns_precondition_error() {
    let mut rig = TestRig::connected("verify-legacy");
    let legacy = InstalledOs {
        sha256_image_written: None,
        ..installed("a".repeat(64))
    };
    rig.machine
        .storage
        .set_installed_os(Some(&legacy))
        .expect("stored");

    rig.machine.handle(Command::OsVerifyStart);

    assert_eq!(
        rig.expect("error").field("code"),
        Some(&"precondition".into())
    );
    assert!(rig.machine.verify.is_none());
}

#[test]
fn test_verify_when_dump_is_a_constant_image_returns_constant_and_stays_unverified() {
    let constant = vec![0xFFu8; OsId::Main.blocks() * 1024];
    let message = os_image::encode(OsId::Main, &constant).expect("encodes");
    let mut rig = listening(
        "verify-constant",
        &installed(sha256_hex(written_range(&constant))),
    );

    rig.transport().push_incoming(&message);
    rig.machine.poll_incoming();

    let result = rig.expect("os_verify");
    assert_eq!(result.field("state"), Some(&"constant".into()));
    assert_eq!(
        rig.machine
            .storage
            .installed_os()
            .and_then(|r| r.verified_at),
        None,
        "a constant dump proves nothing, even when its hash is the expected one"
    );
    assert_eq!(backup_folders(&rig), 1);
}

#[test]
fn test_verify_when_dump_is_damaged_returns_incomplete_and_keeps_listening_without_saving() {
    let mut rig = listening("verify-incomplete", &installed(main_dump_sha256()));
    let mut message = debug_send_messages().remove(0);
    let last = message.len() - 3;
    message[last] ^= 0x01;

    rig.transport().push_incoming(&message);
    rig.machine.poll_incoming();

    let result = rig.expect("os_verify");
    assert_eq!(result.field("state"), Some(&"incomplete".into()));
    assert!(rig.machine.verify.is_some());
    assert_eq!(backup_folders(&rig), 0);
}

#[test]
fn test_verify_when_voice_and_panel_are_heard_returns_nothing_until_the_main_arrives() {
    let mut rig = listening("verify-other-chips", &installed(main_dump_sha256()));
    let voice_and_panel: Vec<Vec<u8>> = debug_send_messages().into_iter().skip(1).collect();

    for message in &voice_and_panel {
        rig.transport().push_incoming(message);
    }
    rig.machine.poll_incoming();

    assert!(rig.events().iter().all(|e| e.name != "os_verify"));
    assert!(rig.machine.verify.is_some());
}

#[test]
fn test_verify_start_when_nothing_was_installed_returns_precondition_error() {
    let mut rig = TestRig::connected("verify-none");

    rig.machine.handle(Command::OsVerifyStart);

    assert_eq!(
        rig.expect("error").field("code"),
        Some(&"precondition".into())
    );
    assert!(rig.machine.verify.is_none());
}

#[test]
fn test_verify_start_when_not_connected_returns_not_connected_error() {
    let mut rig = TestRig::new("verify-off");
    rig.machine
        .storage
        .set_installed_os(Some(&installed("a".repeat(64))))
        .expect("stored");

    rig.machine.handle(Command::OsVerifyStart);

    assert_eq!(
        rig.expect("error").field("code"),
        Some(&"not_connected".into())
    );
}

#[test]
fn test_verify_stop_when_listening_returns_idle_and_later_dumps_are_ignored() {
    let mut rig = listening("verify-stop", &installed(main_dump_sha256()));

    rig.machine.handle(Command::OsVerifyStop);
    hear_debug_sends(&mut rig);

    let events = rig.events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].field("state"), Some(&"idle".into()));
    assert_eq!(backup_folders(&rig), 0);
}

/// The app's record of an install of `version` (a known release), as a send from the app leaves it.
fn record_of(version: &str) -> InstalledOs {
    let v = find_version(version).expect("known version");
    InstalledOs {
        label: format!("Tempester {version}"),
        version: Some(version.into()),
        at: "2026-11-08_100000Z".into(),
        sha256_image: v.sha256_image.into(),
        sha256_image_written: Some(v.sha256_image_written.into()),
        verified_at: None,
    }
}

/// A connected machine that announces `announced`, whose record is `record`, with the check started.
fn listening_announced(label: &str, announced: [u16; 4], record: Option<&InstalledOs>) -> TestRig {
    let mut rig = TestRig::connected(label);
    if let Some(record) = record {
        rig.machine
            .storage
            .set_installed_os(Some(record))
            .expect("stored");
    }
    rig.machine.announced = Some(TempesterVersion(announced));
    rig.machine.handle(Command::OsVerifyStart);
    rig
}

#[test]
fn test_verify_start_when_record_and_report_differ_returns_listening_on_the_record_first() {
    let mut rig = listening_announced("verify-two", [1, 6, 5, 0], Some(&record_of("1.6.4.0")));

    let listening = rig.expect("os_verify");

    assert_eq!(listening.field("state"), Some(&"listening".into()));
    assert_eq!(listening.field("version"), Some(&"1.6.4.0".into()));
    assert_eq!(listening.field("source"), Some(&"sent".into()));
    assert_eq!(
        rig.machine.verify.as_ref().map(|w| w.candidates.len()),
        Some(2)
    );
}

#[test]
fn test_verify_when_a_test_build_announces_its_base_and_the_dump_is_what_was_sent_returns_match_and_verified(
) {
    // A test build keeps the digits of its base: sent 1.6.5.0-j, the machine announces 1.6.4.0. The dump is the proof.
    let sent = InstalledOs {
        label: "Tempester 1.6.5.0-j (test build)".into(),
        version: None,
        sha256_image: "f".repeat(64),
        sha256_image_written: Some(main_dump_sha256()),
        ..record_of("1.6.4.0")
    };
    let mut rig = listening_announced("verify-test-build", [1, 6, 4, 0], Some(&sent));
    rig.events();

    hear_debug_sends(&mut rig);

    let result = rig.expect("os_verify");
    assert_eq!(result.field("state"), Some(&"match".into()));
    assert_eq!(result.field("source"), Some(&"sent".into()));
    assert_eq!(
        result.field("label"),
        Some(&"Tempester 1.6.5.0-j (test build)".into())
    );
    let record = rig.machine.storage.installed_os().expect("record");
    assert!(record.verified_at.is_some(), "the sent OS is proved");
}

#[test]
fn test_verify_when_the_dump_equals_neither_the_record_nor_the_report_returns_mismatch_naming_both()
{
    let mut rig = listening_announced("verify-neither", [1, 6, 5, 0], Some(&record_of("1.6.4.0")));
    rig.events();

    hear_debug_sends(&mut rig);

    let result = rig.expect("os_verify");
    assert_eq!(result.field("state"), Some(&"mismatch".into()));
    assert_eq!(result.field("label"), Some(&"Tempester 1.6.4.0".into()));
    assert_eq!(result.field("also"), Some(&"Tempester 1.6.5.0".into()));
    assert_eq!(
        rig.machine.storage.installed_os(),
        Some(record_of("1.6.4.0")),
        "nothing is proved"
    );
}

#[test]
fn test_verify_when_the_dump_equals_the_report_only_returns_match_and_keeps_the_record_unverified()
{
    let mut rig = TestRig::connected("verify-report-only");
    let record = record_of("1.6.4.0");
    rig.machine
        .storage
        .set_installed_os(Some(&record))
        .expect("stored");
    // The fake's dump stands for the reported release: the real 1.6.5.0 image is not in the tests (it is built
    // from DSI's file), so its candidate takes the dump's hash.
    let mut candidates = verify_candidates(Some("1.6.5.0"), Some(record.clone()));
    candidates[1].written = main_dump_sha256();
    rig.machine.listen_for_dump(candidates).expect("listening");
    rig.events();

    hear_debug_sends(&mut rig);

    let result = rig.expect("os_verify");
    assert_eq!(result.field("state"), Some(&"match".into()));
    assert_eq!(result.field("source"), Some(&"reported".into()));
    assert_eq!(result.field("label"), Some(&"Tempester 1.6.5.0".into()));
    assert_eq!(backup_folders(&rig), 1);
    assert_eq!(
        rig.machine.storage.installed_os(),
        Some(record),
        "the record of the last send is neither replaced nor marked verified"
    );
}

#[test]
fn test_verify_start_when_machine_reports_a_known_version_and_nothing_was_sent_returns_listening() {
    let mut rig = listening_announced("verify-reported-only", [1, 6, 5, 0], None);

    let listening = rig.expect("os_verify");

    assert_eq!(listening.field("state"), Some(&"listening".into()));
    assert_eq!(listening.field("source"), Some(&"reported".into()));
}

#[test]
fn test_verify_start_when_old_record_lacks_its_written_hash_but_is_the_reported_image_returns_listening(
) {
    let old = InstalledOs {
        sha256_image_written: None,
        ..record_of("1.6.5.0")
    };
    let mut rig = listening_announced("verify-old-record", [1, 6, 5, 0], Some(&old));

    let listening = rig.expect("os_verify");

    assert_eq!(listening.field("state"), Some(&"listening".into()));
    let written = find_version("1.6.5.0").expect("known").sha256_image_written;
    assert_eq!(listening.field("expected"), Some(&written.into()));
}

#[test]
fn test_verify_start_when_nothing_can_be_compared_returns_precondition_error() {
    let mut rig = listening_announced("verify-unknown-report", [9, 9, 9, 9], None);

    assert_eq!(
        rig.expect("error").field("code"),
        Some(&"precondition".into())
    );
    assert!(rig.machine.verify.is_none());
}

#[test]
fn test_verify_when_the_port_closes_while_listening_returns_no_pending_check() {
    let mut rig = listening("verify-close", &installed(main_dump_sha256()));
    assert!(rig.machine.verify.is_some());

    rig.machine.handle(Command::Disconnect);

    assert!(rig.machine.verify.is_none());
}
