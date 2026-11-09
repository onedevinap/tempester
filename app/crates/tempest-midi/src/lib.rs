//! MIDI link to the DSI Tempest, with midir as its only dependency.
//!
//! - `transport`: the `Transport` trait (send bytes, receive callback) and `InputPort` (receive only: a controller);
//!   `midir_transport`: the real ports (the one named "Tempest" by default); `fake`: an in-memory transport for tests;
//! - `message` and `listener`: splitting the byte stream into messages (SysEx, channel messages with running
//!   status, real-time bytes anywhere);
//! - `paced`: sending large SysEx at a MIDI-cable rate, with progress and cancellation;
//! - `request`: send a request and wait for the matching SysEx, with a timeout.
//!
//! Nothing here sends to a machine by itself: callers decide, after the user's explicit go.

pub mod error;
pub mod fake;
pub mod listener;
pub mod message;
pub mod midir_transport;
pub mod paced;
pub mod request;
pub mod transport;

pub use error::MidiError;
pub use fake::FakeTransport;
pub use listener::Listener;
pub use message::{Message, ShortMessage, StreamParser};
pub use midir_transport::{pick_port, port_names, MidirInput, MidirTransport, DEFAULT_PORT_NAME};
pub use paced::{send_paced, CancelToken, Progress, DEFAULT_RATE};
pub use request::{collect_sysex, dsi_command, request};
pub use transport::{InputPort, Receiver, Transport};
