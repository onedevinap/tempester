//! Errors of the MIDI link.

use std::fmt;

/// Why a MIDI operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MidiError {
    /// The MIDI system could not be initialised.
    Init(String),
    /// No port with exactly this name (`direction` is "input" or "output").
    PortNotFound {
        name: String,
        direction: &'static str,
    },
    /// The port exists but could not be opened.
    Connect(String),
    /// Sending failed.
    Send(String),
    /// Nothing (matching) arrived in time.
    Timeout,
    /// The receiving side is gone.
    Disconnected,
    /// A paced send was cancelled after `sent` bytes: the machine may hold a half message.
    Cancelled { sent: usize },
    /// A paced send with a rate of zero bytes per second.
    InvalidRate,
}

impl fmt::Display for MidiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Init(why) => write!(f, "MIDI initialisation failed: {why}"),
            Self::PortNotFound { name, direction } => {
                write!(f, "no MIDI {direction} port named \"{name}\"")
            }
            Self::Connect(why) => write!(f, "cannot open the MIDI port: {why}"),
            Self::Send(why) => write!(f, "MIDI send failed: {why}"),
            Self::Timeout => write!(f, "no answer in time"),
            Self::Disconnected => write!(f, "the MIDI input is closed"),
            Self::Cancelled { sent } => write!(f, "send cancelled after {sent} bytes"),
            Self::InvalidRate => write!(f, "send rate must be above zero"),
        }
    }
}

impl std::error::Error for MidiError {}
