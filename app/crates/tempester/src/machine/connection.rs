//! Connection state machine: Disconnected -> Connected (port "Tempest", or the chosen port) -> Error, and back.
//! Every change is published as a `conn` event; identity, ports, channel and "show in folder" live here too.

use std::path::Path;
use std::process::Command as Process;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use tempest_midi::{request, Listener, MidiError, Transport, DEFAULT_PORT_NAME};
use tempest_proto::identity::{self, IDENTITY_REQUEST};

use super::{fake_machine, Machine};
use crate::command::{Command, Place};
use crate::error::AppError;
use crate::event::Event;
use crate::storage::data_dir::Platform;
use crate::storage::installed::running_json;
use crate::storage::settings::{KEY_CHANNEL, KEY_PANEL_VIEW, KEY_PANEL_ZOOM, KEY_PORT};

const IDENTITY_TIMEOUT: Duration = Duration::from_millis(1500);
/// Version words announced by Tempester Main versions (`refs/firmware-main.md` 6.5).
const TEMPESTER_VERSIONS: std::ops::RangeInclusive<u16> = 0x7F0..=0x7FF;

/// An open port and what listens on it.
pub(super) struct Session {
    pub transport: Arc<dyn Transport>,
    pub listener: Listener,
    pub port: String,
}

impl Machine {
    pub(super) fn handle_link(&mut self, command: Command) -> Option<Command> {
        match command {
            Command::State => self.publish_state(),
            Command::Ports => self.guard("ports", Machine::list_ports),
            Command::Connect(port) => self.connect(port),
            Command::Disconnect => self.disconnect(),
            Command::Identity => self.guard("identity", Machine::identity),
            Command::SetChannel(channel) => self.guard("set_channel", |m| m.set_channel(channel)),
            Command::SetPanelView { full } => self.guard("panel_view", |m| m.set_panel_view(full)),
            Command::SetPanelZoom { percent } => {
                self.guard("panel_zoom", |m| m.set_panel_zoom(percent));
            }
            Command::Reveal(place) => self.guard("reveal", |m| m.reveal(place)),
            Command::FakeDebugSend => self.guard("fake_debug_send", Machine::fake_debug_send),
            other => return Some(other),
        }
        None
    }

    /// The session when connected and no OS send is running (nothing else may talk during an update).
    pub(super) fn idle_session(&self) -> Result<&Session, AppError> {
        if self.os_active.load(Ordering::SeqCst) || self.project_busy() {
            return Err(AppError::Busy);
        }
        self.session.as_ref().ok_or(AppError::NotConnected)
    }

    /// 0-15 channel used for notes, clock and NRPN.
    pub(super) fn channel_index(&self) -> u8 {
        self.storage.load_settings().channel() - 1
    }

    fn publish_conn(&self, state: &'static str, error: Option<&AppError>) {
        let event = Event::new("conn")
            .with("state", state)
            .with("port", self.session.as_ref().map(|s| s.port.clone()))
            .with("fake", self.connector.is_fake())
            .with("message", error.map(ToString::to_string))
            .with("code", error.map(AppError::code));
        self.emit(event);
    }

    fn publish_state(&self) {
        let settings = self.storage.load_settings();
        let installed = self.storage.installed_os();
        self.publish_conn(
            if self.session.is_some() {
                "connected"
            } else {
                "disconnected"
            },
            None,
        );
        self.emit(
            Event::new("state")
                .with("channel", settings.channel())
                .with("port", settings.get(KEY_PORT).map(str::to_string))
                .with("panel_full", settings.panel_view_full())
                .with("panel_zoom", settings.panel_zoom())
                .with(
                    "data_dir",
                    self.storage
                        .place_path(Place::Data)
                        .ok()
                        .map(|p| p.display().to_string()),
                )
                .with("default_port", DEFAULT_PORT_NAME)
                .with("features", self.features())
                .with("announced", self.announced.map(|v| v.to_string()))
                .with(
                    "running",
                    running_json(self.announced.map(|v| v.to_string()).as_deref()),
                )
                .with("installed", installed.map(|i| i.to_json())),
        );
        self.emit(super::pad_map::pad_notes_event(&self.pad_notes));
    }

    fn list_ports(&mut self) -> Result<(), AppError> {
        let ports = self.connector.ports()?;
        self.emit(Event::new("ports").with("ports", ports));
        Ok(())
    }

    fn connect(&mut self, port: Option<String>) {
        if self.os_active.load(Ordering::SeqCst) {
            return self.emit(Event::error("connect", &AppError::Busy));
        }
        self.close_session();
        let name = port.unwrap_or_else(|| DEFAULT_PORT_NAME.to_string());
        match self.connector.open(&name) {
            Ok(transport) => {
                let transport = self.monitor.attach(transport, &name);
                let listener = Listener::attach(Arc::clone(&transport));
                self.session = Some(Session {
                    transport,
                    listener,
                    port: name.clone(),
                });
                let _ = self.storage.update_settings(|s| s.set(KEY_PORT, &name));
                self.publish_conn("connected", None);
            }
            Err(error) => self.publish_conn("error", Some(&error)),
        }
    }

    fn disconnect(&mut self) {
        if self.os_active.load(Ordering::SeqCst) {
            return self.emit(Event::error("disconnect", &AppError::Busy));
        }
        self.close_session();
        self.publish_conn("disconnected", None);
    }

    /// Stops what depends on the port, then drops it.
    fn close_session(&mut self) {
        self.release_held_keys();
        self.release_touched_ribbons();
        self.forget_remote();
        self.announced = None;
        self.beat_wide_in.forget();
        self.pad_channels.reset();
        self.stop_clock();
        self.backup = None;
        self.verify = None;
        self.project = None;
        self.monitor.detach();
        self.session = None;
    }

    /// A send that failed means the port is gone: show the error state and drop the session.
    pub(super) fn note_failure(&mut self, error: &AppError) {
        if matches!(
            error,
            AppError::Midi(MidiError::Send(_) | MidiError::Disconnected)
        ) {
            self.close_session();
            self.publish_conn("error", Some(error));
        }
    }

    fn identity(&mut self) -> Result<(), AppError> {
        let session = self.idle_session()?;
        let reply = request(
            session.transport.as_ref(),
            &session.listener,
            &IDENTITY_REQUEST,
            IDENTITY_TIMEOUT,
            &|message| identity::parse(message).is_ok(),
        )?;
        let found = identity::parse(&reply).map_err(|e| AppError::Invalid(e.to_string()))?;
        self.emit(
            Event::new("identity")
                .with("device", found.device)
                .with("version", found.version)
                .with("tempester", TEMPESTER_VERSIONS.contains(&found.version)),
        );
        if !TEMPESTER_VERSIONS.contains(&found.version) {
            self.forget_announced();
        }
        Ok(())
    }

    fn set_channel(&mut self, channel: u8) -> Result<(), AppError> {
        self.storage
            .update_settings(|s| s.set(KEY_CHANNEL, &channel.to_string()))?;
        self.sequencer
            .update(|settings| settings.channel = channel - 1);
        self.emit(Event::new("channel").with("channel", channel));
        Ok(())
    }

    fn set_panel_view(&mut self, full: bool) -> Result<(), AppError> {
        let view = if full { "full" } else { "machine" };
        self.storage
            .update_settings(|s| s.set(KEY_PANEL_VIEW, view))?;
        Ok(())
    }

    fn set_panel_zoom(&mut self, percent: u16) -> Result<(), AppError> {
        self.storage
            .update_settings(|s| s.set(KEY_PANEL_ZOOM, &percent.to_string()))?;
        Ok(())
    }

    fn reveal(&self, place: Place) -> Result<(), AppError> {
        let path = self.storage.place_path(place)?;
        let (program, args) = reveal_command(Platform::current(), &path);
        Process::new(program).args(args).spawn()?;
        Ok(())
    }

    fn fake_debug_send(&mut self) -> Result<(), AppError> {
        let fake = self
            .connector
            .fake()
            .filter(|_| self.session.is_some())
            .ok_or(AppError::Precondition(
                "only available with the fake machine, once connected",
            ))?;
        fake_machine::debug_send(&fake);
        Ok(())
    }
}

/// The program and arguments that show `path` in the file manager.
pub fn reveal_command(platform: Platform, path: &Path) -> (&'static str, Vec<String>) {
    let shown = path.display().to_string();
    match platform {
        Platform::MacOs => ("open", vec![shown]),
        Platform::Windows => ("explorer", vec![shown]),
        Platform::Linux => ("xdg-open", vec![shown]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::testkit::TestRig;

    #[test]
    fn test_reveal_command_when_each_platform_returns_its_file_manager() {
        let path = Path::new("/data/Tempester");

        assert_eq!(reveal_command(Platform::MacOs, path).0, "open");
        assert_eq!(reveal_command(Platform::Windows, path).0, "explorer");
        assert_eq!(reveal_command(Platform::Linux, path).0, "xdg-open");
        assert_eq!(
            reveal_command(Platform::Linux, path).1,
            vec!["/data/Tempester".to_string()]
        );
    }

    #[test]
    fn test_connect_when_fake_port_returns_connected_state_and_saves_the_port() {
        let mut rig = TestRig::new("connect");

        rig.machine.handle(Command::Connect(None));

        let conn = rig.expect("conn");
        assert_eq!(conn.field("state"), Some(&"connected".into()));
        assert_eq!(
            rig.machine.storage.load_settings().get(KEY_PORT),
            Some("Tempest")
        );
    }

    #[test]
    fn test_connect_when_port_missing_returns_error_state_and_stays_disconnected() {
        let mut rig = TestRig::new("missing");

        rig.machine.handle(Command::Connect(Some("Nope".into())));

        let conn = rig.expect("conn");
        assert_eq!(conn.field("state"), Some(&"error".into()));
        assert_eq!(conn.field("code"), Some(&"midi".into()));
        assert!(rig.machine.session.is_none());
    }

    #[test]
    fn test_disconnect_when_connected_returns_disconnected_state_and_drops_the_session() {
        let mut rig = TestRig::connected("disc");
        rig.events();

        rig.machine.handle(Command::Disconnect);

        assert_eq!(
            rig.expect("conn").field("state"),
            Some(&"disconnected".into())
        );
        assert!(rig.machine.session.is_none());
    }

    #[test]
    fn test_connect_when_already_connected_returns_a_fresh_session() {
        let mut rig = TestRig::connected("again");

        rig.machine.handle(Command::Connect(None));

        assert_eq!(rig.expect("conn").field("state"), Some(&"connected".into()));
    }

    #[test]
    fn test_identity_when_fake_machine_answers_returns_tempester_identity_event() {
        let mut rig = TestRig::connected("identity");

        rig.machine.handle(Command::Identity);

        let event = rig.expect("identity");
        assert_eq!(event.field("version"), Some(&0x7F2u16.into()));
        assert_eq!(event.field("tempester"), Some(&true.into()));
    }

    #[test]
    fn test_identity_when_not_connected_returns_not_connected_error() {
        let mut rig = TestRig::new("identity-off");

        rig.machine.handle(Command::Identity);

        assert_eq!(
            rig.expect("error").field("code"),
            Some(&"not_connected".into())
        );
    }

    #[test]
    fn test_identity_when_machine_silent_returns_timeout_error() {
        let mut rig = TestRig::connected("silent");
        rig.transport().set_responder(Box::new(|_| vec![]));

        rig.machine.handle(Command::Identity);

        assert_eq!(rig.expect("error").field("code"), Some(&"timeout".into()));
    }

    #[test]
    fn test_send_failure_when_noted_returns_error_state_and_drops_the_session() {
        let mut rig = TestRig::connected("fail");
        rig.events();

        rig.machine
            .note_failure(&AppError::Midi(MidiError::Send("gone".into())));

        assert_eq!(rig.expect("conn").field("state"), Some(&"error".into()));
        assert!(rig.machine.session.is_none());
    }

    #[test]
    fn test_set_channel_when_valid_returns_channel_event_and_persists_it() {
        let mut rig = TestRig::new("channel");

        rig.machine.handle(Command::SetChannel(10));

        assert_eq!(rig.expect("channel").field("channel"), Some(&10u8.into()));
        assert_eq!(rig.machine.channel_index(), 9);
    }

    #[test]
    fn test_set_panel_view_when_machine_then_state_returns_panel_full_false() {
        let mut rig = TestRig::new("panel-view");

        rig.machine.handle(Command::State);
        assert_eq!(rig.expect("state").field("panel_full"), Some(&true.into()));
        rig.machine.handle(Command::SetPanelView { full: false });
        rig.machine.handle(Command::SetPanelZoom { percent: 130 });
        rig.machine.handle(Command::State);

        let state = rig.expect("state");
        assert_eq!(state.field("panel_full"), Some(&false.into()));
        assert_eq!(state.field("panel_zoom"), Some(&130u16.into()));
    }

    #[test]
    fn test_fake_debug_send_when_not_connected_returns_precondition_error() {
        let mut rig = TestRig::new("debug-off");

        rig.machine.handle(Command::FakeDebugSend);

        assert_eq!(
            rig.expect("error").field("code"),
            Some(&"precondition".into())
        );
    }
}
