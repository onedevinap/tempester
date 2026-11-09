//! Hexadecimal text for binary data crossing the page boundary (`.syx` files, sounds).

use crate::error::AppError;

/// Decodes `text` (pairs of hex digits, no separators). An odd length or a non-hex character is refused.
pub fn decode(text: &str) -> Result<Vec<u8>, AppError> {
    let digits = text.as_bytes();
    if !digits.len().is_multiple_of(2) {
        return Err(AppError::Invalid("hex data has an odd length".into()));
    }
    digits
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&[high, low]| Ok(nibble(high)? << 4 | nibble(low)?))
        .collect()
}

fn nibble(digit: u8) -> Result<u8, AppError> {
    match digit {
        b'0'..=b'9' => Ok(digit - b'0'),
        b'a'..=b'f' => Ok(digit - b'a' + 10),
        b'A'..=b'F' => Ok(digit - b'A' + 10),
        _ => Err(AppError::Invalid(
            "hex data holds a non-hex character".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempest_proto::sha256::to_hex as encode;

    #[test]
    fn test_decode_when_valid_returns_bytes_for_both_cases() {
        assert_eq!(decode("00ff7Fa0").unwrap(), vec![0x00, 0xFF, 0x7F, 0xA0]);
    }

    #[test]
    fn test_decode_when_empty_returns_empty() {
        assert_eq!(decode("").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn test_decode_when_odd_length_returns_error() {
        assert!(decode("abc").is_err());
    }

    #[test]
    fn test_decode_when_non_hex_character_returns_error() {
        assert!(decode("0g").is_err());
        assert!(decode("é0").is_err());
    }

    #[test]
    fn test_encode_then_decode_when_all_byte_values_returns_same_bytes() {
        let bytes: Vec<u8> = (0..=255).collect();

        assert_eq!(decode(&encode(&bytes)).unwrap(), bytes);
    }
}
