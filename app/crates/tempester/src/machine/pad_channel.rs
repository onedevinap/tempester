//! The MIDI channel of each pad (n°58, Tempester 1.6.5.0, `tempest_proto::pad_channel`): set one pad, read the 32 back,
//! save the System settings. The commands are refused when the OS is known to lack n°58.
//!
//! The machine answers every `CC 15` (address a pad) with `CC 16` (its value) on Remote Pad OUT, and says nothing when
//! it is Off. The answer does not name its pad, so the pairing rests on order and on time:
//! - every `CC 15` the app sends puts one entry in a queue, with a deadline (`REPLY_WAIT`): a read's entry names its
//!   pad, a set's entry names none (its answer is the old value and is dropped);
//! - a `CC 16` is taken as an answer only while an entry is waiting; the oldest entry gets it;
//! - an entry whose deadline passes is dropped, so an answer that never comes cannot shift the next ones;
//! - a read sends one `CC 15` at a time and the next one only when the queue is empty: it waits for a set's answer
//!   still on its way;
//! - during a read, ANY entry that expires (its own, or a set's) stops the read and says `pad_channel_timeout` (check
//!   Remote Pad OUT): a set's answer that comes after the deadline (the machine busy saving) must never be paired with
//!   the pad the read would ask next.
//!
//! What is left: outside a read, an answer later than `REPLY_WAIT` after its `CC 15` is paired with the entry then
//! waiting, if any; a set's late answer only ever costs the value the page shows for the pad of that next set.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use tempest_proto::pad_channel::{self, PADS};

use super::Machine;
use crate::command::Command;
use crate::error::AppError;
use crate::event::Event;
use crate::storage::installed::lacks_feature;

/// How long an answer to a `CC 15` is waited for: far above the machine's delay (a few milliseconds), far below
/// what a user notices.
const REPLY_WAIT: Duration = Duration::from_millis(300);
const NEEDS_1650: &str = "The MIDI channel per pad needs Tempester 1.6.5.0 or later";

#[derive(Debug)]
struct Waiting {
    deadline: Instant,
    /// The pad a read asked about; `None` for a set.
    pad: Option<u8>,
}

/// The answers awaited and the read in progress.
#[derive(Debug)]
pub(super) struct Link {
    waiting: VecDeque<Waiting>,
    /// The next pad a read sends, `None` when no read is in progress.
    next_read: Option<u8>,
    wait: Duration,
}

impl Default for Link {
    fn default() -> Self {
        Self {
            waiting: VecDeque::new(),
            next_read: None,
            wait: REPLY_WAIT,
        }
    }
}

impl Link {
    /// Forgets everything (the port closed).
    pub(super) fn reset(&mut self) {
        self.waiting.clear();
        self.next_read = None;
    }

    /// Drops the entries whose deadline passed. If a read is in progress, any of them stops it: the pad returned is
    /// the one the read was waiting for, or the one it would have asked next.
    fn expire(&mut self, now: Instant) -> Option<u8> {
        let mut timed_out = None;
        while self.waiting.front().is_some_and(|w| w.deadline <= now) {
            let asked = self.waiting.pop_front().and_then(|w| w.pad);
            if let Some(pad) = asked.or(self.next_read) {
                timed_out = Some(pad);
                self.next_read = None;
            }
        }
        timed_out
    }
}

fn invalid(error: pad_channel::PadChannelError) -> AppError {
    AppError::Invalid(error.to_string())
}

impl Machine {
    pub(super) fn handle_pad_channel(&mut self, command: Command) -> Option<Command> {
        match command {
            Command::PadChannel { pad, value } => {
                self.guard("pad_channel", |m| m.set_pad_channel(pad, value));
            }
            Command::PadChannelRead => self.guard("pad_channel", Machine::read_pad_channels),
            Command::PadChannelSave => self.guard("pad_channel", Machine::save_pad_channels),
            other => return Some(other),
        }
        None
    }

    fn require_pad_channels(&self) -> Result<(), AppError> {
        let announced = self.announced.map(|v| v.to_string());
        let record = self.storage.installed_os();
        if lacks_feature(announced.as_deref(), record.as_ref(), "pad_channels") {
            return Err(AppError::Precondition(NEEDS_1650));
        }
        Ok(())
    }

    fn set_pad_channel(&mut self, pad: u8, value: u8) -> Result<(), AppError> {
        self.require_pad_channels()?;
        let bytes = pad_channel::set(self.channel_index(), pad, value).map_err(invalid)?;
        self.send_bytes(&bytes)?;
        let deadline = Instant::now() + self.pad_channels.wait;
        self.pad_channels.waiting.push_back(Waiting {
            deadline,
            pad: None,
        });
        self.emit(
            Event::new("pad_channel")
                .with("pad", pad)
                .with("value", value),
        );
        Ok(())
    }

    fn read_pad_channels(&mut self) -> Result<(), AppError> {
        self.require_pad_channels()?;
        // A read entry of an earlier read still waits for its answer: that answer belongs to no pad now.
        for waiting in &mut self.pad_channels.waiting {
            waiting.pad = None;
        }
        self.pad_channels.next_read = Some(0);
        self.advance_pad_read(Instant::now())
    }

    /// Sends the next `CC 15` of the read, only once nothing else waits for an answer.
    fn advance_pad_read(&mut self, now: Instant) -> Result<(), AppError> {
        let Some(pad) = self.pad_channels.next_read else {
            return Ok(());
        };
        if !self.pad_channels.waiting.is_empty() {
            return Ok(());
        }
        let sent = pad_channel::select(self.channel_index(), pad)
            .map_err(invalid)
            .and_then(|bytes| self.send_bytes(&bytes));
        if let Err(error) = sent {
            self.pad_channels.next_read = None;
            return Err(error);
        }
        let deadline = now + self.pad_channels.wait;
        self.pad_channels.waiting.push_back(Waiting {
            deadline,
            pad: Some(pad),
        });
        self.pad_channels.next_read = (pad + 1 < PADS).then_some(pad + 1);
        Ok(())
    }

    fn save_pad_channels(&mut self) -> Result<(), AppError> {
        self.require_pad_channels()?;
        let bytes = pad_channel::save(self.channel_index()).map_err(invalid)?;
        self.send_bytes(&bytes)?;
        self.emit(Event::new("pad_channel_saved"));
        Ok(())
    }

    /// Drops the entries whose deadline passed (a read stops with a `pad_channel_timeout`), then goes on with the read.
    pub(super) fn tick_pad_channels(&mut self, now: Instant) {
        if let Some(pad) = self.pad_channels.expire(now) {
            self.emit(Event::new("pad_channel_timeout").with("pad", pad));
        }
        self.guard("pad_channel", |m| m.advance_pad_read(now));
    }

    /// A control change that is the answer an entry waits for becomes a `pad_channel` event for the pad it was
    /// asked about (none for a set); `false` for any other message, and for a `CC 16` nobody asked for.
    pub(super) fn on_pad_channel_report(&mut self, status: u8, controller: u8, value: u8) -> bool {
        let now = Instant::now();
        if let Some(pad) = self.pad_channels.expire(now) {
            self.emit(Event::new("pad_channel_timeout").with("pad", pad));
        }
        if self.pad_channels.waiting.is_empty() {
            return false;
        }
        let Some(value) = pad_channel::report(status, controller, value) else {
            return false;
        };
        if let Some(Waiting { pad: Some(pad), .. }) = self.pad_channels.waiting.pop_front() {
            self.emit(
                Event::new("pad_channel")
                    .with("pad", pad)
                    .with("value", value),
            );
        }
        self.guard("pad_channel", |m| m.advance_pad_read(now));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::Json;
    use crate::machine::testkit::TestRig;
    use tempest_proto::identity::TempesterVersion;

    fn values(rig: &mut TestRig) -> Vec<(i64, i64)> {
        rig.events()
            .iter()
            .filter(|e| e.name == "pad_channel")
            .map(|e| match (e.field("pad"), e.field("value")) {
                (Some(Json::Int(pad)), Some(Json::Int(value))) => (*pad, *value),
                other => panic!("{other:?}"),
            })
            .collect()
    }

    /// A machine that answers every `CC 15 pad` it is sent with `CC 16 (pad + 1)`.
    fn answering(rig: &TestRig) {
        rig.transport().set_responder(Box::new(|sent| {
            sent.chunks(3)
                .filter(|m| m[0] & 0xF0 == 0xB0 && m[1] == 15)
                .map(|m| vec![0xB1, 16, m[2] + 1])
                .collect()
        }));
    }

    /// A machine with Remote Pad OUT off: it never answers.
    fn silent(rig: &TestRig) {
        rig.transport().set_responder(Box::new(|_| vec![]));
    }

    fn answer(rig: &mut TestRig, value: u8) {
        rig.transport().push_incoming(&[0xB1, 16, value]);
        rig.machine.poll_incoming();
    }

    fn later(seconds: u64) -> Instant {
        Instant::now() + Duration::from_secs(seconds)
    }

    #[test]
    fn test_set_when_pad_and_value_returns_cc_15_then_cc_16_and_an_event() {
        let mut rig = TestRig::connected("pad-channel-set");
        silent(&rig);

        rig.machine
            .handle(Command::PadChannel { pad: 5, value: 20 });

        assert_eq!(
            rig.transport().sent(),
            vec![vec![0xB0, 15, 5, 0xB0, 16, 20]]
        );
        assert_eq!(values(&mut rig), vec![(5, 20)]);
    }

    #[test]
    fn test_set_when_value_is_above_32_returns_error_and_sends_nothing() {
        let mut rig = TestRig::connected("pad-channel-bad");

        rig.machine
            .handle(Command::PadChannel { pad: 5, value: 33 });

        assert!(rig.transport().sent().is_empty());
        assert!(rig.events().iter().any(|e| e.name == "error"));
    }

    #[test]
    fn test_save_when_asked_returns_cc_17_127() {
        let mut rig = TestRig::connected("pad-channel-save");

        rig.machine.handle(Command::PadChannelSave);

        assert_eq!(rig.transport().sent(), vec![vec![0xB0, 17, 127]]);
        assert!(rig.events().iter().any(|e| e.name == "pad_channel_saved"));
    }

    #[test]
    fn test_commands_when_the_os_is_known_to_lack_n58_returns_a_precondition_and_sends_nothing() {
        let mut rig = TestRig::connected("pad-channel-old-os");
        rig.machine.announced = Some(TempesterVersion([1, 6, 4, 0]));

        rig.machine.handle(Command::PadChannel { pad: 5, value: 1 });
        rig.machine.handle(Command::PadChannelRead);
        rig.machine.handle(Command::PadChannelSave);

        assert!(rig.transport().sent().is_empty());
        let errors = rig.events().iter().filter(|e| e.name == "error").count();
        assert_eq!(errors, 3);
        rig.machine.announced = Some(TempesterVersion([1, 6, 5, 0]));
        rig.machine.handle(Command::PadChannelSave);
        assert_eq!(rig.transport().sent().len(), 1, "1.6.5.0 is accepted");
    }

    #[test]
    fn test_read_when_the_machine_answers_returns_each_pad_s_value_one_request_at_a_time() {
        let mut rig = TestRig::connected("pad-channel-read");
        answering(&rig);

        rig.machine.handle(Command::PadChannelRead);
        assert_eq!(
            rig.transport().sent().len(),
            1,
            "one CC 15 until its answer"
        );
        for _ in 0..40 {
            rig.machine.poll_incoming();
        }

        let sent = rig.transport().sent();
        let asked: Vec<Vec<u8>> = (0..32).map(|p| vec![0xB0, 15, p]).collect();
        assert_eq!(sent, asked);
        let expected: Vec<(i64, i64)> = (0..32).map(|p| (p, p + 1)).collect();
        assert_eq!(values(&mut rig), expected);
    }

    #[test]
    fn test_read_when_remote_pad_out_is_off_returns_a_timeout_and_the_next_set_keeps_its_own_pad() {
        let mut rig = TestRig::connected("pad-channel-no-reply");
        silent(&rig);

        rig.machine.handle(Command::PadChannelRead);
        rig.machine.tick_pad_channels(later(5));

        let timeout = rig.expect("pad_channel_timeout");
        assert_eq!(timeout.field("pad"), Some(&Json::Int(0)));
        assert_eq!(rig.transport().sent().len(), 1, "the read stopped at pad 0");
        // Later the machine is answering again: the set's answer is the old value of pad 5, not a value for pad 0.
        rig.machine.handle(Command::PadChannel { pad: 5, value: 9 });
        answer(&mut rig, 3);
        rig.machine.poll_incoming();
        assert_eq!(values(&mut rig), vec![(5, 9)]);
    }

    #[test]
    fn test_read_when_a_set_answer_is_still_on_its_way_waits_for_it_and_pairs_every_pad_right() {
        let mut rig = TestRig::connected("pad-channel-late-answer");
        silent(&rig);
        rig.machine.handle(Command::PadChannel { pad: 7, value: 9 });
        rig.machine.handle(Command::PadChannelRead);
        assert_eq!(
            rig.transport().sent().len(),
            1,
            "the read waits: only the set was sent"
        );
        rig.events();

        answer(&mut rig, 4); // the set's answer, late
        assert_eq!(rig.transport().sent().len(), 2, "now pad 0 is asked");
        answering(&rig);
        answer(&mut rig, 1); // pad 0's answer
        for _ in 0..40 {
            rig.machine.poll_incoming();
        }

        let read = values(&mut rig);
        let expected: Vec<(i64, i64)> = (0..32).map(|p| (p, p + 1)).collect();
        assert_eq!(read, expected, "the late answer shifted no pad");
    }

    #[test]
    fn test_read_when_a_set_answer_comes_after_its_deadline_stops_the_read_and_pairs_nothing() {
        let mut rig = TestRig::connected("pad-channel-late-set-answer");
        silent(&rig);
        rig.machine.handle(Command::PadChannelRead); // asks pad 0
        rig.machine.handle(Command::PadChannel { pad: 5, value: 9 }); // queued behind it
        answer(&mut rig, 1); // pad 0's answer: the read now waits for the set's entry
        assert_eq!(values(&mut rig), vec![(5, 9), (0, 1)]);

        rig.machine.tick_pad_channels(later(5)); // the set's entry expires (saving: no answer in time)
        answer(&mut rig, 7); // the set's answer, far too late

        let events = rig.events();
        let timeout = events
            .iter()
            .find(|e| e.name == "pad_channel_timeout")
            .expect("the read told it stopped");
        assert_eq!(
            timeout.field("pad"),
            Some(&Json::Int(1)),
            "the pad it would have asked"
        );
        assert_eq!(
            rig.transport().sent().len(),
            2,
            "pad 1 was never asked: read pad 0 and the set only"
        );
        assert!(
            events.iter().all(|e| e.name != "pad_channel"),
            "the late answer was paired with no pad"
        );
    }

    #[test]
    fn test_report_when_nothing_is_awaited_returns_false_so_that_another_handler_gets_it() {
        let mut rig = TestRig::connected("pad-channel-unasked");

        assert!(!rig.machine.on_pad_channel_report(0xB1, 16, 5));
        assert!(rig.events().is_empty());
    }
}
