//! The seam between the protocol code and the MIDI system: bytes out, a callback for bytes in.

use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::error::MidiError;

/// Called with each chunk of bytes the port receives. A chunk is whatever the system delivered: a whole message,
/// part of a SysEx, or several messages; use `Listener` to split a stream into messages.
pub type Receiver = Box<dyn FnMut(&[u8]) + Send + 'static>;

/// A MIDI input+output pair. Implemented by `MidirTransport` (real ports) and `FakeTransport` (tests).
pub trait Transport: Send + Sync {
    /// Writes bytes to the output port (one call = one packet for the system).
    fn send(&self, bytes: &[u8]) -> Result<(), MidiError>;

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
