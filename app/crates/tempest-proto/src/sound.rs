//! A Tempest sound: 128 bytes holding 127 parameters bit-packed little-endian and a 20-character name.
//! The table comes from `profile/dsi-tempest/sound_params.json` (generated into `generated/params.rs`);
//! reference: `sound_param_values` in `tools/tempest_sysex.py`.

use std::fmt;

use crate::generated::params::{NAME_BYTE_OFFSET, NAME_CHARS, NAME_CHAR_BITS, PARAMS};

/// Bytes of a sound.
pub const SOUND_BYTES: usize = 128;
/// Characters of a sound's internal name.
pub const NAME_LEN: usize = NAME_CHARS;
/// Number of parameters.
pub const PARAM_COUNT: usize = PARAMS.len();

/// One parameter of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParamDef {
    pub index: u8,
    pub name: &'static str,
    pub short: &'static str,
    pub group: &'static str,
    /// Width of the field in bits.
    pub bits: u8,
    /// Position of the field's lowest bit in the little-endian sound.
    pub bit: u16,
    pub max: u16,
    pub default: u16,
}

/// All parameters, in index order.
pub fn params() -> &'static [ParamDef] {
    &PARAMS
}

/// Why a sound edit was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SoundError {
    UnknownParam(usize),
    ValueAboveMax {
        index: usize,
        value: u16,
        max: u16,
    },
    /// A name longer than 20 characters or with a character outside 0x20-0x7E.
    BadName,
    /// A message that is not a Tempest sound message of the expected kind.
    BadMessage,
    /// A message whose payload is too short for a sound.
    TooShort,
}

impl fmt::Display for SoundError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownParam(i) => write!(f, "no sound parameter {i}"),
            Self::ValueAboveMax { index, value, max } => {
                write!(f, "parameter {index}: {value} is above its maximum {max}")
            }
            Self::BadName => write!(
                f,
                "sound name must be at most 20 printable ASCII characters"
            ),
            Self::BadMessage => write!(f, "not a Tempest sound message"),
            Self::TooShort => write!(f, "sound message too short"),
        }
    }
}

impl std::error::Error for SoundError {}

/// The 128 bytes of a sound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sound {
    bytes: [u8; SOUND_BYTES],
}

impl Sound {
    pub fn from_bytes(bytes: [u8; SOUND_BYTES]) -> Self {
        Self { bytes }
    }

    /// A sound with every parameter at its default and an empty name.
    pub fn with_defaults() -> Self {
        let mut sound = Self::from_bytes([0; SOUND_BYTES]);
        for p in PARAMS.iter() {
            write_bits(&mut sound.bytes, p.bit, p.bits, p.default);
        }
        sound
    }

    pub fn bytes(&self) -> &[u8; SOUND_BYTES] {
        &self.bytes
    }

    /// Raw value of parameter `index` (not clamped: a stored value may exceed the table maximum).
    pub fn get(&self, index: usize) -> Result<u16, SoundError> {
        let p = PARAMS.get(index).ok_or(SoundError::UnknownParam(index))?;
        Ok(read_bits(&self.bytes, p.bit, p.bits))
    }

    /// Sets parameter `index`; refuses a value above the table maximum.
    pub fn set(&mut self, index: usize, value: u16) -> Result<(), SoundError> {
        let p = PARAMS.get(index).ok_or(SoundError::UnknownParam(index))?;
        if value > p.max {
            return Err(SoundError::ValueAboveMax {
                index,
                value,
                max: p.max,
            });
        }
        write_bits(&mut self.bytes, p.bit, p.bits, value);
        Ok(())
    }

    /// The 127 raw values in index order.
    pub fn values(&self) -> [u16; PARAM_COUNT] {
        std::array::from_fn(|i| read_bits(&self.bytes, PARAMS[i].bit, PARAMS[i].bits))
    }

    /// The internal name without its trailing spaces and NULs; a non-ASCII byte shows as `?`.
    pub fn name(&self) -> String {
        let text: String = (0..NAME_CHARS)
            .map(|k| {
                let byte = read_bits(
                    &self.bytes,
                    (NAME_BYTE_OFFSET * 8 + NAME_CHAR_BITS * k) as u16,
                    NAME_CHAR_BITS as u8,
                ) as u8;
                if byte.is_ascii() {
                    char::from(byte)
                } else {
                    '?'
                }
            })
            .collect();
        text.trim_end_matches([' ', '\0']).to_string()
    }

    /// Sets the internal name, padded with spaces to 20 characters.
    pub fn set_name(&mut self, name: &str) -> Result<(), SoundError> {
        let printable = name.bytes().all(|b| (0x20..0x7F).contains(&b));
        if name.len() > NAME_CHARS || !printable {
            return Err(SoundError::BadName);
        }
        let padded = name.bytes().chain(std::iter::repeat(b' '));
        for (k, byte) in padded.take(NAME_CHARS).enumerate() {
            let bit = (NAME_BYTE_OFFSET * 8 + NAME_CHAR_BITS * k) as u16;
            write_bits(&mut self.bytes, bit, NAME_CHAR_BITS as u8, u16::from(byte));
        }
        Ok(())
    }
}

fn read_bits(bytes: &[u8; SOUND_BYTES], bit: u16, bits: u8) -> u16 {
    (0..u16::from(bits)).fold(0, |value, k| {
        let at = usize::from(bit + k);
        value | u16::from((bytes[at / 8] >> (at % 8)) & 1) << k
    })
}

fn write_bits(bytes: &mut [u8; SOUND_BYTES], bit: u16, bits: u8, value: u16) {
    for k in 0..u16::from(bits) {
        let at = usize::from(bit + k);
        let mask = 1u8 << (at % 8);
        if (value >> k) & 1 == 1 {
            bytes[at / 8] |= mask;
        } else {
            bytes[at / 8] &= !mask;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_params_when_listed_returns_127_in_index_order() {
        assert_eq!(params().len(), 127);
        assert!(params()
            .iter()
            .enumerate()
            .all(|(i, p)| usize::from(p.index) == i));
    }

    #[test]
    fn test_get_when_bytes_known_returns_little_endian_bit_fields() {
        let mut bytes = [0u8; SOUND_BYTES];
        bytes[0] = 0x85; // Frequency: bits 0-6 = 5; bit 7 is the low bit of Fine Freq
        bytes[1] = 0x01; // Fine Freq bits 7-13: bit 7 set (from byte 0) and bit 8 set -> 0b11 = 3
        let sound = Sound::from_bytes(bytes);

        assert_eq!(sound.get(0), Ok(5));
        assert_eq!(sound.get(1), Ok(3));
    }

    #[test]
    fn test_values_when_pattern_bytes_returns_what_the_python_decoder_returns() {
        // Expected values produced by tools/tempest_sysex.py `sound_param_values` on the same bytes.
        let bytes: [u8; SOUND_BYTES] = std::array::from_fn(|i| (i * 73 + 29) as u8);
        let sound = Sound::from_bytes(bytes);
        let values = sound.values();

        let expected = [(0, 29), (1, 76), (2, 61), (4, 1), (6, 7), (30, 8), (31, 35)];
        let expected_end = [(34, 198), (100, 22), (125, 0), (126, 1)];
        for (index, value) in expected.into_iter().chain(expected_end) {
            assert_eq!(values[index], value, "parameter {index}");
        }
        assert_eq!(sound.name(), "{\x0870u\x13z\x18z\x063(es9\x18y\x04/");
    }

    #[test]
    fn test_values_when_one_bit_of_the_pattern_flips_returns_different_values() {
        let bytes: [u8; SOUND_BYTES] = std::array::from_fn(|i| (i * 73 + 29) as u8);
        let mut flipped = bytes;
        flipped[0] ^= 1;

        assert_ne!(
            Sound::from_bytes(bytes).values(),
            Sound::from_bytes(flipped).values()
        );
    }

    #[test]
    fn test_set_when_value_written_returns_same_value_and_neighbours_untouched() {
        let mut sound = Sound::with_defaults();
        let before = sound.values();

        sound.set(34, 100).unwrap();

        let after = sound.values();
        assert_eq!(after[34], 100);
        for i in (0..PARAM_COUNT).filter(|i| *i != 34) {
            assert_eq!(after[i], before[i], "parameter {i}");
        }
    }

    #[test]
    fn test_set_when_value_above_max_returns_error_and_keeps_sound() {
        let mut sound = Sound::with_defaults();
        let max = params()[0].max;

        let result = sound.set(0, max + 1);

        assert_eq!(
            result,
            Err(SoundError::ValueAboveMax {
                index: 0,
                value: max + 1,
                max
            })
        );
        assert_eq!(sound, Sound::with_defaults());
    }

    #[test]
    fn test_get_when_index_out_of_table_returns_unknown_param() {
        assert_eq!(
            Sound::with_defaults().get(127),
            Err(SoundError::UnknownParam(127))
        );
    }

    #[test]
    fn test_values_when_defaults_returns_table_defaults() {
        let values = Sound::with_defaults().values();

        assert!(params()
            .iter()
            .all(|p| values[usize::from(p.index)] == p.default));
    }

    #[test]
    fn test_set_when_each_parameter_at_max_returns_max_for_all() {
        let mut sound = Sound::with_defaults();
        for p in params() {
            sound.set(usize::from(p.index), p.max).unwrap();
        }

        let values = sound.values();

        assert!(params()
            .iter()
            .all(|p| values[usize::from(p.index)] == p.max));
    }

    #[test]
    fn test_name_when_set_returns_trimmed_name_and_params_untouched() {
        let mut sound = Sound::with_defaults();
        let before = sound.values();

        sound.set_name("Electro Zap").unwrap();

        assert_eq!(sound.name(), "Electro Zap");
        assert_eq!(sound.values(), before);
    }

    #[test]
    fn test_set_name_when_too_long_or_not_ascii_returns_bad_name() {
        let mut sound = Sound::with_defaults();

        assert_eq!(sound.set_name(&"x".repeat(21)), Err(SoundError::BadName));
        assert_eq!(sound.set_name("é"), Err(SoundError::BadName));
        assert_eq!(sound.set_name("a\nb"), Err(SoundError::BadName));
        assert_eq!(sound.set_name(&"x".repeat(20)), Ok(()));
    }
}
