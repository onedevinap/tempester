//! The real MIDI link through midir (CoreMIDI, WinMM, ALSA): one input and one output port with the same name.
//! Opening a port is never done by tests; port choice is a pure function (`pick_port`) so it can be tested.

use std::sync::{Arc, Mutex};

use midir::{Ignore, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};

use crate::error::MidiError;
use crate::transport::{lock, InputPort, Receiver, Transport};

/// Name of the Tempest's USB MIDI port.
pub const DEFAULT_PORT_NAME: &str = "Tempest";
const CLIENT_NAME: &str = "Tempester";

/// Index of the first port whose name is exactly `wanted`.
pub fn pick_port(names: &[String], wanted: &str) -> Option<usize> {
    names.iter().position(|name| name == wanted)
}

/// Names of the input and output ports currently present.
pub fn port_names() -> Result<(Vec<String>, Vec<String>), MidiError> {
    let input = MidiInput::new(CLIENT_NAME).map_err(|e| MidiError::Init(e.to_string()))?;
    let output = MidiOutput::new(CLIENT_NAME).map_err(|e| MidiError::Init(e.to_string()))?;
    let inputs = input
        .ports()
        .iter()
        .filter_map(|p| input.port_name(p).ok())
        .collect();
    let outputs = output
        .ports()
        .iter()
        .filter_map(|p| output.port_name(p).ok())
        .collect();
    Ok((inputs, outputs))
}

type SharedReceiver = Arc<Mutex<Option<Receiver>>>;

/// The input connection, kept alive for its callback and never touched again: it is only dropped, which closes the port.
struct KeepAlive {
    _connection: MidiInputConnection<()>,
}

// SAFETY (Send, Windows only): midir's WinMM input connection holds a WinMM handle and sysex buffer pointers (raw
// pointers, so not `Send`). `KeepAlive` has no method and no access to them: the only operation is the drop, which
// calls `midiInClose` on the handle, and WinMM allows closing a handle from any thread (not from its own callback,
// which this drop never is). The boxed callback is called only by WinMM's own thread, never through this wrapper.
#[cfg(windows)]
unsafe impl Send for KeepAlive {}
// SAFETY (Sync, every system): `Sync` only promises that a shared `&KeepAlive` may be used from several threads. A
// `&KeepAlive` gives access to nothing: the struct has no method and its only field is private and never read. So no
// data of the connection (WinMM handles on Windows, the ALSA port subscription pointer on Linux) can be reached
// through it, whatever that data is.
unsafe impl Sync for KeepAlive {}

/// An open input+output pair. Dropping it closes both ports.
pub struct MidirTransport {
    output: Mutex<MidiOutputConnection>,
    receiver: SharedReceiver,
    _input: KeepAlive,
}

impl MidirTransport {
    /// Opens the port named "Tempest".
    pub fn open_tempest() -> Result<Self, MidiError> {
        Self::open(DEFAULT_PORT_NAME)
    }

    /// Opens the input and output ports whose name is exactly `name`.
    pub fn open(name: &str) -> Result<Self, MidiError> {
        let receiver: SharedReceiver = Arc::new(Mutex::new(None));
        let input = connect_input(name, Arc::clone(&receiver))?;
        let output = connect_output(name)?;
        Ok(Self {
            output: Mutex::new(output),
            receiver,
            _input: KeepAlive { _connection: input },
        })
    }
}

/// An input port alone (a MIDI controller). Dropping it closes the port. It holds no output: nothing can be sent to
/// the controller through it.
pub struct MidirInput {
    receiver: SharedReceiver,
    _input: KeepAlive,
}

impl MidirInput {
    /// Opens the input port whose name is exactly `name`.
    pub fn open(name: &str) -> Result<Self, MidiError> {
        let receiver: SharedReceiver = Arc::new(Mutex::new(None));
        let input = connect_input(name, Arc::clone(&receiver))?;
        Ok(Self {
            receiver,
            _input: KeepAlive { _connection: input },
        })
    }
}

impl InputPort for MidirInput {
    fn listen(&self, receiver: Option<Receiver>) {
        *lock(&self.receiver) = receiver;
    }
}

fn connect_input(
    name: &str,
    receiver: SharedReceiver,
) -> Result<MidiInputConnection<()>, MidiError> {
    let mut midi = MidiInput::new(CLIENT_NAME).map_err(|e| MidiError::Init(e.to_string()))?;
    midi.ignore(Ignore::None); // SysEx, clock and sensing all reach the listener, which filters
    let ports = midi.ports();
    let names: Vec<String> = ports
        .iter()
        .map(|p| midi.port_name(p).unwrap_or_default())
        .collect();
    let port = pick_port(&names, name)
        .and_then(|i| ports.get(i))
        .ok_or_else(|| MidiError::PortNotFound {
            name: name.into(),
            direction: "input",
        })?;
    midi.connect(
        port,
        "tempester-in",
        move |_timestamp, bytes, _| {
            if let Some(callback) = lock(&receiver).as_mut() {
                callback(bytes);
            }
        },
        (),
    )
    .map_err(|e| MidiError::Connect(e.to_string()))
}

fn connect_output(name: &str) -> Result<MidiOutputConnection, MidiError> {
    let midi = MidiOutput::new(CLIENT_NAME).map_err(|e| MidiError::Init(e.to_string()))?;
    let ports = midi.ports();
    let names: Vec<String> = ports
        .iter()
        .map(|p| midi.port_name(p).unwrap_or_default())
        .collect();
    let port = pick_port(&names, name)
        .and_then(|i| ports.get(i))
        .ok_or_else(|| MidiError::PortNotFound {
            name: name.into(),
            direction: "output",
        })?;
    midi.connect(port, "tempester-out")
        .map_err(|e| MidiError::Connect(e.to_string()))
}

impl Transport for MidirTransport {
    fn send(&self, bytes: &[u8]) -> Result<(), MidiError> {
        lock(&self.output)
            .send(bytes)
            .map_err(|e| MidiError::Send(e.to_string()))
    }

    fn set_receiver(&self, receiver: Option<Receiver>) {
        *lock(&self.receiver) = receiver;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_pick_port_when_exact_name_present_returns_its_index() {
        assert_eq!(
            pick_port(&names(&["IAC Driver", "Tempest", "Other"]), "Tempest"),
            Some(1)
        );
    }

    #[test]
    fn test_pick_port_when_only_similar_names_returns_none() {
        let ports = names(&["Tempest MIDI 2", "tempest", "Tempest ", "My Tempest"]);

        assert_eq!(pick_port(&ports, "Tempest"), None);
    }

    #[test]
    fn test_pick_port_when_two_have_the_name_returns_the_first() {
        assert_eq!(
            pick_port(&names(&["Tempest", "Tempest"]), "Tempest"),
            Some(0)
        );
    }

    #[test]
    fn test_pick_port_when_chosen_name_given_returns_that_port() {
        assert_eq!(pick_port(&names(&["A", "B"]), "B"), Some(1));
        assert_eq!(pick_port(&[], "Tempest"), None);
    }
}
