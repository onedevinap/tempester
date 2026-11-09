//! Request/response over the listener: send a request, collect SysEx until the wanted answer or the deadline.
//! Clock (`F8`), active sensing (`FE`) and every non-SysEx message are ignored.

use std::time::{Duration, Instant};

use crate::error::MidiError;
use crate::listener::Listener;
use crate::message::Message;
use crate::transport::Transport;

/// Collects SysEx messages until `done` (given everything collected so far) returns true.
/// Fails with `Timeout` when the deadline passes first.
pub fn collect_sysex(
    listener: &Listener,
    timeout: Duration,
    done: &mut dyn FnMut(&[Vec<u8>]) -> bool,
) -> Result<Vec<Vec<u8>>, MidiError> {
    let deadline = Instant::now() + timeout;
    let mut collected = Vec::new();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match listener.recv_timeout(left)? {
            Message::SysEx(message) => collected.push(message),
            _ => continue,
        }
        if done(&collected) {
            return Ok(collected);
        }
    }
}

/// Sends `request` and returns the first SysEx answer for which `accept` is true (other SysEx are skipped).
/// Anything queued before the request is discarded first, so a late answer to an earlier request cannot match.
pub fn request(
    transport: &dyn Transport,
    listener: &Listener,
    request: &[u8],
    timeout: Duration,
    accept: &dyn Fn(&[u8]) -> bool,
) -> Result<Vec<u8>, MidiError> {
    listener.drain();
    transport.send(request)?;
    let mut answers = collect_sysex(listener, timeout, &mut |all| {
        all.last().is_some_and(|last| accept(last))
    })?;
    answers.pop().ok_or(MidiError::Timeout)
}

/// Matches a DSI Tempest message with this command byte: `F0 01 28 <command> …`.
pub fn dsi_command(command: u8) -> impl Fn(&[u8]) -> bool {
    move |message| message.len() >= 5 && message[..3] == [0xF0, 0x01, 0x28] && message[3] == command
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeTransport;
    use std::sync::Arc;

    const SHORT: Duration = Duration::from_millis(30);

    fn setup() -> (Arc<FakeTransport>, Listener) {
        let fake = Arc::new(FakeTransport::new());
        let listener = Listener::attach(Arc::clone(&fake) as Arc<dyn Transport>);
        (fake, listener)
    }

    const ASK: [u8; 5] = [0xF0, 0x01, 0x28, 0x40, 0xF7];
    const ANSWER: [u8; 7] = [0xF0, 0x01, 0x28, 0x60, 0x11, 0x22, 0xF7];

    #[test]
    fn test_request_when_machine_answers_returns_the_60_message() {
        let (fake, listener) = setup();
        fake.set_responder(Box::new(|sent| {
            if sent == ASK {
                vec![ANSWER.to_vec()]
            } else {
                vec![]
            }
        }));

        let answer = request(fake.as_ref(), &listener, &ASK, SHORT, &dsi_command(0x60));

        assert_eq!(answer, Ok(ANSWER.to_vec()));
        assert_eq!(fake.sent(), vec![ASK.to_vec()]);
    }

    #[test]
    fn test_request_when_clock_and_sensing_surround_the_answer_returns_it_unchanged() {
        let (fake, listener) = setup();
        fake.set_responder(Box::new(|_| {
            vec![
                vec![0xF8, 0xFE],
                vec![0xF0, 0x01, 0x28, 0x60, 0xF8, 0x11, 0x22, 0xFE, 0xF7],
                vec![0xF8],
            ]
        }));

        let answer = request(fake.as_ref(), &listener, &ASK, SHORT, &dsi_command(0x60));

        assert_eq!(answer, Ok(ANSWER.to_vec()));
    }

    #[test]
    fn test_request_when_answer_arrives_in_pieces_returns_it_whole() {
        let (fake, listener) = setup();
        fake.set_responder(Box::new(|_| {
            vec![vec![0xF0, 0x01, 0x28], vec![0x60, 0x11], vec![0x22, 0xF7]]
        }));

        let answer = request(fake.as_ref(), &listener, &ASK, SHORT, &dsi_command(0x60));

        assert_eq!(answer, Ok(ANSWER.to_vec()));
    }

    #[test]
    fn test_request_when_other_sysex_comes_first_returns_the_accepted_one() {
        let (fake, listener) = setup();
        fake.set_responder(Box::new(|_| {
            vec![vec![0xF0, 0x7E, 0x7F, 0x06, 0x02, 0xF7], ANSWER.to_vec()]
        }));

        let answer = request(fake.as_ref(), &listener, &ASK, SHORT, &dsi_command(0x60));

        assert_eq!(answer, Ok(ANSWER.to_vec()));
    }

    #[test]
    fn test_request_when_only_wrong_sysex_answers_returns_timeout() {
        let (fake, listener) = setup();
        fake.set_responder(Box::new(|_| vec![vec![0xF0, 0x01, 0x28, 0x5F, 0x01, 0xF7]]));

        let answer = request(fake.as_ref(), &listener, &ASK, SHORT, &dsi_command(0x60));

        assert_eq!(answer, Err(MidiError::Timeout));
    }

    #[test]
    fn test_request_when_machine_silent_returns_timeout() {
        let (fake, listener) = setup();

        assert_eq!(
            request(fake.as_ref(), &listener, &ASK, SHORT, &dsi_command(0x60)),
            Err(MidiError::Timeout)
        );
    }

    #[test]
    fn test_request_when_stale_answer_is_queued_returns_only_the_fresh_one() {
        let (fake, listener) = setup();
        fake.push_incoming(&[0xF0, 0x01, 0x28, 0x60, 0x7F, 0x7F, 0xF7]);
        fake.set_responder(Box::new(|_| vec![ANSWER.to_vec()]));

        let answer = request(fake.as_ref(), &listener, &ASK, SHORT, &dsi_command(0x60));

        assert_eq!(answer, Ok(ANSWER.to_vec()));
    }

    #[test]
    fn test_request_when_send_fails_returns_send_error() {
        let (fake, listener) = setup();
        fake.fail_sends();

        let answer = request(fake.as_ref(), &listener, &ASK, SHORT, &dsi_command(0x60));

        assert!(matches!(answer, Err(MidiError::Send(_))));
    }

    #[test]
    fn test_collect_sysex_when_done_after_two_returns_both() {
        let (fake, listener) = setup();
        fake.push_incoming(&[0xF0, 1, 0xF7, 0xF8, 0xF0, 2, 0xF7, 0xF0, 3, 0xF7]);

        let all = collect_sysex(&listener, SHORT, &mut |got| got.len() == 2);

        assert_eq!(all, Ok(vec![vec![0xF0, 1, 0xF7], vec![0xF0, 2, 0xF7]]));
    }

    #[test]
    fn test_dsi_command_when_other_command_or_manufacturer_returns_false() {
        let is_60 = dsi_command(0x60);

        assert!(is_60(&ANSWER));
        assert!(!is_60(&[0xF0, 0x01, 0x28, 0x5F, 0x00, 0xF7]));
        assert!(!is_60(&[0xF0, 0x02, 0x28, 0x60, 0x00, 0xF7]));
        assert!(!is_60(&[0xF0, 0x01, 0x28, 0x60]));
    }
}
