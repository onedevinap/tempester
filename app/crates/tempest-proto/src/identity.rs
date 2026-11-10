//! Identity reply `F0 7E <device> 06 02 01 28 01 00 00 15 <v & 0xF> <(v & 0xFFF) >> 4> F7` (`refs/firmware-main.md`
//! 6.5): `v` is the Voice OS version word; Tempester Main versions announce 0x7F1 / 0x7F2 (patch n°1).
//!
//! The exact Tempester version (n°39): request `F0 01 28 43 F7`, answer `F0 01 28 43 a1 a0 b1 b0 c1 c0 d1 d0 F7`, each
//! digit `N` as `N >> 7` then `N & 0x7F`, the number the machine's System screen shows. The identity reply stays as it
//! is, so every existing reader keeps working; a machine without n°39 does not answer the request.

use std::fmt;

/// Universal identity request to every device: `F0 7E 7F 06 01 F7`.
pub const IDENTITY_REQUEST: [u8; 6] = [0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7];
const REPLY_LEN: usize = 14;
const REPLY_PREFIX: [u8; 3] = [0x7E, 0x06, 0x02];
const REPLY_IDS: [u8; 6] = [0x01, 0x28, 0x01, 0x00, 0x00, 0x15];

/// Tempester n°39: asks for the exact Tempester version.
pub const VERSION_REQUEST: [u8; 5] = [0xF0, 0x01, 0x28, CMD_VERSION, 0xF7];
const CMD_VERSION: u8 = 0x43;
const VERSION_REPLY_LEN: usize = 13;
/// A digit is two 7-bit bytes.
pub const VERSION_DIGIT_MAX: u16 = 0x3FFF;

/// The exact version a Tempester Main announces (n°39): `a.b.c.d`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TempesterVersion(pub [u16; 4]);

impl fmt::Display for TempesterVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c, d] = self.0;
        write!(f, "{a}.{b}.{c}.{d}")
    }
}

impl TempesterVersion {
    /// The answer the machine sends for this version; `None` when a digit is above `VERSION_DIGIT_MAX`.
    pub fn reply(&self) -> Option<Vec<u8>> {
        if self.0.iter().any(|d| *d > VERSION_DIGIT_MAX) {
            return None;
        }
        let mut out = VERSION_REQUEST[..4].to_vec();
        for digit in self.0 {
            out.extend([(digit >> 7) as u8, (digit & 0x7F) as u8]);
        }
        out.push(0xF7);
        Some(out)
    }
}

/// The version carried by a complete answer to `VERSION_REQUEST`, or `None` for any other message.
pub fn parse_version(message: &[u8]) -> Option<TempesterVersion> {
    if message.len() != VERSION_REPLY_LEN
        || message[..4] != VERSION_REQUEST[..4]
        || message[VERSION_REPLY_LEN - 1] != 0xF7
    {
        return None;
    }
    let body = &message[4..VERSION_REPLY_LEN - 1];
    if body.iter().any(|b| *b > 0x7F) {
        return None;
    }
    let digit = |k: usize| u16::from(body[2 * k]) << 7 | u16::from(body[2 * k + 1]);
    Some(TempesterVersion([digit(0), digit(1), digit(2), digit(3)]))
}

/// Why a message is not a Tempest identity reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotIdentity;

impl fmt::Display for NotIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not a DSI Tempest identity reply")
    }
}

impl std::error::Error for NotIdentity {}

/// A parsed identity reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity {
    /// Device id the machine answered with (its MIDI SysEx id, or 0x7F).
    pub device: u8,
    /// Version word `v`.
    pub version: u16,
}

/// Parses a complete reply.
pub fn parse(message: &[u8]) -> Result<Identity, NotIdentity> {
    let valid = message.len() == REPLY_LEN
        && message[0] == 0xF0
        && message[1..2] == REPLY_PREFIX[..1]
        && message[3..5] == REPLY_PREFIX[1..]
        && message[5..11] == REPLY_IDS
        && message[REPLY_LEN - 1] == 0xF7
        && message[11] < 0x80
        && message[12] < 0x80;
    if !valid {
        return Err(NotIdentity);
    }
    Ok(Identity {
        device: message[2],
        version: u16::from(message[12]) << 4 | u16::from(message[11] & 0x0F),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPLY: [u8; 14] = [
        0xF0, 0x7E, 0x7F, 0x06, 0x02, 0x01, 0x28, 0x01, 0x00, 0x00, 0x15, 0x02, 0x7F, 0xF7,
    ];

    #[test]
    fn test_parse_when_tempester_reply_returns_version_0x7f2() {
        assert_eq!(
            parse(&REPLY),
            Ok(Identity {
                device: 0x7F,
                version: 0x7F2
            })
        );
    }

    #[test]
    fn test_parse_when_device_id_set_returns_it() {
        let mut reply = REPLY;
        reply[2] = 0x04;

        assert_eq!(parse(&reply).map(|i| i.device), Ok(4));
    }

    #[test]
    fn test_parse_when_other_manufacturer_returns_error() {
        let mut reply = REPLY;
        reply[5] = 0x00;

        assert_eq!(parse(&reply), Err(NotIdentity));
    }

    #[test]
    fn test_parse_when_wrong_length_or_command_returns_error() {
        assert_eq!(parse(&REPLY[..13]), Err(NotIdentity));
        assert_eq!(parse(&IDENTITY_REQUEST), Err(NotIdentity));
        let mut reply = REPLY;
        reply[4] = 0x01;
        assert_eq!(parse(&reply), Err(NotIdentity));
    }

    #[test]
    fn test_parse_version_when_reply_of_1_6_3_0_returns_it() {
        // The bytes the simulated 1.6.4.0-c test build sends (tools/sim_version_request.py).
        let reply = [
            0xF0, 0x01, 0x28, 0x43, 0x00, 0x01, 0x00, 0x06, 0x00, 0x03, 0x00, 0x00, 0xF7,
        ];

        let version = parse_version(&reply).expect("a version reply");

        assert_eq!(version, TempesterVersion([1, 6, 3, 0]));
        assert_eq!(version.to_string(), "1.6.3.0");
    }

    #[test]
    fn test_parse_version_when_digits_use_both_bytes_returns_14_bit_digits() {
        let reply = [
            0xF0, 0x01, 0x28, 0x43, 0x7F, 0x7F, 0x01, 0x7F, 0x00, 0x01, 0x01, 0x00, 0xF7,
        ];

        assert_eq!(
            parse_version(&reply),
            Some(TempesterVersion([16383, 255, 1, 128]))
        );
    }

    #[test]
    fn test_reply_when_parsed_back_returns_the_same_version_and_refuses_wide_digits() {
        for digits in [
            [0, 0, 0, 0],
            [1, 6, 4, 0],
            [2, 10, 127, 128],
            [16383, 0, 7, 1],
        ] {
            let version = TempesterVersion(digits);
            let reply = version.reply().expect("digits in range");
            assert_eq!(parse_version(&reply), Some(version), "{digits:?}");
        }
        assert_eq!(TempesterVersion([1, 16384, 0, 0]).reply(), None);
    }

    #[test]
    fn test_parse_version_when_other_message_returns_none() {
        let good = TempesterVersion([1, 6, 3, 0]).reply().expect("reply");
        assert_eq!(parse_version(&VERSION_REQUEST), None);
        assert_eq!(parse_version(&REPLY), None);
        assert_eq!(parse_version(&good[..12]), None);
        let mut other_command = good.clone();
        other_command[3] = 0x42;
        assert_eq!(parse_version(&other_command), None);
        let mut status_byte = good.clone();
        status_byte[5] = 0x81;
        assert_eq!(parse_version(&status_byte), None);
        let mut no_end = good;
        no_end[12] = 0x00;
        assert_eq!(parse_version(&no_end), None);
    }

    #[test]
    fn test_parse_when_version_byte_has_bit_7_returns_error() {
        let mut reply = REPLY;
        reply[12] = 0x80;

        assert_eq!(parse(&reply), Err(NotIdentity));
    }
}
