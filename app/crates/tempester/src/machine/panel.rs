//! Panel: a knob moved on screen becomes an NRPN sent to the machine (Distort and Compress go as CC 12 and 13). The reverse way (a knob turned on the machine)
//! is `Machine::on_channel`. NRPN need Tempester Main 1.5.3.0 or later: the original OS ignores them.

use tempest_proto::nrpn;
use tempest_proto::sound::PARAM_COUNT;

use super::Machine;
use crate::command::Command;
use crate::error::AppError;

impl Machine {
    pub(super) fn handle_panel(&mut self, command: Command) -> Option<Command> {
        match command {
            Command::Nrpn { number, value } => self.guard("nrpn", |m| {
                m.check_param_value(number, value)?;
                m.edit_knob(number, value);
                m.send_nrpn(number, value)
            }),
            Command::Cc { controller, value } => self.guard("cc", |m| {
                m.send_cc(controller, value)?;
                m.note_cc_sent(controller, value);
                Ok(())
            }),
            other => return Some(other),
        }
        None
    }

    /// Refuses a sound parameter value above the maximum this machine enforces (`release_max`: the five Releases stop at
    /// 123 under the n°63 cap and reach 127 when it is lifted). Numbers that are no sound parameter (the samples 156 and
    /// 157) and the numbers `nrpn::encode` refuses are left to it.
    fn check_param_value(&self, number: u16, value: u16) -> Result<(), AppError> {
        let Ok(index) = u8::try_from(number) else {
            return Ok(());
        };
        if usize::from(index) >= PARAM_COUNT {
            return Ok(());
        }
        let max = self.limits().max_of(index);
        if value > max {
            return Err(AppError::Invalid(format!(
                "value {value} is above the maximum {max} of parameter {number} on this machine"
            )));
        }
        Ok(())
    }

    fn send_cc(&mut self, controller: u8, value: u8) -> Result<(), AppError> {
        let status = 0xB0 | self.channel_index();
        self.send_bytes(&[status, controller, value])
    }

    /// Sends raw bytes to the machine; a port failure is noted like for every other send.
    pub(super) fn send_bytes(&mut self, bytes: &[u8]) -> Result<(), AppError> {
        let sent = self.idle_session()?.transport.send(bytes);
        sent.map_err(|e| {
            let error = AppError::from(e);
            self.note_failure(&error);
            error
        })
    }

    pub(super) fn send_nrpn(&mut self, number: u16, value: u16) -> Result<(), AppError> {
        let bytes = nrpn::encode(self.channel_index(), number, value)
            .map_err(|e| AppError::Invalid(e.to_string()))?;
        let sent = self.idle_session()?.transport.send(&bytes);
        sent.map_err(|e| {
            let error = AppError::from(e);
            self.note_failure(&error);
            error
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::testkit::TestRig;

    #[test]
    fn test_nrpn_when_connected_returns_the_four_control_changes_on_the_chosen_channel() {
        let mut rig = TestRig::connected("nrpn");
        rig.machine.handle(Command::SetChannel(3));

        rig.machine.handle(Command::Nrpn {
            number: 34,
            value: 96,
        });

        let expected = nrpn::encode(2, 34, 96).unwrap();
        assert_eq!(rig.transport().sent(), vec![expected.to_vec()]);
        assert_eq!(expected[0], 0xB2);
    }

    #[test]
    fn test_cc_when_connected_returns_the_three_bytes_on_the_chosen_channel() {
        let mut rig = TestRig::connected("cc-send");
        rig.machine.handle(Command::SetChannel(3));

        rig.machine.handle(Command::Cc {
            controller: 12,
            value: 99,
        });
        rig.machine.handle(Command::Cc {
            controller: 13,
            value: 0,
        });

        assert_eq!(
            rig.transport().sent(),
            vec![vec![0xB2, 12, 99], vec![0xB2, 13, 0]]
        );
    }

    #[test]
    fn test_cc_when_not_connected_returns_not_connected_error() {
        let mut rig = TestRig::new("cc-off");

        rig.machine.handle(Command::Cc {
            controller: 12,
            value: 1,
        });

        assert_eq!(
            rig.expect("error").field("code"),
            Some(&"not_connected".into())
        );
    }

    #[test]
    fn test_nrpn_when_number_is_not_a_tempest_parameter_returns_error_and_sends_nothing() {
        let mut rig = TestRig::connected("nrpn-bad");

        rig.machine.handle(Command::Nrpn {
            number: 127,
            value: 1,
        });

        assert_eq!(rig.expect("error").field("code"), Some(&"invalid".into()));
        assert!(rig.transport().sent().is_empty());
    }

    #[test]
    fn test_nrpn_when_not_connected_returns_not_connected_error() {
        let mut rig = TestRig::new("nrpn-off");

        rig.machine.handle(Command::Nrpn {
            number: 1,
            value: 1,
        });

        assert_eq!(
            rig.expect("error").field("code"),
            Some(&"not_connected".into())
        );
    }

    #[test]
    fn test_nrpn_when_the_port_fails_returns_error_state() {
        let mut rig = TestRig::connected("nrpn-fail");
        rig.transport().fail_sends();

        rig.machine.handle(Command::Nrpn {
            number: 1,
            value: 1,
        });

        let events = rig.events();
        assert!(events.iter().any(|e| e.name == "error"));
        assert!(events
            .iter()
            .any(|e| e.name == "conn" && e.field("state") == Some(&"error".into())));
    }

    #[test]
    fn test_poll_incoming_when_machine_sends_an_nrpn_returns_an_nrpn_event() {
        let mut rig = TestRig::connected("nrpn-in");
        let knob = nrpn::encode(0, 35, 40).unwrap();

        rig.transport().push_incoming(&knob);
        rig.machine.poll_incoming();

        let event = rig.expect("nrpn");
        assert_eq!(event.field("number"), Some(&35u16.into()));
        assert_eq!(event.field("value"), Some(&40u16.into()));
    }

    #[test]
    fn test_poll_incoming_when_cc_is_not_a_complete_nrpn_returns_no_event() {
        let mut rig = TestRig::connected("cc");

        rig.transport().push_incoming(&[0xB0, 7, 100, 0xB0, 1, 5]);
        rig.machine.poll_incoming();

        assert!(rig.events().is_empty());
    }

    #[test]
    fn test_poll_incoming_when_nrpn_number_is_not_a_tempest_one_returns_no_event() {
        let mut rig = TestRig::connected("nrpn-foreign");

        rig.transport()
            .push_incoming(&[0xB0, 99, 3, 0xB0, 98, 0, 0xB0, 6, 0, 0xB0, 38, 5]);
        rig.machine.poll_incoming();

        assert!(rig.events().is_empty());
    }

    use crate::machine::connector::FakeConnector;
    use crate::machine::fake_machine::FakeOs;
    use tempest_proto::identity::TempesterVersion;
    use tempest_proto::release_cap::RELEASES;

    /// A rig on a scripted machine playing `version` with Voice `words`, its version and Voice words already read.
    fn rig_playing(label: &str, version: [u16; 4], words: Option<[u16; 6]>) -> TestRig {
        let os = FakeOs {
            announced: TempesterVersion(version),
            voice_words: words,
        };
        let mut rig = TestRig::with_connector(label, FakeConnector::with_os(os));
        rig.machine.handle(Command::Connect(None));
        rig.machine.handle(Command::TempesterVersion);
        rig.machine.poll_incoming();
        rig.events();
        rig
    }

    /// How a sender takes `value` for parameter `index`: true when it went out on the cable.
    type SendFn = fn(&mut TestRig, u16, u16) -> bool;

    fn through_the_command(rig: &mut TestRig, index: u16, value: u16) -> bool {
        let before = rig.transport().sent().len();
        rig.machine.handle(Command::Nrpn {
            number: index,
            value,
        });
        rig.transport().sent().len() > before
    }

    /// The checks as the code stood before: only the 14 bits of the NRPN are looked at.
    fn bypassing_the_check(rig: &mut TestRig, index: u16, value: u16) -> bool {
        rig.machine.send_nrpn(index, value).is_ok()
    }

    /// The table's maxima whatever the OS: the cap is ignored.
    fn with_stock_maxima(rig: &mut TestRig, index: u16, value: u16) -> bool {
        let stock = tempest_proto::sound::params()[usize::from(index)].max;
        value <= stock && through_the_command(rig, index, value)
    }

    /// A machine to judge: its name, the version it plays, its Voice words, the largest Release it takes.
    type Case = (&'static str, [u16; 4], Option<[u16; 6]>, u16);

    /// Every OS and Voice combination, every Release at the edges of the cap, as one judge for any sender.
    fn judge_senders(send: SendFn) -> Result<(), String> {
        let tempester = [0x16u16; 6];
        let mut cases: Vec<Case> = vec![
            ("1.6.5.0", [1, 6, 5, 0], Some(tempester), 127),
            ("1.6.6.0 six chips", [1, 6, 6, 0], Some(tempester), 123),
            ("1.6.6.0 not reported", [1, 6, 6, 0], None, 123),
            ("1.6.7.0 six chips", [1, 6, 7, 0], Some(tempester), 127),
            ("1.6.7.0 not reported", [1, 6, 7, 0], None, 123),
        ];
        for chip in 0..6 {
            let mut words = tempester;
            words[chip] = 0x15;
            cases.push(("1.6.7.0 one DSI chip", [1, 6, 7, 0], Some(words), 123));
        }
        for (name, version, words, top) in cases {
            for index in RELEASES {
                // Amp Release (75) is 123 in the table: the stock OS keeps it there.
                let max = if top == 127 && index == 75 && version != [1, 6, 7, 0] {
                    123
                } else {
                    top
                };
                for value in [max - 1, max, max + 1, 127] {
                    let mut rig = rig_playing("nrpn-judge", version, words);
                    let sent = send(&mut rig, u16::from(index), value);
                    if sent != (value <= max) {
                        return Err(format!(
                            "{name}, parameter {index}, value {value}: sent {sent}, maximum {max}"
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn test_nrpn_when_a_release_is_above_the_machine_s_maximum_returns_error_and_sends_nothing() {
        assert_eq!(judge_senders(through_the_command), Ok(()));
    }

    #[test]
    fn test_judge_senders_when_given_deliberately_wrong_senders_returns_the_case_it_gets_wrong() {
        for (name, wrong) in [
            ("bypass", bypassing_the_check as SendFn),
            ("stock maxima", with_stock_maxima as SendFn),
        ] {
            assert!(judge_senders(wrong).is_err(), "{name} must be refused");
        }
    }

    #[test]
    fn test_nrpn_when_refused_returns_an_invalid_error_and_leaves_the_history_alone() {
        let mut rig = rig_playing("nrpn-refused", [1, 6, 6, 0], None);
        let before = *rig.machine.edit.history.values();

        rig.machine.handle(Command::Nrpn {
            number: 65,
            value: 127,
        });

        assert_eq!(rig.expect("error").field("code"), Some(&"invalid".into()));
        assert_eq!(*rig.machine.edit.history.values(), before);
    }

    #[test]
    fn test_nrpn_when_the_cap_is_lifted_returns_127_in_the_history_and_on_the_cable() {
        let mut rig = rig_playing("nrpn-lifted", [1, 6, 7, 0], Some([0x16; 6]));

        rig.machine.handle(Command::Nrpn {
            number: 75,
            value: 127,
        });

        assert_eq!(rig.machine.edit.history.values()[75], 127);
        let expected = nrpn::encode(0, 75, 127).unwrap();
        assert_eq!(rig.transport().sent().last(), Some(&expected.to_vec()));
    }

    #[test]
    fn test_nrpn_when_the_voice_versions_arrive_later_follows_them_for_the_history() {
        let mut rig = rig_playing("nrpn-late", [1, 6, 7, 0], None);
        rig.machine.handle(Command::Nrpn {
            number: 70,
            value: 100,
        });
        assert_eq!(rig.machine.edit.history.values()[70], 100);

        rig.machine.voice_words = Some([0x16; 6]);
        rig.machine.handle(Command::Nrpn {
            number: 70,
            value: 127,
        });

        assert_eq!(rig.machine.edit.history.values()[70], 127);
    }
}
