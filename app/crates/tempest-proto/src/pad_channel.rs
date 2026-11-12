//! The MIDI output channel of each pad (n°58, Tempester 1.6.5.0, `public-src/docs/midi-protocol.md` "Channel per pad").
//!
//! Each of the 32 pads has a value 0-32: 0 = the usual route, 1-16 = that channel with the note the pad plays,
//! 17-32 = channel (value - 16) with the pad's "MIDI Note: Pad" note (a fixed drum note). Set over MIDI on Remote Pad
//! IN: `Bn 0F pad` addresses a pad (the machine answers `Bm 10 value` on Remote Pad OUT), `Bn 10 value` sets the
//! addressed pad's value, `Bn 11 7F` saves the System settings (only while stopped: the save stops the transport).

use std::fmt;

/// CC that addresses a pad (to the machine).
pub const CC_PAD_SELECT: u8 = 15;
/// CC that carries the value: set (to the machine) or answer to `CC_PAD_SELECT` (from it).
pub const CC_PAD_CHANNEL: u8 = 16;
/// CC that saves the System settings when its value is 127.
pub const CC_SYSTEM_SAVE: u8 = 17;
/// Pads of the machine, A1-B16.
pub const PADS: u8 = 32;
/// The highest value: channel 16 with the pad's own note.
pub const VALUE_MAX: u8 = 32;
const SAVE_VALUE: u8 = 127;

/// Why a message could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadChannelError {
    BadChannel(u8),
    BadPad(u8),
    BadValue(u8),
}

impl fmt::Display for PadChannelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadChannel(c) => write!(f, "MIDI channel {c} is above 15"),
            Self::BadPad(p) => write!(f, "pad {p} is not 0-{}", PADS - 1),
            Self::BadValue(v) => write!(f, "pad channel {v} is above {VALUE_MAX}"),
        }
    }
}

impl std::error::Error for PadChannelError {}

fn status(channel: u8) -> Result<u8, PadChannelError> {
    if channel > 15 {
        return Err(PadChannelError::BadChannel(channel));
    }
    Ok(0xB0 | channel)
}

/// `Bn 0F pad`: addresses `pad` (0-31); the machine answers with its value.
pub fn select(channel: u8, pad: u8) -> Result<[u8; 3], PadChannelError> {
    if pad >= PADS {
        return Err(PadChannelError::BadPad(pad));
    }
    Ok([status(channel)?, CC_PAD_SELECT, pad])
}

/// `Bn 0F pad`, `Bn 10 value`: sets the value (0-32) of `pad`.
pub fn set(channel: u8, pad: u8, value: u8) -> Result<[u8; 6], PadChannelError> {
    if value > VALUE_MAX {
        return Err(PadChannelError::BadValue(value));
    }
    let [a, b, c] = select(channel, pad)?;
    Ok([a, b, c, a, CC_PAD_CHANNEL, value])
}

/// `Bn 11 7F`: saves the System settings (ignored by the machine while it plays or records).
pub fn save(channel: u8) -> Result<[u8; 3], PadChannelError> {
    Ok([status(channel)?, CC_SYSTEM_SAVE, SAVE_VALUE])
}

/// The value of an answer `Bm 10 value` from the machine, on any channel; `None` for any other message or a value
/// above 32.
pub fn report(status: u8, controller: u8, value: u8) -> Option<u8> {
    (status & 0xF0 == 0xB0 && controller == CC_PAD_CHANNEL && value <= VALUE_MAX).then_some(value)
}

/// What the value means, as the pad settings page of the machine says it: "Off", "5", "5 Fix".
pub fn label(value: u8) -> String {
    match value {
        0 => "Off".to_string(),
        1..=16 => value.to_string(),
        17..=VALUE_MAX => format!("{} Fix", value - 16),
        _ => "?".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_when_pad_and_value_are_valid_returns_cc_15_then_cc_16() {
        assert_eq!(set(0, 31, 32), Ok([0xB0, 0x0F, 31, 0xB0, 0x10, 32]));
        assert_eq!(set(2, 0, 0), Ok([0xB2, 0x0F, 0, 0xB2, 0x10, 0]));
    }

    #[test]
    fn test_set_when_out_of_range_returns_error() {
        assert_eq!(set(0, 32, 0), Err(PadChannelError::BadPad(32)));
        assert_eq!(set(0, 0, 33), Err(PadChannelError::BadValue(33)));
        assert_eq!(set(16, 0, 0), Err(PadChannelError::BadChannel(16)));
    }

    #[test]
    fn test_select_and_save_when_valid_return_the_documented_bytes() {
        assert_eq!(select(1, 5), Ok([0xB1, 0x0F, 5]));
        assert_eq!(save(0), Ok([0xB0, 0x11, 0x7F]));
    }

    #[test]
    fn test_report_when_answer_returns_the_value_on_any_channel_and_ignores_the_rest() {
        assert_eq!(report(0xB1, 0x10, 20), Some(20));
        assert_eq!(report(0xBF, 0x10, 0), Some(0));
        assert_eq!(report(0xB0, 0x10, 33), None);
        assert_eq!(report(0xB0, 0x0F, 3), None);
        assert_eq!(report(0x90, 0x10, 3), None);
    }

    #[test]
    fn test_label_when_each_range_returns_off_channel_or_fixed_channel() {
        assert_eq!(label(0), "Off");
        assert_eq!(label(16), "16");
        assert_eq!(label(17), "1 Fix");
        assert_eq!(label(32), "16 Fix");
    }
}
