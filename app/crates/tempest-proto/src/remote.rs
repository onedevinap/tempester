//! The remote panel of Tempester Main 1.6.0.0 (`Docs/analyses/feature-remote-panel.md` §5, tables in
//! `refs/panel_keys.tsv`, `refs/panel_leds.tsv`, `refs/panel_controls.tsv`).
//!
//! Keys: `CC 102` press and `CC 103` release, value = key code 0-48, on the Remote Pad IN channel. LEDs: the machine
//! sends `CC 104` (on) and `CC 105` (off), value = LED index 0-59, on its Remote Pad OUT channel; `CC 107` asks for
//! the whole state (value bit 0 = include the 16 pad LEDs). Absolute control `k`: NRPN `220 + k`, value 0-255.
//! The original OS and earlier Tempester versions never send these control changes, so a received 104 or 105 can
//! only come from the remote panel.

use std::fmt;

use crate::nrpn;

pub const CC_KEY_PRESS: u8 = 102;
pub const CC_KEY_RELEASE: u8 = 103;
pub const CC_LED_ON: u8 = 104;
pub const CC_LED_OFF: u8 = 105;
pub const CC_STATE_REQUEST: u8 = 107;
/// Key codes are 0-48.
pub const KEY_COUNT: u8 = 49;
/// LED indexes are 0-59; 0x2C-0x3B are the 16 pad LEDs.
pub const LED_COUNT: u8 = 60;
pub const FIRST_PAD_LED: u8 = 0x2C;
/// Absolute controls 0-9 are NRPN 220-229.
pub const CONTROL_COUNT: u8 = 10;
pub const FIRST_CONTROL_NRPN: u16 = 220;
pub const CONTROL_MAX_VALUE: u16 = 255;
/// Absolute control 8 (NRPN 228).
pub const VOLUME_CONTROL: u8 = 8;
/// Encoders 0-13 are NRPN 200-213; the value is 8192 plus a signed step.
pub const ENCODER_COUNT: u8 = 14;
pub const FIRST_ENCODER_NRPN: u16 = 200;
pub const ENCODER_CENTER: u16 = 8192;
/// The machine clamps a relative step to -16..=15 (a Panel message carries one signed byte).
pub const ENCODER_MIN_STEP: i8 = -16;
pub const ENCODER_MAX_STEP: i8 = 15;

/// Why a remote-panel message could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteError {
    BadChannel(u8),
    BadKey(u8),
    BadControl(u8),
    BadValue(u16),
    BadEncoder(u8),
    BadStep(i8),
}

impl fmt::Display for RemoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadChannel(c) => write!(f, "MIDI channel {c} is above 15"),
            Self::BadKey(k) => write!(f, "key code {k} is not 0-{}", KEY_COUNT - 1),
            Self::BadControl(k) => write!(f, "control {k} is not 0-{}", CONTROL_COUNT - 1),
            Self::BadValue(v) => write!(f, "control value {v} is above {CONTROL_MAX_VALUE}"),
            Self::BadEncoder(e) => write!(f, "encoder {e} is not 0-{}", ENCODER_COUNT - 1),
            Self::BadStep(s) => write!(
                f,
                "encoder step {s} is not {ENCODER_MIN_STEP} to {ENCODER_MAX_STEP}"
            ),
        }
    }
}

impl std::error::Error for RemoteError {}

/// A LED turned on or off by the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LedChange {
    pub index: u8,
    pub on: bool,
}

fn status(channel: u8) -> Result<u8, RemoteError> {
    if channel > 15 {
        return Err(RemoteError::BadChannel(channel));
    }
    Ok(0xB0 | channel)
}

/// Key `code` pressed (`down`) or released on `channel` (0-15).
pub fn key(channel: u8, down: bool, code: u8) -> Result<[u8; 3], RemoteError> {
    let status = status(channel)?;
    if code >= KEY_COUNT {
        return Err(RemoteError::BadKey(code));
    }
    let controller = if down { CC_KEY_PRESS } else { CC_KEY_RELEASE };
    Ok([status, controller, code])
}

/// The request for every LED, pads included or not.
pub fn state_request(channel: u8, with_pads: bool) -> Result<[u8; 3], RemoteError> {
    Ok([status(channel)?, CC_STATE_REQUEST, u8::from(with_pads)])
}

/// Absolute control `index` (0-9) set to `value` (0-255; the machine uses `value >> 1`).
pub fn control(channel: u8, index: u8, value: u16) -> Result<[u8; 12], RemoteError> {
    status(channel)?;
    if index >= CONTROL_COUNT {
        return Err(RemoteError::BadControl(index));
    }
    if value > CONTROL_MAX_VALUE {
        return Err(RemoteError::BadValue(value));
    }
    Ok(nrpn::frame(
        channel,
        FIRST_CONTROL_NRPN + u16::from(index),
        value,
    ))
}

/// Encoder `index` (0-13) turned by `step` detents (-16 to 15; 0 does nothing on the machine): NRPN `200 + index`,
/// value `8192 + step`.
pub fn encoder(channel: u8, index: u8, step: i8) -> Result<[u8; 12], RemoteError> {
    status(channel)?;
    if index >= ENCODER_COUNT {
        return Err(RemoteError::BadEncoder(index));
    }
    if !(ENCODER_MIN_STEP..=ENCODER_MAX_STEP).contains(&step) {
        return Err(RemoteError::BadStep(step));
    }
    let value = ENCODER_CENTER.wrapping_add_signed(i16::from(step));
    Ok(nrpn::frame(
        channel,
        FIRST_ENCODER_NRPN + u16::from(index),
        value,
    ))
}

/// The LED change carried by a channel message, if it is one. The channel is not checked: the app does not know
/// Remote Pad OUT, and nothing else sends these controllers (see the module notes). An index outside 0-59 is not a
/// LED.
pub fn led_change(status: u8, controller: u8, value: u8) -> Option<LedChange> {
    if status & 0xF0 != 0xB0 || value >= LED_COUNT {
        return None;
    }
    match controller {
        CC_LED_ON => Some(LedChange {
            index: value,
            on: true,
        }),
        CC_LED_OFF => Some(LedChange {
            index: value,
            on: false,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_when_pressed_and_released_returns_cc_102_and_103() {
        assert_eq!(key(0, true, 46), Ok([0xB0, 0x66, 46]));
        assert_eq!(key(2, false, 48), Ok([0xB2, 0x67, 48]));
    }

    #[test]
    fn test_key_when_code_or_channel_out_of_range_returns_error() {
        assert_eq!(key(0, true, 49), Err(RemoteError::BadKey(49)));
        assert_eq!(key(0, false, 255), Err(RemoteError::BadKey(255)));
        assert_eq!(key(16, true, 0), Err(RemoteError::BadChannel(16)));
    }

    #[test]
    fn test_state_request_when_with_and_without_pads_returns_cc_107_bit_0() {
        assert_eq!(state_request(0, false), Ok([0xB0, 0x6B, 0]));
        assert_eq!(state_request(1, true), Ok([0xB1, 0x6B, 1]));
        assert_eq!(state_request(16, true), Err(RemoteError::BadChannel(16)));
    }

    #[test]
    fn test_control_when_volume_returns_the_bytes_of_the_analysis() {
        // §5: `Bn 63 01`, `Bn 62 (5C+k)`, `Bn 06 (v>>7)`, `Bn 26 (v&7F)`; k = 8, v = 200.
        assert_eq!(
            control(0, VOLUME_CONTROL, 200),
            Ok([0xB0, 0x63, 0x01, 0xB0, 0x62, 0x64, 0xB0, 0x06, 0x01, 0xB0, 0x26, 0x48])
        );
    }

    #[test]
    fn test_control_when_every_index_returns_nrpn_220_plus_index() {
        for index in 0..CONTROL_COUNT {
            let bytes = control(0, index, 0).unwrap();
            assert_eq!((bytes[2], bytes[5]), (1, 0x5C + index), "control {index}");
        }
    }

    #[test]
    fn test_control_when_out_of_range_returns_error() {
        assert_eq!(control(0, 10, 0), Err(RemoteError::BadControl(10)));
        assert_eq!(control(0, 8, 256), Err(RemoteError::BadValue(256)));
        assert_eq!(control(16, 8, 0), Err(RemoteError::BadChannel(16)));
        assert!(control(0, 9, 255).is_ok());
    }

    #[test]
    fn test_encoder_when_one_step_each_way_returns_the_bytes_of_the_analysis() {
        // §5: +1 = `Bn 06 40 Bn 26 01`, -1 = `Bn 06 3F Bn 26 7F`; encoder 1 = NRPN 201 = 1 * 128 + 73.
        assert_eq!(
            encoder(0, 1, 1),
            Ok([0xB0, 0x63, 0x01, 0xB0, 0x62, 0x49, 0xB0, 0x06, 0x40, 0xB0, 0x26, 0x01])
        );
        assert_eq!(
            encoder(0, 1, -1),
            Ok([0xB0, 0x63, 0x01, 0xB0, 0x62, 0x49, 0xB0, 0x06, 0x3F, 0xB0, 0x26, 0x7F])
        );
    }

    #[test]
    fn test_encoder_when_every_index_returns_nrpn_200_plus_index() {
        for index in 0..ENCODER_COUNT {
            let bytes = encoder(0, index, 1).unwrap();
            let number = u16::from(bytes[2]) << 7 | u16::from(bytes[5]);
            assert_eq!(number, 200 + u16::from(index), "encoder {index}");
        }
    }

    #[test]
    fn test_encoder_when_out_of_range_returns_error() {
        assert_eq!(encoder(0, 14, 1), Err(RemoteError::BadEncoder(14)));
        assert_eq!(encoder(0, 0, 16), Err(RemoteError::BadStep(16)));
        assert_eq!(encoder(0, 0, -17), Err(RemoteError::BadStep(-17)));
        assert_eq!(encoder(16, 0, 1), Err(RemoteError::BadChannel(16)));
        assert!(encoder(0, 13, -16).is_ok());
        assert!(encoder(0, 13, 15).is_ok());
    }

    #[test]
    fn test_led_change_when_cc_104_or_105_returns_the_change_on_any_channel() {
        let on = LedChange {
            index: 0x2C,
            on: true,
        };
        assert_eq!(led_change(0xB1, 104, 0x2C), Some(on));
        assert_eq!(led_change(0xBF, 104, 0x2C), Some(on));
        assert_eq!(
            led_change(0xB0, 105, 59),
            Some(LedChange {
                index: 59,
                on: false
            })
        );
    }

    #[test]
    fn test_led_change_when_not_104_105_or_not_a_control_change_or_index_too_high_returns_none() {
        assert_eq!(led_change(0xB0, 103, 5), None);
        assert_eq!(led_change(0xB0, 106, 5), None);
        assert_eq!(led_change(0xB0, 107, 5), None);
        assert_eq!(led_change(0x90, 104, 5), None);
        assert_eq!(led_change(0xA0, 105, 5), None);
        assert_eq!(led_change(0xB0, 104, 60), None);
        assert_eq!(led_change(0xB0, 105, 127), None);
    }
}
