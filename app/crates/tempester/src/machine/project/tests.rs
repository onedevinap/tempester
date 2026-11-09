use std::time::Duration;

use super::*;
use crate::json::Json;
use crate::machine::testkit::TestRig;
use tempest_proto::beat::{synthetic_beat_data, PADS};
use tempest_proto::project::{beat_message, header_message};
use tempest_proto::sound_message::SoundMessage;

fn listening(label: &str) -> TestRig {
    let mut rig = TestRig::connected(label);
    rig.machine.handle(Command::ReceiveProject);
    rig.events();
    rig
}

/// Plays a Send Project made of `messages` on the cable and lets the worker hear it.
fn hear(rig: &mut TestRig, messages: &[Vec<u8>]) {
    for message in messages {
        rig.transport().push_incoming(message);
    }
    rig.machine.poll_incoming();
    support::accept_review(rig);
}

fn later(minutes: u64) -> Instant {
    Instant::now() + Duration::from_secs(minutes * 60)
}

fn list(rig: &TestRig, folder: Folder) -> Vec<String> {
    rig.machine
        .storage
        .list(folder)
        .unwrap()
        .into_iter()
        .map(|entry| entry.name)
        .collect()
}

/// The last `project` event: the verdict after the progress events.
fn last_project(rig: &mut TestRig) -> crate::event::Event {
    let events = rig.events();
    events
        .into_iter()
        .rev()
        .find(|e| e.name == "project")
        .expect("a project event")
}

fn state_of(event: &crate::event::Event) -> Option<&Json> {
    event.field("state")
}

fn project_of(messages: &[Vec<u8>]) -> Project {
    let mut assembler = ProjectAssembler::new();
    for message in messages {
        assembler.accept(message).unwrap();
    }
    assembler.finish().unwrap()
}

#[test]
fn test_receive_when_machine_sends_a_project_returns_the_exact_capture_and_a_standard_file() {
    let mut rig = listening("project-raw");

    hear(&mut rig, &fake_machine::project_messages());

    let saved = rig.expect("project_saved");
    assert_eq!(saved.field("beats"), Some(&16usize.into()));
    let files = list(&rig, Folder::Projects);
    assert_eq!(files.len(), 2);
    let read = |name: &str| rig.machine.storage.read(Folder::Projects, name).unwrap();
    let received = files
        .iter()
        .find(|name| name.ends_with(" (as received).syx"))
        .expect("the as-received file");
    let standard = files
        .iter()
        .find(|name| !name.ends_with(" (as received).syx"))
        .expect("the standard file");
    let capture = fake_machine::project_messages().concat();
    assert_eq!(read(received), capture);
    assert_eq!(saved.field("received_file"), Some(&received.clone().into()));
    assert_eq!(saved.field("project_file"), Some(&standard.clone().into()));
    assert_eq!(
        read(standard),
        project_of(&fake_machine::project_messages()).standard()
    );
    assert_ne!(
        read(standard),
        capture,
        "the machine's tails are re-encoded"
    );
}

#[test]
fn test_receive_when_project_saved_returns_16_beat_files_named_from_the_beat_names() {
    let mut rig = listening("project-beats");

    hear(&mut rig, &fake_machine::project_messages());

    let beats = list(&rig, Folder::Beats);
    assert_eq!(beats.len(), 16);
    assert!(beats[0].ends_with("/01 Fake beat 01.syx"), "{}", beats[0]);
    assert!(beats[15].ends_with("/16 Fake beat 16.syx"), "{}", beats[15]);
}

#[test]
fn test_receive_when_beat_file_opened_and_sent_returns_the_5f_dump_the_beats_page_sends() {
    let mut rig = listening("project-beat-send");
    hear(&mut rig, &fake_machine::project_messages());
    let name = list(&rig, Folder::Beats).remove(2);
    let file = rig.machine.storage.read(Folder::Beats, &name).unwrap();
    rig.events();

    rig.machine.handle(Command::OpenLibrary {
        kind: Kind::Beats,
        name,
    });
    rig.machine.handle(Command::SendBeat);

    assert_eq!(
        rig.expect("beat_opened").field("name"),
        Some(&"Fake beat 03".into())
    );
    assert_eq!(rig.transport().sent_bytes(), file);
    assert_eq!(&file[..4], &[0xF0, 0x01, 0x28, 0x5F]);
}

#[test]
fn test_receive_when_project_saved_returns_only_the_distinct_sounds_as_63_files() {
    let mut rig = listening("project-sounds");

    hear(&mut rig, &fake_machine::project_messages());

    let saved = rig.expect("project_saved");
    assert_eq!(saved.field("sounds"), Some(&24usize.into()));
    assert_eq!(saved.field("duplicates"), Some(&488usize.into()));
    let sounds = list(&rig, Folder::Sounds);
    assert_eq!(sounds.len(), 24);
    let bytes = rig
        .machine
        .storage
        .read(Folder::Sounds, &sounds[0])
        .unwrap();
    assert!(matches!(
        sound_message::decode(&bytes),
        Ok(SoundMessage::File { .. })
    ));
}

#[test]
fn test_receive_when_a_sound_file_is_opened_and_sent_returns_the_60_message_of_the_same_sound() {
    let mut rig = listening("project-sound-send");
    hear(&mut rig, &fake_machine::project_messages());
    let name = list(&rig, Folder::Sounds).remove(0);
    rig.events();

    rig.machine.handle(Command::OpenLibrary {
        kind: Kind::Sounds,
        name,
    });
    rig.machine.handle(Command::SendSound);

    let sent = rig.transport().sent_bytes();
    let Ok(SoundMessage::Current(sound)) = sound_message::decode(&sent) else {
        panic!("not a 60 message");
    };
    assert_eq!(sound.name(), "Fake 1 v1");
}

#[test]
fn test_receive_when_messages_arrive_returns_progress_beat_by_beat_then_saved() {
    let mut rig = listening("project-progress");

    hear(&mut rig, &fake_machine::project_messages());

    let states: Vec<(String, Option<Json>)> = rig
        .events()
        .iter()
        .filter(|e| e.name == "project")
        .map(|e| (e.render(), e.field("beats").cloned()))
        .collect();
    assert_eq!(states.len(), 18);
    assert!(states[3].0.contains(r#""beat":3"#), "{}", states[3].0);
    assert_eq!(states[0].1, Some(0usize.into()));
    assert_eq!(states[15].1, Some(15usize.into()));
    assert!(
        states[16].0.contains(r#""state":"review""#),
        "the review comes before anything is written"
    );
    assert!(states[17].0.contains(r#""state":"saved""#));
    assert!(states[0].0.contains(r#""state":"receiving""#));
    assert!(states[15].0.contains(r#""state":"receiving""#));
}

#[test]
fn test_receive_when_complete_returns_saved_state_and_stops_listening() {
    let mut rig = listening("project-done");

    hear(&mut rig, &fake_machine::project_messages());

    let events = rig.events();
    let last = events.iter().rev().find(|e| e.name == "project").unwrap();
    assert_eq!(state_of(last), Some(&"saved".into()));
    assert!(rig.machine.project.is_none());
    assert!(events.iter().any(|e| e.name == "library"));
}

#[test]
fn test_receive_when_not_listening_returns_nothing_saved() {
    let mut rig = TestRig::connected("project-idle");

    hear(&mut rig, &fake_machine::project_messages());

    assert!(rig.events().is_empty());
    assert!(list(&rig, Folder::Projects).is_empty());
}

#[test]
fn test_receive_when_not_connected_returns_not_connected_error() {
    let mut rig = TestRig::new("project-off");

    rig.machine.handle(Command::ReceiveProject);

    assert_eq!(
        rig.expect("error").field("code"),
        Some(&"not_connected".into())
    );
}

#[test]
fn test_fetch_when_machine_answers_returns_the_project_saved_and_the_request_sent() {
    let mut rig = TestRig::connected("project-fetch");

    rig.machine.handle(Command::FetchProject);
    rig.machine.poll_incoming();
    support::accept_review(&mut rig);

    assert_eq!(rig.transport().sent_bytes(), REQUEST_PROJECT);
    assert_eq!(list(&rig, Folder::Beats).len(), 16);
    assert_eq!(
        rig.expect("project_saved").field("beats"),
        Some(&16usize.into())
    );
}

#[test]
fn test_fetch_when_machine_is_silent_returns_no_answer_naming_tempester_1_6_1_0() {
    let mut rig = TestRig::connected("project-no-answer");
    rig.transport().set_responder(Box::new(|_| vec![]));
    rig.machine.handle(Command::FetchProject);
    rig.events();

    rig.machine.poll_incoming();
    assert!(
        rig.events().is_empty(),
        "no failure before the wait is over"
    );
    rig.machine
        .expire_project(Instant::now() + Duration::from_secs(10));
    support::accept_review(&mut rig);

    let failed = rig.expect("project");
    assert_eq!(failed.field("code"), Some(&"no_answer".into()));
    assert!(failed.render().contains("1.6.1.0"), "{}", failed.render());
    assert!(failed.render().contains("Send Project"));
    assert!(rig.machine.project.is_none());
    assert!(list(&rig, Folder::Projects).is_empty());
}

#[test]
fn test_listen_when_nothing_is_heard_for_two_minutes_returns_timeout_message() {
    let mut rig = listening("project-timeout");

    rig.machine
        .expire_project(Instant::now() + Duration::from_secs(60));
    support::accept_review(&mut rig);
    assert!(rig.events().is_empty());
    rig.machine.expire_project(later(3));
    support::accept_review(&mut rig);

    let failed = rig.expect("project");
    assert_eq!(failed.field("code"), Some(&"timeout".into()));
    assert!(failed.render().contains("Send Project"));
}

/// The `project_saved` event of the run, as (partial flag, absent beats, project file name).
fn saved_summary(rig: &mut TestRig) -> (Option<Json>, Option<Json>, String) {
    let saved = rig.expect("project_saved");
    let file = saved.render();
    (
        saved.field("partial").cloned(),
        saved.field("absent_beats").cloned(),
        file,
    )
}

fn numbers(list: &[u8]) -> Json {
    list.to_vec().into()
}

#[test]
fn test_listen_when_the_machine_stops_after_five_beats_returns_a_partial_project_saved_and_marked()
{
    let mut rig = listening("project-partial");
    hear(&mut rig, &fake_machine::project_messages()[..6]);
    rig.events();

    rig.machine.expire_project(later(1));
    support::accept_review(&mut rig);

    let (partial, absent, rendered) = saved_summary(&mut rig);
    assert_eq!(partial, Some(true.into()));
    assert_eq!(absent, Some(numbers(&(6..=16).collect::<Vec<u8>>())));
    assert!(rendered.contains("(5 of 16 beats)"), "{rendered}");
    let files = list(&rig, Folder::Projects);
    assert_eq!(files.len(), 2);
    assert!(
        files.iter().all(|f| f.contains("(5 of 16 beats)")),
        "{files:?}"
    );
    assert_eq!(list(&rig, Folder::Beats).len(), 5);
    assert!(list(&rig, Folder::Beats)
        .iter()
        .all(|n| n.contains("(5 of 16 beats)/")));
    assert!(rig.machine.project.is_none());
}

#[test]
fn test_listen_when_beat_2_is_absent_returns_it_named_in_the_saved_event() {
    let mut rig = listening("project-missing-2");
    let mut messages = fake_machine::project_messages();
    messages.remove(2);
    hear(&mut rig, &messages);

    let (partial, absent, _) = saved_summary(&mut rig);

    assert_eq!(partial, Some(true.into()));
    assert_eq!(absent, Some(numbers(&[2])));
    assert!(list(&rig, Folder::Projects)[0].contains("(15 of 16 beats)"));
    assert_eq!(list(&rig, Folder::Beats).len(), 15);
}

#[test]
fn test_listen_when_beat_15_arrives_after_gaps_returns_the_project_saved_at_once() {
    let mut rig = listening("project-beat-15");
    let messages = fake_machine::project_messages();
    let kept: Vec<Vec<u8>> = [0usize, 1, 3, 6, 16]
        .iter()
        .map(|at| messages[*at].clone())
        .collect();

    hear(&mut rig, &kept);

    let (partial, absent, _) = saved_summary(&mut rig);
    assert_eq!(partial, Some(true.into()));
    assert_eq!(
        absent,
        Some(numbers(&[2, 4, 5, 7, 8, 9, 10, 11, 12, 13, 14, 15]))
    );
    assert!(rig.machine.project.is_none(), "no wait for a later beat");
    assert_eq!(list(&rig, Folder::Beats).len(), 4);
}

#[test]
fn test_listen_when_the_fake_machine_sends_absent_beats_returns_the_wait_then_a_partial_save() {
    let mut rig = listening("project-fake-partial");

    rig.machine.handle(Command::FakeProjectSendPartial);
    rig.machine.poll_incoming();

    assert!(rig.machine.project.is_some(), "beat 15 never came: waiting");
    assert!(rig.events().iter().all(|e| e.name != "project_saved"));
    rig.machine.expire_project(later(1));
    support::accept_review(&mut rig);
    let (partial, absent, _) = saved_summary(&mut rig);
    assert_eq!(partial, Some(true.into()));
    assert_eq!(absent, Some(numbers(&[2, 5, 16])));
    assert_eq!(list(&rig, Folder::Beats).len(), 13);
}

#[test]
fn test_listen_when_only_the_header_came_returns_a_partial_project_with_no_beat_file() {
    let mut rig = listening("project-header-only");
    hear(&mut rig, &fake_machine::project_messages()[..1]);
    rig.events();

    rig.machine.expire_project(later(1));
    support::accept_review(&mut rig);

    let (partial, absent, _) = saved_summary(&mut rig);
    assert_eq!(partial, Some(true.into()));
    assert_eq!(absent, Some(numbers(&(1..=16).collect::<Vec<u8>>())));
    let files = list(&rig, Folder::Projects);
    assert!(files[0].contains("(0 of 16 beats)"), "{}", files[0]);
    assert!(list(&rig, Folder::Beats).is_empty());
    assert!(list(&rig, Folder::Sounds).is_empty());
}

#[test]
fn test_listen_when_the_gap_is_not_over_returns_nothing_saved_yet() {
    let mut rig = listening("project-gap-open");
    hear(&mut rig, &fake_machine::project_messages()[..6]);
    rig.events();

    rig.machine
        .expire_project(Instant::now() + Duration::from_secs(5));
    support::accept_review(&mut rig);

    assert!(rig.events().is_empty());
    assert!(rig.machine.project.is_some());
    assert!(list(&rig, Folder::Projects).is_empty());
}

#[test]
fn test_listen_when_no_header_comes_before_the_wait_ends_returns_timeout_and_saves_nothing() {
    let mut rig = listening("project-no-header");
    hear(&mut rig, &[vec![0xF0, 0x01, 0x28, 0x60, 0x00, 0xF7]]);
    rig.events();

    rig.machine.expire_project(later(3));
    support::accept_review(&mut rig);

    let failed = rig.expect("project");
    assert_eq!(failed.field("code"), Some(&"timeout".into()));
    for folder in [Folder::Projects, Folder::Beats, Folder::Sounds] {
        assert!(list(&rig, folder).is_empty());
    }
}

#[test]
fn test_listen_when_a_beat_repeats_returns_refused_and_saves_nothing() {
    let mut rig = listening("project-repeat");
    let messages = fake_machine::project_messages();
    let mut heard = messages[..4].to_vec();
    heard.push(messages[3].clone());

    hear(&mut rig, &heard);

    let failed = last_project(&mut rig);
    assert_eq!(failed.field("code"), Some(&"refused".into()));
    assert!(failed.render().contains("beat 3 arrived after beat 3"));
    assert!(list(&rig, Folder::Projects).is_empty());
    assert!(rig.machine.project.is_none());
}

#[test]
fn test_listen_when_a_beat_number_decreases_returns_refused_and_saves_nothing() {
    let mut rig = listening("project-decrease");
    let messages = fake_machine::project_messages();
    let heard = vec![
        messages[0].clone(),
        messages[6].clone(),
        messages[3].clone(),
    ];

    hear(&mut rig, &heard);

    let failed = last_project(&mut rig);
    assert_eq!(failed.field("code"), Some(&"refused".into()));
    assert!(failed.render().contains("beat 3 arrived after beat 6"));
    assert!(list(&rig, Folder::Projects).is_empty());
}

#[test]
fn test_listen_when_a_beat_number_is_16_returns_refused_and_saves_nothing() {
    let mut rig = listening("project-n16");
    let messages = fake_machine::project_messages();
    let mut bad = messages[1].clone();
    bad[4] = 16;

    hear(&mut rig, &[messages[0].clone(), bad]);

    let failed = last_project(&mut rig);
    assert_eq!(failed.field("code"), Some(&"refused".into()));
    assert!(failed.render().contains("beat number 16"));
    assert!(list(&rig, Folder::Projects).is_empty());
}

#[test]
fn test_listen_when_the_project_is_complete_returns_no_mark_and_partial_false() {
    let mut rig = listening("project-complete-mark");

    hear(&mut rig, &fake_machine::project_messages());

    let (partial, absent, _) = saved_summary(&mut rig);
    assert_eq!(partial, Some(false.into()));
    assert_eq!(absent, Some(numbers(&[])));
    let files = list(&rig, Folder::Projects);
    assert!(!files[0].contains("beats)"), "{}", files[0]);
}

#[test]
fn test_listen_when_beats_are_absent_returns_never_the_complete_name_nor_partial_false() {
    let mut rig = listening("project-never-complete");
    hear(&mut rig, &fake_machine::project_messages()[..16]);
    rig.events();
    rig.machine.expire_project(later(1));
    support::accept_review(&mut rig);

    let (partial, _, rendered) = saved_summary(&mut rig);

    assert_ne!(partial, Some(false.into()));
    assert!(!rendered.contains(r#""partial":false"#));
    let files = list(&rig, Folder::Projects);
    assert!(files[0].contains("(15 of 16 beats)"), "{}", files[0]);
    assert!(list(&rig, Folder::Beats)
        .iter()
        .all(|n| n.contains("(15 of 16 beats)/")));
}

#[test]
fn test_listen_when_a_beat_comes_before_the_header_returns_refused() {
    let mut rig = listening("project-order");
    let messages = fake_machine::project_messages();

    hear(&mut rig, &messages[1..3]);

    let failed = last_project(&mut rig);
    assert!(failed.render().contains("before the project header"));
}

#[test]
fn test_listen_when_a_beat_is_truncated_returns_refused_with_the_reason() {
    let mut rig = listening("project-cut");
    let mut messages = fake_machine::project_messages();
    let data = synthetic_beat_data(
        "Cut",
        &[],
        3,
        &std::array::from_fn(|_| Sound::with_defaults()),
    );
    messages[1] = beat_message(0, &data[..data.len() - 400]);

    hear(&mut rig, &messages[..2]);

    let failed = last_project(&mut rig);
    assert!(failed
        .render()
        .contains("beat 1: the data ends before the sound of pad"));
    assert!(list(&rig, Folder::Beats).is_empty());
}

#[test]
fn test_listen_when_other_sysex_are_heard_in_between_returns_the_project_all_the_same() {
    let mut rig = listening("project-noise");
    let mut messages = fake_machine::project_messages();
    messages.insert(3, vec![0xF0, 0x01, 0x28, 0x60, 0x00, 0xF7]);
    messages.insert(1, vec![0xF0, 0x7E, 0x7F, 0x06, 0x02, 0xF7]);

    hear(&mut rig, &messages);

    assert_eq!(
        rig.expect("project_saved").field("beats"),
        Some(&16usize.into())
    );
}

#[test]
fn test_cancel_when_listening_returns_idle_and_a_later_project_is_not_saved() {
    let mut rig = listening("project-cancel");

    rig.machine.handle(Command::ProjectCancel);
    hear(&mut rig, &fake_machine::project_messages());

    let events = rig.events();
    assert_eq!(state_of(&events[0]), Some(&"idle".into()));
    assert!(events.iter().all(|e| e.name != "project_saved"));
    assert!(list(&rig, Folder::Projects).is_empty());
}

#[test]
fn test_disconnect_when_listening_returns_no_capture_left() {
    let mut rig = listening("project-disconnect");

    rig.machine.handle(Command::Disconnect);

    assert!(rig.machine.project.is_none());
}

#[test]
fn test_fake_project_send_when_connected_returns_the_project_on_the_cable() {
    let mut rig = listening("project-fake");

    rig.machine.handle(Command::FakeProjectSend);
    rig.machine.poll_incoming();
    support::accept_review(&mut rig);

    assert!(rig.events().iter().any(|e| e.name == "project_saved"));
}

#[test]
fn test_save_project_when_the_same_project_is_saved_again_returns_everything_identical_and_no_new_file(
) {
    let mut rig = TestRig::connected("project-again");
    let project = project_of(&fake_machine::project_messages());
    support::save_project_now(&mut rig, &project, "2026-11-08_100000Z");
    let counts = |rig: &TestRig| {
        (
            list(rig, Folder::Projects).len(),
            list(rig, Folder::Beats).len(),
            list(rig, Folder::Sounds).len(),
        )
    };
    assert_eq!(counts(&rig), (2, 16, 24));
    rig.events();

    rig.machine
        .plan_project(&project, "2026-11-08_110000Z")
        .unwrap();
    let plan = rig.expect("lib_plan");

    assert_eq!(plan.field("identical"), Some(&41usize.into()));
    assert_eq!(plan.field("new"), Some(&0usize.into()));
    assert_eq!(plan.field("modified"), Some(&0usize.into()));
    support::accept_review(&mut rig);
    assert_eq!(
        counts(&rig),
        (2, 16, 24),
        "nothing was written the second time"
    );
    let done = rig
        .events()
        .into_iter()
        .find(|e| e.name == "lib_done")
        .expect("a message");
    assert!(matches!(done.field("lines"), Some(Json::Arr(l)) if l.len() == 41));
}

#[test]
fn test_save_project_when_identical_copies_are_asked_returns_the_files_written_again_so_the_default_is_what_prevents_them(
) {
    // Counter-proof of the test above: the same plan with « keep a second copy » does write everything again.
    let mut rig = TestRig::connected("project-again-copy");
    let project = project_of(&fake_machine::project_messages());
    support::save_project_now(&mut rig, &project, "2026-11-08_100000Z");
    rig.machine
        .plan_project(&project, "2026-11-08_100000Z")
        .unwrap();

    rig.machine
        .handle(Command::Lib(crate::library::command::LibCommand::Apply(
            crate::library::plan::Policy {
                identical: crate::library::plan::OnIdentical::KeepCopy,
                ..crate::library::plan::Policy::default()
            },
        )));

    assert_eq!(list(&rig, Folder::Projects).len(), 4);
    assert_eq!(list(&rig, Folder::Beats).len(), 32);
    assert_eq!(list(&rig, Folder::Sounds).len(), 48);
}

#[test]
fn test_save_project_when_a_beat_changed_returns_it_updated_in_place_and_the_old_one_in_history() {
    let mut rig = TestRig::connected("project-modified");
    let first = project_of(&fake_machine::project_messages());
    support::save_project_now(&mut rig, &first, "2026-11-08_100000Z");
    let mut changed = fake_machine::project_messages();
    let sounds: [Sound; PADS] = std::array::from_fn(|_| Sound::with_defaults());
    changed[3] = beat_message(
        2,
        &synthetic_beat_data("Fake beat 03", &[(0, 5)], 99, &sounds),
    );
    let second = project_of(&changed);
    rig.events();

    rig.machine
        .plan_project(&second, "2026-11-08_110000Z")
        .unwrap();
    let plan = rig.expect("lib_plan");

    assert_eq!(
        plan.field("modified"),
        Some(&2usize.into()),
        "the project and the beat"
    );
    support::accept_review(&mut rig);
    let beats = list(&rig, Folder::Beats);
    assert_eq!(
        beats.len(),
        16,
        "the beat was updated in place: no second folder"
    );
    assert!(beats
        .iter()
        .all(|n| n.starts_with("Project 2026-11-08_100000Z/")));
    let (items, _) = rig.machine.library().list(Kind::Beats).unwrap();
    let updated = items
        .iter()
        .find(|i| i.name == "Fake beat 03")
        .expect("the beat");
    assert_eq!(updated.history.len(), 1);
}

fn unnamed_project(sound_of: impl Fn(usize, usize) -> Sound) -> Project {
    let mut messages = vec![header_message(0)];
    for beat in 0..BEATS {
        let sounds: [Sound; PADS] = std::array::from_fn(|pad| sound_of(beat, pad));
        let name = if beat == 0 { "" } else { "Groove" };
        messages.push(beat_message(
            beat as u8,
            &synthetic_beat_data(name, &[], 0, &sounds),
        ));
    }
    project_of(&messages)
}

#[test]
fn test_plan_sounds_when_every_pad_holds_the_same_sound_returns_one_entry_and_the_rest_as_duplicates(
) {
    let project = unnamed_project(|_, _| Sound::with_defaults());

    let (planned, duplicates) = plan_sounds(&project);

    assert_eq!(planned.len(), 1);
    assert_eq!(duplicates, 511);
}

#[test]
fn test_plan_sounds_when_sounds_differ_returns_them_in_beat_then_pad_order_with_names_from_beat_and_pad(
) {
    let project = unnamed_project(|beat, pad| {
        let mut sound = Sound::with_defaults();
        let _ = sound.set(34, (beat * 2 + pad % 2) as u16);
        let _ = sound.set_name(if pad == 0 { "Kick" } else { "" });
        sound
    });

    let (planned, duplicates) = plan_sounds(&project);

    assert_eq!(planned.len(), 48);
    assert_eq!(duplicates, 464);
    assert_eq!(planned[0].stem, "01 Beat 01 pad 01 Kick");
    assert_eq!(planned[1].stem, "01 Beat 01 pad 02");
    assert_eq!(planned[2].stem, "01 Beat 01 pad 03");
    assert_eq!(planned[3].stem, "02 Groove pad 01 Kick");
}

#[test]
fn test_plan_sounds_when_a_pad_has_a_different_sound_than_the_first_returns_it_not_as_duplicate() {
    // Counter-proof of the duplicate rule: one byte apart is a different sound.
    let project = unnamed_project(|beat, pad| {
        let mut sound = Sound::with_defaults();
        if beat == 7 && pad == 9 {
            let _ = sound.set(34, 1);
        }
        sound
    });

    let (planned, duplicates) = plan_sounds(&project);

    assert_eq!(planned.len(), 2);
    assert_eq!(duplicates, 510);
    assert_eq!(planned[1].stem, "08 Groove pad 10");
}

#[test]
fn test_receive_when_a_beat_has_no_name_returns_a_file_named_beat_and_its_number() {
    let mut rig = TestRig::connected("project-unnamed");
    let project = unnamed_project(|_, _| Sound::with_defaults());

    support::save_project_now(&mut rig, &project, "x");

    assert!(list(&rig, Folder::Beats)[0].ends_with("/01 Beat 01.syx"));
}
