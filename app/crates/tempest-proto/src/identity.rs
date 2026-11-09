//! Identity reply `F0 7E <device> 06 02 01 28 01 00 00 15 <v & 0xF> <(v & 0xFFF) >> 4> F7` (`refs/firmware-main.md`
//! 6.5): `v` is the Voice OS version word; Tempester Main versions announce 0x7F1 / 0x7F2 (patch n°1).

use std::fmt;

/// Universal identity request to every device: `F0 7E 7F 06 01 F7`.
pub const IDENTITY_REQUEST: [u8; 6] = [0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7];
const REPLY_LEN: usize = 14;
const REPLY_PREFIX: [u8; 3] = [0x7E, 0x06, 0x02];
const REPLY_IDS: [u8; 6] = [0x01, 0x28, 0x01, 0x00, 0x00, 0x15];

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
    fn test_parse_when_version_byte_has_bit_7_returns_error() {
        let mut reply = REPLY;
        reply[12] = 0x80;

        assert_eq!(parse(&reply), Err(NotIdentity));
    }
}
