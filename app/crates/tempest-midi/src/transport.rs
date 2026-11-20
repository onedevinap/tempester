//! The seam between the protocol code and the MIDI system: bytes out, a callback for bytes in.

use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::error::MidiError;

/// Called with each chunk of bytes the port receives. A chunk is whatever the system delivered: a whole message,
/// part of a SysEx, or several messages; use `Listener` to split a stream into messages.
pub type Receiver = Box<dyn FnMut(&[u8]) + Send + 'static>;

/// A MIDI input+output pair. Implemented by `MidirTransport` (real ports) and `FakeTransport` (tests).
pub trait Transport: Send + Sync {
    /// Writes bytes to the output port: one or several whole messages, or (where `accepts_sysex_fragments`) part of
    /// a SysEx. `MidirTransport` hands them to a system that takes one message per call message by message.
    fn send(&self, bytes: &[u8]) -> Result<(), MidiError>;

    /// Whether `send` takes part of a SysEx message (a packet that does not start with `F0`, or lacks its `F7`).
    /// When false, a SysEx must be sent whole, `F0` to `F7`, in one call. No default: a wrapper must forward the
    /// answer of the transport it wraps, a guess would break large sends on one system or another.
    fn accepts_sysex_fragments(&self) -> bool;

    /// Replaces the receiving callback (`None` drops incoming bytes).
    fn set_receiver(&self, receiver: Option<Receiver>);
}

/// A MIDI input alone: a controller whose knobs the app learns. Bytes in, nothing out: there is no method to send,
/// so nothing the app does can reach (or echo back to) that controller.
pub trait InputPort: Send + Sync {
    /// Replaces the receiving callback (`None` drops incoming bytes). Named apart from `Transport::set_receiver` so
    /// that a type with both (the fake) is never ambiguous.
    fn listen(&self, receiver: Option<Receiver>);
}

/// Locks a mutex even if a panicking thread poisoned it: the guarded data stays valid for our uses.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
