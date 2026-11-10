//! The mixer, delay and main values the machine reports (n°38, Tempester 1.6.4.0, `patch/mixer_reports.json`), machine
//! to app only, on Remote Pad OUT (the app does not know it: reports are read on any channel, like the other reports
//! of `performance`).
//!
//! Each report is a full NRPN, four full-status control changes: `Bm 63 01`, `Bm 62 (66 + item)`, `Bm 06 (v >> 7)`,
//! `Bm 26 (v & 7F)`, so number `230 + item` with the 14-bit value `v` (`nrpn::NrpnDecoder` rebuilds it). The machine
//! never reads these numbers: `nrpn::encode` refuses them, and the app must never send them.

use std::fmt;

use crate::nrpn::{self, Nrpn};

/// NRPN number of item 0.
pub const FIRST_REPORT_NRPN: u16 = 230;

/// What the machine reports. Level, Pan and Delay Send are those of the selected pad, Delay Time and Delay Repeats
/// those of the beat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixerItem {
    Level,
    Pan,
    DelaySend,
    DelayTime,
    DelayRepeats,
    /// Tempo times ten (30.0-250.0 BPM is 300-2500); 0 when the machine does not know it.
    Tempo,
    Swing,
    Volume,
    /// The machine's raw 0-255 knob value (0 below 4).
    Distort,
    /// The machine's raw 0-255 knob value.
    Compress,
}

impl MixerItem {
    pub const ALL: [Self; 10] = [
        Self::Level,
        Self::Pan,
        Self::DelaySend,
        Self::DelayTime,
        Self::DelayRepeats,
        Self::Tempo,
        Self::Swing,
        Self::Volume,
        Self::Distort,
        Self::Compress,
    ];

    /// The NRPN number carrying this item.
    pub fn number(self) -> u16 {
        FIRST_REPORT_NRPN + self as u16
    }

    /// The name the page knows it by.
    pub fn name(self) -> &'static str {
        match self {
            Self::Level => "level",
            Self::Pan => "pan",
            Self::DelaySend => "delay_send",
            Self::DelayTime => "delay_time",
            Self::DelayRepeats => "delay_repeats",
            Self::Tempo => "tempo",
            Self::Swing => "swing",
            Self::Volume => "volume",
            Self::Distort => "distort",
            Self::Compress => "compress",
        }
    }

    /// The largest value the machine reports for this item.
    pub fn max(self) -> u16 {
        match self {
            Self::Level | Self::Pan | Self::DelaySend | Self::Volume => 127,
            Self::DelayTime => 11,
            Self::DelayRepeats => 126,
            Self::Tempo => 2500,
            Self::Swing => 12,
            Self::Distort | Self::Compress => 255,
        }
    }

    pub fn from_number(number: u16) -> Option<Self> {
        let index = number.checked_sub(FIRST_REPORT_NRPN)?;
        Self::ALL.get(usize::from(index)).copied()
    }
}

/// Why a report could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixerReportError {
    BadChannel(u8),
    BadValue(MixerItem, u16),
}

impl fmt::Display for MixerReportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadChannel(c) => write!(f, "MIDI channel {c} is above 15"),
            Self::BadValue(item, v) => {
                write!(f, "{} {v} is above {}", item.name(), item.max())
            }
        }
    }
}

impl std::error::Error for MixerReportError {}

/// The report carried by a completed NRPN: its item and value, when the number is 230-239 and the value is within what
/// the machine reports for that item (anything else is not a report).
pub fn mixer_report(nrpn: Nrpn) -> Option<(MixerItem, u16)> {
    let item = MixerItem::from_number(nrpn.number)?;
    (nrpn.value <= item.max()).then_some((item, nrpn.value))
}

/// The 12 bytes the machine sends for `item` at `value` on `channel` (0-15): what the scripted machine plays.
pub fn report_bytes(
    channel: u8,
    item: MixerItem,
    value: u16,
) -> Result<[u8; 12], MixerReportError> {
    if channel > 15 {
        return Err(MixerReportError::BadChannel(channel));
    }
    if value > item.max() {
        return Err(MixerReportError::BadValue(item, value));
    }
    Ok(nrpn::frame(channel, item.number(), value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nrpn::NrpnDecoder;

    fn decode(bytes: &[u8]) -> Option<(MixerItem, u16)> {
        let mut decoder = NrpnDecoder::new();
        bytes
            .chunks(3)
            .filter_map(|m| decoder.feed_message(m[0], m[1], m[2]))
            .last()
            .and_then(mixer_report)
    }

    #[test]
    fn test_report_bytes_when_level_72_on_channel_3_returns_the_12_bytes_of_the_protocol() {
        assert_eq!(
            report_bytes(2, MixerItem::Level, 72),
            Ok([0xB2, 0x63, 0x01, 0xB2, 0x62, 0x66, 0xB2, 0x06, 0x00, 0xB2, 0x26, 0x48])
        );
    }

    #[test]
    fn test_mixer_report_when_every_item_is_sent_at_its_bounds_returns_it_back() {
        for (k, item) in MixerItem::ALL.into_iter().enumerate() {
            assert_eq!(item.number(), 230 + k as u16);
            for value in [0, 1, item.max() / 2, item.max()] {
                let bytes = report_bytes(5, item, value).unwrap();
                assert_eq!(decode(&bytes), Some((item, value)), "{}", item.name());
            }
        }
    }

    #[test]
    fn test_mixer_report_when_value_above_the_item_maximum_returns_none() {
        for item in MixerItem::ALL {
            let nrpn = Nrpn {
                channel: 0,
                number: item.number(),
                value: item.max() + 1,
            };
            assert_eq!(mixer_report(nrpn), None, "{}", item.name());
        }
    }

    #[test]
    fn test_mixer_report_when_other_numbers_returns_none() {
        for number in [0, 126, 156, 200, 213, 220, 229, 240, 16383] {
            let nrpn = Nrpn {
                channel: 0,
                number,
                value: 1,
            };
            assert_eq!(mixer_report(nrpn), None, "NRPN {number}");
        }
    }

    #[test]
    fn test_mixer_report_when_the_tempo_is_unknown_returns_zero() {
        let bytes = report_bytes(0, MixerItem::Tempo, 0).unwrap();
        assert_eq!(decode(&bytes), Some((MixerItem::Tempo, 0)));
        let bytes = report_bytes(0, MixerItem::Tempo, 1215).unwrap();
        assert_eq!(decode(&bytes), Some((MixerItem::Tempo, 1215)));
    }

    #[test]
    fn test_report_bytes_when_channel_or_value_out_of_range_returns_error() {
        assert_eq!(
            report_bytes(16, MixerItem::Pan, 0),
            Err(MixerReportError::BadChannel(16))
        );
        assert_eq!(
            report_bytes(0, MixerItem::DelayTime, 12),
            Err(MixerReportError::BadValue(MixerItem::DelayTime, 12))
        );
    }

    #[test]
    fn test_nrpn_encode_when_a_report_number_returns_error_the_app_never_sends_them() {
        for item in MixerItem::ALL {
            assert!(nrpn::encode(0, item.number(), 1).is_err());
        }
    }
}
