//! The MIDI worker: one thread owns the connection and runs the commands of the page one at a time, so the UI
//! thread never waits on MIDI. Long jobs that must not block it (the OS send, the clock) run on their own threads
//! and report through the same `Emitter`.
//!
//! One file per subject: `connection` (ports, identity, state), `panel` (NRPN), `sound`, `beat`, `library`,
//! `manager` (the library manager: duplicate check, review, item actions, sending an item), `kit` (library sounds onto the pads of the machine's beat), `midi_export` (a beat as a Standard MIDI File), `backup`, `project`, `project_send` (a whole project back to the machine), `os_update`, `os_verify` (read the installed OS back from the machine), `play` (pads, clock master), `pad_map` (the machine's note of each pad: kept, edited, learned from the machine), `sequencer` (Euclidean and probability sequencer on the clock thread), `remote_panel` (keys, LEDs, controls of the 1.6.0.0 remote panel), `perform` (beat-wide positions, FX ribbons, play probability of 1.6.2.0 / 1.6.3.0), `editor` (the Panel page's editing tools: history, A/B, variations, MIDI learn), `monitor` (every message in and out, for the Machine page); `connector` and `fake_machine` stand in for real ports.

mod backup;
mod beat;
pub mod connection;
pub mod connector;
mod editor;
pub mod fake_machine;
mod fake_panel;
mod kit;
mod library;
mod manager;
mod midi_export;
mod monitor;
mod os_update;
mod os_verify;
mod pad_map;
mod panel;
mod perform;
mod play;
mod project;
mod project_send;
mod remote_panel;
mod send;
pub mod sequencer;
mod sound;
#[cfg(test)]
pub mod testkit;

use std::collections::BTreeSet;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use tempest_midi::message::Message;
use tempest_midi::paced::DEFAULT_RATE;
use tempest_midi::CancelToken;
use tempest_proto::beat::BeatSummary;
use tempest_proto::nrpn::{self, NrpnDecoder};
use tempest_proto::pad_notes::PadNotes;
use tempest_proto::performance::BeatWideReader;
use tempest_proto::remote;
use tempest_proto::sound::Sound;

use crate::command::Command;
use crate::error::AppError;
use crate::event::{Emitter, Event};
use crate::library::plan::Candidate;
use crate::storage::Storage;
use backup::Capture;
use connection::Session;
use connector::Connector;
use monitor::Monitor;
use os_update::PendingOs;
use os_verify::VerifyWatch;
use play::ClockHandle;
use project::ProjectCapture;
use project_send::SentProject;
use remote_panel::RemoteState;

/// How long the loop waits for a command before looking at incoming MIDI again.
const TICK: Duration = Duration::from_millis(5);

/// A beat kept as the machine sent it, with its summary.
struct BeatData {
    message: Vec<u8>,
    summary: BeatSummary,
}

/// A batch waiting for the user's choice (the review dialog): nothing is written until `lib_apply`.
struct PendingPlan {
    candidates: Vec<Candidate>,
    title: String,
    /// `sound`, `beat`, `import` or `project`.
    source: &'static str,
    project: Option<project::ProjectNote>,
}

/// The state of the worker.
pub struct Machine {
    emit: Emitter,
    storage: Storage,
    connector: Box<dyn Connector>,
    session: Option<Session>,
    fetched_sound: Option<Sound>,
    /// When the sound / the beat was fetched: the date the library gives the item (the machine stores none).
    fetched_sound_at: i64,
    fetched_beat_at: i64,
    plan: Option<PendingPlan>,
    project_note: Option<project::ProjectNote>,
    opened_sound: Option<Sound>,
    fetched_beat: Option<BeatData>,
    opened_beat: Option<BeatData>,
    backup: Option<Capture>,
    project: Option<ProjectCapture>,
    /// True while a project goes to the machine on its own thread: nothing else may talk on the port.
    project_sending: Arc<AtomicBool>,
    project_cancel: Option<CancelToken>,
    /// How long a request for the machine's project waits for its first message (shortened by tests).
    fetch_wait: Duration,
    sent_project: Option<SentProject>,
    pending_os: Option<PendingOs>,
    verify: Option<VerifyWatch>,
    os_active: Arc<AtomicBool>,
    os_cancel: Option<CancelToken>,
    clock: Option<ClockHandle>,
    /// The Euclidean sequencer's settings, shared with the clock thread.
    sequencer: sequencer::Shared,
    /// Bytes per second of a paced SysEx send: a MIDI cable's rate, lowered or raised only by tests.
    rate: u32,
    nrpn_in: NrpnDecoder,
    /// Remote-panel keys pressed and not yet released, so that none stays down on the machine.
    held_keys: BTreeSet<u8>,
    /// FX ribbons touched by the app and not yet released (n°31), so that none stays touched on the machine.
    touched_ribbons: BTreeSet<u8>,
    /// The beat-wide positions reported by the machine or sent by the app (n°29).
    beat_wide_in: BeatWideReader,
    remote: RemoteState,
    monitor: Monitor,
    /// The Panel page's editing tools: history, A/B, variations, MIDI learn.
    edit: editor::EditState,
    /// The machine's note of each pad, as the app knows it (`pad_map`).
    pad_notes: PadNotes,
    pad_learn: pad_map::PadLearn,
}

impl Machine {
    pub fn new(connector: Box<dyn Connector>, storage: Storage, emit: Emitter) -> Self {
        let edit = editor::EditState::load(&storage);
        let pad_notes = storage.load_settings().pad_notes();
        let mut machine = Self {
            emit,
            storage,
            connector,
            session: None,
            fetched_sound: None,
            fetched_sound_at: 0,
            fetched_beat_at: 0,
            plan: None,
            project_note: None,
            opened_sound: None,
            fetched_beat: None,
            opened_beat: None,
            backup: None,
            project: None,
            project_sending: Arc::new(AtomicBool::new(false)),
            project_cancel: None,
            fetch_wait: project::REQUEST_WAIT,
            sent_project: None,
            pending_os: None,
            verify: None,
            os_active: Arc::new(AtomicBool::new(false)),
            os_cancel: None,
            clock: None,
            sequencer: sequencer::Shared::default(),
            rate: DEFAULT_RATE,
            nrpn_in: NrpnDecoder::new(),
            held_keys: BTreeSet::new(),
            touched_ribbons: BTreeSet::new(),
            beat_wide_in: BeatWideReader::default(),
            remote: RemoteState::Unknown,
            monitor: Monitor::new(),
            edit,
            pad_notes,
            pad_learn: pad_map::PadLearn::default(),
        };
        machine.apply_pad_notes(pad_notes);
        machine
    }

    fn emit(&self, event: Event) {
        (self.emit)(event);
    }

    /// Runs `action`; a failure becomes an `error` event naming `during`.
    fn guard(
        &mut self,
        during: &'static str,
        action: impl FnOnce(&mut Self) -> Result<(), AppError>,
    ) {
        if let Err(error) = action(self) {
            self.emit(Event::error(during, &error));
        }
    }

    /// Runs one command of the page: each handler takes its own commands and hands back the others.
    pub fn handle(&mut self, command: Command) {
        let handlers: [fn(&mut Self, Command) -> Option<Command>; 15] = [
            Self::handle_link,
            Self::handle_edit,
            Self::handle_monitor,
            Self::handle_panel,
            Self::handle_remote,
            Self::handle_perform,
            Self::handle_sound,
            Self::handle_beat,
            Self::handle_manager,
            Self::handle_library,
            Self::handle_backup,
            Self::handle_project,
            Self::handle_os,
            Self::handle_play,
            Self::handle_pad_map,
        ];
        let mut rest = Some(command);
        for handler in handlers {
            rest = rest.and_then(|command| handler(self, command));
        }
        if rest.is_some() {
            let why = AppError::Invalid("this command has no handler".into());
            self.emit(Event::error("command", &why));
        }
    }

    /// Looks at what the machine sent since the last call: knob moves become `nrpn` events, SysEx feed the backup
    /// capture when one is listening.
    pub fn poll_incoming(&mut self) {
        self.poll_monitor(Instant::now());
        self.expire_project(Instant::now());
        self.poll_controller();
        let now = Instant::now();
        let Some(session) = &self.session else { return };
        let mut leds = Vec::new();
        for message in session.listener.drain() {
            match message {
                Message::Channel(short) => {
                    let &[controller, value] = short.data() else {
                        continue;
                    };
                    match remote::led_change(short.status, controller, value) {
                        Some(change) => {
                            self.observe_led(now, change);
                            leds.push(change);
                        }
                        None => {
                            self.observe_note(now, short.status, short.data());
                            self.on_channel(short.status, short.data());
                        }
                    }
                }
                Message::SysEx(bytes) => {
                    self.on_sysex(&bytes);
                    self.on_project_sysex(&bytes);
                }
                _ => {}
            }
        }
        self.on_led_changes(&leds);
        self.settle_pad_learn(now, false);
        self.check_probe();
    }

    fn on_channel(&mut self, status: u8, data: &[u8]) {
        let &[controller, value] = data else { return };
        if self.on_performance_report(status, controller, value) {
            return;
        }
        if let Some(change) = self.nrpn_in.feed_message(status, controller, value) {
            if nrpn::is_tempest_number(change.number) {
                self.edit_observe(change.number, change.value);
                self.emit(
                    Event::new("nrpn")
                        .with("number", change.number)
                        .with("value", change.value)
                        .with("channel", change.channel + 1),
                );
            }
        }
    }

    /// Runs until the page's side of the channel is dropped.
    fn run(mut self, commands: &Receiver<Command>) {
        loop {
            match commands.recv_timeout(TICK) {
                Ok(first) => {
                    for command in coalesce(drain_pending(first, commands)) {
                        if command == Command::Shutdown {
                            return self.shutdown();
                        }
                        self.handle(command);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            self.poll_incoming();
        }
        self.shutdown();
    }

    /// Stops what runs on other threads; nothing stays half-sent by our choice.
    fn shutdown(&mut self) {
        self.release_held_keys();
        self.release_touched_ribbons();
        self.stop_clock();
        if let Some(cancel) = &self.os_cancel {
            cancel.cancel();
        }
        if let Some(cancel) = &self.project_cancel {
            cancel.cancel();
        }
    }
}

fn drain_pending(first: Command, commands: &Receiver<Command>) -> Vec<Command> {
    let mut batch = vec![first];
    batch.extend(commands.try_iter());
    batch
}

/// Keeps only the last NRPN of each number, the last CC of each controller, the last value of each remote control, of each beat-wide position, of each pad's probability, the last touch of each ribbon and the last tempo change: a knob dragged fast queues many values and
/// only the final one matters (last value wins).
pub fn coalesce(batch: Vec<Command>) -> Vec<Command> {
    let superseded = |at: usize, command: &Command| {
        batch[at + 1..].iter().any(|later| match (command, later) {
            (Command::Nrpn { number: a, .. }, Command::Nrpn { number: b, .. }) => a == b,
            (Command::Cc { controller: a, .. }, Command::Cc { controller: b, .. }) => a == b,
            (Command::Control { index: a, .. }, Command::Control { index: b, .. }) => a == b,
            (Command::BeatWide { index: a, .. }, Command::BeatWide { index: b, .. }) => a == b,
            (Command::PadProbability { pad: a, .. }, Command::PadProbability { pad: b, .. }) => {
                a == b
            }
            // Only a touch gives way to a later touch of the same ribbon: a release always goes out, so that a quick
            // tap is a touch and a release on the machine.
            (
                Command::Ribbon {
                    ribbon: a,
                    position: Some(_),
                    ..
                },
                Command::Ribbon {
                    ribbon: b,
                    position: Some(_),
                    ..
                },
            ) => a == b,
            (Command::ClockBpm(_), Command::ClockBpm(_)) => true,
            _ => false,
        })
    };
    let keep: Vec<bool> = batch
        .iter()
        .enumerate()
        .map(|(at, command)| !superseded(at, command))
        .collect();
    batch
        .into_iter()
        .zip(keep)
        .filter_map(|(command, keep)| keep.then_some(command))
        .collect()
}

/// The running worker: the page's way in, and what the window needs to close it cleanly.
pub struct Worker {
    pub commands: Sender<Command>,
    /// True while an OS image is being sent: the window must not close then.
    pub os_active: Arc<AtomicBool>,
    thread: thread::JoinHandle<()>,
}

impl Worker {
    /// Stops the worker (the clock sends its Stop first) and waits for it.
    pub fn stop(self) {
        // A worker that is already gone has nothing left to stop.
        let _ = self.commands.send(Command::Shutdown);
        let _ = self.thread.join();
    }
}

/// Starts the worker thread; the page talks to it through `Worker::commands`.
pub fn spawn(
    connector: Box<dyn Connector>,
    storage: Storage,
    emit: Emitter,
) -> Result<Worker, AppError> {
    let (commands, receiver) = mpsc::channel();
    let machine = Machine::new(connector, storage, emit);
    let os_active = Arc::clone(&machine.os_active);
    let thread = thread::Builder::new()
        .name("tempester-midi".into())
        .spawn(move || machine.run(&receiver))?;
    Ok(Worker {
        commands,
        os_active,
        thread,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nrpn(number: u16, value: u16) -> Command {
        Command::Nrpn { number, value }
    }

    #[test]
    fn test_coalesce_when_same_nrpn_repeated_returns_only_the_last_value() {
        let batch = vec![nrpn(34, 1), nrpn(34, 2), nrpn(34, 3)];

        assert_eq!(coalesce(batch), vec![nrpn(34, 3)]);
    }

    #[test]
    fn test_coalesce_when_different_numbers_interleave_returns_last_of_each_in_order() {
        let batch = vec![nrpn(34, 1), nrpn(35, 1), nrpn(34, 2), nrpn(35, 2)];

        assert_eq!(coalesce(batch), vec![nrpn(34, 2), nrpn(35, 2)]);
    }

    #[test]
    fn test_coalesce_when_other_commands_between_returns_them_untouched() {
        let batch = vec![
            nrpn(34, 1),
            Command::FetchSound,
            nrpn(34, 2),
            Command::FetchSound,
        ];

        assert_eq!(
            coalesce(batch),
            vec![Command::FetchSound, nrpn(34, 2), Command::FetchSound]
        );
    }

    #[test]
    fn test_coalesce_when_ribbons_and_beat_wide_repeat_returns_last_touch_every_release_and_last_position(
    ) {
        let touch = |ribbon, p| Command::Ribbon {
            ribbon,
            position: Some(p),
            pressure: None,
        };
        let release = |ribbon| Command::Ribbon {
            ribbon,
            position: None,
            pressure: None,
        };
        let wide = |index, position| Command::BeatWide { index, position };
        let batch = vec![
            touch(0, 1),
            touch(1, 9),
            touch(0, 2),
            release(0),
            wide(3, 10),
            wide(3, 11),
            release(1),
        ];

        assert_eq!(
            coalesce(batch),
            vec![
                touch(1, 9),
                touch(0, 2),
                release(0),
                wide(3, 11),
                release(1)
            ]
        );
    }

    #[test]
    fn test_coalesce_when_same_cc_repeated_returns_the_last_value_of_each_controller() {
        let cc = |controller, value| Command::Cc { controller, value };
        let batch = vec![cc(12, 1), cc(13, 5), cc(12, 9), cc(13, 6)];

        assert_eq!(coalesce(batch), vec![cc(12, 9), cc(13, 6)]);
    }

    #[test]
    fn test_coalesce_when_remote_controls_repeat_returns_the_last_of_each_and_every_key_event() {
        let control = |index, value| Command::Control { index, value };
        let key = |down| Command::Key { down, code: 5 };
        let batch = vec![
            control(8, 1),
            key(true),
            control(8, 2),
            key(false),
            control(7, 9),
        ];

        assert_eq!(
            coalesce(batch),
            vec![key(true), control(8, 2), key(false), control(7, 9)]
        );
    }

    #[test]
    fn test_coalesce_when_clock_tempo_changes_returns_the_last_one_only() {
        let batch = vec![
            Command::ClockBpm(100),
            Command::ClockBpm(110),
            Command::ClockStop,
        ];

        assert_eq!(
            coalesce(batch),
            vec![Command::ClockBpm(110), Command::ClockStop]
        );
    }

    #[test]
    fn test_coalesce_when_sysex_commands_repeat_returns_all_of_them() {
        let batch = vec![Command::FetchSound, Command::FetchSound];

        assert_eq!(coalesce(batch.clone()), batch);
    }
}
