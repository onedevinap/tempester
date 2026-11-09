//! Turns the bytes a transport delivers into a queue of messages.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;

use crate::error::MidiError;
use crate::message::{Message, StreamParser};
use crate::transport::Transport;

/// Receives from a transport and queues parsed messages. Dropping it stops the reception.
pub struct Listener {
    transport: Arc<dyn Transport>,
    queue: mpsc::Receiver<Message>,
}

impl Listener {
    /// Installs itself as the transport's receiver (replacing any other).
    pub fn attach(transport: Arc<dyn Transport>) -> Self {
        let (sender, queue) = mpsc::channel();
        let mut parser = StreamParser::new();
        transport.set_receiver(Some(Box::new(move |bytes| {
            for message in parser.push_bytes(bytes) {
                // A closed queue means the listener is gone: nobody to tell.
                let _ = sender.send(message);
            }
        })));
        Self { transport, queue }
    }

    /// The next queued message, if any.
    pub fn try_recv(&self) -> Option<Message> {
        self.queue.try_recv().ok()
    }

    /// Waits up to `timeout` for the next message.
    pub fn recv_timeout(&self, timeout: Duration) -> Result<Message, MidiError> {
        self.queue.recv_timeout(timeout).map_err(|e| match e {
            RecvTimeoutError::Timeout => MidiError::Timeout,
            RecvTimeoutError::Disconnected => MidiError::Disconnected,
        })
    }

    /// Takes everything queued so far.
    pub fn drain(&self) -> Vec<Message> {
        std::iter::from_fn(|| self.try_recv()).collect()
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.transport.set_receiver(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeTransport;

    fn setup() -> (Arc<FakeTransport>, Listener) {
        let fake = Arc::new(FakeTransport::new());
        let listener = Listener::attach(Arc::clone(&fake) as Arc<dyn Transport>);
        (fake, listener)
    }

    #[test]
    fn test_try_recv_when_stream_pushed_returns_parsed_messages_in_order() {
        let (fake, listener) = setup();

        fake.push_incoming(&[0x90, 60, 100, 0xF8]);
        fake.push_incoming(&[0xF0, 1, 0xF7]);

        assert!(matches!(listener.try_recv(), Some(Message::Channel(_))));
        assert_eq!(listener.try_recv(), Some(Message::RealTime(0xF8)));
        assert_eq!(
            listener.try_recv(),
            Some(Message::SysEx(vec![0xF0, 1, 0xF7]))
        );
        assert_eq!(listener.try_recv(), None);
    }

    #[test]
    fn test_try_recv_when_sysex_arrives_in_two_pushes_returns_it_whole() {
        let (fake, listener) = setup();

        fake.push_incoming(&[0xF0, 1, 2]);
        assert_eq!(listener.try_recv(), None);
        fake.push_incoming(&[3, 0xF7]);

        assert_eq!(
            listener.try_recv(),
            Some(Message::SysEx(vec![0xF0, 1, 2, 3, 0xF7]))
        );
    }

    #[test]
    fn test_recv_timeout_when_nothing_arrives_returns_timeout() {
        let (_fake, listener) = setup();

        assert_eq!(
            listener.recv_timeout(Duration::from_millis(5)),
            Err(MidiError::Timeout)
        );
    }

    #[test]
    fn test_drain_when_several_queued_returns_all_and_empties_the_queue() {
        let (fake, listener) = setup();
        fake.push_incoming(&[0xFA, 0xFC]);

        assert_eq!(listener.drain().len(), 2);
        assert!(listener.drain().is_empty());
    }

    #[test]
    fn test_drop_when_listener_gone_returns_receiver_removed() {
        let (fake, listener) = setup();
        assert!(fake.has_receiver());

        drop(listener);

        assert!(!fake.has_receiver());
    }
}
