//! Paced sending of large SysEx (OS images, backups). The machine reads a DIN-speed stream: 3,125 bytes per second
//! is the rate of a MIDI cable and the proven default (`send` in `tools/tempest_midi.swift`: 128-byte packets,
//! each followed by a wait that keeps the average at the chosen rate).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::error::MidiError;
use crate::transport::Transport;

/// Bytes per second of a MIDI cable.
pub const DEFAULT_RATE: u32 = 3125;
/// Bytes per packet: one packet every 41 ms at the default rate.
pub const CHUNK_SIZE: usize = 128;

/// Time source, replaceable so that tests do not wait.
pub trait Clock {
    /// Time elapsed since the clock started.
    fn elapsed(&self) -> Duration;
    fn sleep(&self, duration: Duration);
}

/// The real clock.
pub struct SystemClock(Instant);

impl SystemClock {
    pub fn new() -> Self {
        Self(Instant::now())
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn elapsed(&self) -> Duration {
        self.0.elapsed()
    }

    fn sleep(&self, duration: Duration) {
        thread::sleep(duration);
    }
}

/// Lets another thread stop a paced send between two packets.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Reported after each packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub sent: usize,
    pub total: usize,
}

/// Sends `data` in 128-byte packets at `rate` bytes per second, reporting progress after each packet.
/// Returns `Cancelled` if the token fires (checked before each packet): the machine then holds a half message and
/// must be restarted. The call returns once the last byte has had time to travel.
pub fn send_paced(
    transport: &dyn Transport,
    data: &[u8],
    rate: u32,
    cancel: &CancelToken,
    progress: &mut dyn FnMut(Progress),
) -> Result<(), MidiError> {
    send_paced_with(&SystemClock::new(), transport, data, rate, cancel, progress)
}

/// `send_paced` with an explicit clock.
pub fn send_paced_with(
    clock: &dyn Clock,
    transport: &dyn Transport,
    data: &[u8],
    rate: u32,
    cancel: &CancelToken,
    progress: &mut dyn FnMut(Progress),
) -> Result<(), MidiError> {
    if rate == 0 {
        return Err(MidiError::InvalidRate);
    }
    let mut sent = 0;
    for packet in data.chunks(CHUNK_SIZE) {
        if cancel.is_cancelled() {
            return Err(MidiError::Cancelled { sent });
        }
        transport.send(packet)?;
        sent += packet.len();
        wait_until_due(clock, sent, rate);
        progress(Progress {
            sent,
            total: data.len(),
        });
    }
    Ok(())
}

fn wait_until_due(clock: &dyn Clock, sent: usize, rate: u32) {
    let due = Duration::from_secs_f64(sent as f64 / f64::from(rate));
    if let Some(wait) = due.checked_sub(clock.elapsed()) {
        clock.sleep(wait);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeTransport;
    use std::cell::Cell;

    /// A clock that only moves when asked to sleep.
    #[derive(Default)]
    struct ManualClock(Cell<Duration>);

    impl Clock for ManualClock {
        fn elapsed(&self) -> Duration {
            self.0.get()
        }

        fn sleep(&self, duration: Duration) {
            self.0.set(self.0.get() + duration);
        }
    }

    fn run(
        data: &[u8],
        rate: u32,
        cancel: &CancelToken,
    ) -> (
        Result<(), MidiError>,
        FakeTransport,
        Duration,
        Vec<Progress>,
    ) {
        let (clock, fake) = (ManualClock::default(), FakeTransport::new());
        let mut seen = Vec::new();
        let result = send_paced_with(&clock, &fake, data, rate, cancel, &mut |p| seen.push(p));
        (result, fake, clock.elapsed(), seen)
    }

    #[test]
    fn test_send_paced_when_3125_bytes_at_default_rate_returns_one_second() {
        let data = vec![0x55; 3125];

        let (result, fake, elapsed, _) = run(&data, DEFAULT_RATE, &CancelToken::new());

        assert_eq!(result, Ok(()));
        assert_eq!(fake.sent_bytes(), data);
        assert!((elapsed.as_secs_f64() - 1.0).abs() < 1e-6, "{elapsed:?}");
    }

    #[test]
    fn test_send_paced_when_rate_doubles_returns_half_the_time() {
        let data = vec![0u8; 3125];

        let (_, _, slow, _) = run(&data, 3125, &CancelToken::new());
        let (_, _, fast, _) = run(&data, 6250, &CancelToken::new());

        assert!((fast.as_secs_f64() - 0.5).abs() < 1e-6);
        assert_ne!(slow, fast);
    }

    #[test]
    fn test_send_paced_when_data_returns_128_byte_packets_and_a_shorter_last() {
        let data = vec![1u8; 300];

        let (_, fake, _, _) = run(&data, DEFAULT_RATE, &CancelToken::new());

        let sizes: Vec<usize> = fake.sent().iter().map(Vec::len).collect();
        assert_eq!(sizes, vec![128, 128, 44]);
    }

    #[test]
    fn test_send_paced_when_running_returns_progress_after_each_packet() {
        let (_, _, _, seen) = run(&[0u8; 300], DEFAULT_RATE, &CancelToken::new());

        assert_eq!(
            seen,
            vec![
                Progress {
                    sent: 128,
                    total: 300
                },
                Progress {
                    sent: 256,
                    total: 300
                },
                Progress {
                    sent: 300,
                    total: 300
                },
            ]
        );
    }

    #[test]
    fn test_send_paced_when_cancelled_before_start_returns_cancelled_and_sends_nothing() {
        let cancel = CancelToken::new();
        cancel.cancel();

        let (result, fake, _, _) = run(&[0u8; 300], DEFAULT_RATE, &cancel);

        assert_eq!(result, Err(MidiError::Cancelled { sent: 0 }));
        assert!(fake.sent().is_empty());
    }

    #[test]
    fn test_send_paced_when_cancelled_after_first_packet_returns_sent_count() {
        let (clock, fake) = (ManualClock::default(), FakeTransport::new());
        let cancel = CancelToken::new();
        let stop = cancel.clone();

        let result = send_paced_with(
            &clock,
            &fake,
            &[0u8; 300],
            DEFAULT_RATE,
            &cancel,
            &mut |_| {
                stop.cancel();
            },
        );

        assert_eq!(result, Err(MidiError::Cancelled { sent: 128 }));
        assert_eq!(fake.sent().len(), 1);
    }

    #[test]
    fn test_send_paced_when_rate_zero_returns_invalid_rate() {
        let (result, fake, _, _) = run(&[0u8; 10], 0, &CancelToken::new());

        assert_eq!(result, Err(MidiError::InvalidRate));
        assert!(fake.sent().is_empty());
    }

    #[test]
    fn test_send_paced_when_transport_fails_returns_send_error() {
        let (clock, fake) = (ManualClock::default(), FakeTransport::new());
        fake.fail_sends();

        let result = send_paced_with(
            &clock,
            &fake,
            &[0u8; 10],
            3125,
            &CancelToken::new(),
            &mut |_| {},
        );

        assert!(matches!(result, Err(MidiError::Send(_))));
    }

    #[test]
    fn test_send_paced_when_no_data_returns_ok_without_progress() {
        let (result, _, elapsed, seen) = run(&[], DEFAULT_RATE, &CancelToken::new());

        assert_eq!(result, Ok(()));
        assert!(seen.is_empty());
        assert_eq!(elapsed, Duration::ZERO);
    }
}
