//! The reference hashes in the update flow: a build is kept only when it equals the recorded hashes, a file is
//! recognised or called unknown, an unknown build needs its own confirmation, and a Main send is remembered.
//! The DSI file is not in the repository, so the builds here run on a synthetic Main image and a synthetic version.

use super::*;
use crate::machine::testkit::TestRig;
use tempest_proto::recipe::{Edit, Op};
use tempest_proto::versions::{Changes, Kind, Patch, BASE};

const WORD: [u8; 4] = [1, 2, 3, 4];
const NEW: [u8; 4] = [9, 9, 9, 9];

fn original_image() -> Vec<u8> {
    let mut image = vec![0u8; OsId::Main.blocks() * 1024];
    image[0x100..0x104].copy_from_slice(&WORD);
    image
}

fn leak<T>(value: T) -> &'static T {
    Box::leak(Box::new(value))
}

/// A synthetic version over the synthetic original, and the DSI-style file of that original. `image_hash` and
/// `syx_hash` replace the honest reference hashes (a stale or tampered manifest).
fn synthetic(image_hash: Option<&str>, syx_hash: Option<&str>) -> (Version, Vec<u8>) {
    let original = original_image();
    let patch: &'static Patch = leak(Patch {
        name: "p",
        file: "patch/p.json",
        input_sha256: Box::leak(sha256_hex(&original).into_boxed_str()),
        edits: Box::leak(
            vec![Edit {
                address: BASE + 0x100,
                count: 1,
                op: Op::Word(u32::from_le_bytes(NEW)),
            }]
            .into_boxed_slice(),
        ),
    });
    let honest = versions::build_from_image(&[patch], &original).expect("synthetic build");
    let text = |given: Option<&str>, honest: &[u8]| -> &'static str {
        Box::leak(
            given
                .map_or_else(|| sha256_hex(honest), str::to_string)
                .into_boxed_str(),
        )
    };
    let version = Version {
        version: "0.0.0.1",
        name: "synthetic",
        kind: Kind::Release,
        date: "2026-01-01",
        summary: "",
        changes: Changes::default(),
        description: "",
        sha256_image: text(image_hash, &honest.image),
        sha256_image_written: text(None, versions::written_range(&honest.image)),
        sha256_syx: text(syx_hash, &honest.syx),
        patches: Box::leak(vec![patch].into_boxed_slice()),
    };
    let dsi = os_image::encode(OsId::Main, &original).expect("synthetic DSI file");
    (version, dsi)
}

fn sam_file() -> Vec<u8> {
    os_image::encode(OsId::Sam, &vec![7u8; OsId::Sam.blocks() * 1024]).expect("SAM file")
}

fn fast(rig: &mut TestRig) {
    rig.machine.rate = 100_000_000;
}

fn error_codes(rig: &mut TestRig) -> Vec<Option<Json>> {
    rig.events()
        .iter()
        .filter(|e| e.name == "error")
        .map(|e| e.field("code").cloned())
        .collect()
}

#[test]
fn test_build_ready_when_hashes_match_returns_known_file_sent_without_extra_confirmation() {
    let mut rig = TestRig::connected("ref-match");
    fast(&mut rig);
    let (version, dsi) = synthetic(None, None);

    rig.machine
        .guard_ready("os_build", |m| m.build_ready(&version, &dsi));
    let ready = rig.expect("os_ready");
    rig.machine.handle(Command::OsSend);
    rig.wait_for("os_finished");

    assert_eq!(ready.field("known"), Some(&true.into()));
    assert_eq!(ready.field("version"), Some(&"0.0.0.1".into()));
    assert_eq!(
        ready.field("sha256_image"),
        Some(&version.sha256_image.into())
    );
    assert_eq!(ready.field("sha256_syx"), Some(&version.sha256_syx.into()));
    assert_eq!(
        sha256_hex(&rig.transport().sent_bytes()),
        version.sha256_syx
    );
}

#[test]
fn test_build_ready_when_image_hash_is_altered_returns_error_and_nothing_sendable() {
    let mut rig = TestRig::connected("ref-image");
    let (version, dsi) = synthetic(Some(&"0".repeat(64)), None);
    rig.machine.handle(Command::OsValidate {
        name: "older.syx".into(),
        data: sam_file(),
    });
    rig.events();

    rig.machine
        .guard_ready("os_build", |m| m.build_ready(&version, &dsi));
    rig.machine.handle(Command::OsSendUnknown);

    let events = rig.events();
    assert!(events.iter().all(|e| e.name != "os_ready"));
    let refusal = events.iter().find(|e| e.name == "error").expect("refusal");
    assert!(
        refusal
            .render()
            .contains("does not match the reference of Tempester 0.0.0.1"),
        "{}",
        refusal.render()
    );
    assert!(
        rig.machine.pending_os.is_none(),
        "an older file stayed ready"
    );
    assert!(rig.transport().sent().is_empty());
}

#[test]
fn test_build_ready_when_syx_hash_is_altered_returns_error_and_nothing_ready() {
    let mut rig = TestRig::connected("ref-syx");
    let (version, dsi) = synthetic(None, Some(&"f".repeat(64)));

    rig.machine
        .guard_ready("os_build", |m| m.build_ready(&version, &dsi));

    assert_eq!(error_codes(&mut rig), vec![Some("invalid".into())]);
    assert!(rig.machine.pending_os.is_none());
}

#[test]
fn test_os_validate_when_file_matches_no_version_returns_unknown_and_send_needs_confirmation() {
    let mut rig = TestRig::connected("ref-unknown");
    fast(&mut rig);
    let file = sam_file();
    rig.machine.handle(Command::OsValidate {
        name: "mine.syx".into(),
        data: file.clone(),
    });
    let ready = rig.expect("os_ready");

    rig.machine.handle(Command::OsSend);

    assert_eq!(ready.field("known"), Some(&false.into()));
    assert_eq!(ready.field("label"), Some(&"mine.syx".into()));
    assert_eq!(error_codes(&mut rig), vec![Some("precondition".into())]);
    assert!(rig.machine.pending_os.is_some(), "the file stays ready");
    assert!(rig.transport().sent().is_empty(), "nothing went out");
    rig.machine.handle(Command::OsSendUnknown);
    rig.wait_for("os_finished");
    assert_eq!(rig.transport().sent_bytes(), file);
}

#[test]
fn test_identify_when_main_file_hash_is_in_the_table_returns_that_version() {
    let (version, dsi) = synthetic(None, None);
    let os = os_image::decode(&dsi).expect("decodes");
    let built = versions::build(&version, &dsi).expect("builds");
    let built_os = os_image::decode(&built.syx).expect("decodes");
    let table = [version];

    assert_eq!(
        identify(&built_os, &sha256_hex(&built.syx), &table),
        Origin::Reference("0.0.0.1")
    );
    assert_eq!(identify(&os, &sha256_hex(&dsi), &table), Origin::Unknown);
}

#[test]
fn test_identify_when_hash_matches_but_chip_is_not_main_returns_unknown() {
    let sam = sam_file();
    let os = os_image::decode(&sam).expect("decodes");
    let (mut version, _) = synthetic(None, None);
    version.sha256_syx = Box::leak(sha256_hex(&sam).into_boxed_str());

    assert_eq!(
        identify(&os, &sha256_hex(&sam), &[version]),
        Origin::Unknown
    );
}

#[test]
fn test_os_send_when_main_file_sent_to_the_end_returns_installed_record_of_its_image() {
    let mut rig = TestRig::connected("ref-installed");
    fast(&mut rig);
    let (_, dsi) = synthetic(None, None);
    rig.machine.handle(Command::OsValidate {
        name: "my main.syx".into(),
        data: dsi.clone(),
    });
    rig.events();

    rig.machine.handle(Command::OsSendUnknown);
    rig.wait_for("os_finished");

    let record = rig.machine.storage.installed_os().expect("record");
    assert_eq!(record.label, "my main.syx");
    assert_eq!(record.version, None);
    assert_eq!(record.verified_at, None);
    let image = os_image::decode(&dsi).expect("decodes").image;
    assert_eq!(record.sha256_image, sha256_hex(&image));
}

#[test]
fn test_os_send_when_other_chip_sent_returns_installed_record_untouched() {
    let mut rig = TestRig::connected("ref-other-chip");
    fast(&mut rig);
    let before = InstalledOs {
        label: "Tempester 1.6.1.1".into(),
        version: Some("1.6.1.1".into()),
        at: "2026-11-08_100000Z".into(),
        sha256_image: "c".repeat(64),
        sha256_image_written: Some("b".repeat(64)),
        verified_at: Some("2026-11-08_110000Z".into()),
    };
    rig.machine
        .storage
        .set_installed_os(Some(&before))
        .expect("stored");
    rig.machine.handle(Command::OsValidate {
        name: "sam.syx".into(),
        data: sam_file(),
    });

    rig.machine.handle(Command::OsSendUnknown);
    rig.wait_for("os_finished");

    assert_eq!(rig.machine.storage.installed_os(), Some(before));
}

#[test]
fn test_os_send_when_main_send_is_cancelled_half_way_returns_no_installed_record() {
    let mut rig = TestRig::connected("ref-cancel");
    rig.machine.rate = 400_000;
    let (_, dsi) = synthetic(None, None);
    let before = InstalledOs {
        label: "Tempester 1.6.1.0".into(),
        version: Some("1.6.1.0".into()),
        at: "2026-11-08_100000Z".into(),
        sha256_image: "d".repeat(64),
        sha256_image_written: Some("b".repeat(64)),
        verified_at: None,
    };
    rig.machine
        .storage
        .set_installed_os(Some(&before))
        .expect("stored");
    rig.machine.handle(Command::OsValidate {
        name: "main.syx".into(),
        data: dsi,
    });
    rig.events();

    rig.machine.handle(Command::OsSendUnknown);
    rig.wait_for("os_progress");
    rig.machine.handle(Command::OsCancel);
    let finished = rig.wait_for("os_finished");

    assert_eq!(finished.field("state"), Some(&"cancelled".into()));
    assert_eq!(
        rig.machine.storage.installed_os(),
        None,
        "the machine holds a half-written OS: no version may be claimed"
    );
}

#[test]
fn test_os_send_when_main_send_fails_returns_no_installed_record() {
    let mut rig = TestRig::connected("ref-fail");
    fast(&mut rig);
    let (_, dsi) = synthetic(None, None);
    rig.machine.handle(Command::OsValidate {
        name: "main.syx".into(),
        data: dsi,
    });
    rig.transport().fail_sends();

    rig.machine.handle(Command::OsSendUnknown);
    let finished = rig.wait_for("os_finished");

    assert_eq!(finished.field("state"), Some(&"failed".into()));
    assert_eq!(rig.machine.storage.installed_os(), None);
}
