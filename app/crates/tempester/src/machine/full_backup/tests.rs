use super::*;
use crate::machine::fake_machine::{debug_send, debug_send_messages};
use crate::machine::testkit::TestRig;
use crate::storage::Folder;
use tempest_proto::full_backup::{synthetic, PAD_CHANNELS};
use tempest_proto::identity::TempesterVersion;

const STAMP: &str = "2026-11-13_100000Z";

/// The machine answers the request `44` with `messages`.
fn answering(rig: &TestRig, messages: Vec<Vec<u8>>) {
    rig.transport().set_responder(Box::new(move |sent| {
        if sent == REQUEST_FULL_BACKUP {
            messages.clone()
        } else {
            Vec::new()
        }
    }));
}

fn started(label: &str, messages: Vec<Vec<u8>>) -> TestRig {
    let mut rig = TestRig::connected(label);
    answering(&rig, messages);
    rig.machine.handle(Command::FullBackupStart);
    rig.machine.poll_incoming();
    rig
}

fn last_state(rig: &mut TestRig) -> Event {
    rig.events()
        .into_iter()
        .rev()
        .find(|e| e.name == "full_backup")
        .expect("a full_backup event")
}

fn rows(event: &Event) -> Vec<(String, String)> {
    let Some(Json::Arr(rows)) = event.field("rows") else {
        panic!("no rows in {event:?}");
    };
    rows.iter()
        .map(|row| {
            let text = row.render();
            let get = |key: &str| {
                let tag = format!("\"{key}\":\"");
                let from = text.find(&tag).expect("the key") + tag.len();
                text[from..from + text[from..].find('"').expect("closing quote")].to_string()
            };
            (get("chip"), get("verdict"))
        })
        .collect()
}

fn backup_dir(rig: &TestRig) -> std::path::PathBuf {
    rig.machine
        .storage
        .folder(Folder::Backups)
        .expect("backups folder")
        .join(format!("Full backup {STAMP}"))
}

fn no_backup_folder(rig: &TestRig) -> bool {
    let backups = rig.machine.storage.folder(Folder::Backups).expect("folder");
    std::fs::read_dir(backups).expect("readable").count() == 0
}

#[test]
fn test_start_when_machine_answers_returns_ready_with_eight_ok_rows_and_only_the_request_sent() {
    let mut rig = TestRig::connected("full-ready");

    rig.machine.handle(Command::FullBackupStart);
    rig.machine.poll_incoming();

    let event = last_state(&mut rig);
    assert_eq!(event.field("state"), Some(&"ready".into()));
    let rows = rows(&event);
    assert_eq!(rows.len(), 8);
    assert!(rows.iter().all(|(_, verdict)| verdict == "ok"), "{rows:?}");
    assert_eq!(rig.transport().sent(), vec![REQUEST_FULL_BACKUP.to_vec()]);
}

#[test]
fn test_save_when_ready_returns_the_files_as_received_a_manifest_and_a_report() {
    let mut rig = started("full-save", synthetic::answer());
    rig.events();

    rig.machine.full_save(STAMP).expect("saved");

    let dir = backup_dir(&rig);
    for (chip, message) in Chip::all().into_iter().zip(synthetic::answer()) {
        let name = format!("{}.syx", chip.label());
        assert_eq!(
            std::fs::read(dir.join(&name)).expect("file"),
            message,
            "{name}"
        );
    }
    let manifest = std::fs::read_to_string(dir.join("manifest.json")).expect("manifest");
    let voice0 = sha256_hex(&synthetic::answer()[0]);
    assert!(manifest.contains(&voice0), "the SHA-256 of Voice 0.syx");
    assert!(manifest.contains(r#""kind":"full_backup""#));
    assert!(manifest.contains("never send these files"), "{manifest}");
    let words = synthetic::calibration_words(2);
    assert!(manifest.contains(&format!("{:04x}", words[0])));
    let report = std::fs::read_to_string(dir.join("report.txt")).expect("report");
    assert!(report.contains("Voice 5 (id 0x7d)"), "{report}");
    assert!(report.contains("Never send them to the machine"));
    let scalars: Vec<String> = synthetic::calibration_words(4)[32..]
        .iter()
        .map(|w| format!("{w:04x}"))
        .collect();
    assert!(
        report.contains(&format!("  scalars: {}\n", scalars.join(" "))),
        "each Voice's calibration in the report: {report}"
    );
    assert!(rig.expect("full_backup_saved").field("dir").is_some());
    assert!(rig.machine.full_backup.is_none());
}

#[test]
fn test_save_when_debug_menu_dumps_were_heard_returns_them_in_the_same_folder() {
    let mut rig = started("full-extras", synthetic::answer());
    debug_send(&rig.transport());
    rig.machine.poll_incoming();
    rig.events();

    rig.machine.full_save(STAMP).expect("saved");

    let mut names: Vec<String> = std::fs::read_dir(backup_dir(&rig))
        .expect("folder")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    for wanted in [
        "Main.syx",
        "Panel.syx",
        "Voice (Debug menu - voice 0) (constant 0xff - suspect).syx",
        "Voice 0.syx",
    ] {
        assert!(names.iter().any(|n| n == wanted), "{wanted} in {names:?}");
    }
    assert_eq!(
        std::fs::read(backup_dir(&rig).join("Main.syx")).expect("Main"),
        debug_send_messages()[0]
    );
}

#[test]
fn test_start_when_a_misaligned_dump_arrives_returns_refused_with_the_power_cycle_advice() {
    let mut image = synthetic::voice_image(2);
    image.rotate_left(1);
    let mut messages = synthetic::answer();
    messages[2] = synthetic::message(Chip::Voice(2), &image);
    let mut rig = started("full-misaligned", messages);

    let event = last_state(&mut rig);

    assert_eq!(event.field("state"), Some(&"refused".into()));
    assert!(rows(&event).contains(&("Voice 2".into(), "refused".into())));
    assert!(event.render().contains("power the machine off and on"));
    assert!(rig.machine.full_save(STAMP).is_err());
    assert!(
        no_backup_folder(&rig),
        "nothing is written from a refused set"
    );
}

#[test]
fn test_start_when_1_6_8_0_sends_empty_voice_messages_returns_not_read_with_the_power_cycle_advice()
{
    let mut messages = synthetic::answer();
    for k in [0u8, 3] {
        messages[usize::from(k)] = vec![0xF0, 0x01, 0x28, 0x78 + k, 0xF7];
    }
    let mut rig = started("full-not-read", messages);

    let event = last_state(&mut rig);

    assert_eq!(event.field("state"), Some(&"refused".into()));
    let rows = rows(&event);
    assert!(
        rows.contains(&("Voice 0".into(), "refused".into())),
        "{rows:?}"
    );
    assert!(
        rows.contains(&("Voice 3".into(), "refused".into())),
        "{rows:?}"
    );
    assert!(rows.contains(&("Voice 1".into(), "ok".into())), "{rows:?}");
    let text = event.render();
    assert!(text.contains("Voice 0 was not read"), "{text}");
    assert!(text.contains("Voice 3 was not read"), "{text}");
    assert!(text.contains("power the machine off and on"), "{text}");
    assert!(rig.machine.full_save(STAMP).is_err());
    assert!(
        no_backup_folder(&rig),
        "nothing is written from a set with a chip not read"
    );
}

#[test]
fn test_start_when_a_chip_never_answers_returns_missing_after_the_wait_and_refuses_to_save() {
    let mut messages = synthetic::answer();
    messages.remove(3);
    let mut rig = started("full-missing", messages);
    rig.events();

    rig.machine
        .expire_full_backup(Instant::now() + Duration::from_secs(1000));

    let event = last_state(&mut rig);
    assert_eq!(event.field("state"), Some(&"refused".into()));
    assert!(rows(&event).contains(&("Voice 3".into(), "missing".into())));
    assert!(event.render().contains("Voice 3 did not come"));
    assert_eq!(
        rig.machine.full_save(STAMP),
        Err(AppError::Precondition(
            "nothing to save: the backup is incomplete or a message was refused"
        ))
    );
    assert!(no_backup_folder(&rig));
}

#[test]
fn test_start_when_the_wait_is_not_over_returns_receiving_and_no_verdict_yet() {
    let mut messages = synthetic::answer();
    messages.truncate(4);
    let mut rig = started("full-partial", messages);

    rig.machine
        .expire_full_backup(Instant::now() + Duration::from_secs(10));

    assert_eq!(
        last_state(&mut rig).field("state"),
        Some(&"receiving".into())
    );
}

#[test]
fn test_start_when_a_block_is_corrupt_returns_refused_naming_the_checksum() {
    let mut messages = synthetic::answer();
    messages[1][4 + 1171 * 9 + 5] ^= 0x01;
    let mut rig = started("full-corrupt", messages);

    let event = last_state(&mut rig);

    assert_eq!(event.field("state"), Some(&"refused".into()));
    assert!(rows(&event).contains(&("Voice 1".into(), "refused".into())));
    assert!(event.render().contains("checksum"));
    assert!(rig.machine.full_save(STAMP).is_err());
}

#[test]
fn test_start_when_a_calibration_page_is_blank_returns_refused_naming_the_calibration() {
    let mut image = synthetic::voice_image(4);
    let blank_from = image.len() - 1024;
    image[blank_from..].fill(0xFF);
    image[0xFC00..].fill(0xFF);
    let mut messages = synthetic::answer();
    messages[4] = synthetic::message(Chip::Voice(4), &image);
    let mut rig = started("full-blank", messages);

    let event = last_state(&mut rig);

    assert_eq!(event.field("state"), Some(&"refused".into()));
    assert!(event.render().contains("calibration page: blank"));
    assert!(rig.machine.full_save(STAMP).is_err());
    assert!(no_backup_folder(&rig));
}

#[test]
fn test_start_when_every_chip_answers_with_voice_0_returns_the_copies_refused() {
    let first = synthetic::voice_image(0);
    let mut messages = synthetic::answer();
    for k in 1..6u8 {
        messages[usize::from(k)] = synthetic::message(Chip::Voice(k), &first);
    }
    let mut rig = started("full-same", messages);

    let event = last_state(&mut rig);

    assert_eq!(event.field("state"), Some(&"refused".into()));
    assert!(event.render().contains("identical to Voice 0"));
    assert!(rig.machine.full_save(STAMP).is_err());
}

#[test]
fn test_start_when_the_machine_answered_as_the_dsi_os_returns_a_precondition_error_and_sends_nothing(
) {
    let mut rig = TestRig::connected("full-dsi");
    rig.machine.dsi_identity = true;

    rig.machine.handle(Command::FullBackupStart);

    assert_eq!(
        rig.expect("error").field("code"),
        Some(&"precondition".into())
    );
    assert!(rig.transport().sent().is_empty());
    assert!(rig.machine.full_backup.is_none());
}

#[test]
fn test_start_when_not_connected_returns_not_connected() {
    let mut rig = TestRig::new("full-off");

    rig.machine.handle(Command::FullBackupStart);

    assert_eq!(
        rig.expect("error").field("code"),
        Some(&"not_connected".into())
    );
}

#[test]
fn test_cancel_when_a_backup_is_open_returns_idle_and_later_messages_are_ignored() {
    let mut rig = started("full-cancel", synthetic::answer());
    rig.events();

    rig.machine.handle(Command::FullBackupCancel);
    rig.transport().push_incoming(&synthetic::answer()[0]);
    rig.machine.poll_incoming();

    assert_eq!(last_state(&mut rig).field("state"), Some(&"idle".into()));
    assert!(rig.machine.full_backup.is_none());
}

#[test]
fn test_message_when_it_is_not_a_backup_or_debug_message_returns_nothing_published() {
    let mut rig = started("full-foreign", Vec::new());
    rig.events();

    rig.transport()
        .push_incoming(&[0xF0, 0x7E, 0x7F, 0x06, 0x02, 0xF7]);
    rig.machine.poll_incoming();

    assert!(rig.events().iter().all(|e| e.name != "full_backup"));
}

#[test]
fn test_save_when_a_second_request_replaces_a_refused_set_returns_the_new_set_only() {
    let mut bad = synthetic::answer();
    bad[0][4 + 100] ^= 0x01;
    let mut rig = started("full-retry", bad);
    answering(&rig, synthetic::answer());

    rig.machine.handle(Command::FullBackupStart);
    rig.machine.poll_incoming();

    assert_eq!(last_state(&mut rig).field("state"), Some(&"ready".into()));
    assert!(rig.machine.full_save(STAMP).is_ok());
}
/// Pad k's channel value k + 1 (1-32).
fn every_pad_channel() -> [u8; PAD_CHANNELS] {
    std::array::from_fn(|k| k as u8 + 1)
}

#[test]
fn test_save_when_ready_returns_the_pad_channels_and_the_system_file_from_76_without_any_pad_read()
{
    let channels = every_pad_channel();
    let answer = synthetic::answer_with(&channels);
    let mut rig = started("full-pads", answer.clone());
    rig.events();

    rig.machine.full_save(STAMP).expect("saved");

    let dir = backup_dir(&rig);
    assert_eq!(
        rig.transport().sent(),
        vec![REQUEST_FULL_BACKUP.to_vec()],
        "the pad channels come from 76, no pad was read"
    );
    let listed: Vec<String> = channels.iter().map(u8::to_string).collect();
    let file = std::fs::read_to_string(dir.join("Pad channels.json")).expect("the table");
    assert!(file.contains(&format!("[{}]", listed.join(","))), "{file}");
    let manifest = std::fs::read_to_string(dir.join("manifest.json")).expect("manifest");
    assert!(manifest.contains(r#""pad_channels":[1,2,3,"#), "{manifest}");
    let Ok(Backup {
        content: Content::System(system),
        ..
    }) = full_backup::decode(&answer[7])
    else {
        panic!("76 decodes");
    };
    let rebuilt = std::fs::read(dir.join("System settings file.bin")).expect("the file");
    assert_eq!(rebuilt, system.machine_file());
    assert_eq!(rebuilt.len(), 406);
    assert!(
        manifest.contains(&sha256_hex(&rebuilt)),
        "the rebuilt file is listed with its SHA-256: {manifest}"
    );
    assert_eq!(
        rebuilt[2 * 49 + 1],
        1,
        "pad 1's channel in setting 48's high byte"
    );
    let report = std::fs::read_to_string(dir.join("report.txt")).expect("report");
    assert!(report.contains("Pad channels.json"), "{report}");
}

#[test]
fn test_start_when_76_has_only_the_settings_returns_system_refused_and_nothing_saved() {
    // The study's guess: 76 without the 32 pad channels (version byte + 202 settings, 203 bytes).
    let mut messages = synthetic::answer();
    let payload = synthetic::system_payload(&synthetic::STOCK_ROUTE);
    messages[7] = synthetic::message(Chip::System, &payload[..203]);
    let mut rig = started("full-203", messages);

    let event = last_state(&mut rig);

    assert_eq!(event.field("state"), Some(&"refused".into()));
    assert!(rows(&event).contains(&("System settings".into(), "refused".into())));
    assert!(
        event.render().contains("32 pad channels"),
        "{}",
        event.render()
    );
    assert!(rig.machine.full_save(STAMP).is_err());
    assert!(no_backup_folder(&rig));
}

#[test]
fn test_start_when_the_messages_come_out_of_order_returns_the_misplaced_ones_refused() {
    let mut messages = synthetic::answer();
    messages.swap(6, 7);
    let mut rig = started("full-order", messages);

    let event = last_state(&mut rig);

    assert_eq!(event.field("state"), Some(&"refused".into()));
    let rows = rows(&event);
    assert!(rows.contains(&("Main boot flash".into(), "refused".into())));
    assert!(rows.contains(&("System settings".into(), "refused".into())));
    assert!(event.render().contains("not the answer of request 44"));
    assert!(rig.machine.full_save(STAMP).is_err());
    assert!(no_backup_folder(&rig));
}

#[test]
fn test_start_when_a_chip_comes_twice_returns_refused_not_replaced() {
    let mut messages = synthetic::answer();
    messages.insert(1, messages[0].clone());
    let mut rig = started("full-twice", messages);

    let event = last_state(&mut rig);

    assert_eq!(event.field("state"), Some(&"refused".into()));
    assert!(rows(&event).contains(&("Voice 0".into(), "refused".into())));
    assert!(rig.machine.full_save(STAMP).is_err());
}

#[test]
fn test_start_when_the_machine_announced_an_os_without_the_backup_returns_a_precondition_error() {
    // An OS the app knows decides, even on the fake machine (an unknown number is refused on a real machine only:
    // `storage::installed` tests).
    for version in [[1, 6, 4, 0], [1, 6, 5, 0]] {
        let mut rig = TestRig::connected("full-old");
        rig.machine.announced = Some(TempesterVersion(version));

        rig.machine.handle(Command::FullBackupStart);

        assert_eq!(
            rig.expect("error").field("code"),
            Some(&"precondition".into()),
            "{version:?}: refused, on the fake machine too"
        );
        assert!(rig.transport().sent().is_empty());
    }
}

#[test]
fn test_start_when_the_machine_announced_1_6_6_0_returns_ready() {
    let mut rig = TestRig::connected("full-1660");
    rig.machine.announced = Some(TempesterVersion([1, 6, 6, 0]));

    rig.machine.handle(Command::FullBackupStart);
    rig.machine.poll_incoming();

    assert_eq!(last_state(&mut rig).field("state"), Some(&"ready".into()));
}

type AllowedFn = fn(bool, bool) -> bool;

/// The rule of a second full backup, every case: an OS whose n°62 never reads forward (1.6.8.0 on) always; an older one
/// only when no backup was requested since the user said the machine was powered off and on.
fn judge_second_backup(allowed: AllowedFn) -> Result<(), String> {
    for (repeat, read, expected) in [
        (true, false, true),
        (true, true, true),
        (false, false, true),
        (false, true, false),
    ] {
        if allowed(repeat, read) != expected {
            return Err(format!("repeat {repeat}, read {read}: expected {expected}"));
        }
    }
    Ok(())
}

#[test]
fn test_second_backup_allowed_when_every_case_returns_the_rule() {
    assert_eq!(judge_second_backup(second_backup_allowed), Ok(()));
}

#[test]
fn test_judge_second_backup_when_given_deliberately_wrong_rules_returns_the_case_it_gets_wrong() {
    let always: AllowedFn = |_, _| true;
    let never_twice: AllowedFn = |_, read| !read;
    let repeat_ignored: AllowedFn = |repeat, _| repeat;
    for (name, wrong) in [
        ("always", always),
        ("never twice, even on 1.6.8.0", never_twice),
        ("the first one refused on old OSes", repeat_ignored),
    ] {
        assert!(judge_second_backup(wrong).is_err(), "{name} was accepted");
    }
}

fn error_texts(rig: &mut TestRig) -> Vec<String> {
    rig.events()
        .iter()
        .filter(|e| e.name == "error")
        .map(Event::render)
        .collect()
}

#[test]
fn test_start_when_1_6_7_0_already_backed_up_in_this_power_on_returns_the_power_cycle_refusal_until_confirmed(
) {
    let mut rig = TestRig::connected("full-1670-twice");
    rig.machine.announced = Some(TempesterVersion([1, 6, 7, 0]));
    answering(&rig, synthetic::answer());
    rig.machine.handle(Command::FullBackupStart);
    rig.machine.poll_incoming();
    assert!(rig.machine.full_save(STAMP).is_ok());
    rig.events();

    rig.machine.handle(Command::FullBackupStart);

    let errors = error_texts(&mut rig);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].contains("One full backup per power-on"),
        "{errors:?}"
    );
    assert!(
        errors[0].contains("Power the machine off and on"),
        "{errors:?}"
    );
    assert_eq!(
        rig.transport().sent(),
        vec![REQUEST_FULL_BACKUP.to_vec()],
        "the second request never reaches the machine"
    );

    rig.machine.handle(Command::FullBackupPowerCycled);
    rig.machine.handle(Command::FullBackupStart);
    rig.machine.poll_incoming();

    assert!(error_texts(&mut rig).is_empty());
    assert_eq!(
        rig.transport().sent().len(),
        2,
        "allowed once the power cycle is confirmed"
    );
}

#[test]
fn test_start_when_1_6_8_0_asked_twice_in_a_power_on_returns_both_backups() {
    let mut rig = TestRig::connected("full-1680-twice");
    rig.machine.announced = Some(TempesterVersion([1, 6, 8, 0]));
    answering(&rig, synthetic::answer());
    for _ in 0..2 {
        rig.machine.handle(Command::FullBackupStart);
        rig.machine.poll_incoming();
        assert_eq!(last_state(&mut rig).field("state"), Some(&"ready".into()));
        rig.machine.handle(Command::FullBackupCancel);
    }
    assert!(error_texts(&mut rig).is_empty());
    assert_eq!(rig.transport().sent().len(), 2);
}

#[test]
fn test_disconnect_when_a_backup_was_made_on_1_6_6_0_keeps_it_counted_for_this_power_on() {
    // A port that closes and opens again may only be a cable moved: it is not taken for a power cycle.
    let mut rig = TestRig::connected("full-1660-reconnect");
    rig.machine.announced = Some(TempesterVersion([1, 6, 6, 0]));
    answering(&rig, synthetic::answer());
    rig.machine.handle(Command::FullBackupStart);
    rig.machine.poll_incoming();
    rig.machine.handle(Command::FullBackupCancel);
    rig.machine.handle(Command::Disconnect);
    assert!(rig.machine.full_backup_read, "kept across a port change");
}

#[test]
fn test_request_when_a_backup_is_running_returns_busy_and_sends_nothing() {
    let mut messages = synthetic::answer();
    messages.truncate(3);
    let mut rig = started("full-busy", messages);
    rig.events();

    rig.machine.handle(Command::FullBackupStart);
    rig.machine.handle(Command::FetchSound);

    let codes: Vec<Json> = rig
        .events()
        .iter()
        .filter(|e| e.name == "error")
        .filter_map(|e| e.field("code").cloned())
        .collect();
    assert_eq!(codes, vec![Json::from("busy"), Json::from("busy")]);
    assert_eq!(rig.transport().sent(), vec![REQUEST_FULL_BACKUP.to_vec()]);
    // Counter-proof: once the wait is over the session is free again.
    rig.machine
        .expire_full_backup(Instant::now() + Duration::from_secs(1000));
    assert!(rig.machine.idle_session().is_ok());
}

#[test]
fn test_waits_when_compared_with_the_din_pace_cover_twice_the_slowest_message() {
    // midi-protocol.md § Full backup: a Voice message is 74,951 bytes at 31,250 baud (10 bits a byte), plus up to
    // 1.6 s to skip to the start of the chip.
    let slowest = Duration::from_millis(74_951 * 10 * 1000 / 31_250 + 1600);

    assert!(GAP_WAIT >= 2 * slowest, "{GAP_WAIT:?} vs {slowest:?}");
    assert!(FIRST_WAIT >= 3 * slowest, "{FIRST_WAIT:?} vs {slowest:?}");
}
// ---- What a Voice row says of the chip's OS (voice_checks) ----

use crate::storage::voice_state::VoiceInstalled;
use tempest_proto::full_backup::{calibration_page, code_pages, PAGE_BYTES};

fn sent_record(image: &[u8]) -> VoiceInstalled {
    VoiceInstalled {
        version: "1.5.1.0".into(),
        at: "2026-11-14_100000Z".into(),
        pages_sha256: sha256_hex(code_pages(image)),
    }
}

/// The eight messages of a healthy machine whose Voice `chip` was changed by `change`.
fn answer_changing(chip: u8, change: impl Fn(&mut Vec<u8>)) -> Vec<Vec<u8>> {
    Chip::all()
        .into_iter()
        .map(|c| match c {
            Chip::Voice(k) => {
                let mut image = synthetic::voice_image(k);
                if k == chip {
                    change(&mut image);
                }
                synthetic::message(c, &image)
            }
            Chip::MainBoot => synthetic::message(c, &synthetic::boot_flash_image()),
            Chip::System => {
                synthetic::message(c, &synthetic::system_payload(&synthetic::STOCK_ROUTE))
            }
        })
        .collect()
}

fn voice_details(event: &Event) -> Vec<String> {
    let Some(Json::Arr(rows)) = event.field("rows") else {
        panic!("no rows in {event:?}");
    };
    rows.iter()
        .map(Json::render)
        .filter(|row| row.contains(r#""chip":"Voice "#))
        .collect()
}

#[test]
fn test_start_when_the_chips_hold_the_update_sent_returns_rows_that_say_so_and_unknown_build() {
    let mut rig = TestRig::connected("full-voice-sent");
    let image = synthetic::voice_image(0);
    rig.machine
        .storage
        .set_voice_installed(Some(&sent_record(&image)))
        .expect("stored");
    answering(&rig, synthetic::answer());

    rig.machine.handle(Command::FullBackupStart);
    rig.machine.poll_incoming();

    let details = voice_details(&last_state(&mut rig));
    assert_eq!(details.len(), 6);
    for row in &details {
        assert!(
            row.contains("pages 0-40 equal the Tempester Voice 1.5.1.0 update sent"),
            "{row}"
        );
        assert!(
            row.contains("Voice OS: unknown build"),
            "synthetic pages are in no table: {row}"
        );
        assert!(row.contains(r#""verdict":"ok""#), "{row}");
    }
}

#[test]
fn test_start_when_one_chip_kept_other_code_returns_that_row_saying_the_update_did_not_reach_it() {
    let mut rig = TestRig::connected("full-voice-stale");
    let reference = synthetic::voice_image(0);
    rig.machine
        .storage
        .set_voice_installed(Some(&sent_record(&reference)))
        .expect("stored");
    answering(&rig, answer_changing(3, |image| image[5000] ^= 0x55));

    rig.machine.handle(Command::FullBackupStart);
    rig.machine.poll_incoming();

    let details = voice_details(&last_state(&mut rig));
    for (k, row) in details.iter().enumerate() {
        let differs = row.contains("DIFFER from the Tempester Voice 1.5.1.0 update sent");
        assert_eq!(differs, k == 3, "Voice {k}: {row}");
    }
    assert!(details[3].contains("did not reach this chip"));
}

#[test]
fn test_save_when_no_update_was_sent_returns_unknown_build_in_the_manifest_and_the_report() {
    let mut rig = started("full-voice-none", synthetic::answer());
    rig.events();

    rig.machine.full_save(STAMP).expect("saved");

    let dir = backup_dir(&rig);
    let manifest = std::fs::read_to_string(dir.join("manifest.json")).expect("manifest");
    assert_eq!(
        manifest.matches(r#""voice_os":"unknown build""#).count(),
        6,
        "{manifest}"
    );
    assert!(manifest.contains(r#""equals_update_sent":null"#));
    let pages = sha256_hex(code_pages(&synthetic::voice_image(0)));
    assert!(manifest.contains(&format!(r#""pages_0_40_sha256":"{pages}""#)));
    let report = std::fs::read_to_string(dir.join("report.txt")).expect("report");
    assert!(report.contains("Voice OS: unknown build"), "{report}");
    assert!(report.contains("no previous full backup to compare the calibration page with"));
}

#[test]
fn test_save_when_the_update_was_sent_returns_the_manifest_with_equals_update_sent_true() {
    let mut rig = TestRig::connected("full-voice-manifest");
    rig.machine
        .storage
        .set_voice_installed(Some(&sent_record(&synthetic::voice_image(0))))
        .expect("stored");
    answering(&rig, synthetic::answer());
    rig.machine.handle(Command::FullBackupStart);
    rig.machine.poll_incoming();
    rig.events();

    rig.machine.full_save(STAMP).expect("saved");

    let manifest =
        std::fs::read_to_string(backup_dir(&rig).join("manifest.json")).expect("manifest");
    assert_eq!(
        manifest.matches(r#""equals_update_sent":true"#).count(),
        6,
        "{manifest}"
    );
}

#[test]
fn test_start_when_a_calibration_page_changed_since_the_last_saved_backup_returns_only_that_row_changed(
) {
    let mut rig = started("full-voice-calibration", synthetic::answer());
    rig.events();
    rig.machine.full_save(STAMP).expect("first backup saved");
    rig.events();
    assert_eq!(
        rig.machine.storage.last_calibration().map(|c| c.len()),
        Some(6),
        "the calibration pages are kept for the next backup"
    );
    assert!(!calibration_page(&synthetic::voice_image(4)).is_empty());
    // The page 42 of the synthetic image starts at the calibration start: flip a byte of its first word.
    let start = 42 * PAGE_BYTES;
    answering(&rig, answer_changing(4, move |image| image[start] ^= 0x01));

    rig.machine.handle(Command::FullBackupStart);
    rig.machine.poll_incoming();

    let details = voice_details(&last_state(&mut rig));
    for (k, row) in details.iter().enumerate() {
        assert_eq!(
            row.contains("calibration page CHANGED since the previous full backup"),
            k == 4,
            "Voice {k}: {row}"
        );
        assert_eq!(
            row.contains("calibration page unchanged since the previous full backup"),
            k != 4,
            "Voice {k}: {row}"
        );
    }
}

#[test]
fn test_save_when_it_succeeds_keeps_the_calibration_pages_and_a_refused_set_keeps_the_old_ones() {
    let mut rig = started("full-voice-keep", synthetic::answer());
    rig.events();
    rig.machine.full_save(STAMP).expect("saved");
    let kept = rig.machine.storage.last_calibration().expect("kept");
    assert_eq!(
        kept[2],
        sha256_hex(calibration_page(&synthetic::voice_image(2)))
    );
    rig.events();
    let start = 42 * PAGE_BYTES;
    answering(&rig, answer_changing(1, move |image| image[start] ^= 0x01));
    rig.machine.handle(Command::FullBackupStart);
    rig.machine.poll_incoming();

    assert!(rig.machine.full_save("2026-11-14_110000Z").is_ok());
    assert_ne!(rig.machine.storage.last_calibration(), Some(kept));
}

fn dsi_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("TEMPESTER_DSI_SYX").map(std::path::PathBuf::from)
}

#[test]
#[ignore = "needs the real DSI files (TEMPESTER_DSI_SYX)"]
fn test_start_when_the_chips_hold_the_built_voice_1_5_1_0_returns_rows_naming_it_known() {
    use tempest_proto::os_image;
    let dir = dsi_dir().expect("set TEMPESTER_DSI_SYX");
    let dsi = std::fs::read(dir.join("Tempest_Voice_1.5.syx")).expect("DSI Voice file");
    let version = tempest_proto::voice_versions::find_voice_version("1.5.1.0").expect("version");
    let built = tempest_proto::voice_versions::build_voice(version, &dsi).expect("builds");
    let origin = os_image::decode(&dsi).expect("decodes").image;
    // What a chip holds: pages 0-40 of the built image, its own boot and calibration pages (synthetic here).
    let chip_image = |k: u8, pages: &[u8]| {
        let mut image = synthetic::voice_image(k);
        image[..pages.len().min(0xF600)].copy_from_slice(&pages[..0xF600]);
        image
    };
    let messages = |pages: &[u8]| -> Vec<Vec<u8>> {
        Chip::all()
            .into_iter()
            .map(|c| match c {
                Chip::Voice(k) => synthetic::message(c, &chip_image(k, pages)),
                Chip::MainBoot => synthetic::message(c, &synthetic::boot_flash_image()),
                Chip::System => {
                    synthetic::message(c, &synthetic::system_payload(&synthetic::STOCK_ROUTE))
                }
            })
            .collect()
    };
    let mut rig = TestRig::connected("full-voice-real");
    rig.machine
        .storage
        .set_voice_dsi_pages(&sha256_hex(&origin[..0xF600]))
        .expect("stored");
    answering(&rig, messages(&built.image));

    rig.machine.handle(Command::FullBackupStart);
    rig.machine.poll_incoming();
    let tempester = voice_details(&last_state(&mut rig));
    rig.machine.handle(Command::FullBackupCancel);
    answering(&rig, messages(&origin));
    rig.machine.handle(Command::FullBackupStart);
    rig.machine.poll_incoming();
    let dsi_rows = voice_details(&last_state(&mut rig));

    assert!(
        tempester
            .iter()
            .all(|r| r.contains("Voice OS: Tempester Voice 1.5.1.0")),
        "{tempester:?}"
    );
    assert!(
        dsi_rows
            .iter()
            .all(|r| r.contains("Voice OS: DSI Voice 1.5")),
        "{dsi_rows:?}"
    );
}
