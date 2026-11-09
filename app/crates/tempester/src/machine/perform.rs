//! The performance controls of Tempester 1.6.2.0 / 1.6.3.0 (`tempest_proto::performance`): beat-wide positions (n°29),
//! FX ribbons (n°31) and the play probability of a pad (n°36), to the machine and, for the first two, back from it.
//!
//! The machine does not echo what the app sends, so a beat-wide position the app sets is recorded here before it is
//! reported to the page; a Revert by CC 19 (not reported either) sets all eight to neutral. A ribbon the app touched
//! is remembered until released, and released before the port closes, as held keys are: a ribbon left touched holds
//! its effect on the machine.

use tempest_proto::performance::{self, BEAT_WIDE_COUNT};

use super::Machine;
use crate::command::Command;
use crate::error::AppError;
use crate::event::Event;

/// The CC that reverts the beat-wide parameters (stock OS) when its value is not 0.
const CC_REVERT: u8 = 19;
/// The velocity of the press that selects a pad: the one of the Panel page's pads.
const SELECT_VELOCITY: u8 = 100;

fn invalid(error: performance::PerformanceError) -> AppError {
    AppError::Invalid(error.to_string())
}

impl Machine {
    pub(super) fn handle_perform(&mut self, command: Command) -> Option<Command> {
        match command {
            Command::BeatWide { index, position } => {
                self.guard("beat_wide", |m| m.send_beat_wide(index, position));
            }
            Command::Ribbon {
                ribbon,
                position,
                pressure,
            } => self.guard("ribbon", |m| m.send_ribbon(ribbon, position, pressure)),
            Command::PadProbability { pad, percent } => {
                self.guard("pad_probability", |m| m.send_probability(pad, percent));
            }
            Command::FakePerform => self.guard("fake_perform", Machine::fake_perform),
            other => return Some(other),
        }
        None
    }

    fn send_beat_wide(&mut self, index: u8, position: u8) -> Result<(), AppError> {
        let bytes =
            performance::beat_wide(self.channel_index(), index, position).map_err(invalid)?;
        self.send_bytes(&bytes)?;
        self.beat_wide_in.set(index, position);
        Ok(())
    }

    fn send_ribbon(
        &mut self,
        ribbon: u8,
        position: Option<u8>,
        pressure: Option<u8>,
    ) -> Result<(), AppError> {
        let channel = self.channel_index();
        let mut bytes = Vec::new();
        match position {
            Some(position) => {
                bytes
                    .extend(performance::ribbon_touch(channel, ribbon, position).map_err(invalid)?);
                if let Some(pressure) = pressure {
                    bytes.extend(
                        performance::ribbon_pressure(channel, ribbon, pressure).map_err(invalid)?,
                    );
                }
            }
            None => bytes.extend(performance::ribbon_release(channel, ribbon).map_err(invalid)?),
        }
        self.send_bytes(&bytes)?;
        if position.is_some() {
            self.touched_ribbons.insert(ribbon);
        } else {
            self.touched_ribbons.remove(&ribbon);
        }
        Ok(())
    }

    /// CC 14 acts on the pad selected on the machine: the pad is pressed first (its note in the machine's table,
    /// `pad_map`), as the Panel page's pads do (in 16 Sounds a pad press selects its sound; the page sends this only
    /// in that mode).
    fn send_probability(&mut self, pad: u8, percent: u8) -> Result<(), AppError> {
        let channel = self.channel_index();
        let cc = performance::play_probability(channel, percent).map_err(invalid)?;
        if pad >= 16 {
            return Err(AppError::Invalid(format!("pad {pad} is not 0-15")));
        }
        let note = self.pad_note(pad)?;
        let mut bytes = vec![
            0x90 | channel,
            note,
            SELECT_VELOCITY,
            0x80 | channel,
            note,
            0,
        ];
        bytes.extend(cc);
        self.send_bytes(&bytes)?;
        self.emit(
            Event::new("pad_probability")
                .with("pad", pad)
                .with("percent", percent),
        );
        Ok(())
    }

    /// A CC 19 the app sent: the eight positions are neutral now, and the machine does not say so.
    pub(super) fn note_cc_sent(&mut self, controller: u8, value: u8) {
        if controller != CC_REVERT || value == 0 {
            return;
        }
        self.beat_wide_in.revert();
        for index in 0..BEAT_WIDE_COUNT {
            self.emit_beat_wide(index, performance::BEAT_WIDE_NEUTRAL);
        }
    }

    fn emit_beat_wide(&self, index: u8, position: u8) {
        self.emit(
            Event::new("beat_wide")
                .with("index", index)
                .with("position", position),
        );
    }

    /// A control change from the machine that is a beat-wide or ribbon report; `false` for any other.
    pub(super) fn on_performance_report(&mut self, status: u8, controller: u8, value: u8) -> bool {
        if status & 0xF0 != 0xB0 {
            return false;
        }
        if let Some(report) = performance::ribbon_report(status, controller, value) {
            self.emit(
                Event::new("ribbon")
                    .with("ribbon", report.ribbon)
                    .with("position", report.position),
            );
            return true;
        }
        let msb = performance::CC_BEAT_WIDE..performance::CC_BEAT_WIDE + BEAT_WIDE_COUNT;
        let lsb = performance::CC_BEAT_WIDE_LSB..performance::CC_BEAT_WIDE_LSB + BEAT_WIDE_COUNT;
        if !msb.contains(&controller) && !lsb.contains(&controller) {
            return false;
        }
        if let Some((index, position)) = self.beat_wide_in.feed(controller, value) {
            self.emit_beat_wide(index, position);
        }
        true
    }

    /// Best effort, like `release_held_keys`: releases every ribbon the app left touched.
    pub(super) fn release_touched_ribbons(&mut self) {
        let touched = std::mem::take(&mut self.touched_ribbons);
        let Some(session) = &self.session else { return };
        for ribbon in touched {
            if let Ok(bytes) = performance::ribbon_release(self.channel_index(), ribbon) {
                // A port that fails here is closing anyway.
                let _ = session.transport.send(&bytes);
            }
        }
    }

    fn fake_perform(&mut self) -> Result<(), AppError> {
        let fake = self
            .connector
            .fake()
            .filter(|_| self.session.is_some())
            .ok_or(AppError::Precondition(
                "only available with the fake machine, once connected",
            ))?;
        super::fake_machine::perform_moves(&fake);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::testkit::TestRig;

    fn beat_wide(index: u8, position: u8) -> Command {
        Command::BeatWide { index, position }
    }
    fn ribbon(ribbon: u8, position: Option<u8>, pressure: Option<u8>) -> Command {
        Command::Ribbon {
            ribbon,
            position,
            pressure,
        }
    }

    #[test]
    fn test_beat_wide_when_odd_and_even_returns_msb_then_lsb_on_the_chosen_channel() {
        let mut rig = TestRig::connected("bw-send");
        rig.machine.handle(Command::SetChannel(2));

        rig.machine.handle(beat_wide(2, 131));
        rig.machine.handle(beat_wide(7, 254));

        assert_eq!(
            rig.transport().sent(),
            vec![
                vec![0xB1, 0x16, 65, 0xB1, 0x36, 0x40],
                vec![0xB1, 0x1B, 127]
            ]
        );
        assert_eq!(rig.machine.beat_wide_in.position(2), Some(131));
    }

    #[test]
    fn test_beat_wide_when_machine_reports_returns_position_events_from_any_channel() {
        let mut rig = TestRig::connected("bw-report");
        rig.transport().push_incoming(&[
            0xB1, 0x16, 0x3F, 0xB1, 0x36, 0x40, 0xB1, 0x14, 100, 0xB5, 0x34, 0,
        ]);

        rig.machine.poll_incoming();

        let rows: Vec<String> = rig
            .events()
            .iter()
            .filter(|e| e.name == "beat_wide")
            .map(|e| e.render())
            .collect();
        assert_eq!(
            rows,
            vec![
                r#"{"event":"beat_wide","index":2,"position":126}"#,
                r#"{"event":"beat_wide","index":2,"position":127}"#,
                r#"{"event":"beat_wide","index":0,"position":200}"#,
                r#"{"event":"beat_wide","index":0,"position":200}"#,
            ]
        );
    }

    #[test]
    fn test_beat_wide_when_the_app_sent_cc_19_returns_eight_neutral_positions() {
        let mut rig = TestRig::connected("bw-revert");
        rig.machine.handle(beat_wide(1, 10));
        rig.events();

        rig.machine.handle(Command::Cc {
            controller: 19,
            value: 127,
        });

        let events = rig.events();
        let neutral: Vec<_> = events.iter().filter(|e| e.name == "beat_wide").collect();
        assert_eq!(neutral.len(), 8);
        assert!(neutral
            .iter()
            .all(|e| e.field("position") == Some(&127u8.into())));
        assert_eq!(rig.machine.beat_wide_in.position(1), Some(127));
    }

    #[test]
    fn test_ribbon_when_touched_with_pressure_then_released_returns_cc_108_112_110() {
        let mut rig = TestRig::connected("ribbon");

        rig.machine.handle(ribbon(0, Some(64), Some(90)));
        rig.machine.handle(ribbon(1, Some(3), None));
        rig.machine.handle(ribbon(0, None, None));

        assert_eq!(
            rig.transport().sent(),
            vec![
                vec![0xB0, 0x6C, 64, 0xB0, 0x70, 90],
                vec![0xB0, 0x6D, 3],
                vec![0xB0, 0x6E, 0]
            ]
        );
        assert_eq!(
            rig.machine
                .touched_ribbons
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![1]
        );
    }

    #[test]
    fn test_ribbon_when_the_port_closes_while_touched_releases_it() {
        let mut rig = TestRig::connected("ribbon-held");
        let transport = rig.transport();
        rig.machine.handle(ribbon(1, Some(50), None));

        rig.machine.handle(Command::Disconnect);

        assert_eq!(transport.sent().last(), Some(&vec![0xB0, 0x6F, 0]));
        assert!(rig.machine.touched_ribbons.is_empty());
    }

    #[test]
    fn test_ribbon_when_machine_reports_a_touch_and_a_release_returns_ribbon_events() {
        let mut rig = TestRig::connected("ribbon-report");
        rig.transport()
            .push_incoming(&[0xB1, 0x6D, 77, 0xB1, 0x6F, 0]);

        rig.machine.poll_incoming();

        let rows: Vec<String> = rig.events().iter().map(|e| e.render()).collect();
        assert_eq!(
            rows,
            vec![
                r#"{"event":"ribbon","ribbon":1,"position":77}"#,
                r#"{"event":"ribbon","ribbon":1,"position":null}"#,
            ]
        );
    }

    #[test]
    fn test_ribbon_when_bad_values_returns_invalid_and_sends_nothing() {
        let mut rig = TestRig::connected("ribbon-bad");

        rig.machine.handle(ribbon(2, Some(1), None));

        assert_eq!(rig.expect("error").field("code"), Some(&"invalid".into()));
        assert!(rig.transport().sent().is_empty());
    }

    #[test]
    fn test_probability_when_set_returns_the_pad_press_then_cc_14_and_an_event() {
        let mut rig = TestRig::connected("prob");

        rig.machine.handle(Command::PadProbability {
            pad: 4,
            percent: 75,
        });

        assert_eq!(
            rig.transport().sent(),
            vec![vec![0x90, 43, 100, 0x80, 43, 0, 0xB0, 0x0E, 75]],
            "pad A5 is note 43 on the author's machine"
        );
        assert_eq!(
            rig.expect("pad_probability").render(),
            r#"{"event":"pad_probability","pad":4,"percent":75}"#
        );
    }

    #[test]
    fn test_probability_when_out_of_range_returns_invalid_and_sends_nothing() {
        let mut rig = TestRig::connected("prob-bad");

        rig.machine.handle(Command::PadProbability {
            pad: 16,
            percent: 5,
        });
        rig.machine.handle(Command::PadProbability {
            pad: 1,
            percent: 101,
        });

        assert_eq!(rig.expect("error").field("code"), Some(&"invalid".into()));
        assert!(rig.transport().sent().is_empty());
    }

    #[test]
    fn test_probability_when_set_on_the_fake_machine_returns_it_in_the_next_beat_dump() {
        let mut rig = TestRig::connected("prob-fake");
        rig.machine.handle(Command::PadProbability {
            pad: 2,
            percent: 30,
        });
        rig.events();

        rig.machine.handle(Command::FetchBeat);

        let beat = rig.expect("beat");
        let Some(crate::json::Json::Arr(values)) = beat.field("play_probability") else {
            panic!("no play_probability in {}", beat.render());
        };
        assert_eq!(values.len(), 32);
        assert_eq!(values[2], 30u8.into());
        assert_eq!(values[0], 100u8.into());
    }

    #[test]
    fn test_fake_perform_when_connected_returns_ribbon_and_beat_wide_reports() {
        let mut rig = TestRig::connected("fake-perform");

        rig.machine.handle(Command::FakePerform);
        rig.machine.poll_incoming();

        let names: Vec<String> = rig.events().iter().map(|e| e.render()).collect();
        assert!(names.contains(&r#"{"event":"ribbon","ribbon":0,"position":96}"#.to_string()));
        assert!(names.contains(&r#"{"event":"ribbon","ribbon":0,"position":null}"#.to_string()));
        assert!(names.contains(&r#"{"event":"beat_wide","index":2,"position":151}"#.to_string()));
    }
}
