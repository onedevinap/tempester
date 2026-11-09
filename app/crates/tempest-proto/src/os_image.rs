//! DSI OS SysEx files: `F0 01 28 <id>`, then N blocks of 1,024 bytes EACH packed separately in 7 bits (1,171 bytes),
//! then the 14-bit checksum (low 7 bits, high 7 bits) and `F7`. Checksum = sum of the 16-bit little-endian words of
//! the image, modulo 2^14, as the Main firmware computes it. References: `tools/dsi_pack.py`, `tools/dsi_unpack.py`.
//!
//! Pitfall: unpacking the whole body as one stream drifts by 3 bytes per block (1,171 is not a multiple of 8).

use std::fmt;

use crate::pack7::{pack, unpack};
use crate::DSI_PREFIX;

/// Bytes of a decoded block.
pub const BLOCK_SIZE: usize = 1024;
/// Bytes of a packed block (146 full groups of 8 plus a head byte and 2 data bytes).
pub const PACKED_BLOCK_SIZE: usize = 1171;
const EOX: u8 = 0xF7;
const FRAME_OVERHEAD: usize = 4 + 2 + 1;

/// Which chip an OS file is for; the value is the SysEx command byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsId {
    Main = 0x71,
    Voice = 0x72,
    Panel = 0x73,
    Sam = 0x74,
}

impl OsId {
    pub fn from_byte(byte: u8) -> Option<Self> {
        [Self::Main, Self::Voice, Self::Panel, Self::Sam]
            .into_iter()
            .find(|id| *id as u8 == byte)
    }

    /// Number of 1,024-byte blocks of the chip's image (Main 512 KB, Voice and Panel 64 KB, SAM 32 KB).
    pub fn blocks(self) -> usize {
        match self {
            Self::Main => 512,
            Self::Voice | Self::Panel => 64,
            Self::Sam => 32,
        }
    }
}

/// Why an OS file or image was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OsImageError {
    /// Shorter than the smallest possible frame.
    TooShort(usize),
    /// Does not start with `F0 01 28`.
    BadHeader,
    UnknownId(u8),
    /// Does not end with `F7`.
    MissingEox,
    /// The body is not a whole number of packed blocks plus the checksum.
    BadLength(usize),
    BlockCount {
        id: OsId,
        expected: usize,
        found: usize,
    },
    /// A byte of the body has bit 7 set (offset from the file start).
    NotSevenBit(usize),
    Checksum {
        stored: u16,
        computed: u16,
    },
    /// An image to encode that is empty or not a multiple of 1,024 bytes.
    BadImageSize(usize),
}

impl fmt::Display for OsImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort(n) => write!(f, "file too short ({n} bytes)"),
            Self::BadHeader => write!(f, "not a DSI Tempest SysEx (expected F0 01 28)"),
            Self::UnknownId(id) => write!(f, "unknown OS id {id:#04x}"),
            Self::MissingEox => write!(f, "last byte is not F7"),
            Self::BadLength(n) => write!(f, "body of {n} bytes is not whole 1,171-byte blocks"),
            Self::BlockCount {
                id,
                expected,
                found,
            } => write!(f, "{id:?}: {found} blocks, expected {expected}"),
            Self::NotSevenBit(at) => write!(f, "byte at offset {at} has bit 7 set"),
            Self::Checksum { stored, computed } => {
                write!(f, "checksum {stored:#06x} stored, {computed:#06x} computed")
            }
            Self::BadImageSize(n) => write!(f, "image of {n} bytes is not a multiple of 1,024"),
        }
    }
}

impl std::error::Error for OsImageError {}

/// A decoded, checked OS file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OsImage {
    pub id: OsId,
    pub image: Vec<u8>,
    pub checksum: u16,
}

/// 14-bit checksum of an image.
pub fn checksum(image: &[u8]) -> u16 {
    let sum = image
        .chunks(2)
        .map(|w| u32::from(w[0]) | u32::from(*w.get(1).unwrap_or(&0)) << 8)
        .fold(0u32, u32::wrapping_add);
    (sum & 0x3FFF) as u16
}

/// Builds the SysEx file of `image` for chip `id`.
pub fn encode(id: OsId, image: &[u8]) -> Result<Vec<u8>, OsImageError> {
    if image.is_empty() || !image.len().is_multiple_of(BLOCK_SIZE) {
        return Err(OsImageError::BadImageSize(image.len()));
    }
    let ck = checksum(image);
    let mut out = DSI_PREFIX.to_vec();
    out.push(id as u8);
    for block in image.chunks(BLOCK_SIZE) {
        out.extend(pack(block));
    }
    out.extend([(ck & 0x7F) as u8, (ck >> 7) as u8, EOX]);
    Ok(out)
}

/// Decodes and fully checks an OS file: header, id, end marker, block framing and count, checksum.
pub fn decode(syx: &[u8]) -> Result<OsImage, OsImageError> {
    let id = check_frame(syx)?;
    let body = &syx[4..syx.len() - 3];
    let blocks = body.len() / PACKED_BLOCK_SIZE;
    if !body.len().is_multiple_of(PACKED_BLOCK_SIZE) || blocks == 0 {
        return Err(OsImageError::BadLength(body.len()));
    }
    if blocks != id.blocks() {
        return Err(OsImageError::BlockCount {
            id,
            expected: id.blocks(),
            found: blocks,
        });
    }
    if let Some(at) = syx[4..syx.len() - 1].iter().position(|b| *b >= 0x80) {
        return Err(OsImageError::NotSevenBit(at + 4));
    }
    let image: Vec<u8> = body.chunks(PACKED_BLOCK_SIZE).flat_map(unpack).collect();
    let stored = u16::from(syx[syx.len() - 3]) | u16::from(syx[syx.len() - 2]) << 7;
    let computed = checksum(&image);
    if stored != computed {
        return Err(OsImageError::Checksum { stored, computed });
    }
    Ok(OsImage {
        id,
        image,
        checksum: computed,
    })
}

fn check_frame(syx: &[u8]) -> Result<OsId, OsImageError> {
    if syx.len() < FRAME_OVERHEAD {
        return Err(OsImageError::TooShort(syx.len()));
    }
    if syx[..3] != DSI_PREFIX {
        return Err(OsImageError::BadHeader);
    }
    let id = OsId::from_byte(syx[3]).ok_or(OsImageError::UnknownId(syx[3]))?;
    if syx[syx.len() - 1] != EOX {
        return Err(OsImageError::MissingEox);
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(blocks: usize) -> Vec<u8> {
        (0..blocks * BLOCK_SIZE)
            .map(|i| (i * 31 + i / 7) as u8)
            .collect()
    }

    fn voice_file() -> Vec<u8> {
        encode(OsId::Voice, &image(64)).unwrap()
    }

    #[test]
    fn test_decode_when_encoded_returns_identical_image_for_every_id() {
        for id in [OsId::Main, OsId::Voice, OsId::Panel, OsId::Sam] {
            let img = image(id.blocks());

            let decoded = decode(&encode(id, &img).unwrap()).unwrap();

            assert_eq!(decoded.id, id);
            assert_eq!(decoded.image, img);
        }
    }

    #[test]
    fn test_encode_when_voice_image_returns_frame_of_expected_size() {
        assert_eq!(voice_file().len(), 4 + 64 * 1171 + 3);
    }

    #[test]
    fn test_checksum_when_words_summed_returns_value_modulo_14_bits() {
        assert_eq!(checksum(&[0x34, 0x12, 0x01, 0x00]), 0x1235);
        assert_eq!(checksum(&[0xFF, 0xFF, 0x02, 0x00]), 0x0001);
        assert_eq!(checksum(&[0xFF, 0x7F]), 0x3FFF);
        assert_eq!(checksum(&[0x00, 0x40]), 0x0000);
        assert_eq!(checksum(&[0x00, 0x80]), 0x0000);
    }

    #[test]
    fn test_decode_when_whole_body_unpacked_as_one_stream_returns_drifting_data() {
        let img = image(64);
        let syx = voice_file();

        let drifting = unpack(&syx[4..syx.len() - 3]);

        assert_ne!(drifting, img);
        assert_ne!(
            drifting[BLOCK_SIZE..BLOCK_SIZE + 8],
            img[BLOCK_SIZE..BLOCK_SIZE + 8]
        );
    }

    #[test]
    fn test_decode_when_checksum_wrong_returns_checksum_error() {
        let mut syx = voice_file();
        let at = syx.len() - 3;
        syx[at] ^= 1;

        assert!(matches!(decode(&syx), Err(OsImageError::Checksum { .. })));
    }

    #[test]
    fn test_decode_when_one_image_byte_changed_returns_checksum_error() {
        let mut syx = voice_file();
        syx[100] ^= 0x01;

        assert!(matches!(decode(&syx), Err(OsImageError::Checksum { .. })));
    }

    #[test]
    fn test_decode_when_block_count_wrong_returns_block_count_error() {
        let syx = encode(OsId::Voice, &image(63)).unwrap();

        assert_eq!(
            decode(&syx),
            Err(OsImageError::BlockCount {
                id: OsId::Voice,
                expected: 64,
                found: 63
            })
        );
    }

    #[test]
    fn test_decode_when_id_swapped_returns_block_count_error() {
        let mut syx = voice_file();
        syx[3] = OsId::Main as u8;

        assert!(matches!(decode(&syx), Err(OsImageError::BlockCount { .. })));
    }

    #[test]
    fn test_decode_when_header_wrong_returns_bad_header() {
        let mut syx = voice_file();
        syx[1] = 0x02;

        assert_eq!(decode(&syx), Err(OsImageError::BadHeader));
    }

    #[test]
    fn test_decode_when_id_unknown_returns_unknown_id() {
        let mut syx = voice_file();
        syx[3] = 0x75;

        assert_eq!(decode(&syx), Err(OsImageError::UnknownId(0x75)));
    }

    #[test]
    fn test_decode_when_eox_missing_returns_missing_eox() {
        let mut syx = voice_file();
        let last = syx.len() - 1;
        syx[last] = 0x00;

        assert_eq!(decode(&syx), Err(OsImageError::MissingEox));
    }

    #[test]
    fn test_decode_when_truncated_returns_bad_length() {
        let mut syx = voice_file();
        syx.remove(10);

        assert!(matches!(decode(&syx), Err(OsImageError::BadLength(_))));
    }

    #[test]
    fn test_decode_when_tiny_returns_too_short() {
        assert_eq!(decode(&[0xF0, 0x01]), Err(OsImageError::TooShort(2)));
    }

    #[test]
    fn test_decode_when_byte_has_bit_7_returns_not_seven_bit_at_offset() {
        let mut syx = voice_file();
        syx[2000] |= 0x80;

        assert_eq!(decode(&syx), Err(OsImageError::NotSevenBit(2000)));
    }

    #[test]
    fn test_encode_when_size_not_multiple_of_block_returns_bad_image_size() {
        assert_eq!(
            encode(OsId::Main, &[0u8; 1000]),
            Err(OsImageError::BadImageSize(1000))
        );
        assert_eq!(encode(OsId::Main, &[]), Err(OsImageError::BadImageSize(0)));
    }
}
