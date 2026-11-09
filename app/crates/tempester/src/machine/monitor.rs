//! The MIDI monitor: every message the app sends and every message the machine sends, as readable lines for the page.
//!
//! Two parts, so that the MIDI paths never wait:
//! - `TappedTransport` wraps the transport of a session. When the monitor is off its only cost is one atomic load.
//!   When it is on, each chunk of bytes (sent, or received by the port's callback) is copied into a bounded channel
//!   with `try_send`: a full channel drops the chunk and counts it, nothing ever blocks.
//! - `Monitor`, owned by the worker, empties that channel on its own pass of the loop, cuts the chunks into messages
//!   (`StreamParser`), describes them (`tempest_proto::describe`) and sends batches of lines to the page.
//!
//! Clock and active sensing are counted, and listed only when the page asks for them: a running clock would hide
//! everything else.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tempest_midi::transport::Receiver as ByteReceiver;
use tempest_midi::{Message, MidiError, StreamParser, Transport};
use tempest_proto::describe::{Describer, Kind, Line};
use tempest_proto::pad_notes::PadNotes;

use super::Machine;
use crate::command::Command;
use crate::event::Event;
use crate::json::{obj, Json};

/// Chunks the channel holds between two passes of the worker.
const CHANNEL_CAPACITY: usize = 1024;
/// Lines kept for the page when the worker cannot send them as fast as they come.
const PENDING_MAX: usize = 4000;
/// Lines in one `midi_log` event.
const LINES_PER_EVENT: usize = 200;
/// Shortest time between two `midi_log` events: the page is not woken for every byte.
const EMIT_EVERY: Duration = Duration::from_millis(50);
/// Chunks taken from the channel in one pass, so that a flood cannot keep the worker here.
const CHUNKS_PER_PASS: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    In,
    Out,
}

impl Direction {
    fn index(self) -> usize {
        match self {
            Self::In => 0,
            Self::Out => 1,
        }
    }
}

struct Chunk {
    ms: u64,
    direction: Direction,
    bytes: Vec<u8>,
}

/// What the transports of a session write to: shared with the wrapper, which may run on any thread.
struct Tap {
    enabled: Arc<AtomicBool>,
    dropped: Arc<AtomicU64>,
    started: Instant,
    sender: SyncSender<Chunk>,
}

impl Tap {
    fn record(&self, direction: Direction, bytes: &[u8]) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        let ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let chunk = Chunk {
            ms,
            direction,
            bytes: bytes.to_vec(),
        };
        // A disconnected channel means the session is over: nothing to count.
        if let Err(TrySendError::Full(_)) = self.sender.try_send(chunk) {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// The session's transport seen through the tap.
struct TappedTransport {
    inner: Arc<dyn Transport>,
    tap: Arc<Tap>,
}

impl Transport for TappedTransport {
    /// Recorded before the send: a machine that answers at once (the fake does) must not show its answer first.
    fn send(&self, bytes: &[u8]) -> Result<(), MidiError> {
        self.tap.record(Direction::Out, bytes);
        self.inner.send(bytes)
    }

    fn set_receiver(&self, receiver: Option<ByteReceiver>) {
        let Some(mut receiver) = receiver else {
            return self.inner.set_receiver(None);
        };
        let tap = Arc::clone(&self.tap);
        self.inner.set_receiver(Some(Box::new(move |bytes| {
            tap.record(Direction::In, bytes);
            receiver(bytes);
        })));
    }
}

/// A line of the page: when, which way, what, and the bytes.
struct Entry {
    ms: u64,
    /// `in`, `out`, or `sys` for the monitor's own notes (connection, switch).
    direction: &'static str,
    kind: &'static str,
    label: String,
    raw: String,
}

impl Entry {
    fn from_line(ms: u64, direction: Direction, line: Line) -> Self {
        Self {
            ms,
            direction: match direction {
                Direction::In => "in",
                Direction::Out => "out",
            },
            kind: line.kind.name(),
            label: line.label,
            raw: line.raw,
        }
    }

    fn note(ms: u64, label: String) -> Self {
        Self {
            ms,
            direction: "sys",
            kind: "note",
            label,
            raw: String::new(),
        }
    }

    fn json(&self) -> Json {
        obj([
            ("t", Json::Int(i64::try_from(self.ms).unwrap_or(i64::MAX))),
            ("dir", self.direction.into()),
            ("kind", self.kind.into()),
            ("text", self.label.as_str().into()),
            ("raw", self.raw.as_str().into()),
        ])
    }
}

/// The reading side of one session.
struct Source {
    channel: Receiver<Chunk>,
    started: Instant,
    parsers: [StreamParser; 2],
    describers: [Describer; 2],
}

fn describers_for(notes: PadNotes) -> [Describer; 2] {
    std::array::from_fn(|_| {
        let mut describer = Describer::new();
        describer.set_pad_notes(notes);
        describer
    })
}

/// The worker's side of the monitor.
pub(super) struct Monitor {
    enabled: Arc<AtomicBool>,
    dropped: Arc<AtomicU64>,
    channel_capacity: usize,
    pending_max: usize,
    show_clock: bool,
    source: Option<Source>,
    pending: VecDeque<Entry>,
    clocks: [u64; 2],
    /// `dropped` as of the last pass: a change means a gap, so a SysEx cut by it must not be glued together.
    seen_dropped: u64,
    /// The totals the page was last told: an event goes out when they change.
    announced: ([u64; 2], u64),
    last_emit: Option<Instant>,
    /// The machine's pad of each note, so that a note line names the right pad.
    pad_notes: PadNotes,
}

impl Monitor {
    pub(super) fn new() -> Self {
        Self::with_limits(CHANNEL_CAPACITY, PENDING_MAX)
    }

    fn with_limits(channel_capacity: usize, pending_max: usize) -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(false)),
            dropped: Arc::new(AtomicU64::new(0)),
            channel_capacity,
            pending_max,
            show_clock: false,
            source: None,
            pending: VecDeque::new(),
            clocks: [0; 2],
            seen_dropped: 0,
            announced: ([0; 2], 0),
            last_emit: None,
            pad_notes: PadNotes::default(),
        }
    }

    fn is_on(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Wraps the transport of a session that is opening; the time of its lines counts from now.
    pub(super) fn attach(&mut self, inner: Arc<dyn Transport>, port: &str) -> Arc<dyn Transport> {
        let (sender, channel) = mpsc::sync_channel(self.channel_capacity);
        let started = Instant::now();
        let tap = Arc::new(Tap {
            enabled: Arc::clone(&self.enabled),
            dropped: Arc::clone(&self.dropped),
            started,
            sender,
        });
        self.source = Some(Source {
            channel,
            started,
            parsers: Default::default(),
            describers: self.describers(),
        });
        self.note(format!("Connected to \"{port}\""));
        Arc::new(TappedTransport { inner, tap })
    }

    fn describers(&self) -> [Describer; 2] {
        describers_for(self.pad_notes)
    }

    /// The table that names the pad of a note line, from now on.
    pub(super) fn set_pad_notes(&mut self, notes: PadNotes) {
        self.pad_notes = notes;
        if let Some(source) = &mut self.source {
            for describer in &mut source.describers {
                describer.set_pad_notes(notes);
            }
        }
    }

    /// The session is closing: what it still held is kept, then it is forgotten.
    pub(super) fn detach(&mut self) {
        self.collect();
        if self.source.take().is_some() {
            self.note("Port closed".into());
        }
    }

    fn note(&mut self, label: String) {
        if !self.is_on() {
            return;
        }
        let ms = self.source.as_ref().map_or(0, |s| elapsed_ms(s.started));
        self.push(Entry::note(ms, label));
    }

    fn push(&mut self, entry: Entry) {
        if self.pending.len() >= self.pending_max {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        }
        self.pending.push_back(entry);
    }

    fn set_enabled(&mut self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
        self.pending.clear();
        self.clocks = [0; 2];
        self.dropped.store(0, Ordering::Relaxed);
        self.seen_dropped = 0;
        self.announced = ([0; 2], 0);
        if let Some(source) = &mut self.source {
            // Whatever a racing sender queued around the switch belongs to the other state.
            while source.channel.try_recv().is_ok() {}
            source.parsers = Default::default();
            source.describers = describers_for(self.pad_notes);
        }
        if on {
            let state = if self.source.is_some() {
                "Monitor on"
            } else {
                "Monitor on, not connected"
            };
            self.note(state.into());
        }
    }

    /// Empties the channel into `pending`.
    fn collect(&mut self) {
        if !self.is_on() {
            return;
        }
        let dropped = self.dropped.load(Ordering::Relaxed);
        let gap = dropped != self.seen_dropped;
        self.seen_dropped = dropped;
        let Some(mut source) = self.source.take() else {
            return;
        };
        if gap {
            source.parsers = Default::default();
        }
        for _ in 0..CHUNKS_PER_PASS {
            let Ok(chunk) = source.channel.try_recv() else {
                break;
            };
            self.describe_chunk(&mut source, &chunk);
        }
        let now = elapsed_ms(source.started);
        for direction in [Direction::In, Direction::Out] {
            for line in source.describers[direction.index()].flush_stale(now) {
                self.push(Entry::from_line(now, direction, line));
            }
        }
        self.source = Some(source);
    }

    fn describe_chunk(&mut self, source: &mut Source, chunk: &Chunk) {
        let at = chunk.direction.index();
        for message in source.parsers[at].push_bytes(&chunk.bytes) {
            let bytes = message_bytes(&message);
            for line in source.describers[at].feed(chunk.ms, &bytes) {
                if line.kind == Kind::Clock {
                    self.clocks[at] += 1;
                    if !self.show_clock {
                        continue;
                    }
                }
                self.push(Entry::from_line(chunk.ms, chunk.direction, line));
            }
        }
    }

    /// The next `midi_log` event, if the monitor is on, something changed and the page is not being flooded.
    pub(super) fn poll(&mut self, now: Instant) -> Option<Event> {
        if !self.is_on() {
            return None;
        }
        self.collect();
        let totals = (self.clocks, self.dropped.load(Ordering::Relaxed));
        if self.pending.is_empty() && totals == self.announced {
            return None;
        }
        if self
            .last_emit
            .is_some_and(|last| now.saturating_duration_since(last) < EMIT_EVERY)
        {
            return None;
        }
        self.last_emit = Some(now);
        self.announced = totals;
        let take = self.pending.len().min(LINES_PER_EVENT);
        let lines: Vec<Json> = self.pending.drain(..take).map(|e| e.json()).collect();
        Some(
            Event::new("midi_log")
                .with("lines", Json::Arr(lines))
                .with("dropped", i64::try_from(totals.1).unwrap_or(i64::MAX))
                .with("clock_in", i64::try_from(totals.0[0]).unwrap_or(i64::MAX))
                .with("clock_out", i64::try_from(totals.0[1]).unwrap_or(i64::MAX))
                .with("backlog", self.pending.len()),
        )
    }
}

fn elapsed_ms(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// The bytes of a parsed message, as the describer wants them: status first, a SysEx whole.
fn message_bytes(message: &Message) -> Vec<u8> {
    match message {
        Message::SysEx(bytes) => bytes.clone(),
        Message::Channel(short) | Message::SystemCommon(short) => {
            let mut bytes = vec![short.status];
            bytes.extend_from_slice(short.data());
            bytes
        }
        Message::RealTime(byte) => vec![*byte],
    }
}

impl Machine {
    pub(super) fn handle_monitor(&mut self, command: Command) -> Option<Command> {
        match command {
            Command::Monitor(on) => {
                self.monitor.set_enabled(on);
                self.publish_monitor();
            }
            Command::MonitorClock(show) => {
                self.monitor.show_clock = show;
                self.publish_monitor();
            }
            other => return Some(other),
        }
        None
    }

    fn publish_monitor(&self) {
        self.emit(
            Event::new("monitor")
                .with("on", self.monitor.is_on())
                .with("clock", self.monitor.show_clock),
        );
    }

    /// Sends the page the lines gathered since the last pass.
    pub(super) fn poll_monitor(&mut self, now: Instant) {
        if let Some(event) = self.monitor.poll(now) {
            self.emit(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::Json;
    use crate::machine::testkit::TestRig;
    use tempest_midi::FakeTransport;
    use tempest_proto::identity::IDENTITY_REQUEST;

    /// The page-bound lines of an event as `(dir, text)`.
    fn lines_of(event: &Event) -> Vec<(String, String)> {
        let Some(Json::Arr(lines)) = event.field("lines") else {
            panic!("no lines in {event:?}");
        };
        lines
            .iter()
            .map(|line| {
                let Json::Obj(fields) = line else {
                    panic!("not an object: {line:?}");
                };
                let get = |key: &str| match fields.iter().find(|(k, _)| *k == key) {
                    Some((_, Json::Str(s))) => s.clone(),
                    other => panic!("no string {key}: {other:?}"),
                };
                (get("dir"), get("text"))
            })
            .collect()
    }

    /// A poll one second after the previous one of any test, so that the pause between events never hides lines.
    fn later(rig: &mut TestRig) -> Option<Event> {
        static SECONDS: AtomicU64 = AtomicU64::new(1);
        let at = Instant::now() + Duration::from_secs(SECONDS.fetch_add(1, Ordering::Relaxed));
        rig.machine.monitor.poll(at)
    }

    fn texts(event: &Event) -> Vec<String> {
        lines_of(event).into_iter().map(|(_, t)| t).collect()
    }

    #[test]
    fn test_poll_when_monitor_is_off_returns_nothing_and_collects_nothing() {
        let mut rig = TestRig::connected("mon-off");

        rig.machine.handle(Command::Identity);
        rig.transport().push_incoming(&[0x90, 36, 100]);
        let event = later(&mut rig);

        assert!(event.is_none());
        let source = rig.machine.monitor.source.as_ref().unwrap();
        assert!(source.channel.try_recv().is_err(), "a chunk was recorded");
        assert!(rig.machine.monitor.pending.is_empty());
    }

    #[test]
    fn test_poll_when_monitor_is_on_returns_what_was_sent_and_received_in_order() {
        let mut rig = TestRig::connected("mon-on");
        rig.machine.handle(Command::Monitor(true));
        rig.events();

        rig.machine.handle(Command::Identity);
        rig.transport().push_incoming(&[0x99, 36, 100]);
        let event = later(&mut rig).expect("a midi_log event");

        let lines = lines_of(&event);
        let wanted = [
            ("out", "Identity request (all devices)"),
            ("in", "Identity reply: device 0x7f, version word 0x7F2"),
            ("in", "Ch 10 Note on C2 (36) velocity 100, pad A1"),
        ];
        for (at, (dir, text)) in wanted.iter().enumerate() {
            let found = lines.get(at + 1).unwrap_or_else(|| panic!("{lines:?}"));
            assert_eq!(found.0, *dir, "{lines:?}");
            assert!(found.1.starts_with(text), "{lines:?}");
        }
        assert_eq!(lines[0].0, "sys");
    }

    #[test]
    fn test_poll_when_monitor_is_switched_off_again_returns_nothing_more() {
        let mut rig = TestRig::connected("mon-toggle");
        rig.machine.handle(Command::Monitor(true));
        rig.machine.handle(Command::Monitor(false));

        rig.transport().push_incoming(&[0x90, 36, 100]);

        assert!(later(&mut rig).is_none());
        assert!(rig.machine.monitor.pending.is_empty());
    }

    #[test]
    fn test_poll_when_clock_runs_returns_a_count_and_lists_it_only_on_request() {
        let mut rig = TestRig::connected("mon-clock");
        rig.machine.handle(Command::Monitor(true));
        rig.events();

        rig.transport().push_incoming(&[0xF8, 0xF8, 0xFE, 0xFA]);
        let hidden = later(&mut rig).expect("event");

        assert_eq!(hidden.field("clock_in"), Some(&Json::Int(3)));
        assert!(texts(&hidden).iter().all(|t| !t.contains("Clock")));
        assert!(texts(&hidden).contains(&"Start".to_string()));

        rig.machine.handle(Command::MonitorClock(true));
        rig.transport().push_incoming(&[0xF8]);
        let shown = later(&mut rig).expect("event");

        assert_eq!(shown.field("clock_in"), Some(&Json::Int(4)));
        assert_eq!(texts(&shown), vec!["Clock".to_string()]);
    }

    #[test]
    fn test_poll_when_a_sysex_is_sent_in_pieces_returns_one_line() {
        let mut rig = TestRig::connected("mon-pieces");
        rig.machine.handle(Command::Monitor(true));
        rig.events();
        let transport = Arc::clone(&rig.machine.session.as_ref().unwrap().transport);

        transport.send(&[0xF0, 0x01, 0x28]).unwrap();
        transport.send(&[0x40, 0xF7]).unwrap();
        let event = later(&mut rig).expect("event");

        let found: Vec<_> = texts(&event)
            .into_iter()
            .filter(|t| t.starts_with("Request"))
            .collect();
        assert_eq!(found, vec!["Request 40: current sound".to_string()]);
    }

    #[test]
    fn test_poll_when_more_lines_than_one_event_holds_returns_the_rest_in_the_next() {
        let mut rig = TestRig::connected("mon-batch");
        rig.machine.handle(Command::Monitor(true));
        rig.events();
        let total = LINES_PER_EVENT + 50;
        for _ in 0..total {
            rig.transport().push_incoming(&[0x90, 36, 1]);
        }

        let first = later(&mut rig).expect("first");
        let second = later(&mut rig).expect("second");

        assert_eq!(lines_of(&first).len(), LINES_PER_EVENT);
        assert_eq!(first.field("backlog"), Some(&Json::Int(51)));
        assert_eq!(lines_of(&second).len(), 51);
    }

    #[test]
    fn test_poll_when_called_twice_within_the_pause_returns_the_lines_once_later() {
        let mut rig = TestRig::connected("mon-pace");
        rig.machine.handle(Command::Monitor(true));
        rig.events();
        rig.transport().push_incoming(&[0x90, 36, 1]);
        let start = Instant::now() + Duration::from_secs(1);

        let first = rig.machine.monitor.poll(start);
        rig.transport().push_incoming(&[0x90, 37, 1]);
        let too_soon = rig.machine.monitor.poll(start + Duration::from_millis(5));
        let after = rig.machine.monitor.poll(start + EMIT_EVERY);

        assert!(first.is_some() && too_soon.is_none());
        assert_eq!(texts(&after.expect("event")).len(), 1);
    }

    #[test]
    fn test_record_when_channel_is_full_returns_at_once_and_counts_the_drops() {
        let (sender, _kept_unread) = mpsc::sync_channel(2);
        let dropped = Arc::new(AtomicU64::new(0));
        let tap = Tap {
            enabled: Arc::new(AtomicBool::new(true)),
            dropped: Arc::clone(&dropped),
            started: Instant::now(),
            sender,
        };

        for _ in 0..10 {
            tap.record(Direction::Out, &[0x90, 36, 1]);
        }

        assert_eq!(dropped.load(Ordering::Relaxed), 8);
    }

    #[test]
    fn test_poll_when_the_channel_overflowed_returns_the_drop_count_to_the_page() {
        let mut monitor = Monitor::with_limits(2, PENDING_MAX);
        monitor.set_enabled(true);
        let fake = Arc::new(FakeTransport::new());
        let tapped = monitor.attach(fake, "Tempest");

        for _ in 0..10 {
            tapped.send(&[0x90, 36, 1]).unwrap();
        }
        let event = monitor.poll(Instant::now()).expect("event");

        assert_eq!(event.field("dropped"), Some(&Json::Int(8)));
        assert_eq!(texts(&event).len(), 4, "two notes and the two kept chunks");
    }

    #[test]
    fn test_poll_when_pending_lines_overflow_returns_the_extra_as_dropped() {
        let mut monitor = Monitor::with_limits(CHANNEL_CAPACITY, 5);
        monitor.set_enabled(true);
        let fake = Arc::new(FakeTransport::new());
        let tapped = monitor.attach(fake, "Tempest");

        for _ in 0..10 {
            tapped.send(&[0x90, 36, 1]).unwrap();
        }
        let event = monitor.poll(Instant::now()).expect("event");

        assert_eq!(texts(&event).len(), 5);
        // Two notes (switch, connection) and three lines fit; seven are dropped.
        assert_eq!(event.field("dropped"), Some(&Json::Int(7)));
    }

    #[test]
    fn test_poll_when_a_nrpn_comes_from_the_machine_returns_one_nrpn_line() {
        let mut rig = TestRig::connected("mon-nrpn");
        rig.machine.handle(Command::Monitor(true));
        rig.events();

        rig.transport()
            .push_incoming(&[0xB0, 99, 0, 0xB0, 98, 34, 0xB0, 6, 0, 0xB0, 38, 100]);
        let event = later(&mut rig).expect("event");

        let nrpn: Vec<_> = texts(&event)
            .into_iter()
            .filter(|t| t.contains("NRPN"))
            .collect();
        assert_eq!(nrpn.len(), 1, "{nrpn:?}");
        assert!(nrpn[0].starts_with("Ch 1 NRPN 34 "), "{nrpn:?}");
    }

    #[test]
    fn test_poll_when_a_nrpn_is_never_completed_returns_its_parts_after_the_delay() {
        let mut rig = TestRig::connected("mon-stale");
        rig.machine.handle(Command::Monitor(true));
        rig.events();
        rig.transport()
            .push_incoming(&[0xB0, 99, 0, 0xB0, 98, 34, 0xB0, 6, 0]);
        let early = later(&mut rig).expect("the connection note");
        assert_eq!(texts(&early).len(), 1);

        std::thread::sleep(Duration::from_millis(300));
        let late = later(&mut rig).expect("the parts");

        assert_eq!(texts(&late).len(), 3, "{:?}", texts(&late));
    }

    #[test]
    fn test_detach_when_monitor_is_on_returns_the_last_lines_and_a_closing_note() {
        let mut rig = TestRig::connected("mon-detach");
        rig.machine.handle(Command::Monitor(true));
        rig.events();
        rig.transport().push_incoming(&[0xFA]);

        rig.machine.handle(Command::Disconnect);
        let event = later(&mut rig).expect("event");

        let found = texts(&event);
        assert_eq!(
            found,
            vec![
                "Monitor on".to_string(),
                "Start".to_string(),
                "Port closed".to_string()
            ]
        );
    }

    #[test]
    fn test_poll_when_reconnected_returns_a_new_connection_note_and_restarts_the_clock() {
        let mut rig = TestRig::connected("mon-reconnect");
        rig.machine.handle(Command::Monitor(true));
        rig.machine.handle(Command::Connect(None));

        let event = later(&mut rig).expect("event");

        assert!(texts(&event)
            .iter()
            .any(|t| t == "Connected to \"Tempest\""));
    }

    #[test]
    fn test_identity_request_constant_matches_what_the_tap_labels() {
        let mut monitor = Monitor::new();
        monitor.set_enabled(true);
        let tapped = monitor.attach(Arc::new(FakeTransport::new()), "Tempest");

        tapped.send(&IDENTITY_REQUEST).unwrap();
        let event = monitor.poll(Instant::now()).expect("event");

        assert!(texts(&event).contains(&"Identity request (all devices)".to_string()));
    }
}
