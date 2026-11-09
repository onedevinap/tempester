//! Panel: a knob moved on screen becomes an NRPN sent to the machine (Distort and Compress go as CC 12 and 13). The reverse way (a knob turned on the machine)
//! is `Machine::on_channel`. NRPN need Tempester Main 1.5.3.0 or later: the original OS ignores them.

use tempest_proto::nrpn;

use super::Machine;
use crate::command::Command;
use crate::error::AppError;

impl Machine {
    pub(super) fn handle_panel(&mut self, command: Command) -> Option<Command> {
        match command {
            Command::Nrpn { number, value } => {
                self.edit_knob(number, value);
                self.guard("nrpn", |m| m.send_nrpn(number, value));
            }
            Command::Cc { controller, value } => self.guard("cc", |m| {
                m.send_cc(controller, value)?;
                m.note_cc_sent(controller, value);
                Ok(())
            }),
            other => return Some(other),
        }
        None
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
}
