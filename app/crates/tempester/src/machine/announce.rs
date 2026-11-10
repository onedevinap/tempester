//! What a Tempester Main says about itself (`Docs/analyses/feature-identity-mode.md`): its exact version on request
//! (n°39, `F0 01 28 43 F7`) and its Pad Function mode on Remote Pad OUT (n°40, `CC 106`).
//!
//! The version request never blocks the worker: it is sent, and the answer is read with the other incoming SysEx.
//! A machine without n°39 does not answer, so the features keep coming from the install record. The announced
//! version is forgotten when the port closes and when an OS update starts (the machine then runs something else).

use tempest_proto::identity::{self, VERSION_REQUEST};
use tempest_proto::remote;

use super::Machine;
use crate::command::Command;
use crate::error::AppError;
use crate::event::Event;
use crate::json::Json;
use crate::storage::installed::features_for;

impl Machine {
    pub(super) fn handle_announce(&mut self, command: Command) -> Option<Command> {
        match command {
            Command::TempesterVersion => self.guard("tempester_version", Self::ask_version),
            other => return Some(other),
        }
        None
    }

    fn ask_version(&mut self) -> Result<(), AppError> {
        self.idle_session()?;
        self.send_bytes(&VERSION_REQUEST)
    }

    /// The features of the connected machine: from its announced version when the app knows it, else the record.
    pub(super) fn features(&self) -> Json {
        let announced = self.announced.map(|v| v.to_string());
        features_for(announced.as_deref(), self.storage.installed_os().as_ref())
    }

    /// A SysEx from the machine: the answer to `VERSION_REQUEST` updates the version and the features.
    pub(super) fn on_version_reply(&mut self, message: &[u8]) {
        let Some(version) = identity::parse_version(message) else {
            return;
        };
        self.announced = Some(version);
        self.emit(Event::new("tempester_version").with("version", version.to_string()));
        self.emit(
            Event::new("installed")
                .with(
                    "installed",
                    self.storage.installed_os().map(|r| r.to_json()),
                )
                .with("features", self.features())
                .with("announced", version.to_string()),
        );
    }

    /// A control change that is the mode report of n°40 becomes a `pad_mode` event; `false` for any other.
    pub(super) fn on_pad_mode(&mut self, status: u8, controller: u8, value: u8) -> bool {
        let Some(mode) = remote::pad_mode(status, controller, value) else {
            return false;
        };
        self.emit(Event::new("pad_mode").with("mode", mode));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::testkit::TestRig;

    #[test]
    fn test_version_when_the_fake_machine_answers_returns_its_version_and_features_from_it() {
        let mut rig = TestRig::connected("version");

        rig.machine.handle(Command::TempesterVersion);
        rig.machine.poll_incoming();

        assert_eq!(rig.transport().sent(), vec![VERSION_REQUEST.to_vec()]);
        let events = rig.events();
        let version = events
            .iter()
            .find(|e| e.name == "tempester_version")
            .expect("tempester_version");
        assert_eq!(version.field("version"), Some(&"1.6.3.0".into()));
        let installed = events
            .iter()
            .find(|e| e.name == "installed")
            .expect("installed");
        assert_eq!(installed.field("announced"), Some(&"1.6.3.0".into()));
        let features = installed.field("features").expect("features").render();
        // Every feature 1.6.3.0 brings is on; the later ones (n°38, 1.6.4.0) stay off.
        for name in ["beat_wide", "ribbons", "pedal_pad", "probability"] {
            let on = format!(r#""{name}":{{"on":true"#);
            assert!(features.contains(&on), "{name}: {features}");
        }
    }

    #[test]
    fn test_version_when_the_machine_does_not_answer_keeps_the_record_s_features() {
        let mut rig = TestRig::connected("version-silent");
        rig.transport().set_responder(Box::new(|_| vec![]));

        rig.machine.handle(Command::TempesterVersion);
        rig.machine.poll_incoming();

        assert!(rig.events().is_empty());
        assert_eq!(rig.machine.announced, None);
        let state = {
            rig.machine.handle(Command::State);
            rig.expect("state")
        };
        let features = state.field("features").expect("features").render();
        assert!(!features.contains(r#""on":true"#), "{features}");
    }

    #[test]
    fn test_version_when_the_port_closes_is_forgotten() {
        let mut rig = TestRig::connected("version-forget");
        rig.machine.handle(Command::TempesterVersion);
        rig.machine.poll_incoming();
        assert!(rig.machine.announced.is_some());

        rig.machine.handle(Command::Disconnect);

        assert_eq!(rig.machine.announced, None);
    }

    #[test]
    fn test_version_when_not_connected_returns_not_connected_error() {
        let mut rig = TestRig::new("version-off");

        rig.machine.handle(Command::TempesterVersion);

        assert_eq!(
            rig.expect("error").field("code"),
            Some(&"not_connected".into())
        );
    }

    #[test]
    fn test_pad_mode_when_cc_106_arrives_on_any_channel_returns_pad_mode_events() {
        let mut rig = TestRig::connected("pad-mode");
        rig.transport()
            .push_incoming(&[0xB1, 106, 4, 0xB5, 106, 0, 0xB1, 106, 6]);

        rig.machine.poll_incoming();

        let modes: Vec<String> = rig
            .events()
            .iter()
            .filter(|e| e.name == "pad_mode")
            .map(Event::render)
            .collect();
        assert_eq!(
            modes,
            vec![
                r#"{"event":"pad_mode","mode":4}"#,
                r#"{"event":"pad_mode","mode":0}"#
            ]
        );
    }

    #[test]
    fn test_pad_mode_when_the_fake_machine_changes_mode_and_answers_a_state_request_reports_it() {
        let mut rig = TestRig::connected("pad-mode-fake");

        rig.machine.handle(Command::Key {
            down: true,
            code: 25,
        });
        rig.machine.poll_incoming();
        let after_key = rig.events();
        rig.machine.handle(Command::PanelState { pads: false });
        rig.machine.poll_incoming();
        let after_request = rig.events();

        let mode_of = |events: &[Event]| {
            events
                .iter()
                .filter(|e| e.name == "pad_mode")
                .map(|e| e.render())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            mode_of(&after_key),
            vec![r#"{"event":"pad_mode","mode":4}"#]
        );
        assert_eq!(
            mode_of(&after_request),
            vec![r#"{"event":"pad_mode","mode":4}"#]
        );
    }
}
