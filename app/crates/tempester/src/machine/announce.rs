//! What a Tempester Main says about itself (`Docs/analyses/feature-identity-mode.md`): its exact version on request
//! (n°39, `F0 01 28 43 F7`) and its Pad Function mode on Remote Pad OUT (n°40, `CC 106`).
//!
//! The version request never blocks the worker: it is sent, and the answer is read with the other incoming SysEx.
//! A machine without n°39 does not answer, so the features keep coming from the install record. The announced
//! version is forgotten when the port closes and when an OS update starts (the machine then runs something else).

use tempest_proto::identity::{self, VoiceVersion, VERSION_REQUEST};
use tempest_proto::remote;

use super::Machine;
use crate::command::Command;
use crate::edit::history::Limits;
use crate::error::AppError;
use crate::event::Event;
use crate::json::Json;
use crate::storage::installed::{
    cap_for, features_for, os_known, release_max_json, running_json, voice_installed_json,
};

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

    /// Forgets the announced version and the Voice versions with it, and tells the page, when there was any.
    pub(super) fn forget_announced(&mut self) {
        let had = self.announced.take().is_some() | self.voice_words.take().is_some();
        if had {
            self.emit(self.installed_event());
        }
    }

    /// The Release maxima of the connected machine: its Main OS (announced, else the record) and its Voice chips.
    pub(super) fn release_max(&self) -> Json {
        release_max_json(
            self.announced_text().as_deref(),
            self.storage.installed_os().as_ref(),
            self.voice_words.as_ref(),
        )
    }

    /// The maxima the connected machine enforces on the sound parameters (`release_max`, the one rule): what the
    /// command checks and the edit history follow.
    pub(super) fn limits(&self) -> Limits {
        let cap = cap_for(
            self.announced_text().as_deref(),
            self.storage.installed_os().as_ref(),
        );
        Limits::new(cap, self.voice_words)
    }

    /// The Voice chips' versions as the page shows them: `[{word, label}; 6]`, `null` when the machine did not report
    /// them (a Main without n°64, or no answer yet).
    pub(super) fn voice_versions(&self) -> Json {
        let Some(words) = &self.voice_words else {
            return Json::Null;
        };
        Json::Arr(
            words
                .iter()
                .map(|word| {
                    crate::json::obj([
                        ("word", (*word).into()),
                        ("label", VoiceVersion::from_word(*word).to_string().into()),
                    ])
                })
                .collect(),
        )
    }

    /// The version the machine announced (n°39) as the text the storage helpers take.
    pub(super) fn announced_text(&self) -> Option<String> {
        self.announced.map(|v| v.to_string())
    }

    /// Whether the app can name the OS of the connected machine (announced, else the record).
    pub(super) fn os_known(&self) -> bool {
        os_known(
            self.announced_text().as_deref(),
            self.storage.installed_os().as_ref(),
        )
    }

    /// The features of the connected machine: from its announced version when the app knows it, else the record.
    pub(super) fn features(&self) -> Json {
        let announced = self.announced.map(|v| v.to_string());
        features_for(
            announced.as_deref(),
            self.storage.installed_os().as_ref(),
            self.voice_words.as_ref(),
        )
    }

    /// A SysEx from the machine: the answer to `VERSION_REQUEST` updates the version and the features; the message
    /// `45` that follows it (n°64) gives the Voice chips' versions, and with them the Release maxima.
    pub(super) fn on_version_reply(&mut self, message: &[u8]) {
        if let Some(words) = identity::parse_voice_versions(message) {
            self.voice_words = Some(words);
            self.emit(self.installed_event());
            return;
        }
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
            .with("release_max", self.release_max())
            .with("voice_versions", self.voice_versions())
            .with(
                "voice_installed",
                voice_installed_json(self.voice_words.as_ref()),
            )
            .with("os_known", self.os_known())
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
    use crate::machine::connector::FakeConnector;
    use crate::machine::fake_machine::FakeOs;
    use crate::machine::testkit::TestRig;
    use tempest_proto::identity::TempesterVersion;

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
        assert_eq!(version.field("version"), Some(&"1.6.7.0".into()));
        let installed = events
            .iter()
            .find(|e| e.name == "installed")
            .expect("installed");
        assert_eq!(installed.field("announced"), Some(&"1.6.7.0".into()));
        assert_eq!(installed.field("os_known"), Some(&true.into()));
        let features = installed.field("features").expect("features").render();
        // The fake plays 1.6.7.0: every feature the app has a control for is on.
        for name in [
            "beat_wide",
            "ribbons",
            "pedal_pad",
            "probability",
            "mixer_reports",
            "long_notes",
            "quantize_fine",
            "pad_channels",
            "full_backup",
            "release_cap",
            "release_cap_voice",
            "voice_versions",
        ] {
            let on = format!(r#""{name}":{{"on":true"#);
            assert!(features.contains(&on), "{name}: {features}");
        }
    }

    const ALL_127: &str = "[[65,127],[70,127],[75,127],[83,127],[91,127]]";
    const ALL_123: &str = "[[65,123],[70,123],[75,123],[83,123],[91,123]]";

    /// A machine playing `os`, asked for its version; the last `installed` event.
    fn after_version_request(label: &str, os: FakeOs) -> Event {
        let mut rig = TestRig::with_connector(label, FakeConnector::with_os(os));
        rig.machine.handle(Command::Connect(None));
        rig.events();
        rig.machine.handle(Command::TempesterVersion);
        rig.machine.poll_incoming();
        rig.events()
            .into_iter()
            .rev()
            .find(|e| e.name == "installed")
            .expect("installed")
    }

    fn os(version: [u16; 4], words: Option<[u16; 6]>) -> FakeOs {
        FakeOs {
            announced: TempesterVersion(version),
            voice_words: words,
        }
    }

    #[test]
    fn test_version_when_the_fake_plays_1_6_7_0_with_six_tempester_voices_reports_them_and_127() {
        let installed = after_version_request("v67", FakeOs::default());

        let voices = installed.field("voice_versions").expect("voices").render();
        assert_eq!(
            voices
                .matches(r#""label":"Tempester 1.5.1.0 or 1.5.1.1""#)
                .count(),
            6,
            "{voices}"
        );
        assert!(voices.contains(r#""word":22"#));
        assert_eq!(
            installed.field("voice_installed").map(Json::render),
            Some(
                concat!(
                    r#"[{"version":"1.5.1.0","count":6,"of":6,"same_word":["1.5.1.0","1.5.1.1"]},"#,
                    r#"{"version":"1.5.1.1","count":6,"of":6,"same_word":["1.5.1.0","1.5.1.1"]}]"#
                )
                .into()
            ),
            "the Voice card's badge"
        );
        assert_eq!(
            installed.field("release_max").map(Json::render),
            Some(ALL_127.into())
        );
    }

    #[test]
    fn test_version_when_one_chip_answers_dsi_15_returns_123_for_each_chip_position() {
        for chip in 0..6 {
            let mut words = [0x16; 6];
            words[chip] = 0x15;

            let installed = after_version_request("v67-one", os([1, 6, 7, 0], Some(words)));

            let voices = installed.field("voice_versions").expect("voices").render();
            assert_eq!(
                voices.matches(r#""label":"DSI 1.5""#).count(),
                1,
                "{voices}"
            );
            assert_eq!(
                installed.field("release_max").map(Json::render),
                Some(ALL_123.into()),
                "chip {chip}: a cap of 127 here would be the wrong variant"
            );
        }
    }

    #[test]
    fn test_version_when_1_6_9_0_and_six_voice_1_5_1_2_chips_reports_six_rows_its_badge_and_127() {
        let installed = after_version_request("v69-1512", os([1, 6, 9, 0], Some([0x17; 6])));

        let voices = installed.field("voice_versions").expect("voices").render();
        assert_eq!(
            voices.matches(r#""label":"Tempester 1.5.1.2""#).count(),
            6,
            "{voices}"
        );
        assert_eq!(voices.matches(r#""word":23"#).count(), 6, "{voices}");
        assert_eq!(
            installed.field("voice_installed").map(Json::render),
            Some(r#"[{"version":"1.5.1.2","count":6,"of":6}]"#.into()),
            "a word of its own: no same_word"
        );
        assert_eq!(
            installed.field("release_max").map(Json::render),
            Some(ALL_127.into())
        );
    }

    #[test]
    fn test_version_when_1_6_9_0_and_chips_mixing_0x16_and_0x17_returns_127() {
        let words = [0x16, 0x17, 0x16, 0x17, 0x16, 0x17];
        let installed = after_version_request("v69-mixed", os([1, 6, 9, 0], Some(words)));

        let voices = installed.field("voice_versions").expect("voices").render();
        assert_eq!(voices.matches("Tempester 1.5.1.2").count(), 3, "{voices}");
        assert_eq!(
            voices.matches("Tempester 1.5.1.0 or 1.5.1.1").count(),
            3,
            "{voices}"
        );
        assert_eq!(
            installed.field("release_max").map(Json::render),
            Some(ALL_127.into())
        );
    }

    #[test]
    fn test_version_when_1_6_8_0_and_a_voice_1_5_1_2_chip_keeps_123_and_says_main_first() {
        // Main 1.6.8.0 tests the exact 0x16: the order Main 1.6.9.0, then Voice 1.5.1.2.
        let installed = after_version_request(
            "v68-1512",
            os([1, 6, 8, 0], Some([0x16, 0x16, 0x17, 0x16, 0x16, 0x16])),
        );

        let voices = installed.field("voice_versions").expect("voices").render();
        assert!(
            voices.contains(r#""label":"Tempester 1.5.1.2""#),
            "{voices}"
        );
        assert_eq!(
            installed.field("release_max").map(Json::render),
            Some(ALL_123.into())
        );
        let features = installed.field("features").expect("features").render();
        assert!(
            features.contains("Inactive: Tempester Voice 1.5.1.2 needs Main 1.6.9.0 or later"),
            "{features}"
        );
    }

    #[test]
    fn test_version_when_an_unknown_word_is_reported_shows_the_raw_word_and_keeps_123() {
        let installed = after_version_request(
            "v67-raw",
            os([1, 6, 7, 0], Some([0x16, 0x16, 0x18, 0x16, 0x16, 0x16])),
        );

        let voices = installed.field("voice_versions").expect("voices").render();

        assert!(voices.contains(r#""label":"unknown (0x0018)""#), "{voices}");
        assert_eq!(
            installed.field("release_max").map(Json::render),
            Some(ALL_123.into())
        );
    }

    #[test]
    fn test_version_when_1_6_7_0_sends_no_45_returns_not_reported_and_123() {
        let installed = after_version_request("v67-none", os([1, 6, 7, 0], None));

        assert_eq!(installed.field("voice_versions"), Some(&Json::Null));
        assert_eq!(
            installed.field("release_max").map(Json::render),
            Some(ALL_123.into())
        );
    }

    #[test]
    fn test_version_when_1_6_6_0_returns_123_and_no_voice_versions() {
        let installed = after_version_request("v66", os([1, 6, 6, 0], None));

        assert_eq!(installed.field("voice_versions"), Some(&Json::Null));
        assert_eq!(installed.field("voice_installed"), Some(&Json::Null));
        assert_eq!(
            installed.field("release_max").map(Json::render),
            Some(ALL_123.into())
        );
    }

    #[test]
    fn test_version_when_1_6_5_0_returns_the_stock_maxima() {
        let installed = after_version_request("v65", os([1, 6, 5, 0], None));

        assert_eq!(
            installed.field("release_max").map(Json::render),
            Some("[[65,127],[70,127],[75,123],[83,127],[91,127]]".into())
        );
    }

    #[test]
    fn test_version_when_the_45_has_the_wrong_length_is_ignored() {
        let mut rig = TestRig::connected("v67-bad45");
        rig.machine.handle(Command::TempesterVersion);
        rig.machine.poll_incoming();
        rig.machine.voice_words = None;
        rig.events();
        let good = identity::voice_versions_reply(&[0x16; 6]);
        let mut long = good.clone();
        long.insert(22, 0);

        for bad in [&good[..22], &long[..]] {
            rig.transport().push_incoming(bad);
            rig.machine.poll_incoming();
            assert_eq!(
                rig.machine.voice_words,
                None,
                "{} bytes accepted",
                bad.len()
            );
        }
        rig.transport().push_incoming(&good);
        rig.machine.poll_incoming();
        assert_eq!(rig.machine.voice_words, Some([0x16; 6]));
    }

    #[test]
    fn test_version_when_asked_again_and_the_machine_goes_silent_forgets_the_voice_versions() {
        let mut rig = TestRig::connected("v67-forget");
        rig.machine.handle(Command::TempesterVersion);
        rig.machine.poll_incoming();
        assert!(rig.machine.voice_words.is_some());
        rig.transport().set_responder(Box::new(|_| vec![]));

        rig.machine.handle(Command::TempesterVersion);
        rig.machine.poll_incoming();

        assert_eq!(rig.machine.voice_words, None);
        let installed = rig
            .events()
            .into_iter()
            .rev()
            .find(|e| e.name == "installed")
            .expect("installed");
        assert_eq!(installed.field("voice_versions"), Some(&Json::Null));
        rig.machine.handle(Command::Disconnect);
        assert_eq!(rig.machine.voice_words, None);
    }

    #[test]
    fn test_state_when_asked_carries_the_release_maxima_and_the_voice_versions() {
        let mut rig = TestRig::connected("v67-state");
        rig.machine.handle(Command::TempesterVersion);
        rig.machine.poll_incoming();
        rig.events();

        rig.machine.handle(Command::State);
        let state = rig.expect("state");

        assert_eq!(
            state.field("release_max").map(Json::render),
            Some(ALL_127.into())
        );
        assert!(state
            .field("voice_versions")
            .expect("voices")
            .render()
            .contains("Tempester 1.5.1.0"));
    }

    #[test]
    fn test_version_when_the_record_says_1_6_4_0_and_the_machine_says_1_6_7_0_returns_both() {
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
            Some(r#"{"version":"1.6.7.0","label":"Tempester 1.6.7.0","known":true}"#.into())
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
        assert!(rig.machine.dsi_identity, "a DSI answer is remembered");
        rig.machine.handle(Command::Disconnect);
        assert!(!rig.machine.dsi_identity, "and forgotten with the port");
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
        assert!(!rig.machine.dsi_identity);
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
        assert_eq!(
            state.field("os_known"),
            Some(&false.into()),
            "no version and no record: the OS is unknown"
        );
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
