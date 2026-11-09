//! How the app reaches a MIDI port: the real system ports, or the scripted fake machine (`TEMPESTER_FAKE_MIDI=1`).
//! The worker only knows the `Connector` trait, so tests and demos never open a real port.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tempest_midi::{port_names, FakeTransport, InputPort, MidirInput, MidirTransport, Transport};
use tempest_proto::pad_notes::PadNotes;

use crate::error::AppError;
use crate::machine::fake_machine;

/// Name of the one port of the fake machine: the same as the real one, so "connect" works unchanged.
pub const FAKE_PORT: &str = "Tempest";
/// Name of the fake machine's simulated MIDI controller (MIDI learn).
pub const FAKE_CONTROLLER: &str = "Fake Controller";

pub trait Connector: Send {
    /// Names of the ports that have both an input and an output.
    fn ports(&self) -> Result<Vec<String>, AppError>;

    /// Opens the input+output pair called `name`.
    fn open(&self, name: &str) -> Result<Arc<dyn Transport>, AppError>;

    /// The scripted machine behind the last `open`, when this connector is the fake.
    fn fake(&self) -> Option<Arc<FakeTransport>>;

    /// Names of every input port: where a MIDI controller can be (MIDI learn).
    fn input_ports(&self) -> Result<Vec<String>, AppError>;

    /// Opens the input port called `name` alone: nothing can be sent to it.
    fn open_input(&self, name: &str) -> Result<Arc<dyn InputPort>, AppError>;

    /// The simulated controller behind the last `open_input`, when this connector is the fake.
    fn fake_controller(&self) -> Option<Arc<FakeTransport>> {
        None
    }

    /// True for the scripted machine: the page shows a banner so nobody mistakes it for the real thing.
    fn is_fake(&self) -> bool;

    /// The scripted machine reads a received project's Play Lists like the stock OS (n°28 defect); no effect on the
    /// real ports.
    fn set_fake_project_defect(&self, _on: bool) {}
}

/// The system MIDI ports (midir).
pub struct RealConnector;

impl Connector for RealConnector {
    fn ports(&self) -> Result<Vec<String>, AppError> {
        let (inputs, outputs) = port_names()?;
        Ok(outputs
            .into_iter()
            .filter(|name| inputs.contains(name))
            .collect())
    }

    fn open(&self, name: &str) -> Result<Arc<dyn Transport>, AppError> {
        Ok(Arc::new(MidirTransport::open(name)?))
    }

    fn fake(&self) -> Option<Arc<FakeTransport>> {
        None
    }

    fn input_ports(&self) -> Result<Vec<String>, AppError> {
        Ok(port_names()?.0)
    }

    fn open_input(&self, name: &str) -> Result<Arc<dyn InputPort>, AppError> {
        Ok(Arc::new(MidirInput::open(name)?))
    }

    fn is_fake(&self) -> bool {
        false
    }
}

/// A scripted machine in memory: nothing reaches a MIDI port.
#[derive(Default)]
pub struct FakeConnector {
    machine: Mutex<Option<Arc<FakeTransport>>>,
    controller: Mutex<Option<Arc<FakeTransport>>>,
    /// False: the scripted machine has no remote panel (`TEMPESTER_FAKE_PANEL=0`).
    without_remote_panel: bool,
    /// Shared with every scripted machine this connector opens: the n°28 defect switch.
    project_defect: Arc<AtomicBool>,
    /// The scripted machine's own note of each pad (System > MIDI Note : Pad), the author's by default.
    pad_notes: PadNotes,
}

impl FakeConnector {
    pub fn new() -> Self {
        Self::default()
    }

    /// A scripted machine that never answers keys or LED requests, like one without Tempester 1.6.0.0.
    pub fn without_remote_panel() -> Self {
        Self {
            without_remote_panel: true,
            ..Self::default()
        }
    }

    /// A scripted machine whose pads answer the notes of `notes` (tests: a table that is not the app's default
    /// proves the app plays the table it was given).
    #[cfg(test)]
    pub fn with_pad_notes(notes: PadNotes) -> Self {
        Self {
            pad_notes: notes,
            ..Self::default()
        }
    }

    fn slot(&self) -> MutexGuard<'_, Option<Arc<FakeTransport>>> {
        self.machine.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Connector for FakeConnector {
    fn ports(&self) -> Result<Vec<String>, AppError> {
        Ok(vec![FAKE_PORT.to_string()])
    }

    fn open(&self, name: &str) -> Result<Arc<dyn Transport>, AppError> {
        if name != FAKE_PORT {
            return Err(AppError::Midi(tempest_midi::MidiError::PortNotFound {
                name: name.to_string(),
                direction: "output",
            }));
        }
        let transport = fake_machine::scripted_with(
            !self.without_remote_panel,
            Arc::clone(&self.project_defect),
            self.pad_notes,
        );
        *self.slot() = Some(Arc::clone(&transport));
        Ok(transport)
    }

    fn fake(&self) -> Option<Arc<FakeTransport>> {
        self.slot().clone()
    }

    fn input_ports(&self) -> Result<Vec<String>, AppError> {
        Ok(vec![FAKE_CONTROLLER.to_string()])
    }

    fn open_input(&self, name: &str) -> Result<Arc<dyn InputPort>, AppError> {
        if name != FAKE_CONTROLLER {
            return Err(AppError::Midi(tempest_midi::MidiError::PortNotFound {
                name: name.to_string(),
                direction: "input",
            }));
        }
        let controller = Arc::new(FakeTransport::new());
        let mut slot = self
            .controller
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *slot = Some(Arc::clone(&controller));
        Ok(controller)
    }

    fn fake_controller(&self) -> Option<Arc<FakeTransport>> {
        self.controller
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn is_fake(&self) -> bool {
        true
    }

    fn set_fake_project_defect(&self, on: bool) {
        self.project_defect.store(on, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ports_when_fake_returns_only_the_tempest_port() {
        assert_eq!(
            FakeConnector::new().ports().unwrap(),
            vec!["Tempest".to_string()]
        );
    }

    #[test]
    fn test_open_when_fake_and_other_name_returns_port_not_found() {
        let result = FakeConnector::new().open("IAC Driver");

        assert!(matches!(
            result.err(),
            Some(AppError::Midi(tempest_midi::MidiError::PortNotFound { .. }))
        ));
    }

    #[test]
    fn test_fake_when_not_opened_returns_none_and_when_opened_returns_the_machine() {
        let connector = FakeConnector::new();
        assert!(connector.fake().is_none());

        connector.open("Tempest").unwrap();

        assert!(connector.fake().is_some());
    }
}
