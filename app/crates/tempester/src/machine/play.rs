//! Play: on-screen pads send notes on the chosen channel; the app can be the clock master of a Tempest set to
//! slave (MIDI Start `FA`, 24 clocks `F8` per quarter note at the chosen tempo, Stop `FC`), like
//! `tools/tempest_clock.swift`. The clock runs on its own thread so the worker stays free; the Euclidean sequencer
//! (`sequencer`) plays on that same thread, right after each clock byte.

use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tempest_midi::paced::{Clock, SystemClock};
use tempest_midi::{CancelToken, MidiError, Transport};

use super::sequencer::{Message, PadPattern, Sequencer};
use super::Machine;
use crate::command::Command;
use crate::error::AppError;
use crate::event::Event;

const START: u8 = 0xFA;
const CLOCK: u8 = 0xF8;
const STOP: u8 = 0xFC;
const CLOCKS_PER_QUARTER: f64 = 24.0;

/// The running clock thread.
pub(super) struct ClockHandle {
    cancel: CancelToken,
    bpm: Arc<AtomicU16>,
    thread: JoinHandle<()>,
}

/// A seed for the probability draws: the start time, so that two runs differ.
fn seed() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |since| since.subsec_nanos() ^ (since.as_secs() as u32))
}

fn seq_pad_event(pad: u8, pattern: &PadPattern) -> Event {
    Event::new("seq_pad")
        .with("pad", pad)
        .with("steps", pattern.steps())
        .with("pulses", pattern.pulses())
        .with("rotation", pattern.rotation())
        .with("probability", pattern.probability())
        .with("pattern", pattern.text())
}

fn seq_run_event(enabled: bool, velocity: u8) -> Event {
    Event::new("seq_run")
        .with("enabled", enabled)
        .with("velocity", velocity)
}

/// Time between two clock bytes at `bpm` quarter notes per minute.
pub(super) fn tick(bpm: u16) -> Duration {
    Duration::from_secs_f64(60.0 / (f64::from(bpm.max(1)) * CLOCKS_PER_QUARTER))
}

/// Start, clocks until `cancel` fires (the tempo is read before every tick), Stop. The schedule accumulates the
/// ticks from the start instead of sleeping a fixed time after each send, so that the tempo does not drift. The
/// sequencer's notes go out right after the clock byte they belong to; when the clock stops, every note still
/// sounding gets its Note Off before the Stop, and the Stop is sent even when a Note Off fails.
pub(super) fn run_clock(
    transport: &dyn Transport,
    clock: &dyn Clock,
    bpm: &AtomicU16,
    cancel: &CancelToken,
    mut sequencer: Option<&mut Sequencer>,
) -> Result<(), MidiError> {
    transport.send(&[START])?;
    let mut due = Duration::ZERO;
    while !cancel.is_cancelled() {
        transport.send(&[CLOCK])?;
        if let Some(sequencer) = sequencer.as_deref_mut() {
            send_all(transport, &sequencer.on_clock())?;
        }
        due += tick(bpm.load(Ordering::SeqCst));
        if let Some(wait) = due.checked_sub(clock.elapsed()) {
            clock.sleep(wait);
        }
    }
    let released = match sequencer {
        Some(sequencer) => send_all(transport, &sequencer.release_all()),
        None => Ok(()),
    };
    let stopped = transport.send(&[STOP]);
    released.and(stopped)
}

fn send_all(transport: &dyn Transport, messages: &[Message]) -> Result<(), MidiError> {
    messages
        .iter()
        .try_for_each(|message| transport.send(message))
}

impl Machine {
    pub(super) fn handle_play(&mut self, command: Command) -> Option<Command> {
        match command {
            Command::Note { on, note, velocity } => {
                self.guard("note", |m| m.send_note(on, note, velocity));
            }
            Command::ClockStart(bpm) => self.guard("clock_start", |m| m.start_clock(bpm)),
            Command::ClockBpm(bpm) => self.guard("clock_bpm", |m| m.change_tempo(bpm)),
            Command::ClockStop => self.stop_clock(),
            Command::SeqPad { pad, pattern } => self.set_seq_pad(pad, pattern),
            Command::SeqRun { enabled, velocity } => self.set_seq_run(enabled, velocity),
            Command::SeqState => self.publish_seq(),
            Command::Transport { start } => self.guard("transport", |m| m.transport(start)),
            other => return Some(other),
        }
        None
    }

    /// True while the clock thread runs (it ends by itself if the port fails).
    pub(super) fn clock_running(&self) -> bool {
        self.clock
            .as_ref()
            .is_some_and(|clock| !clock.thread.is_finished())
    }

    pub(super) fn send_note(&mut self, on: bool, note: u8, velocity: u8) -> Result<(), AppError> {
        let status = if on { 0x90 } else { 0x80 } | self.channel_index();
        let sent = self
            .idle_session()?
            .transport
            .send(&[status, note, velocity]);
        sent.map_err(|e| {
            let error = AppError::from(e);
            self.note_failure(&error);
            error
        })
    }

    /// Start or Stop for a Tempest set to slave. Stop also ends the app's own clock (which sends its Stop on its
    /// way out); Start would double the clock's own Start, so it is refused while the clock runs.
    fn transport(&mut self, start: bool) -> Result<(), AppError> {
        self.idle_session()?;
        if self.clock_running() {
            if start {
                return Err(AppError::Precondition("the clock is already running"));
            }
            self.stop_clock();
            return Ok(());
        }
        self.send_bytes(&[if start { START } else { STOP }])
    }

    fn start_clock(&mut self, bpm: u16) -> Result<(), AppError> {
        let transport = Arc::clone(&self.idle_session()?.transport);
        if self.clock_running() {
            return Err(AppError::Precondition("the clock is already running"));
        }
        let (cancel, tempo) = (CancelToken::new(), Arc::new(AtomicU16::new(bpm)));
        let (job_cancel, job_tempo, emit) = (cancel.clone(), Arc::clone(&tempo), self.emit.clone());
        let channel = self.channel_index();
        self.sequencer.update(|settings| settings.channel = channel);
        let mut sequencer = Sequencer::new(self.sequencer.clone(), seed());
        let thread = thread::Builder::new()
            .name("tempester-clock".into())
            .spawn(move || {
                let result = run_clock(
                    transport.as_ref(),
                    &SystemClock::new(),
                    &job_tempo,
                    &job_cancel,
                    Some(&mut sequencer),
                );
                let event = Event::new("clock").with("running", false);
                emit(match result {
                    Ok(()) => event,
                    Err(e) => event.with("message", e.to_string()),
                });
            })?;
        self.clock = Some(ClockHandle {
            cancel,
            bpm: tempo,
            thread,
        });
        self.emit(Event::new("clock").with("running", true).with("bpm", bpm));
        Ok(())
    }

    fn set_seq_pad(&mut self, pad: u8, pattern: PadPattern) {
        self.sequencer
            .update(|settings| settings.pads[usize::from(pad)] = pattern);
        self.emit(seq_pad_event(pad, &pattern));
    }

    fn set_seq_run(&mut self, enabled: bool, velocity: u8) {
        self.sequencer.update(|settings| {
            settings.enabled = enabled;
            settings.velocity = velocity;
        });
        self.emit(seq_run_event(enabled, velocity));
    }

    /// The page asks for the whole sequencer state when it is built.
    fn publish_seq(&mut self) {
        let settings = self.sequencer.get();
        self.emit(seq_run_event(settings.enabled, settings.velocity));
        for (pad, pattern) in settings.pads.iter().enumerate() {
            self.emit(seq_pad_event(pad as u8, pattern));
        }
    }

    fn change_tempo(&mut self, bpm: u16) -> Result<(), AppError> {
        match &self.clock {
            Some(clock) if self.clock_running() => {
                clock.bpm.store(bpm, Ordering::SeqCst);
                self.emit(Event::new("clock").with("running", true).with("bpm", bpm));
                Ok(())
            }
            _ => Err(AppError::Precondition("the clock is not running")),
        }
    }

    /// Stops the clock thread (it sends Stop on its way out) and waits for it.
    pub(super) fn stop_clock(&mut self) {
        if let Some(clock) = self.clock.take() {
            clock.cancel.cancel();
            // A panicked clock thread has nothing more to tell: it only sends bytes.
            let _ = clock.thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::testkit::TestRig;
    use std::cell::Cell;
    use std::time::Instant;
    use tempest_midi::FakeTransport;

    /// A clock that only moves when asked to sleep, and runs `on_sleep(count)` after each sleep.
    struct ManualClock<'a> {
        now: Cell<Duration>,
        sleeps: Cell<usize>,
        on_sleep: &'a dyn Fn(usize),
    }

    impl<'a> ManualClock<'a> {
        fn new(on_sleep: &'a dyn Fn(usize)) -> Self {
            Self {
                now: Cell::new(Duration::ZERO),
                sleeps: Cell::new(0),
                on_sleep,
            }
        }
    }

    impl Clock for ManualClock<'_> {
        fn elapsed(&self) -> Duration {
            self.now.get()
        }

        fn sleep(&self, duration: Duration) {
            self.now.set(self.now.get() + duration);
            self.sleeps.set(self.sleeps.get() + 1);
            (self.on_sleep)(self.sleeps.get());
        }
    }

    fn run_for(ticks: usize, bpm: u16, change_at: Option<(usize, u16)>) -> (Vec<u8>, Duration) {
        let (fake, cancel, tempo) = (
            FakeTransport::new(),
            CancelToken::new(),
            AtomicU16::new(bpm),
        );
        let on_sleep = |count: usize| {
            if let Some((at, new_bpm)) = change_at {
                if count == at {
                    tempo.store(new_bpm, Ordering::SeqCst);
                }
            }
            if count == ticks {
                cancel.cancel();
            }
        };
        let clock = ManualClock::new(&on_sleep);
        let result = run_clock(&fake, &clock, &tempo, &cancel, None);
        assert_eq!(result, Ok(()));
        (fake.sent_bytes(), clock.elapsed())
    }

    #[test]
    fn test_run_clock_when_stopped_after_48_ticks_at_120_bpm_returns_start_48_clocks_stop_in_one_second(
    ) {
        let (bytes, elapsed) = run_for(48, 120, None);

        assert_eq!(bytes.first(), Some(&START));
        assert_eq!(bytes.last(), Some(&STOP));
        assert_eq!(bytes.iter().filter(|b| **b == CLOCK).count(), 48);
        assert!((elapsed.as_secs_f64() - 1.0).abs() < 1e-6, "{elapsed:?}");
    }

    #[test]
    fn test_run_clock_when_tempo_doubles_after_two_ticks_returns_shorter_following_ticks() {
        let (_, elapsed) = run_for(4, 120, Some((2, 240)));

        let expected = tick(120) * 2 + tick(240) * 2;
        assert!(
            (elapsed.as_secs_f64() - expected.as_secs_f64()).abs() < 1e-6,
            "{elapsed:?}"
        );
    }

    #[test]
    fn test_run_clock_when_cancelled_before_the_first_tick_returns_start_then_stop_only() {
        let (fake, cancel, tempo) = (
            FakeTransport::new(),
            CancelToken::new(),
            AtomicU16::new(120),
        );
        cancel.cancel();
        let noop = |_: usize| {};

        let result = run_clock(&fake, &ManualClock::new(&noop), &tempo, &cancel, None);

        assert_eq!(result, Ok(()));
        assert_eq!(fake.sent_bytes(), vec![START, STOP]);
    }

    #[test]
    fn test_run_clock_when_port_fails_returns_error_and_no_stop_byte() {
        let (fake, cancel, tempo) = (
            FakeTransport::new(),
            CancelToken::new(),
            AtomicU16::new(120),
        );
        fake.fail_sends();
        let noop = |_: usize| {};

        let result = run_clock(&fake, &ManualClock::new(&noop), &tempo, &cancel, None);

        assert!(matches!(result, Err(MidiError::Send(_))));
        assert!(fake.sent().is_empty());
    }

    #[test]
    fn test_tick_when_120_bpm_returns_one_forty_eighth_of_a_second() {
        assert!((tick(120).as_secs_f64() - 1.0 / 48.0).abs() < 1e-9);
    }

    #[test]
    fn test_send_note_when_channel_10_returns_status_99_and_80() {
        let mut rig = TestRig::connected("note");
        rig.machine.handle(Command::SetChannel(10));

        rig.machine.handle(Command::Note {
            on: true,
            note: 36,
            velocity: 100,
        });
        rig.machine.handle(Command::Note {
            on: false,
            note: 36,
            velocity: 0,
        });

        assert_eq!(
            rig.transport().sent(),
            vec![vec![0x99, 36, 100], vec![0x89, 36, 0]]
        );
    }

    #[test]
    fn test_transport_when_start_then_stop_returns_fa_then_fc() {
        let mut rig = TestRig::connected("transport");

        rig.machine.handle(Command::Transport { start: true });
        rig.machine.handle(Command::Transport { start: false });

        assert_eq!(rig.transport().sent(), vec![vec![START], vec![STOP]]);
    }

    #[test]
    fn test_transport_when_not_connected_returns_not_connected_error() {
        let mut rig = TestRig::new("transport-off");

        rig.machine.handle(Command::Transport { start: true });

        assert_eq!(
            rig.expect("error").field("code"),
            Some(&"not_connected".into())
        );
    }

    fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !condition() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn test_clock_when_started_and_stopped_returns_start_clocks_then_stop_last() {
        let mut rig = TestRig::connected("clock");

        rig.machine.handle(Command::ClockStart(300));
        let fake = rig.transport();
        wait_until("three clock bytes", || {
            fake.sent_bytes().iter().filter(|b| **b == CLOCK).count() >= 3
        });
        rig.machine.handle(Command::ClockBpm(240));
        rig.machine.handle(Command::ClockStop);

        let bytes = fake.sent_bytes();
        assert_eq!(bytes.first(), Some(&START));
        assert_eq!(bytes.last(), Some(&STOP));
        assert!(rig
            .events()
            .iter()
            .any(|e| e.name == "clock" && e.field("running") == Some(&false.into())));
        assert!(!rig.machine.clock_running());
    }

    #[test]
    fn test_transport_when_clock_runs_returns_stop_ending_the_clock_and_start_refused() {
        let mut rig = TestRig::connected("transport-clock");
        rig.machine.handle(Command::ClockStart(300));
        let fake = rig.transport();
        wait_until("a clock byte", || fake.sent_bytes().contains(&CLOCK));

        rig.machine.handle(Command::Transport { start: true });
        assert_eq!(
            rig.expect("error").field("code"),
            Some(&"precondition".into())
        );
        rig.machine.handle(Command::Transport { start: false });

        assert!(!rig.machine.clock_running());
        assert_eq!(fake.sent_bytes().last(), Some(&STOP));
        assert_eq!(fake.sent_bytes().iter().filter(|b| **b == START).count(), 1);
    }

    #[test]
    fn test_clock_start_when_already_running_returns_precondition_error() {
        let mut rig = TestRig::connected("clock-twice");
        rig.machine.handle(Command::ClockStart(60));

        rig.machine.handle(Command::ClockStart(60));

        let codes: Vec<_> = rig
            .events()
            .iter()
            .filter(|e| e.name == "error")
            .map(|e| e.field("code").cloned())
            .collect();
        assert_eq!(codes, vec![Some("precondition".into())]);
        rig.machine.handle(Command::ClockStop);
    }

    #[test]
    fn test_clock_bpm_when_not_running_returns_precondition_error() {
        let mut rig = TestRig::connected("clock-bpm-off");

        rig.machine.handle(Command::ClockBpm(100));

        assert_eq!(
            rig.expect("error").field("code"),
            Some(&"precondition".into())
        );
    }

    #[test]
    fn test_os_send_when_clock_running_returns_precondition_error_and_sends_no_os_bytes() {
        let mut rig = TestRig::connected("clock-os");
        rig.machine.handle(Command::ClockStart(60));
        let file = tempest_proto::os_image::encode(
            tempest_proto::os_image::OsId::Sam,
            &vec![1u8; 32 * 1024],
        )
        .unwrap();
        rig.machine.handle(Command::OsValidate {
            name: "s".into(),
            data: file,
        });
        rig.events();

        rig.machine.handle(Command::OsSend);

        assert_eq!(
            rig.expect("error").field("code"),
            Some(&"precondition".into())
        );
        rig.machine.handle(Command::ClockStop);
        assert!(rig.transport().sent().iter().all(|p| p.len() == 1));
    }

    #[test]
    fn test_disconnect_when_clock_running_returns_stop_sent_and_clock_ended() {
        let mut rig = TestRig::connected("clock-disc");
        rig.machine.handle(Command::ClockStart(300));
        let fake = rig.transport();
        wait_until("a clock byte", || fake.sent_bytes().contains(&CLOCK));

        rig.machine.handle(Command::Disconnect);

        assert_eq!(fake.sent_bytes().last(), Some(&STOP));
        assert!(!rig.machine.clock_running());
    }

    fn playing(pads: &[(usize, PadPattern)]) -> Sequencer {
        let shared = super::super::sequencer::Shared::default();
        shared.update(|settings| {
            settings.enabled = true;
            settings.channel = 9;
            for (pad, pattern) in pads {
                settings.pads[*pad] = *pattern;
            }
        });
        Sequencer::new(shared, 1)
    }

    fn every_step() -> PadPattern {
        PadPattern::new(1, 1, 0, 100).expect("valid")
    }

    #[test]
    fn test_run_clock_when_cancelled_inside_a_gate_returns_the_note_off_before_the_stop() {
        let (fake, cancel, tempo) = (
            FakeTransport::new(),
            CancelToken::new(),
            AtomicU16::new(120),
        );
        let on_sleep = |count: usize| {
            if count == 2 {
                cancel.cancel();
            }
        };
        let mut sequencer = playing(&[(0, every_step())]);

        let result = run_clock(
            &fake,
            &ManualClock::new(&on_sleep),
            &tempo,
            &cancel,
            Some(&mut sequencer),
        );

        assert_eq!(result, Ok(()));
        assert_eq!(
            fake.sent(),
            vec![
                vec![START],
                vec![CLOCK],
                vec![0x99, 36, 100],
                vec![CLOCK],
                vec![0x89, 36, 0],
                vec![STOP],
            ]
        );
    }

    #[test]
    fn test_run_clock_when_the_sequencer_plays_for_two_steps_returns_each_note_on_after_its_clock_byte(
    ) {
        let (fake, cancel, tempo) = (
            FakeTransport::new(),
            CancelToken::new(),
            AtomicU16::new(120),
        );
        let on_sleep = |count: usize| {
            if count == 13 {
                cancel.cancel();
            }
        };
        let mut sequencer = playing(&[(1, every_step())]);

        run_clock(
            &fake,
            &ManualClock::new(&on_sleep),
            &tempo,
            &cancel,
            Some(&mut sequencer),
        )
        .expect("runs");

        let sent = fake.sent();
        let at = |packet: &[u8]| sent.iter().position(|p| p == packet);
        // Clock bytes 0 and 6 are steps 0 and 1: each note-on directly follows its clock byte.
        assert_eq!(sent[1], vec![CLOCK]);
        assert_eq!(sent[2], vec![0x99, 38, 100], "pad A2 is note 38");
        assert_eq!(
            at(&[0x89, 38, 0]),
            Some(6),
            "the off follows the third clock after the on"
        );
        assert_eq!(sent.iter().filter(|p| p[0] == 0x99).count(), 3);
        assert_eq!(
            sent.iter().filter(|p| p[0] == 0x89).count(),
            3,
            "every note ends"
        );
        assert_eq!(sent.last(), Some(&vec![STOP]));
    }

    #[test]
    fn test_clock_when_the_sequencer_runs_on_the_fake_machine_returns_every_note_on_ended_and_the_stop_last(
    ) {
        let mut rig = TestRig::connected("seq-clock");
        rig.machine.handle(Command::SeqPad {
            pad: 0,
            pattern: every_step(),
        });
        rig.machine.handle(Command::SeqRun {
            enabled: true,
            velocity: 90,
        });
        rig.machine.handle(Command::ClockStart(300));
        let fake = rig.transport();
        wait_until("two notes", || {
            fake.sent().iter().filter(|p| p[0] == 0x90).count() >= 2
        });

        rig.machine.handle(Command::ClockStop);

        let sent = fake.sent();
        let (ons, offs) = (
            sent.iter().filter(|p| p[0] == 0x90).count(),
            sent.iter().filter(|p| p[0] == 0x80).count(),
        );
        assert!(ons >= 2);
        assert_eq!(ons, offs, "no note is left sounding");
        assert_eq!(sent.last(), Some(&vec![STOP]));
        assert!(sent.contains(&vec![0x90, 36, 90]));
    }

    #[test]
    fn test_seq_pad_when_set_returns_the_pattern_event_and_the_state_is_published_on_request() {
        let mut rig = TestRig::connected("seq-state");
        let pattern = PadPattern::new(8, 3, 1, 80).expect("valid");

        rig.machine.handle(Command::SeqPad { pad: 4, pattern });
        let set = rig.expect("seq_pad");
        rig.machine.handle(Command::SeqState);

        assert_eq!(set.field("pattern"), Some(&".x..x..x".into()));
        let events = rig.events();
        assert_eq!(events.iter().filter(|e| e.name == "seq_pad").count(), 16);
        assert!(events.iter().any(|e| e.name == "seq_run"));
        let fifth = events
            .iter()
            .filter(|e| e.name == "seq_pad")
            .nth(4)
            .expect("pad 5");
        assert_eq!(fifth.field("probability"), Some(&80u8.into()));
    }
}
