//! Readable labels for MIDI messages, for a live monitor: one complete message in, zero or more lines out.
//!
//! - `control`: notes, control changes (the ones the Tempest uses are named) and NRPN reassembled from CC 99, 98, 6
//!   and 38 into one line;
//! - `sysex`: identity, the DSI requests and dumps, OS images;
//! - this file: the other messages (transport, clock, system) and the `Describer` that keeps the NRPN state.
//!
//! An NRPN part that is never completed is not lost: it comes out as plain control changes once the next message of
//! another kind arrives or `STALE_MS` has passed, because a knob that sends half an NRPN is exactly what a monitor
//! must show.

mod control;
mod sysex;
mod tables;

use control::ChannelState;

use crate::pad_notes::PadNotes;

/// How long NRPN parts wait for their CC 38 before they are shown as they are.
pub const STALE_MS: u64 = 250;
/// Bytes of a message shown in hex; a longer one ends with its total size.
const RAW_BYTES: usize = 16;

/// What a line is about; the page uses it for filtering and colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Note,
    Control,
    Nrpn,
    SysEx,
    /// Start, Continue, Stop.
    Transport,
    /// Timing clock and active sensing: counted rather than listed by default.
    Clock,
    Other,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Control => "cc",
            Self::Nrpn => "nrpn",
            Self::SysEx => "sysex",
            Self::Transport => "transport",
            Self::Clock => "clock",
            Self::Other => "other",
        }
    }
}

/// One line of the monitor: what it is, a sentence, and the bytes (SysEx truncated).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub kind: Kind,
    pub label: String,
    pub raw: String,
}

impl Line {
    fn new(kind: Kind, label: impl Into<String>, bytes: &[u8]) -> Self {
        Self {
            kind,
            label: label.into(),
            raw: raw_hex(bytes),
        }
    }
}

/// `B0 63 00`; beyond `RAW_BYTES` bytes, the first ones then `... (N bytes)`.
pub fn raw_hex(bytes: &[u8]) -> String {
    let shown: Vec<String> = bytes
        .iter()
        .take(RAW_BYTES)
        .map(|b| format!("{b:02X}"))
        .collect();
    let mut text = shown.join(" ");
    if bytes.len() > RAW_BYTES {
        text.push_str(&format!(" ... ({} bytes)", bytes.len()));
    }
    text
}

/// Describes the messages of one direction of one port, in order. Keep one per direction: the NRPN state of what
/// the app sends must not mix with the NRPN state of what the machine sends.
#[derive(Debug, Default)]
pub struct Describer {
    channels: [ChannelState; 16],
    /// Which note is which pad: the machine's own table, as the app knows it.
    pad_notes: PadNotes,
    /// The channel (0-15) whose notes are the machine's pads (its Remote Pad OUT); `None`: any channel. A note on
    /// another channel is not a pad, whatever its number (the Sequencer or Synth Sound plays its pitch there).
    pad_channel: Option<u8>,
}

impl Describer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets every NRPN state (a new connection); the pad notes and their channel stay.
    pub fn reset(&mut self) {
        *self = Self {
            pad_notes: self.pad_notes,
            pad_channel: self.pad_channel,
            ..Self::default()
        };
    }

    /// The table that names the pad of a note.
    pub fn set_pad_notes(&mut self, notes: PadNotes) {
        self.pad_notes = notes;
    }

    /// Only notes on `channel` (0-15) are named as pads; `None`: notes on any channel.
    pub fn set_pad_channel(&mut self, channel: Option<u8>) {
        self.pad_channel = channel;
    }

    /// The lines of one complete message (status byte first; a SysEx whole, `F0 ... F7`) seen at `now_ms`.
    pub fn feed(&mut self, now_ms: u64, message: &[u8]) -> Vec<Line> {
        let Some(&status) = message.first() else {
            return Vec::new();
        };
        match status {
            0x80..=0xEF => self.channel_message(now_ms, message),
            0xF0 => vec![sysex::describe(message)],
            0xF1..=0xFF => vec![system_message(message)],
            _ => vec![Line::new(
                Kind::Other,
                "Data byte without a status",
                message,
            )],
        }
    }

    /// Shows the NRPN parts that waited `STALE_MS` or more for their CC 38.
    pub fn flush_stale(&mut self, now_ms: u64) -> Vec<Line> {
        self.channels
            .iter_mut()
            .filter(|state| {
                state
                    .waiting_since()
                    .is_some_and(|t| now_ms.saturating_sub(t) >= STALE_MS)
            })
            .flat_map(ChannelState::flush)
            .collect()
    }
}

fn system_message(message: &[u8]) -> Line {
    let (kind, label) = match message[0] {
        0xF8 => (Kind::Clock, "Clock"),
        0xFE => (Kind::Clock, "Active sensing"),
        0xFA => (Kind::Transport, "Start"),
        0xFB => (Kind::Transport, "Continue"),
        0xFC => (Kind::Transport, "Stop"),
        0xFF => (Kind::Other, "System reset"),
        0xF1 => (Kind::Other, "MIDI time code quarter frame"),
        0xF2 => (Kind::Other, "Song position"),
        0xF3 => (Kind::Other, "Song select"),
        0xF6 => (Kind::Other, "Tune request"),
        0xF7 => (Kind::Other, "End of SysEx without a start"),
        _ => (Kind::Other, "Undefined system byte"),
    };
    Line::new(kind, label, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(lines: &[Line]) -> Vec<&str> {
        lines.iter().map(|l| l.label.as_str()).collect()
    }

    #[test]
    fn test_feed_when_transport_and_clock_bytes_returns_their_kinds() {
        let mut d = Describer::new();

        let start = d.feed(0, &[0xFA]);
        let stop = d.feed(0, &[0xFC]);
        let clock = d.feed(0, &[0xF8]);
        let sensing = d.feed(0, &[0xFE]);

        assert_eq!(
            (start[0].kind, labels(&start)),
            (Kind::Transport, vec!["Start"])
        );
        assert_eq!(labels(&stop), vec!["Stop"]);
        assert_eq!(clock[0].kind, Kind::Clock);
        assert_eq!(
            (sensing[0].kind, labels(&sensing)),
            (Kind::Clock, vec!["Active sensing"])
        );
        assert_eq!(labels(&d.feed(0, &[0xFB])), vec!["Continue"]);
    }

    #[test]
    fn test_feed_when_empty_or_stray_data_returns_nothing_or_a_plain_label() {
        let mut d = Describer::new();

        assert!(d.feed(0, &[]).is_empty());
        assert_eq!(
            labels(&d.feed(0, &[0x40])),
            vec!["Data byte without a status"]
        );
    }

    #[test]
    fn test_raw_hex_when_long_message_returns_first_bytes_and_total_size() {
        let bytes: Vec<u8> = (0..40).collect();

        let text = raw_hex(&bytes);

        assert!(text.starts_with("00 01 02"), "{text}");
        assert!(text.ends_with("... (40 bytes)"), "{text}");
        assert_eq!(raw_hex(&[0xB0, 0x63, 0x00]), "B0 63 00");
    }

    #[test]
    fn test_reset_when_nrpn_selected_returns_a_decoder_that_forgets_the_number() {
        let mut d = Describer::new();
        d.feed(0, &[0xB0, 99, 0]);
        d.feed(0, &[0xB0, 98, 34]);
        d.reset();

        let lines = d.feed(1, &[0xB0, 38, 5]);

        assert!(lines[0].label.contains("no NRPN number"), "{:?}", lines[0]);
    }
}
