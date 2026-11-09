//! The remote panel of Tempester Main 1.6.0.0 (`tempest_proto::remote`): keys, LED mirror, absolute controls.
//!
//! The identity reply announces the same version word for every Tempester, so the remote panel is detected by
//! probing: at connect the app asks for the full LED state and the panel counts as present only if LED control
//! changes come back before `PROBE_TIMEOUT`. The wait never blocks the worker: the deadline is checked on each pass
//! of the loop. A machine without the patch, or with Remote Pad OUT off, stays silent and the keys stay greyed.
//!
//! The app does not know Remote Pad OUT, so LED changes are accepted on every channel of the connected port: the
//! stock OS never sends CC 104 or 105, only the remote panel does.
//!
//! A key held on the machine would stay down for ever if its release were lost, so the worker remembers the held keys
//! and releases them before the port is closed, before an OS update starts and when the worker stops.

use std::mem;
use std::time::{Duration, Instant};

use tempest_proto::remote::{self, LedChange};

use super::Machine;
use crate::command::Command;
use crate::error::AppError;
use crate::event::Event;

const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);

/// What the app knows about the remote panel of the connected machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RemoteState {
    /// Not connected, or not asked yet.
    Unknown,
    /// The full state was requested; LED changes are awaited until the deadline.
    Probing {
        deadline: Instant,
    },
    Present,
    Absent,
}

impl Machine {
    pub(super) fn handle_remote(&mut self, command: Command) -> Option<Command> {
        match command {
            Command::Key { down, code } => self.guard("key", |m| m.send_key(down, code)),
            Command::Control { index, value } => {
                self.guard("control", |m| m.send_control(index, value));
            }
            Command::Encoder { index, step } => {
                self.guard("encoder", |m| m.send_encoder(index, step));
            }
            Command::PanelState { pads } => self.guard("panel_state", |m| m.probe_panel(pads)),
            other => return Some(other),
        }
        None
    }

    /// A press is remembered once sent; a release is always sent, even for a key the worker does not know to be held.
    fn send_key(&mut self, down: bool, code: u8) -> Result<(), AppError> {
        let bytes = remote::key(self.channel_index(), down, code)
            .map_err(|e| AppError::Invalid(e.to_string()))?;
        self.send_bytes(&bytes)?;
        if down {
            self.held_keys.insert(code);
        } else {
            self.held_keys.remove(&code);
        }
        Ok(())
    }

    fn send_control(&mut self, index: u8, value: u16) -> Result<(), AppError> {
        let bytes = remote::control(self.channel_index(), index, value)
            .map_err(|e| AppError::Invalid(e.to_string()))?;
        self.send_bytes(&bytes)
    }

    /// Relative steps add up on the machine, so none may be dropped: `coalesce` never merges encoders.
    fn send_encoder(&mut self, index: u8, step: i8) -> Result<(), AppError> {
        let bytes = remote::encoder(self.channel_index(), index, step)
            .map_err(|e| AppError::Invalid(e.to_string()))?;
        self.send_bytes(&bytes)
    }

    /// Asks for the LED state and waits for the answer in the loop (`check_probe`).
    pub(super) fn probe_panel(&mut self, pads: bool) -> Result<(), AppError> {
        let bytes = remote::state_request(self.channel_index(), pads)
            .map_err(|e| AppError::Invalid(e.to_string()))?;
        self.send_bytes(&bytes)?;
        if self.remote != RemoteState::Present {
            self.remote = RemoteState::Probing {
                deadline: Instant::now() + PROBE_TIMEOUT,
            };
        }
        Ok(())
    }

    /// Best effort: releases every held key on the open port. It does not go through `send_bytes`, which would close
    /// the session again on a failure.
    pub(super) fn release_held_keys(&mut self) {
        let held = mem::take(&mut self.held_keys);
        let Some(session) = &self.session else { return };
        for code in held {
            if let Ok(bytes) = remote::key(self.channel_index(), false, code) {
                // A port that fails here is closing anyway.
                let _ = session.transport.send(&bytes);
            }
        }
    }

    /// Forgets the remote panel when the port goes away.
    pub(super) fn forget_remote(&mut self) {
        self.held_keys.clear();
        if self.remote != RemoteState::Unknown {
            self.remote = RemoteState::Unknown;
            self.emit(Event::new("remote").with("available", false));
        }
    }

    /// Mirrors the LED changes of one pass: the panel is present as soon as one arrives.
    pub(super) fn on_led_changes(&mut self, changes: &[LedChange]) {
        if changes.is_empty() {
            return;
        }
        if self.remote != RemoteState::Present {
            self.remote = RemoteState::Present;
            self.emit(Event::new("remote").with("available", true));
        }
        let rows: Vec<Vec<u8>> = changes
            .iter()
            .map(|change| vec![change.index, u8::from(change.on)])
            .collect();
        self.emit(Event::new("leds").with("changes", rows));
    }

    /// Ends the probe when its deadline has passed without a LED change.
    pub(super) fn check_probe(&mut self) {
        if let RemoteState::Probing { deadline } = self.remote {
            if Instant::now() >= deadline {
                self.remote = RemoteState::Absent;
                self.emit(Event::new("remote").with("available", false));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::testkit::TestRig;

    fn key(down: bool, code: u8) -> Command {
        Command::Key { down, code }
    }

    #[test]
    fn test_key_when_pressed_and_released_returns_cc_102_then_103_on_the_chosen_channel() {
        let mut rig = TestRig::connected("key");
        rig.machine.handle(Command::SetChannel(3));

        rig.machine.handle(key(true, 46));
        rig.machine.handle(key(false, 46));

        assert_eq!(
            rig.transport().sent(),
            vec![vec![0xB2, 102, 46], vec![0xB2, 103, 46]]
        );
        assert!(rig.machine.held_keys.is_empty());
    }

    #[test]
    fn test_key_when_released_without_a_known_press_still_sends_the_release() {
        let mut rig = TestRig::connected("key-orphan-release");

        rig.machine.handle(key(false, 7));

        assert_eq!(rig.transport().sent(), vec![vec![0xB0, 103, 7]]);
    }

    #[test]
    fn test_key_when_not_connected_returns_not_connected_error() {
        let mut rig = TestRig::new("key-off");

        rig.machine.handle(key(true, 1));

        assert_eq!(
            rig.expect("error").field("code"),
            Some(&"not_connected".into())
        );
    }

    #[test]
    fn test_key_when_code_out_of_range_returns_invalid_and_sends_nothing() {
        let mut rig = TestRig::connected("key-range");

        rig.machine.handle(key(true, 49));

        assert_eq!(rig.expect("error").field("code"), Some(&"invalid".into()));
        assert!(rig.transport().sent().is_empty());
        assert!(rig.machine.held_keys.is_empty());
    }

    #[test]
    fn test_key_when_an_os_update_runs_returns_busy_and_holds_nothing() {
        let mut rig = TestRig::connected("key-busy");
        rig.machine
            .os_active
            .store(true, std::sync::atomic::Ordering::SeqCst);

        rig.machine.handle(key(true, 3));

        assert_eq!(rig.expect("error").field("code"), Some(&"busy".into()));
        assert!(rig.transport().sent().is_empty());
        assert!(rig.machine.held_keys.is_empty());
    }

    #[test]
    fn test_disconnect_when_keys_are_held_releases_them_before_closing() {
        let mut rig = TestRig::connected("held-disconnect");
        let transport = rig.transport();
        rig.machine.handle(key(true, 5));
        rig.machine.handle(key(true, 30));

        rig.machine.handle(Command::Disconnect);

        let sent = transport.sent();
        assert!(sent.contains(&vec![0xB0, 103, 5]));
        assert!(sent.contains(&vec![0xB0, 103, 30]));
        assert!(rig.machine.held_keys.is_empty());
    }

    #[test]
    fn test_shutdown_when_a_key_is_held_releases_it() {
        let mut rig = TestRig::connected("held-shutdown");
        rig.machine.handle(key(true, 12));

        rig.machine.shutdown();

        assert_eq!(rig.transport().sent().last(), Some(&vec![0xB0, 103, 12]));
    }

    #[test]
    fn test_connect_when_a_key_was_held_on_the_old_session_releases_it() {
        let mut rig = TestRig::connected("held-reconnect");
        let old = rig.transport();
        rig.machine.handle(key(true, 20));

        rig.machine.handle(Command::Connect(None));

        assert_eq!(old.sent().last(), Some(&vec![0xB0, 103, 20]));
    }

    #[test]
    fn test_control_when_volume_returns_nrpn_228_bytes_and_refuses_bad_input() {
        let mut rig = TestRig::connected("control");

        rig.machine.handle(Command::Control {
            index: 8,
            value: 200,
        });
        rig.machine.handle(Command::Control {
            index: 10,
            value: 0,
        });

        assert_eq!(
            rig.transport().sent(),
            vec![vec![
                0xB0, 0x63, 0x01, 0xB0, 0x62, 0x64, 0xB0, 0x06, 0x01, 0xB0, 0x26, 0x48
            ]]
        );
        assert_eq!(rig.expect("error").field("code"), Some(&"invalid".into()));
    }

    #[test]
    fn test_encoder_when_steps_follow_each_other_returns_every_step_and_refuses_bad_input() {
        let mut rig = TestRig::connected("encoder");
        let encoder = |index, step| Command::Encoder { index, step };

        rig.machine.handle(encoder(1, 1));
        rig.machine.handle(encoder(1, 1));
        rig.machine.handle(encoder(14, 1));

        let sent = rig.transport().sent();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0], sent[1]);
        assert_eq!(
            sent[0],
            vec![0xB0, 0x63, 0x01, 0xB0, 0x62, 0x49, 0xB0, 0x06, 0x40, 0xB0, 0x26, 0x01]
        );
        assert_eq!(rig.expect("error").field("code"), Some(&"invalid".into()));
    }

    #[test]
    fn test_connect_when_the_fake_machine_answers_returns_the_panel_present_and_the_led_state() {
        let mut rig = TestRig::connected("probe");

        rig.machine.handle(Command::PanelState { pads: true });
        rig.machine.poll_incoming();
        let events = rig.events();

        assert!(events
            .iter()
            .any(|e| e.name == "remote" && e.field("available") == Some(&true.into())));
        let leds = events.iter().find(|e| e.name == "leds").expect("leds");
        assert!(leds.field("changes").is_some());
    }

    #[test]
    fn test_probe_when_the_machine_stays_silent_returns_the_panel_absent_after_the_deadline() {
        let mut rig = TestRig::connected("probe-silent");
        rig.transport().set_responder(Box::new(|_| vec![]));
        rig.machine.handle(Command::PanelState { pads: true });
        rig.events();
        rig.machine.remote = RemoteState::Probing {
            deadline: Instant::now(),
        };

        rig.machine.poll_incoming();

        let event = rig.expect("remote");
        assert_eq!(event.field("available"), Some(&false.into()));
        assert_eq!(rig.machine.remote, RemoteState::Absent);
    }

    #[test]
    fn test_probe_when_the_deadline_is_not_reached_returns_no_verdict_yet() {
        let mut rig = TestRig::connected("probe-wait");
        rig.transport().set_responder(Box::new(|_| vec![]));
        rig.machine.handle(Command::PanelState { pads: true });
        rig.events();

        rig.machine.poll_incoming();

        assert!(rig.events().is_empty());
    }

    #[test]
    fn test_led_when_cc_104_and_105_arrive_on_any_channel_returns_one_leds_event() {
        let mut rig = TestRig::connected("leds");
        rig.transport()
            .push_incoming(&[0xB1, 104, 0x2C, 0xB1, 105, 3, 0xB5, 104, 9]);

        rig.machine.poll_incoming();

        let event = rig.expect("leds");
        assert_eq!(
            event.render(),
            r#"{"event":"leds","changes":[[44,1],[3,0],[9,1]]}"#
        );
    }

    #[test]
    fn test_led_when_other_controllers_or_bad_index_arrive_returns_no_event() {
        let mut rig = TestRig::connected("leds-ignored");
        rig.transport().push_incoming(&[
            0xB0, 103, 5, 0xB0, 106, 5, 0xB0, 107, 5, 0x90, 104, 5, 0xB0, 104, 60, 0xB0, 105, 127,
        ]);

        rig.machine.poll_incoming();

        assert!(rig.events().is_empty());
    }

    #[test]
    fn test_disconnect_when_the_panel_was_present_returns_it_unavailable() {
        let mut rig = TestRig::connected("forget");
        rig.machine.handle(Command::PanelState { pads: true });
        rig.machine.poll_incoming();
        rig.events();
        rig.machine.handle(Command::Disconnect);

        let events = rig.events();

        assert!(events
            .iter()
            .any(|e| e.name == "remote" && e.field("available") == Some(&false.into())));
        assert_eq!(rig.machine.remote, RemoteState::Unknown);
    }

    #[test]
    fn test_key_when_the_fake_machine_toggles_a_led_returns_a_leds_event_on_the_next_channel() {
        let mut rig = TestRig::connected("fake-led");

        rig.machine.handle(key(true, 46));
        rig.machine.poll_incoming();

        let event = rig.expect("leds");
        assert_eq!(event.render(), r#"{"event":"leds","changes":[[36,1]]}"#);
    }

    #[test]
    fn test_panel_state_when_pads_requested_returns_the_60_leds_in_one_event() {
        let mut rig = TestRig::connected("fake-state");

        rig.machine.handle(Command::PanelState { pads: true });
        rig.machine.poll_incoming();

        let rendered = rig.expect("leds").render();
        assert_eq!(rendered.matches("],[").count(), 59);
    }
}
