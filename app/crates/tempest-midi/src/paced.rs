//! Paced sending of large SysEx (OS images, backups). The machine reads a DIN-speed stream: 3,125 bytes per second
//! is the rate of a MIDI cable and the proven default (`send` in `tools/tempest_midi.swift`: 128-byte packets,
//! each followed by a wait that keeps the average at the chosen rate).
//!
//! Where the MIDI system takes no SysEx in pieces (WinMM, ALSA: `Transport::accepts_sysex_fragments`), each
//! `F0 … F7` message goes out in one `send` instead. Pacing then exists only between messages: an OS file is a single
//! message (about 600 KB for the Main), so it leaves in one call that returns once the driver has taken the whole
//! buffer, with no pacing inside it, one progress report at the end, and no cancel once it has started. The wait after
//! each message still keeps the whole send from ending before `len / rate`. The machine reads USB with flow control
//! (`refs/firmware-main.md` §6.1), so it takes an unpaced USB stream.

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

const SYSEX_START: u8 = 0xF0;
const SYSEX_END: u8 = 0xF7;

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

/// Sends `data` at `rate` bytes per second, reporting progress after each packet. A transport that takes SysEx in
/// pieces gets 128-byte packets; any other gets one packet per `F0 … F7` message (so a one-message file is one
/// packet, one progress report), and data that is not a sequence of such messages is refused before the first byte
/// (`NotWholeSysex`).
/// Returns `Cancelled` if the token fires (checked before each packet): in 128-byte packets the machine then holds a
/// half message and must be restarted; in whole messages a cancel acts only between two messages, never inside one.
/// The call returns no sooner than `len / rate` after it started.
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
    let packets = if transport.accepts_sysex_fragments() {
        data.chunks(CHUNK_SIZE).collect()
    } else {
        whole_sysex_messages(data)?
    };
    let pace = Pace {
        clock,
        rate,
        cancel,
    };
    send_packets(&pace, transport, &packets, data.len(), progress)
}

/// How a send is timed and stopped.
struct Pace<'a> {
    clock: &'a dyn Clock,
    rate: u32,
    cancel: &'a CancelToken,
}

/// Sends `packets` in order, waiting after each until the bytes sent so far have had time to travel at the rate.
fn send_packets(
    pace: &Pace,
    transport: &dyn Transport,
    packets: &[&[u8]],
    total: usize,
    progress: &mut dyn FnMut(Progress),
) -> Result<(), MidiError> {
    let mut sent = 0;
    for packet in packets {
        if pace.cancel.is_cancelled() {
            return Err(MidiError::Cancelled { sent });
        }
        transport.send(packet)?;
        sent += packet.len();
        wait_until_due(pace.clock, sent, pace.rate);
        progress(Progress { sent, total });
    }
    Ok(())
}

/// Splits `data` into its `F0 … F7` messages. Refused at the first byte that is outside a message, a status byte
/// inside one (real-time bytes included: no Tempest file holds any), or the end of data where an `F7` is missing:
/// a transport that only takes whole messages would otherwise drop or reject part of the data midway.
fn whole_sysex_messages(data: &[u8]) -> Result<Vec<&[u8]>, MidiError> {
    let mut messages = Vec::new();
    let mut start = 0;
    while let Some(&first) = data.get(start) {
        if first != SYSEX_START {
            return Err(MidiError::NotWholeSysex { offset: start });
        }
        let body = start + 1;
        let end = match data[body..].iter().position(|&byte| byte >= 0x80) {
            Some(at) if data[body + at] == SYSEX_END => body + at + 1,
            Some(at) => return Err(MidiError::NotWholeSysex { offset: body + at }),
            None => return Err(MidiError::NotWholeSysex { offset: data.len() }),
        };
        messages.push(&data[start..end]);
        start = end;
    }
    Ok(messages)
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
    /// A SysEx message of `len` bytes, `F0` and `F7` included.
    fn sysex(len: usize) -> Vec<u8> {
        let body = (0..len - 2).map(|i| (i % 0x80) as u8);
        std::iter::once(SYSEX_START)
            .chain(body)
            .chain(std::iter::once(SYSEX_END))
            .collect()
    }

    /// Three messages of unequal sizes, 3125 bytes in all (a sound or project send is a few messages; an OS file is
    /// one).
    fn three_messages() -> Vec<u8> {
        [sysex(1171), sysex(1949), sysex(5)].concat()
    }

    /// `run` on a transport that, like midir on Windows, takes SysEx only whole.
    fn run_whole_messages(
        data: &[u8],
        cancel: &CancelToken,
        progress: &mut dyn FnMut(Progress),
    ) -> (Result<(), MidiError>, FakeTransport, Duration) {
        let (clock, fake) = (ManualClock::default(), FakeTransport::new());
        fake.refuse_sysex_fragments();
        let result = send_paced_with(&clock, &fake, data, DEFAULT_RATE, cancel, progress);
        (result, fake, clock.elapsed())
    }

    #[test]
    fn test_send_paced_when_fragments_refused_returns_one_packet_per_message() {
        let data = three_messages();

        let (result, fake, _) = run_whole_messages(&data, &CancelToken::new(), &mut |_| {});

        assert_eq!(result, Ok(()));
        let sizes: Vec<usize> = fake.sent().iter().map(Vec::len).collect();
        assert_eq!(sizes, vec![1171, 1949, 5]);
        assert_eq!(fake.sent_bytes(), data);
    }

    /// The real shape of an OS file: one message (the Main's is 599,559 bytes).
    #[test]
    fn test_send_paced_when_fragments_refused_and_one_large_message_returns_one_packet_and_one_progress(
    ) {
        let data = sysex(600_000);
        let mut seen = Vec::new();

        let (result, fake, elapsed) =
            run_whole_messages(&data, &CancelToken::new(), &mut |p| seen.push(p));

        assert_eq!(result, Ok(()));
        assert_eq!(fake.sent(), vec![data]);
        assert_eq!(
            seen,
            vec![Progress {
                sent: 600_000,
                total: 600_000
            }]
        );
        assert!((elapsed.as_secs_f64() - 192.0).abs() < 1e-6, "{elapsed:?}");
    }

    #[test]
    fn test_send_paced_when_fragments_refused_returns_length_over_rate() {
        let data = three_messages();

        let (_, _, elapsed) = run_whole_messages(&data, &CancelToken::new(), &mut |_| {});

        assert!((elapsed.as_secs_f64() - 1.0).abs() < 1e-6, "{elapsed:?}");
    }

    #[test]
    fn test_send_paced_when_fragments_refused_returns_progress_after_each_message() {
        let mut seen = Vec::new();

        let (result, _, _) = run_whole_messages(&three_messages(), &CancelToken::new(), &mut |p| {
            seen.push(p);
        });

        assert_eq!(result, Ok(()));
        let total = 3125;
        assert_eq!(
            seen,
            vec![
                Progress { sent: 1171, total },
                Progress { sent: 3120, total },
                Progress { sent: 3125, total },
            ]
        );
    }

    #[test]
    fn test_send_paced_when_fragments_refused_and_cancelled_returns_cancelled_between_messages() {
        let cancel = CancelToken::new();
        let stop = cancel.clone();

        let (result, fake, _) =
            run_whole_messages(&three_messages(), &cancel, &mut |_| stop.cancel());

        assert_eq!(result, Err(MidiError::Cancelled { sent: 1171 }));
        assert_eq!(fake.sent(), vec![sysex(1171)]);
    }

    #[test]
    fn test_send_paced_when_fragments_refused_and_no_data_returns_ok_without_sending() {
        let (result, fake, elapsed) = run_whole_messages(&[], &CancelToken::new(), &mut |_| {});

        assert_eq!(result, Ok(()));
        assert!(fake.sent().is_empty());
        assert_eq!(elapsed, Duration::ZERO);
    }

    #[test]
    fn test_send_paced_when_fragments_refused_and_data_malformed_returns_not_whole_sysex_and_sends_nothing(
    ) {
        let outside = [sysex(10), vec![0x00], sysex(10)].concat();
        let unterminated = [sysex(10), vec![SYSEX_START, 0x01, 0x02]].concat();
        let status_inside = [sysex(10), vec![SYSEX_START, 0x01, 0x90, 0x02, SYSEX_END]].concat();
        let cases = [
            (outside, 10),
            (unterminated, 13),
            (status_inside, 12),
            (vec![0x55; 300], 0),
        ];

        for (data, offset) in cases {
            let mut progressed = false;
            let (result, fake, _) =
                run_whole_messages(&data, &CancelToken::new(), &mut |_| progressed = true);

            assert_eq!(
                result,
                Err(MidiError::NotWholeSysex { offset }),
                "{data:02X?}"
            );
            assert!(fake.sent().is_empty());
            assert!(!progressed);
        }
    }

    /// The reported Windows bug: 128-byte pieces on a transport that takes SysEx only whole. The first piece starts
    /// with `F0` and goes out; the second does not and is refused.
    #[test]
    fn test_send_packets_when_fragments_refused_and_128_byte_pieces_returns_send_error_at_second_packet(
    ) {
        let (clock, fake) = (ManualClock::default(), FakeTransport::new());
        fake.refuse_sysex_fragments();
        let data = three_messages();
        let pieces: Vec<&[u8]> = data.chunks(CHUNK_SIZE).collect();
        let cancel = CancelToken::new();
        let pace = Pace {
            clock: &clock,
            rate: DEFAULT_RATE,
            cancel: &cancel,
        };

        let mut seen = Vec::new();

        let result = send_packets(&pace, &fake, &pieces, data.len(), &mut |p| {
            seen.push(p.sent)
        });

        assert_eq!(
            result,
            Err(MidiError::Send(
                "non-sysex message must not be longer than 3 bytes".into()
            ))
        );
        assert_eq!(fake.sent().len(), 1);
        assert_eq!(
            seen,
            vec![128],
            "the user saw \"128 B of …\" before the error"
        );
    }
}
