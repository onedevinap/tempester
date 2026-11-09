//! Sending a whole project from the library to the fake machine: the bytes and their order, the look at the machine's
//! project and its auto backup, the version guard, the round trip compared per beat and Play List (also on the fake
//! that reads Play Lists like the stock OS), and the cancel. Each guarantee has its counter-proof.

use std::time::Duration;

use super::*;
use crate::command::Command;
use crate::event::Event;
use crate::library::command::LibCommand;
use crate::library::inspect::messages_of;
use crate::machine::fake_machine;
use crate::machine::project::support::save_project_now;
use crate::machine::testkit::TestRig;
use tempest_proto::beat::{synthetic_beat_data, PADS};
use tempest_proto::project::{beat_message, header_data_message, header_message};
use tempest_proto::sound::Sound;

const FAST: u32 = 100_000_000;

/// A 16-beat (or fewer) project that is not the fake machine's: other beat names, other sounds.
fn library_project(lists_used: usize, beats: usize) -> Project {
    let mut assembler = ProjectAssembler::new();
    assembler.accept(&header_message(lists_used)).unwrap();
    for n in 0..beats {
        let sounds: [Sound; PADS] = std::array::from_fn(|pad| {
            let mut sound = Sound::with_defaults();
            sound.set(34, ((n * 3 + pad) % 60) as u16).unwrap();
            sound.set_name(&format!("Mine {n}-{pad}")).unwrap();
            sound
        });
        let data = synthetic_beat_data(&format!("Mine {:02}", n + 1), &[(0, 1)], 5 + n, &sounds);
        assembler.accept(&beat_message(n as u8, &data)).unwrap();
    }
    assembler.finish().unwrap()
}

/// A project whose only used Play List is list 4 (the one position the defect cannot touch).
fn list_four_only() -> Project {
    let mut words = vec![0u16; 44];
    words[0] = 5;
    words.push(2);
    words.extend([0xFEFE; 3]);
    words.push(1);
    words.extend([0u16; 110]);
    let data: Vec<u8> = words.iter().flat_map(|w| w.to_be_bytes()).collect();
    let mut assembler = ProjectAssembler::new();
    assembler.accept(&header_data_message(&data)).unwrap();
    assembler.finish().unwrap()
}

fn machine_project() -> Project {
    let mut assembler = ProjectAssembler::new();
    for message in fake_machine::project_messages() {
        assembler.accept(&message).unwrap();
    }
    assembler.finish().unwrap()
}

/// A connected rig with `project` saved in the library (its id is returned) and a fast cable.
fn rig_with(label: &str, project: &Project) -> (TestRig, String) {
    let mut rig = TestRig::connected(label);
    rig.machine.rate = FAST;
    rig.machine.fetch_wait = Duration::from_millis(80);
    save_project_now(&mut rig, project, "2026-11-08_100000Z");
    let id = project_id(&rig);
    rig.events();
    (rig, id)
}

fn project_id(rig: &TestRig) -> String {
    let index = rig.machine.library().snapshot().unwrap();
    index
        .items
        .iter()
        .find(|i| i.kind == Kind::Projects && !i.slot)
        .map(|i| i.id.clone())
        .expect("a project in the library")
}

fn lib(rig: &mut TestRig, command: LibCommand) {
    rig.machine.handle(Command::Lib(command));
}

fn check(rig: &mut TestRig, id: &str) -> Event {
    lib(
        rig,
        LibCommand::ProjectCheck {
            id: id.into(),
            version: None,
        },
    );
    rig.expect("lib_project_plan")
}

fn text(event: &Event, key: &str) -> String {
    event
        .field(key)
        .map(crate::json::Json::render)
        .unwrap_or_default()
}

fn token(plan: &Event) -> String {
    text(plan, "token").trim_matches('"').to_string()
}

fn send(rig: &mut TestRig, id: &str, token: &str, mode: ProjectMode) {
    lib(
        rig,
        LibCommand::ProjectSend {
            id: id.into(),
            version: None,
            machine: token.into(),
            mode,
        },
    );
}

/// Every event up to the end of a send (the last is the `project_send` that says how it ended).
fn finish_all(rig: &mut TestRig) -> Vec<Event> {
    rig.wait_until(|event| {
        event.name == "project_send"
            && ["\"done\"", "\"cancelled\"", "\"failed\""].contains(&text(event, "state").as_str())
    })
}

/// The `project_send` event that says how a send ended.
fn finish(rig: &mut TestRig) -> Event {
    finish_all(rig).pop().expect("the end of the send")
}

fn state(event: &Event) -> String {
    text(event, "state").trim_matches('"').to_string()
}

/// The `5E` and `5C` messages the worker put on the cable, in order.
fn wire_project(rig: &TestRig) -> Vec<Vec<u8>> {
    messages_of(&rig.transport().sent_bytes())
        .into_iter()
        .filter(|m| m.len() > 5 && m[..3] == [0xF0, 0x01, 0x28] && [0x5E, 0x5C].contains(&m[3]))
        .map(<[u8]>::to_vec)
        .collect()
}

fn install(rig: &TestRig, version: Option<&str>) {
    let record = InstalledOs {
        label: version.map_or("my build.syx".into(), |v| format!("Tempester {v}")),
        version: version.map(str::to_string),
        at: "2026-11-08_120000Z".into(),
        sha256_image: "a".repeat(64),
        sha256_image_written: Some("b".repeat(64)),
        verified_at: None,
    };
    rig.machine.storage.set_installed_os(Some(&record)).unwrap();
}

fn installed(version: Option<&str>) -> Option<InstalledOs> {
    version.map(|v| InstalledOs {
        label: format!("Tempester {v}"),
        version: Some(v.into()),
        at: "t".into(),
        sha256_image: "a".repeat(64),
        sha256_image_written: Some("b".repeat(64)),
        verified_at: None,
    })
}

// ---- what goes on the cable -----------------------------------------------------------------------------------

#[test]
fn test_project_send_when_confirmed_returns_standard_layout_5e_first_then_5c_ascending() {
    let project = library_project(2, 16);
    let (mut rig, id) = rig_with("psend-bytes", &project);
    install(&rig, Some("1.6.2.0"));
    let plan = check(&mut rig, &id);

    send(&mut rig, &id, &token(&plan), ProjectMode::Normal);
    let end = finish(&mut rig);

    assert_eq!(state(&end), "done");
    let wire = wire_project(&rig);
    assert_eq!(wire, project.send_messages());
    assert_eq!(wire[0][3], 0x5E);
    let numbers: Vec<u8> = wire[1..].iter().map(|m| m[4]).collect();
    assert_eq!(numbers, (0..16).collect::<Vec<u8>>());
}

#[test]
fn test_project_send_when_item_has_an_as_received_capture_returns_the_standard_file_not_the_capture(
) {
    // Counter-proof: the capture (the machine's flushed layout) is stored next to the item and differs from what is sent.
    let project = library_project(1, 3);
    let (mut rig, id) = rig_with("psend-capture", &project);
    install(&rig, Some("1.6.2.0"));
    let plan = check(&mut rig, &id);

    send(&mut rig, &id, &token(&plan), ProjectMode::Normal);
    finish(&mut rig);

    let sent = wire_project(&rig).concat();
    assert_eq!(sent, project.standard());
    assert_ne!(sent, project.raw());
}

#[test]
fn test_project_send_when_the_file_holds_fewer_than_16_beats_returns_only_those_and_says_the_others_stay(
) {
    let project = library_project(0, 4);
    let (mut rig, id) = rig_with("psend-partial", &project);

    let plan = check(&mut rig, &id);

    assert!(text(&plan, "lines").contains("keep their current content"));
    assert!(text(&plan, "lines").contains("5, 6, 7"));
    send(&mut rig, &id, &token(&plan), ProjectMode::Normal);
    finish(&mut rig);
    assert_eq!(wire_project(&rig).len(), 5);
}

#[test]
fn test_check_when_project_is_complete_returns_the_replacement_text_and_no_partial_note() {
    let (mut rig, id) = rig_with("psend-text", &library_project(0, 16));

    let plan = check(&mut rig, &id);

    let lines = text(&plan, "lines");
    assert!(lines.contains("whole project in RAM"));
    assert!(lines.contains("not saved to the machine's flash"));
    assert!(!lines.contains("keep their current content"));
}

// ---- the look at the machine, and the backup ----------------------------------------------------------------

#[test]
fn test_project_send_when_the_machine_project_is_unknown_returns_an_auto_backup_before_sending() {
    let (mut rig, id) = rig_with("psend-backup", &library_project(0, 16));
    let plan = check(&mut rig, &id);
    assert_eq!(text(&plan, "status"), "\"unknown\"");
    assert!(text(&plan, "lines").contains("auto backup"));

    send(&mut rig, &id, &token(&plan), ProjectMode::Normal);
    let events = finish_all(&mut rig);

    let library = rig.machine.library();
    let slot = library.item("machine-projects").unwrap();
    assert_eq!(slot.history.len(), 1);
    assert!(slot.history[0].label.starts_with("auto backup "));
    let (_, _, bytes) = library.version_bytes("machine-projects", 0).unwrap();
    let kept = inspect(Kind::Projects, &bytes, "").unwrap().hash;
    let theirs = inspect(Kind::Projects, &machine_project().standard(), "")
        .unwrap()
        .hash;
    assert_eq!(kept, theirs);
    let said = events.iter().filter(|e| e.name == "lib_done");
    assert!(said
        .map(|e| text(e, "lines"))
        .any(|l| l.contains("saved as an auto backup first")));
}

#[test]
fn test_project_send_when_the_machine_project_is_in_the_library_returns_no_backup() {
    // Counter-proof of the backup rule: a known project is not backed up.
    let (mut rig, id) = rig_with("psend-known", &library_project(0, 16));
    save_project_now(&mut rig, &machine_project(), "2026-11-08_090000Z");
    rig.events();
    let plan = check(&mut rig, &id);
    assert_eq!(text(&plan, "status"), "\"known\"");

    send(&mut rig, &id, &token(&plan), ProjectMode::Normal);
    finish(&mut rig);

    assert!(rig
        .machine
        .library()
        .snapshot()
        .unwrap()
        .item("machine-projects")
        .is_none());
}

#[test]
fn test_project_send_when_the_machine_already_holds_it_returns_nothing_to_send() {
    let (mut rig, id) = rig_with("psend-identical", &machine_project());
    install(&rig, Some("1.6.2.0"));
    let plan = check(&mut rig, &id);
    assert_eq!(text(&plan, "status"), "\"identical\"");

    send(&mut rig, &id, &token(&plan), ProjectMode::Normal);

    assert!(text(&rig.expect("lib_done"), "lines").contains("nothing to send"));
    assert!(wire_project(&rig).is_empty());
}

#[test]
fn test_project_send_when_the_machine_is_silent_returns_no_answer_and_needs_the_none_token() {
    let (mut rig, id) = rig_with("psend-silent", &library_project(0, 2));
    rig.transport().set_responder(Box::new(|_| Vec::new()));
    let plan = check(&mut rig, &id);
    assert_eq!(text(&plan, "status"), "\"no_answer\"");
    assert_eq!(token(&plan), "none");
    assert!(text(&plan, "lines").contains("cannot be backed up"));

    // Counter-proof: with a hash instead of the explicit « none », a machine that went silent stops the send.
    send(&mut rig, &id, &"a".repeat(64), ProjectMode::Normal);
    assert!(text(&rig.expect("error"), "message").contains("no longer answers"));
    assert!(wire_project(&rig).is_empty());

    send(&mut rig, &id, "none", ProjectMode::Normal);
    let end = finish(&mut rig);
    assert_eq!(state(&end), "done");
    assert_eq!(wire_project(&rig).len(), 3);
    assert!(rig
        .machine
        .library()
        .snapshot()
        .unwrap()
        .item("machine-projects")
        .is_none());
}

#[test]
fn test_project_send_when_the_machine_changed_since_the_check_returns_a_new_plan_and_sends_nothing()
{
    let (mut rig, id) = rig_with("psend-changed", &library_project(0, 2));
    let plan = check(&mut rig, &id);

    send(&mut rig, &id, &"b".repeat(64), ProjectMode::Normal);

    let again = rig.expect("lib_project_plan");
    assert_eq!(text(&again, "changed"), "true");
    assert_ne!(token(&again), "b".repeat(64));
    assert_eq!(token(&again), token(&plan));
    assert!(wire_project(&rig).is_empty());
}

// ---- the version guard ----------------------------------------------------------------------------------------

#[test]
fn test_guard_of_when_installed_version_has_the_fix_returns_fixed_and_otherwise_at_risk() {
    let project = library_project(2, 0);

    for (version, want) in [
        (Some("1.6.2.0"), Guard::Fixed),
        (Some("1.6.3.0"), Guard::Fixed),
        (Some("1.6.3.1"), Guard::Fixed),
        // Archived test builds are no longer in the app's table, so the number rule decides: it warns when it cannot
        // prove the fix (1.6.1.3 does have n°28, 1.6.1.4 does not).
        (Some("1.6.1.3"), Guard::AtRisk),
        (Some("1.6.1.4"), Guard::AtRisk),
        (Some("1.5.2.0"), Guard::AtRisk),
        (Some("1.6.1.2"), Guard::AtRisk),
        (Some("not a version"), Guard::AtRisk),
        (None, Guard::AtRisk),
    ] {
        assert_eq!(
            guard_of(installed(version).as_ref(), &project),
            want,
            "{version:?}"
        );
    }
}

#[test]
fn test_guard_of_when_installed_build_is_unknown_returns_at_risk() {
    let unknown = InstalledOs {
        label: "my build.syx".into(),
        version: None,
        at: "t".into(),
        sha256_image: "a".repeat(64),
        sha256_image_written: Some("b".repeat(64)),
        verified_at: None,
    };

    assert_eq!(
        guard_of(Some(&unknown), &library_project(1, 0)),
        Guard::AtRisk
    );
}

#[test]
fn test_guard_of_when_no_list_can_be_misread_returns_no_risk_even_without_a_record() {
    assert_eq!(guard_of(None, &library_project(0, 3)), Guard::NoRisk);
    assert_eq!(guard_of(None, &list_four_only()), Guard::NoRisk);
    // Counter-proof: a used list 1, 2 or 3 is at risk.
    assert_eq!(guard_of(None, &library_project(1, 3)), Guard::AtRisk);
}

#[test]
fn test_project_send_when_the_os_may_misread_lists_returns_a_refusal_unless_confirmed() {
    let project = library_project(2, 16);
    let (mut rig, id) = rig_with("psend-guard", &project);
    let plan = check(&mut rig, &id);
    assert_eq!(text(&plan, "guard"), "\"at_risk\"");
    assert!(text(&plan, "warning").contains("may misread Play Lists"));
    assert!(text(&plan, "warning").contains("beats and sounds are not affected"));
    assert!(text(&plan, "warning").contains("Play List(s) 1, 2"));

    send(&mut rig, &id, &token(&plan), ProjectMode::Normal);
    assert!(text(&rig.expect("error"), "message").contains("Send anyway"));
    assert!(wire_project(&rig).is_empty());

    send(&mut rig, &id, &token(&plan), ProjectMode::Anyway);
    assert_eq!(state(&finish(&mut rig)), "done");
    assert_eq!(wire_project(&rig), project.send_messages());
}

#[test]
fn test_project_send_when_the_recorded_version_has_the_fix_returns_no_warning() {
    let (mut rig, id) = rig_with("psend-fixed", &library_project(2, 2));
    install(&rig, Some("1.6.2.0"));

    let plan = check(&mut rig, &id);

    assert_eq!(text(&plan, "guard"), "\"fixed\"");
    assert_eq!(text(&plan, "warning"), "[]");
}

#[test]
fn test_project_send_when_no_list_is_used_returns_no_warning_and_a_plain_send_works() {
    let (mut rig, id) = rig_with("psend-nolists", &library_project(0, 2));

    let plan = check(&mut rig, &id);
    assert_eq!(text(&plan, "guard"), "\"no_risk\"");
    assert_eq!(text(&plan, "warning"), "[]");
    send(&mut rig, &id, &token(&plan), ProjectMode::Normal);

    assert_eq!(state(&finish(&mut rig)), "done");
}

#[test]
fn test_project_send_when_sent_without_play_lists_returns_four_empty_lists_and_the_same_beats() {
    let project = library_project(3, 16);
    let (mut rig, id) = rig_with("psend-nolist-mode", &project);
    let plan = check(&mut rig, &id);
    assert!(text(&plan, "without_lists").contains("become empty"));

    send(&mut rig, &id, &token(&plan), ProjectMode::WithoutLists);
    finish(&mut rig);

    let wire = wire_project(&rig);
    let mut assembler = ProjectAssembler::new();
    wire.iter().for_each(|m| {
        assembler.accept(m).unwrap();
    });
    let sent = assembler.finish().unwrap();
    assert_eq!(sent.header.lists_used(), [false; 4]);
    assert_eq!(sent.beats, project.beats);
}

// ---- the round trip ---------------------------------------------------------------------------------------------

fn verify(rig: &mut TestRig) -> Event {
    lib(rig, LibCommand::ProjectVerify);
    rig.expect("project_verify")
}

#[test]
fn test_project_verify_when_the_machine_keeps_what_was_sent_returns_match_with_all_beats_compared()
{
    let (mut rig, id) = rig_with("pverify-match", &library_project(2, 16));
    install(&rig, Some("1.6.2.0"));
    let plan = check(&mut rig, &id);
    send(&mut rig, &id, &token(&plan), ProjectMode::Normal);
    finish(&mut rig);

    let result = verify(&mut rig);

    assert_eq!(text(&result, "state"), "\"match\"");
    assert_eq!(text(&result, "beats_compared"), "16");
    assert_eq!(text(&result, "differences"), "[]");
    assert!(text(&result, "lines").contains("identical"));
}

#[test]
fn test_project_verify_when_the_machine_misreads_the_lists_returns_the_differences_precisely() {
    // Counter-proof: the same send on the fake with the n°28 defect must NOT match, and name the lists.
    let (mut rig, id) = rig_with("pverify-defect", &library_project(2, 16));
    rig.machine.handle(Command::FakeProjectDefect(true));
    let plan = check(&mut rig, &id);
    send(&mut rig, &id, &token(&plan), ProjectMode::Anyway);
    finish(&mut rig);

    let result = verify(&mut rig);

    assert_eq!(text(&result, "state"), "\"differ\"");
    let differences = text(&result, "differences");
    assert!(differences.contains("Play List 2"), "{differences}");
    assert!(differences.contains("Play List 3"), "{differences}");
    assert!(
        !differences.contains("beat"),
        "beats are not touched: {differences}"
    );
}

#[test]
fn test_project_verify_when_sent_without_play_lists_on_the_defect_machine_returns_match() {
    let (mut rig, id) = rig_with("pverify-nolists", &library_project(2, 16));
    rig.machine.handle(Command::FakeProjectDefect(true));
    let plan = check(&mut rig, &id);
    send(&mut rig, &id, &token(&plan), ProjectMode::WithoutLists);
    finish(&mut rig);

    assert_eq!(text(&verify(&mut rig), "state"), "\"match\"");
}

#[test]
fn test_project_verify_when_no_list_is_used_on_the_defect_machine_returns_match() {
    let (mut rig, id) = rig_with("pverify-defect-nolist", &library_project(0, 16));
    rig.machine.handle(Command::FakeProjectDefect(true));
    let plan = check(&mut rig, &id);
    send(&mut rig, &id, &token(&plan), ProjectMode::Normal);
    finish(&mut rig);

    assert_eq!(text(&verify(&mut rig), "state"), "\"match\"");
}

#[test]
fn test_project_verify_when_a_beat_changed_on_the_machine_returns_that_beat() {
    let project = library_project(0, 16);
    let (mut rig, id) = rig_with("pverify-beat", &project);
    let plan = check(&mut rig, &id);
    send(&mut rig, &id, &token(&plan), ProjectMode::Normal);
    finish(&mut rig);
    let other = synthetic_beat_data(
        "Changed",
        &[(0, 1)],
        3,
        &std::array::from_fn(|_| Sound::with_defaults()),
    );
    rig.transport().send(&beat_message(6, &other)).unwrap();

    let result = verify(&mut rig);

    assert_eq!(text(&result, "state"), "\"differ\"");
    assert!(text(&result, "differences").contains("beat 7: name"));
}

#[test]
fn test_project_verify_when_nothing_was_sent_returns_a_precondition_error() {
    let mut rig = TestRig::connected("pverify-none");

    lib(&mut rig, LibCommand::ProjectVerify);

    assert_eq!(
        rig.expect("error").field("code"),
        Some(&"precondition".into())
    );
}

// ---- cancel and busy --------------------------------------------------------------------------------------------

#[test]
fn test_project_send_when_cancelled_mid_send_returns_cancelled_with_part_of_the_bytes_and_no_verify(
) {
    let project = library_project(0, 16);
    let (mut rig, id) = rig_with("psend-cancel", &project);
    let plan = check(&mut rig, &id);
    rig.machine.rate = 40_000;

    send(&mut rig, &id, &token(&plan), ProjectMode::Normal);
    rig.wait_for("project_send");
    assert!(rig.machine.project_busy());
    // Nothing else may talk on the port meanwhile.
    rig.machine.handle(Command::FetchBeat);
    assert_eq!(rig.expect("error").field("code"), Some(&"busy".into()));
    lib(&mut rig, LibCommand::ProjectCancel);
    let end = finish(&mut rig);

    assert_eq!(state(&end), "cancelled");
    let wire = wire_project(&rig).concat();
    assert!(wire.len() < project.standard().len());
    assert!(!rig.machine.project_busy());
    lib(&mut rig, LibCommand::ProjectVerify);
    assert_eq!(
        rig.expect("error").field("code"),
        Some(&"precondition".into())
    );
}

#[test]
fn test_project_cancel_when_nothing_is_sent_returns_a_precondition_error() {
    let mut rig = TestRig::connected("pcancel-none");

    lib(&mut rig, LibCommand::ProjectCancel);

    assert_eq!(
        rig.expect("error").field("code"),
        Some(&"precondition".into())
    );
}

#[test]
fn test_project_send_when_the_item_is_not_a_project_returns_an_error_and_sends_nothing() {
    let mut rig = TestRig::connected("psend-notproject");
    rig.machine.handle(Command::FetchSound);
    rig.machine.handle(Command::SaveSound("Kick".into()));
    rig.events();

    lib(
        &mut rig,
        LibCommand::ProjectCheck {
            id: "s1".into(),
            version: None,
        },
    );

    assert_eq!(rig.expect("error").field("code"), Some(&"invalid".into()));
}

// ---- the fake machine -------------------------------------------------------------------------------------------

#[test]
fn test_fake_machine_when_sent_a_project_returns_it_on_request_and_keeps_beats_not_sent() {
    let transport = fake_machine::scripted(true);
    let project = library_project(1, 2);

    for message in project.send_messages() {
        transport.send(&message).unwrap();
    }
    let mut parsed = ProjectAssembler::new();
    let got = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = std::sync::Arc::clone(&got);
    transport.set_receiver(Some(Box::new(move |bytes| {
        sink.lock().unwrap().extend_from_slice(bytes);
    })));
    transport
        .send(&tempest_proto::sound_message::REQUEST_PROJECT)
        .unwrap();
    let heard = got.lock().unwrap().clone();
    messages_of(&heard).into_iter().for_each(|m| {
        parsed.accept(m).unwrap();
    });
    let back = parsed.finish().unwrap();

    assert_eq!(back.header_data(), project.header_data());
    assert_eq!(back.beats[0].data(), project.beats[0].data());
    assert_eq!(back.beats[1].data(), project.beats[1].data());
    assert_eq!(back.beats.len(), 16);
    assert_eq!(back.beats[2].data(), machine_project().beats[2].data());
}

#[test]
fn test_shifted_lists_when_the_defect_is_on_returns_the_rows_of_the_analysis() {
    // `Docs/analyses/bug-project-receive.md` 1: which lists come back wrong for which lists used.
    fn wrong_lists(used: [bool; 4]) -> Vec<usize> {
        let mut words = vec![0u16; 44];
        words[0] = 5;
        words.push(2);
        for (at, used) in used.iter().enumerate() {
            if *used {
                words.push(1);
                words.extend((0..110).map(|k| 0x0100 + (at as u16) * 0x1000 + k));
            } else {
                words.push(0xFEFE);
            }
        }
        let data: Vec<u8> = words.iter().flat_map(|w| w.to_be_bytes()).collect();
        let back = fake_machine::shifted_lists(&data);
        let mut assembler = ProjectAssembler::new();
        assembler.accept(&header_data_message(&data)).unwrap();
        let sent = assembler.finish().unwrap();
        let mut assembler = ProjectAssembler::new();
        assembler.accept(&header_data_message(&back)).unwrap();
        let received = assembler.finish().unwrap();
        let result = tempest_proto::project::compare(&sent, &received);
        let mut lists: Vec<usize> = result
            .differences
            .iter()
            .filter_map(|d| match d {
                tempest_proto::project::Difference::PlayListContent { list, .. }
                | tempest_proto::project::Difference::PlayListUse { list, .. } => Some(*list),
                _ => None,
            })
            .collect();
        lists.dedup();
        lists
    }

    assert_eq!(wrong_lists([false; 4]), Vec::<usize>::new());
    assert_eq!(
        wrong_lists([false, false, false, true]),
        Vec::<usize>::new()
    );
    assert_eq!(wrong_lists([true, false, false, false]), vec![4]);
    assert_eq!(wrong_lists([true, true, false, false]), vec![2, 3, 4]);
    assert_eq!(wrong_lists([true; 4]), vec![2, 3, 4]);
    assert_eq!(wrong_lists([false, true, false, true]), vec![3, 4]);
}

// ---- the author's capture ---------------------------------------------------------------------------------------

#[test]
#[ignore = "needs the captured project (TEMPESTER_PROJECT_CAPTURE)"]
#[allow(clippy::expect_used)] // a test on the author's data: a panic with a message is the failure report
fn test_project_send_when_real_capture_is_sent_to_the_fake_returns_it_back_identical() {
    let path = std::env::var_os("TEMPESTER_PROJECT_CAPTURE")
        .expect("set TEMPESTER_PROJECT_CAPTURE to the captured project .syx file");
    let capture = std::fs::read(path).expect("capture readable");
    let mut assembler = ProjectAssembler::new();
    for message in messages_of(&capture) {
        assembler.accept(message).expect("a message of the capture");
    }
    let project = assembler.finish().expect("the capture holds a project");
    assert!(project.is_complete());
    let (mut rig, id) = rig_with("psend-real-capture", &project);
    install(&rig, Some("1.6.2.0"));
    let plan = check(&mut rig, &id);

    send(&mut rig, &id, &token(&plan), ProjectMode::Normal);
    assert_eq!(state(&finish(&mut rig)), "done");
    let result = verify(&mut rig);

    assert_eq!(
        text(&result, "state"),
        "\"match\"",
        "{}",
        text(&result, "differences")
    );
    assert_eq!(text(&result, "beats_compared"), "16");
    assert_eq!(wire_project(&rig), project.send_messages());
}
