//! NRPN of Tempester Main 1.5.3.0 and later (`patch/nrpn_reception.json`, `patch/nrpn_emission.json`).
//!
//! A change is `CC 99 = number >> 7`, `CC 98 = number & 0x7F`, `CC 6 = value >> 7`, `CC 38 = value & 0x7F`; it is
//! applied on CC 38. Numbers: 0-126 are the sound parameters of the same index, 156 and 157 the Osc 3 and 4 sample.
//! Every other number is refused by the machine, and so by `encode`. The original DSI OS ignores NRPN.

use std::fmt;

pub const CC_DATA_MSB: u8 = 6;
pub const CC_DATA_LSB: u8 = 38;
pub const CC_NRPN_LSB: u8 = 98;
pub const CC_NRPN_MSB: u8 = 99;
const CC_RPN_LSB: u8 = 100;
const CC_RPN_MSB: u8 = 101;
const NULL_NUMBER: u16 = 127 << 7 | 127;
/// Largest 14-bit value.
pub const MAX_VALUE: u16 = 0x3FFF;
/// Osc 3 sample and Osc 4 sample.
pub const SAMPLE_PARAMS: [u16; 2] = [156, 157];

/// Why an NRPN could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NrpnError {
    /// MIDI channel above 15 (channels are 0-15 here, shown as 1-16).
    BadChannel(u8),
    /// A number that is neither 0-126 nor 156-157.
    BadNumber(u16),
    /// A value above 14 bits.
    BadValue(u16),
}

impl fmt::Display for NrpnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadChannel(c) => write!(f, "MIDI channel {c} is above 15"),
            Self::BadNumber(n) => write!(f, "NRPN {n} is not a Tempest parameter (0-126, 156-157)"),
            Self::BadValue(v) => write!(f, "NRPN value {v} does not fit in 14 bits"),
        }
    }
}

impl std::error::Error for NrpnError {}

/// A complete NRPN as received or to send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Nrpn {
    /// Zero-based MIDI channel (0-15).
    pub channel: u8,
    pub number: u16,
    pub value: u16,
}

/// True for the numbers the Tempester Main accepts.
pub fn is_tempest_number(number: u16) -> bool {
    number <= 126 || SAMPLE_PARAMS.contains(&number)
}

/// The 12 bytes (four full-status Control Change messages) setting `number` to `value` on `channel`.
pub fn encode(channel: u8, number: u16, value: u16) -> Result<[u8; 12], NrpnError> {
    if channel > 15 {
        return Err(NrpnError::BadChannel(channel));
    }
    if !is_tempest_number(number) {
        return Err(NrpnError::BadNumber(number));
    }
    if value > MAX_VALUE {
        return Err(NrpnError::BadValue(value));
    }
    Ok(frame(channel, number, value))
}

/// The four Control Changes of an NRPN, whatever its number: the remote-panel controls (`remote`) use numbers that
/// `encode` refuses. The caller checks `channel` (0-15), `number` and `value` (14 bits).
pub(crate) fn frame(channel: u8, number: u16, value: u16) -> [u8; 12] {
    let status = 0xB0 | (channel & 0x0F);
    [
        status,
        CC_NRPN_MSB,
        (number >> 7) as u8,
        status,
        CC_NRPN_LSB,
        (number & 0x7F) as u8,
        status,
        CC_DATA_MSB,
        (value >> 7) as u8,
        status,
        CC_DATA_LSB,
        (value & 0x7F) as u8,
    ]
}

#[derive(Debug, Clone, Copy, Default)]
struct ChannelState {
    number_msb: Option<u8>,
    number_lsb: Option<u8>,
    data_msb: u8,
}

/// Rebuilds NRPN from a stream of Control Changes, one state per channel (like the machine: select with CC 99/98,
/// CC 100/101 deselect, 127/127 means none, CC 6 stores the data MSB, CC 38 completes the value and applies).
#[derive(Debug, Clone, Default)]
pub struct NrpnDecoder {
    channels: [ChannelState; 16],
}

impl NrpnDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one Control Change; returns the NRPN completed by it, if any.
    /// A completed NRPN is returned even if its number is not a Tempest one: filtering is the caller's choice
    /// (`is_tempest_number`). `channel` is masked to 0-15 and `value` to 7 bits.
    pub fn feed_cc(&mut self, channel: u8, controller: u8, value: u8) -> Option<Nrpn> {
        let state = &mut self.channels[usize::from(channel & 0x0F)];
        let value = value & 0x7F;
        match controller {
            CC_NRPN_MSB => state.number_msb = Some(value),
            CC_NRPN_LSB => state.number_lsb = Some(value),
            CC_RPN_LSB | CC_RPN_MSB => *state = ChannelState::default(),
            CC_DATA_MSB => state.data_msb = value,
            CC_DATA_LSB => return complete(*state, channel & 0x0F, value),
            _ => {}
        }
        None
    }

    /// Feeds one complete channel message (status, data1, data2); only Control Changes count.
    pub fn feed_message(&mut self, status: u8, data1: u8, data2: u8) -> Option<Nrpn> {
        if status & 0xF0 == 0xB0 {
            self.feed_cc(status & 0x0F, data1, data2)
        } else {
            None
        }
    }
}

fn complete(state: ChannelState, channel: u8, data_lsb: u8) -> Option<Nrpn> {
    let number = u16::from(state.number_msb?) << 7 | u16::from(state.number_lsb?);
    if number == NULL_NUMBER {
        return None;
    }
    Some(Nrpn {
        channel,
        number,
        value: u16::from(state.data_msb) << 7 | u16::from(data_lsb),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_all(decoder: &mut NrpnDecoder, bytes: &[u8]) -> Vec<Nrpn> {
        bytes
            .chunks(3)
            .filter_map(|m| decoder.feed_message(m[0], m[1], m[2]))
            .collect()
    }

    #[test]
    fn test_encode_when_lp_freq_100_returns_vector_of_tests_nrpn_usb() {
        // tests/nrpn_usb.txt T1: "B0 63 00 B0 62 22 B0 06 00 B0 26 64"
        let bytes = encode(0, 34, 100).unwrap();

        assert_eq!(
            bytes,
            [0xB0, 0x63, 0x00, 0xB0, 0x62, 0x22, 0xB0, 0x06, 0x00, 0xB0, 0x26, 0x64]
        );
    }

    #[test]
    fn test_encode_when_value_300_returns_split_msb_lsb() {
        // tests/nrpn_usb.txt T2: value 300 = "B0 06 02 B0 26 2C"
        let bytes = encode(0, 34, 300).unwrap();

        assert_eq!(&bytes[6..], &[0xB0, 0x06, 0x02, 0xB0, 0x26, 0x2C]);
    }

    #[test]
    fn test_encode_when_number_157_returns_msb_1_lsb_29_on_channel_5() {
        let bytes = encode(4, 157, 463).unwrap();

        assert_eq!(&bytes[..6], &[0xB4, 0x63, 0x01, 0xB4, 0x62, 0x1D]);
    }

    #[test]
    fn test_encode_when_number_not_a_tempest_parameter_returns_bad_number() {
        for number in [127, 155, 158, 397] {
            assert_eq!(encode(0, number, 0), Err(NrpnError::BadNumber(number)));
        }
        assert!(encode(0, 126, 0).is_ok() && encode(0, 156, 0).is_ok());
    }

    #[test]
    fn test_encode_when_channel_or_value_out_of_range_returns_error() {
        assert_eq!(encode(16, 0, 0), Err(NrpnError::BadChannel(16)));
        assert_eq!(encode(0, 0, 0x4000), Err(NrpnError::BadValue(0x4000)));
        assert!(encode(15, 0, 0x3FFF).is_ok());
    }

    #[test]
    fn test_decoder_when_encoded_stream_returns_same_nrpn() {
        let mut decoder = NrpnDecoder::new();

        let found = feed_all(&mut decoder, &encode(2, 126, 12345).unwrap());

        assert_eq!(
            found,
            [Nrpn {
                channel: 2,
                number: 126,
                value: 12345
            }]
        );
    }

    #[test]
    fn test_decoder_when_two_nrpn_in_a_row_returns_both() {
        let mut stream = encode(0, 34, 100).unwrap().to_vec();
        stream.extend(encode(0, 35, 7).unwrap());

        let found = feed_all(&mut NrpnDecoder::new(), &stream);

        assert_eq!(found.len(), 2);
        assert_eq!((found[1].number, found[1].value), (35, 7));
    }

    #[test]
    fn test_decoder_when_value_without_number_returns_nothing() {
        let mut decoder = NrpnDecoder::new();

        assert_eq!(decoder.feed_cc(0, CC_DATA_MSB, 1), None);
        assert_eq!(decoder.feed_cc(0, CC_DATA_LSB, 1), None);
    }

    #[test]
    fn test_decoder_when_number_is_null_returns_nothing() {
        let mut decoder = NrpnDecoder::new();
        decoder.feed_cc(0, CC_NRPN_MSB, 127);
        decoder.feed_cc(0, CC_NRPN_LSB, 127);

        assert_eq!(decoder.feed_cc(0, CC_DATA_LSB, 5), None);
    }

    #[test]
    fn test_decoder_when_rpn_deselects_returns_nothing_on_data() {
        let mut decoder = NrpnDecoder::new();
        decoder.feed_cc(0, CC_NRPN_MSB, 0);
        decoder.feed_cc(0, CC_NRPN_LSB, 34);
        decoder.feed_cc(0, CC_RPN_MSB, 0);

        assert_eq!(decoder.feed_cc(0, CC_DATA_LSB, 5), None);
    }

    #[test]
    fn test_decoder_when_channels_interleave_returns_each_on_its_own_channel() {
        let mut decoder = NrpnDecoder::new();
        decoder.feed_cc(0, CC_NRPN_LSB, 1);
        decoder.feed_cc(0, CC_NRPN_MSB, 0);
        decoder.feed_cc(1, CC_NRPN_LSB, 2);
        decoder.feed_cc(1, CC_NRPN_MSB, 0);

        let a = decoder.feed_cc(0, CC_DATA_LSB, 10);
        let b = decoder.feed_cc(1, CC_DATA_LSB, 20);

        assert_eq!(a.map(|n| (n.channel, n.number, n.value)), Some((0, 1, 10)));
        assert_eq!(b.map(|n| (n.channel, n.number, n.value)), Some((1, 2, 20)));
    }

    #[test]
    fn test_decoder_when_message_is_not_a_cc_returns_nothing() {
        let mut decoder = NrpnDecoder::new();

        assert_eq!(decoder.feed_message(0x90, CC_DATA_LSB, 5), None);
    }
}
