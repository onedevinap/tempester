//! In-memory transport for tests: records what is sent, lets a test (or a scripted "machine") answer.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::error::MidiError;
use crate::transport::{lock, InputPort, Receiver, Transport};

/// Scripted answers: called with each sent packet, returns the chunks the fake machine sends back.
pub type Responder = Box<dyn FnMut(&[u8]) -> Vec<Vec<u8>> + Send>;

/// A `Transport` that never touches a MIDI port.
#[derive(Default)]
pub struct FakeTransport {
    sent: Mutex<Vec<Vec<u8>>>,
    receiver: Mutex<Option<Receiver>>,
    responder: Mutex<Option<Responder>>,
    failing: AtomicBool,
    refusing_fragments: AtomicBool,
}

impl FakeTransport {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every packet sent so far, in order.
    pub fn sent(&self) -> Vec<Vec<u8>> {
        lock(&self.sent).clone()
    }

    /// Everything sent so far as one byte string.
    pub fn sent_bytes(&self) -> Vec<u8> {
        lock(&self.sent).concat()
    }

    /// Delivers bytes to the receiver as if the machine had sent them.
    pub fn push_incoming(&self, bytes: &[u8]) {
        if let Some(receiver) = lock(&self.receiver).as_mut() {
            receiver(bytes);
        }
    }

    /// Answers every sent packet with the chunks the closure returns.
    pub fn set_responder(&self, responder: Responder) {
        *lock(&self.responder) = Some(responder);
    }

    /// Makes every following `send` fail.
    pub fn fail_sends(&self) {
        self.failing.store(true, Ordering::SeqCst);
    }

    /// Makes the fake behave like midir on Windows (WinMM): a packet that does not start with `F0` and is longer than
    /// a channel message is refused, so a SysEx can only go out whole.
    pub fn refuse_sysex_fragments(&self) {
        self.refusing_fragments.store(true, Ordering::SeqCst);
    }

    /// True when a receiver is installed.
    pub fn has_receiver(&self) -> bool {
        lock(&self.receiver).is_some()
    }
}

impl Transport for FakeTransport {
    fn send(&self, bytes: &[u8]) -> Result<(), MidiError> {
        if self.failing.load(Ordering::SeqCst) {
            return Err(MidiError::Send("fake transport set to fail".into()));
        }
        if self.refusing_fragments.load(Ordering::SeqCst) && is_winmm_refused(bytes) {
            // midir's WinMM wording, so that a test reads like the report of a Windows user.
            return Err(MidiError::Send(
                "non-sysex message must not be longer than 3 bytes".into(),
            ));
        }
        lock(&self.sent).push(bytes.to_vec());
        let answers = match lock(&self.responder).as_mut() {
            Some(responder) => responder(bytes),
            None => Vec::new(),
        };
        for answer in answers {
            self.push_incoming(&answer);
        }
        Ok(())
    }

    fn accepts_sysex_fragments(&self) -> bool {
        !self.refusing_fragments.load(Ordering::SeqCst)
    }

    fn set_receiver(&self, receiver: Option<Receiver>) {
        *lock(&self.receiver) = receiver;
    }
}

/// midir 0.11 on WinMM sends a packet starting with `F0` as one long message and anything else as a short message
/// of at most 3 bytes (`backend/winmm/mod.rs`, `MidiOutputConnection::send`).
fn is_winmm_refused(bytes: &[u8]) -> bool {
    bytes.len() > 3 && bytes.first() != Some(&0xF0)
}

/// A fake controller in tests: its `sent()` stays empty because an `InputPort` cannot send.
impl InputPort for FakeTransport {
    fn listen(&self, receiver: Option<Receiver>) {
        *lock(&self.receiver) = receiver;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_send_when_called_returns_recorded_packets_in_order() {
        let fake = FakeTransport::new();

        fake.send(&[1, 2]).unwrap();
        fake.send(&[3]).unwrap();

        assert_eq!(fake.sent(), vec![vec![1, 2], vec![3]]);
        assert_eq!(fake.sent_bytes(), vec![1, 2, 3]);
    }

    #[test]
    fn test_push_incoming_when_receiver_set_returns_bytes_to_receiver() {
        let fake = FakeTransport::new();
        let got = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&got);
        fake.set_receiver(Some(Box::new(move |b| lock(&sink).extend_from_slice(b))));

        fake.push_incoming(&[9, 8]);

        assert_eq!(*lock(&got), vec![9, 8]);
    }

    #[test]
    fn test_push_incoming_when_receiver_removed_returns_nothing_delivered() {
        let fake = FakeTransport::new();
        let got = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&got);
        fake.set_receiver(Some(Box::new(move |b| lock(&sink).extend_from_slice(b))));
        fake.set_receiver(None);

        fake.push_incoming(&[9]);

        assert!(lock(&got).is_empty());
    }

    #[test]
    fn test_send_when_responder_set_returns_its_answer_to_the_receiver() {
        let fake = FakeTransport::new();
        let got = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&got);
        fake.set_receiver(Some(Box::new(move |b| lock(&sink).extend_from_slice(b))));
        fake.set_responder(Box::new(|sent| vec![sent.iter().rev().copied().collect()]));

        fake.send(&[1, 2, 3]).unwrap();

        assert_eq!(*lock(&got), vec![3, 2, 1]);
    }

    #[test]
    fn test_send_when_set_to_fail_returns_error_and_records_nothing() {
        let fake = FakeTransport::new();
        fake.fail_sends();

        assert!(matches!(fake.send(&[1]), Err(MidiError::Send(_))));
        assert!(fake.sent().is_empty());
    }

    #[test]
    fn test_accepts_sysex_fragments_when_default_returns_true() {
        let fake = FakeTransport::new();

        assert!(fake.accepts_sysex_fragments());
        assert_eq!(fake.send(&[0x01, 0x02, 0x03, 0x04, 0xF7]), Ok(()));
    }

    #[test]
    fn test_send_when_refusing_fragments_and_long_packet_without_f0_returns_send_error() {
        let fake = FakeTransport::new();
        fake.refuse_sysex_fragments();

        let result = fake.send(&[0x01, 0x02, 0x03, 0x04]);

        assert!(!fake.accepts_sysex_fragments());
        assert_eq!(
            result,
            Err(MidiError::Send(
                "non-sysex message must not be longer than 3 bytes".into()
            ))
        );
        assert!(fake.sent().is_empty());
    }

    #[test]
    fn test_send_when_refusing_fragments_and_sysex_or_short_message_returns_ok() {
        let fake = FakeTransport::new();
        fake.refuse_sysex_fragments();

        assert_eq!(fake.send(&[0xF0, 0x01, 0x02, 0x03, 0x04]), Ok(()));
        assert_eq!(fake.send(&[0x90, 36, 100]), Ok(()));

        assert_eq!(fake.sent().len(), 2);
    }
}
