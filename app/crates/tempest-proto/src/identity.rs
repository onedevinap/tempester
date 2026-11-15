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

/// Tempester n°64 (1.6.7.0): the answer to `VERSION_REQUEST` is followed by a second message,
/// `F0 01 28 45 h0 m0 l0 .. h5 m5 l5 F7` (23 bytes): for each Voice chip k = 0..5 the 16-bit word the Main read from it
/// at start-up (`0xFE00` reply), as `v >> 14`, `(v >> 7) & 0x7F`, `v & 0x7F`. `45` is no command of the machine.
pub const CMD_VOICE_VERSIONS: u8 = 0x45;
/// Voice chips of the machine.
pub const VOICE_CHIPS: usize = 6;
const VOICE_VERSIONS_LEN: usize = 4 + 3 * VOICE_CHIPS + 1;
/// The word DSI Voice 1.5 answers to `0xFE00`.
pub const VOICE_WORD_DSI_15: u16 = 0x0015;
/// The word Tempester Voice 1.5.1.0 answers, and 1.5.1.1 too.
pub const VOICE_WORD_TEMPESTER_1510: u16 = 0x0016;
/// The word Tempester Voice 1.5.1.2 answers: the first Tempester Voice with a word of its own.
pub const VOICE_WORD_TEMPESTER_1512: u16 = 0x0017;

/// What a Voice chip's version word says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceVersion {
    Dsi15,
    Tempester1510,
    Tempester1512,
    /// Any other word: unknown, shown as the raw word.
    Other(u16),
}

impl VoiceVersion {
    /// The Tempester Voice OS numbers this word may stand for (the numbers of their `versions/voice/` manifests); empty
    /// for DSI's word and for an unknown one. 1.5.1.0 and 1.5.1.1 answer the same `0x16` (Main 1.6.7.0 / 1.6.8.0
    /// tested that exact word for the Release lift and the backup re-alignment, which both versions do the same): the
    /// word alone cannot tell them apart, a full backup's pages 0-40 can. 1.5.1.2 answers `0x17`, a word of its own.
    pub fn tempester_versions(self) -> &'static [&'static str] {
        match self {
            Self::Tempester1510 => &["1.5.1.0", "1.5.1.1"],
            Self::Tempester1512 => &["1.5.1.2"],
            Self::Dsi15 | Self::Other(_) => &[],
        }
    }

    pub fn from_word(word: u16) -> Self {
        match word {
            VOICE_WORD_DSI_15 => Self::Dsi15,
            VOICE_WORD_TEMPESTER_1510 => Self::Tempester1510,
            VOICE_WORD_TEMPESTER_1512 => Self::Tempester1512,
            other => Self::Other(other),
        }
    }
}

/// The Tempester Voice words this app knows, each naming its versions (`VoiceVersion::tempester_versions`).
pub const TEMPESTER_VOICE_WORDS_KNOWN: [u16; 2] =
    [VOICE_WORD_TEMPESTER_1510, VOICE_WORD_TEMPESTER_1512];

/// The `0xFE00` word a Tempester Voice version answers, `None` for a number no known word names.
pub fn voice_word_of(version: &str) -> Option<u16> {
    TEMPESTER_VOICE_WORDS_KNOWN.into_iter().find(|w| {
        VoiceVersion::from_word(*w)
            .tempester_versions()
            .contains(&version)
    })
}

impl fmt::Display for VoiceVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dsi15 => write!(f, "DSI 1.5"),
            Self::Tempester1510 => write!(f, "Tempester 1.5.1.0 or 1.5.1.1"),
            Self::Tempester1512 => write!(f, "Tempester 1.5.1.2"),
            Self::Other(word) => write!(f, "unknown (0x{word:04X})"),
        }
    }
}

/// The message the machine sends after its version reply: the version word of each Voice chip.
pub fn voice_versions_reply(words: &[u16; VOICE_CHIPS]) -> Vec<u8> {
    let mut out = vec![0xF0, 0x01, 0x28, CMD_VOICE_VERSIONS];
    for word in words {
        out.extend([
            (word >> 14) as u8,
            ((word >> 7) & 0x7F) as u8,
            (word & 0x7F) as u8,
        ]);
    }
    out.push(0xF7);
    out
}

/// The six words of a complete `45` message, or `None` for any other message (a wrong length, a byte with bit 7 set,
/// a first byte above 3: the word has 16 bits).
pub fn parse_voice_versions(message: &[u8]) -> Option<[u16; VOICE_CHIPS]> {
    if message.len() != VOICE_VERSIONS_LEN
        || message[..4] != [0xF0, 0x01, 0x28, CMD_VOICE_VERSIONS]
        || message[VOICE_VERSIONS_LEN - 1] != 0xF7
    {
        return None;
    }
    let body = &message[4..VOICE_VERSIONS_LEN - 1];
    if body.iter().any(|b| *b > 0x7F) || body.chunks(3).any(|c| c[0] > 3) {
        return None;
    }
    let mut words = [0u16; VOICE_CHIPS];
    for (word, c) in words.iter_mut().zip(body.chunks(3)) {
        *word = u16::from(c[0]) << 14 | u16::from(c[1]) << 7 | u16::from(c[2]);
    }
    Some(words)
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
    fn test_voice_versions_reply_when_parsed_back_returns_the_same_six_words() {
        for words in [
            [0x16; 6],
            [0x16, 0x16, 0x15, 0x16, 0x16, 0x16],
            [0, 1, 0x7F, 0x80, 0x3FFF, 0xFFFF],
        ] {
            let reply = voice_versions_reply(&words);

            assert_eq!(reply.len(), 23);
            assert_eq!(parse_voice_versions(&reply), Some(words), "{words:?}");
        }
    }

    #[test]
    fn test_parse_voice_versions_when_a_message_by_hand_returns_the_words() {
        // v >> 14, (v >> 7) & 0x7F, v & 0x7F: 0x0016 is 0 0 0x16, 0xC001 is 3 0 1.
        let mut message = vec![0xF0, 0x01, 0x28, 0x45];
        for _ in 0..5 {
            message.extend([0, 0, 0x16]);
        }
        message.extend([3, 0, 1, 0xF7]);

        assert_eq!(
            parse_voice_versions(&message),
            Some([0x16, 0x16, 0x16, 0x16, 0x16, 0xC001])
        );
    }

    #[test]
    fn test_parse_voice_versions_when_the_message_is_not_a_complete_45_returns_none() {
        let good = voice_versions_reply(&[0x16; 6]);
        let mut too_long = good.clone();
        too_long.insert(22, 0);
        let mut other_command = good.clone();
        other_command[3] = 0x43;
        let mut status_byte = good.clone();
        status_byte[8] = 0x80;
        let mut wide_word = good.clone();
        wide_word[4] = 4;
        let mut no_end = good.clone();
        no_end[22] = 0x00;

        assert_eq!(parse_voice_versions(&good[..22]), None, "one byte short");
        assert_eq!(parse_voice_versions(&too_long), None, "one byte too long");
        assert_eq!(parse_voice_versions(&[]), None);
        assert_eq!(parse_voice_versions(&other_command), None);
        assert_eq!(parse_voice_versions(&status_byte), None);
        assert_eq!(parse_voice_versions(&wide_word), None, "a word has 16 bits");
        assert_eq!(parse_voice_versions(&no_end), None);
        assert_eq!(parse_voice_versions(&VERSION_REQUEST), None);
        assert_eq!(parse_version(&good), None, "a 45 is not a 43");
    }

    /// What a `45` parser must refuse, as one judge for any implementation.
    fn judge_45(parse: fn(&[u8]) -> Option<[u16; VOICE_CHIPS]>) -> Result<(), String> {
        let good = voice_versions_reply(&[0x16; 6]);
        let mut too_long = good.clone();
        too_long.insert(22, 0);
        let mut other_command = good.clone();
        other_command[3] = 0x43;
        let mut status_byte = good.clone();
        status_byte[8] = 0x80;
        let mut wide_word = good.clone();
        wide_word[4] = 4;
        let mut no_end = good.clone();
        no_end[22] = 0;
        if parse(&good) != Some([0x16; 6]) {
            return Err("a good message is refused".into());
        }
        for (what, bad) in [
            ("one byte short", good[..22].to_vec()),
            ("one byte long", too_long),
            ("another command", other_command),
            ("a status byte", status_byte),
            ("a 17-bit word", wide_word),
            ("no F7", no_end),
        ] {
            if parse(&bad).is_some() {
                return Err(format!("accepted: {what}"));
            }
        }
        Ok(())
    }

    #[test]
    fn test_judge_45_when_given_the_real_parser_returns_ok_and_a_lenient_one_returns_the_defect() {
        assert_eq!(judge_45(parse_voice_versions), Ok(()));
        // Variants that each skip one check.
        let any_length: fn(&[u8]) -> Option<[u16; VOICE_CHIPS]> = |m| {
            let body = m.get(4..22)?;
            let mut words = [0u16; VOICE_CHIPS];
            for (word, c) in words.iter_mut().zip(body.chunks(3)) {
                *word = u16::from(c[0]) << 14 | u16::from(c[1]) << 7 | u16::from(c[2]);
            }
            (m.starts_with(&[0xF0, 0x01, 0x28, 0x45])).then_some(words)
        };
        let no_command_check: fn(&[u8]) -> Option<[u16; VOICE_CHIPS]> = |m| {
            let mut m = m.to_vec();
            if m.len() > 3 {
                m[3] = CMD_VOICE_VERSIONS;
            }
            parse_voice_versions(&m)
        };
        let no_status_check: fn(&[u8]) -> Option<[u16; VOICE_CHIPS]> = |m| {
            let mut m = m.to_vec();
            for b in m.iter_mut().skip(4).take(18) {
                *b &= 0x7F;
            }
            parse_voice_versions(&m)
        };
        assert!(judge_45(any_length).is_err(), "any length accepted");
        assert!(judge_45(no_command_check).is_err(), "any command accepted");
        assert!(judge_45(no_status_check).is_err(), "a status byte accepted");
    }

    #[test]
    fn test_voice_version_when_from_a_word_returns_dsi_tempester_or_the_raw_word() {
        assert_eq!(VoiceVersion::from_word(0x15).to_string(), "DSI 1.5");
        assert_eq!(
            VoiceVersion::from_word(0x16).to_string(),
            "Tempester 1.5.1.0 or 1.5.1.1"
        );
        assert_eq!(
            VoiceVersion::from_word(0x17).to_string(),
            "Tempester 1.5.1.2"
        );
        assert_eq!(
            VoiceVersion::from_word(0x18).to_string(),
            "unknown (0x0018)"
        );
        assert_eq!(VoiceVersion::from_word(0), VoiceVersion::Other(0));
    }

    #[test]
    fn test_tempester_versions_when_0x17_returns_exactly_1_5_1_2_and_0x16_keeps_its_two() {
        assert_eq!(
            VoiceVersion::from_word(VOICE_WORD_TEMPESTER_1512).tempester_versions(),
            ["1.5.1.2"]
        );
        assert_eq!(
            VoiceVersion::from_word(VOICE_WORD_TEMPESTER_1510).tempester_versions(),
            ["1.5.1.0", "1.5.1.1"]
        );
        for other in [VOICE_WORD_DSI_15, 0, 0x18, 0x79, 0x0117, 0xFFFF] {
            assert!(
                VoiceVersion::from_word(other)
                    .tempester_versions()
                    .is_empty(),
                "{other:#x}"
            );
        }
    }

    #[test]
    fn test_tempester_versions_when_read_against_the_voice_table_names_each_listed_version_once() {
        // Every Tempester Voice version of the table is named by exactly one word and no word names a version the
        // table does not hold: a new Voice OS without its word here, or a word left behind, turns this red.
        let mut named: Vec<&str> = TEMPESTER_VOICE_WORDS_KNOWN
            .iter()
            .flat_map(|w| {
                VoiceVersion::from_word(*w)
                    .tempester_versions()
                    .iter()
                    .copied()
            })
            .collect();
        named.sort_unstable();
        let mut table: Vec<&str> = crate::voice_versions::voice_versions()
            .iter()
            .map(|v| v.version)
            .collect();
        table.sort_unstable();
        assert_eq!(named, table);
    }

    #[test]
    fn test_voice_word_of_when_a_version_number_returns_its_word_or_none() {
        assert_eq!(voice_word_of("1.5.1.0"), Some(0x16));
        assert_eq!(voice_word_of("1.5.1.1"), Some(0x16));
        assert_eq!(voice_word_of("1.5.1.2"), Some(0x17));
        assert_eq!(voice_word_of("9.9.9.9"), None);
    }

    #[test]
    fn test_parse_when_version_byte_has_bit_7_returns_error() {
        let mut reply = REPLY;
        reply[12] = 0x80;

        assert_eq!(parse(&reply), Err(NotIdentity));
    }
}
