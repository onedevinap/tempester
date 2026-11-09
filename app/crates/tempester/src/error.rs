//! The one error type of the app. Each variant carries a short `code` the page maps to its own wording (and hint),
//! plus the technical message for the log line under it.

use std::fmt;

use tempest_midi::MidiError;

/// Why an operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    /// No MIDI port is open.
    NotConnected,
    /// The MIDI layer failed.
    Midi(MidiError),
    /// A file or folder operation failed.
    Io(String),
    /// A message, file or value does not look like what was expected.
    Invalid(String),
    /// The action needs something done first (a sound fetched, a file confirmed...).
    Precondition(&'static str),
    /// Another long operation (OS send, clock) is running.
    Busy,
}

impl AppError {
    /// Stable identifier for the page.
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotConnected => "not_connected",
            Self::Midi(MidiError::Timeout) => "timeout",
            Self::Midi(MidiError::Cancelled { .. }) => "cancelled",
            Self::Midi(_) => "midi",
            Self::Io(_) => "io",
            Self::Invalid(_) => "invalid",
            Self::Precondition(_) => "precondition",
            Self::Busy => "busy",
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConnected => write!(f, "not connected to a MIDI port"),
            Self::Midi(e) => write!(f, "{e}"),
            Self::Io(why) => write!(f, "file error: {why}"),
            Self::Invalid(why) => write!(f, "{why}"),
            Self::Precondition(why) => write!(f, "{why}"),
            Self::Busy => write!(f, "another operation is running"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<MidiError> for AppError {
    fn from(e: MidiError) -> Self {
        Self::Midi(e)
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_when_midi_timeout_returns_timeout() {
        assert_eq!(AppError::Midi(MidiError::Timeout).code(), "timeout");
    }

    #[test]
    fn test_code_when_other_midi_error_returns_midi() {
        assert_eq!(AppError::Midi(MidiError::InvalidRate).code(), "midi");
    }

    #[test]
    fn test_from_when_io_error_returns_io_variant_with_message() {
        let error = AppError::from(std::io::Error::other("disk full"));

        assert_eq!(error, AppError::Io("disk full".into()));
    }
}
