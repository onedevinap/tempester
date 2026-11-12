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
use crate::storage::installed::{features_for, running_json};

impl Machine {
    pub(super) fn handle_announce(&mut self, command: Command) -> Option<Command> {
        match command {
            Command::TempesterVersion => self.guard("tempester_version", Self::ask_version),
            other => return Some(other),
        }
        None
    }

    /// The old announcement is dropped (and the page told) before the request goes out: the OS may have been changed
    /// since, outside the app, and a machine that no longer answers must not keep the old number on screen.
    fn ask_version(&mut self) -> Result<(), AppError> {
        self.idle_session()?;
        self.forget_announced();
        self.send_bytes(&VERSION_REQUEST)
    }

    /// Forgets the announced version and tells the page, when there was one.
    pub(super) fn forget_announced(&mut self) {
        if self.announced.take().is_some() {
            self.emit(self.installed_event());
        }
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
        self.emit(self.installed_event());
    }

    /// The `installed` event: the app's record, and what the machine announced (version, features) when it did.
    pub(super) fn installed_event(&self) -> Event {
        let announced = self.announced.map(|v| v.to_string());
        Event::new("installed")
            .with(
                "installed",
                self.storage.installed_os().map(|r| r.to_json()),
            )
            .with("features", self.features())
            .with("announced", announced.clone())
            .with("running", running_json(announced.as_deref()))
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
        assert_eq!(version.field("version"), Some(&"1.6.5.0".into()));
        let installed = events
            .iter()
            .find(|e| e.name == "installed")
            .expect("installed");
        assert_eq!(installed.field("announced"), Some(&"1.6.5.0".into()));
        let features = installed.field("features").expect("features").render();
        // The fake plays 1.6.5.0: every feature the app has a control for is on.
        for name in [
            "beat_wide",
            "ribbons",
            "pedal_pad",
            "probability",
            "mixer_reports",
            "long_notes",
            "quantize_fine",
            "pad_channels",
        ] {
            let on = format!(r#""{name}":{{"on":true"#);
            assert!(features.contains(&on), "{name}: {features}");
        }
    }

    #[test]
    fn test_version_when_the_record_says_1_6_4_0_and_the_machine_says_1_6_5_0_returns_both() {
        use crate::storage::installed::InstalledOs;
        let mut rig = TestRig::connected("version-differs");
        let old = tempest_proto::versions::find_version("1.6.4.0").expect("known");
        let record = InstalledOs {
            label: "Tempester 1.6.4.0".into(),
            version: Some("1.6.4.0".into()),
            at: "2026-11-10_100000Z".into(),
            sha256_image: old.sha256_image.into(),
            sha256_image_written: Some(old.sha256_image_written.into()),
            verified_at: None,
        };
        rig.machine
            .storage
            .set_installed_os(Some(&record))
            .expect("stored");

        rig.machine.handle(Command::TempesterVersion);
        rig.machine.poll_incoming();

        let installed = rig
            .events()
            .into_iter()
            .find(|e| e.name == "installed")
            .expect("installed");
        assert_eq!(
            installed.field("running").map(|r| r.render()),
            Some(r#"{"version":"1.6.5.0","label":"Tempester 1.6.5.0","known":true}"#.into())
        );
        let kept = installed.field("installed").expect("record").render();
        assert!(kept.contains(r#""version":"1.6.4.0""#), "{kept}");
        assert_eq!(rig.machine.storage.installed_os(), Some(record));
    }

    /// What the DSI original answers to the identity request (version word 0x150, not a Tempester word).
    const DSI_IDENTITY_REPLY: [u8; 14] = [
        0xF0, 0x7E, 0x7F, 0x06, 0x02, 0x01, 0x28, 0x01, 0x00, 0x00, 0x15, 0x00, 0x15, 0xF7,
    ];

    fn installed_running(events: &[Event]) -> Vec<Option<String>> {
        events
            .iter()
            .filter(|e| e.name == "installed")
            .map(|e| e.field("running").map(|r| r.render()))
            .collect()
    }

    #[test]
    fn test_version_when_asked_again_and_the_machine_stays_silent_forgets_the_old_version() {
        let mut rig = TestRig::connected("version-stale");
        rig.machine.handle(Command::TempesterVersion);
        rig.machine.poll_incoming();
        assert!(rig.machine.announced.is_some());
        rig.events();
        // The OS was changed outside the app: the machine no longer answers request 43.
        rig.transport().set_responder(Box::new(|_| vec![]));

        rig.machine.handle(Command::TempesterVersion);
        rig.machine.poll_incoming();

        assert_eq!(rig.machine.announced, None);
        assert_eq!(
            installed_running(&rig.events()),
            vec![Some("null".to_string())],
            "the page is told before any answer could come"
        );
        let features = rig.machine.features().render();
        assert!(!features.contains(r#""on":true"#), "{features}");
    }

    #[test]
    fn test_identity_when_the_machine_now_answers_as_a_dsi_original_forgets_the_announced_version()
    {
        let mut rig = TestRig::connected("identity-dsi");
        rig.machine.handle(Command::TempesterVersion);
        rig.machine.poll_incoming();
        rig.events();
        rig.transport()
            .set_responder(Box::new(|_| vec![DSI_IDENTITY_REPLY.to_vec()]));

        rig.machine.handle(Command::Identity);

        let events = rig.events();
        assert_eq!(rig.machine.announced, None);
        assert_eq!(installed_running(&events), vec![Some("null".to_string())]);
        let features = rig.machine.features().render();
        assert!(!features.contains(r#""on":true"#), "{features}");
    }

    #[test]
    fn test_identity_when_the_machine_answers_as_a_tempester_keeps_the_announced_version() {
        let mut rig = TestRig::connected("identity-tempester");
        rig.machine.handle(Command::TempesterVersion);
        rig.machine.poll_incoming();
        rig.events();

        rig.machine.handle(Command::Identity);

        assert!(rig.machine.announced.is_some());
        assert!(installed_running(&rig.events()).is_empty());
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
