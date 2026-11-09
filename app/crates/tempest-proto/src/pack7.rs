//! DSI 7-bit packing: groups of up to 7 data bytes behind one head byte holding their MSBs (bit j of the head byte is
//! the MSB of data byte j). Two layouts of a partial last group coexist and are kept apart:
//! - standard (`pack`, `pack_padded`, `unpack`): head bit j for byte j, as the machine's receivers
//!   read it, as OS images and files hold it; what the app sends;
//! - flushed (`pack_flushed`, `unpack_flushed`): what the machine's packer writes at the end of a message when the data
//!   length is not a multiple of 7: head bits shifted, stale bytes in the padding. Proved by simulating the
//!   firmware's own packer on the image.

use std::fmt;

const GROUP: usize = 7;

/// Why a packed stream is not what the machine sends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackError {
    /// A byte with bit 7 set: not 7-bit clean.
    HighBit { at: usize },
    /// The stream does not end on a complete group of 8 bytes: the machine always completes the last group.
    PartialGroup { extra: usize },
    /// The last group does not hold the end of `data_len` bytes of data.
    BadLength { data_len: usize, groups: usize },
    /// The head byte of the last group sets bits no partial group of the machine can set.
    TailHead { head: u8 },
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HighBit { at } => write!(f, "byte {at} has bit 7 set (not 7-bit data)"),
            Self::PartialGroup { extra } => {
                write!(f, "the data stops {extra} byte(s) into a group of 8")
            }
            Self::BadLength { data_len, groups } => {
                write!(
                    f,
                    "{data_len} data bytes do not end in the last of {groups} groups"
                )
            }
            Self::TailHead { head } => {
                write!(
                    f,
                    "last group head byte {head:#04x} does not fit its length"
                )
            }
        }
    }
}

impl std::error::Error for PackError {}

/// Packs `data` exactly: a last partial group keeps its own short length (OS image blocks use this form).
pub fn pack(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(packed_len(data.len()));
    for group in data.chunks(GROUP) {
        let head = group
            .iter()
            .enumerate()
            .fold(0u8, |head, (j, byte)| head | ((byte >> 7) << j));
        out.push(head);
        out.extend(group.iter().map(|byte| byte & 0x7F));
    }
    out
}

/// Packs `data` in the standard layout, the last group completed with zero bytes: what the machine's receivers read
/// (the machine's own output is `pack_flushed`).
pub fn pack_padded(data: &[u8]) -> Vec<u8> {
    let mut padded = data.to_vec();
    padded.resize(data.len().div_ceil(GROUP) * GROUP, 0);
    pack(&padded)
}

/// The strict check of what the app writes and sends: `packed` is made of whole groups of 8 7-bit bytes, the machine's
/// receivers (head bit j for byte j) read exactly `data` out of it, MSBs of the last bytes
/// included, and the padding that completes the last group is zeros. This is `pack_padded(data)` and nothing else: the
/// machine's own flushed tail, an unpadded last group, or a padding that is not zero all fail it.
pub fn is_standard_for(packed: &[u8], data: &[u8]) -> bool {
    let whole_groups = packed.len().is_multiple_of(GROUP + 1);
    let seven_bit = packed.iter().all(|byte| *byte < 0x80);
    let read = unpack(packed);
    whole_groups
        && seven_bit
        && read.starts_with(data)
        && read[data.len()..].iter().all(|byte| *byte == 0)
        && read.len() == data.len().div_ceil(GROUP) * GROUP
}

/// Number of bytes `pack` produces for `len` data bytes.
pub fn packed_len(len: usize) -> usize {
    len + len.div_ceil(GROUP)
}

/// Unpacks a stream of groups (a head byte then up to 7 bytes); a short last group is accepted.
/// Bit 7 of the packed bytes is ignored: validating that the input is 7-bit clean is the caller's job.
pub fn unpack(packed: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(packed.len());
    for group in packed.chunks(GROUP + 1) {
        let head = group[0];
        out.extend(
            group[1..]
                .iter()
                .enumerate()
                .map(|(j, byte)| (byte & 0x7F) | (((head >> j) & 1) << 7)),
        );
    }
    out
}

/// First bit of the head byte that holds the MSB of the first data byte, in the last group of a message of
/// `7 * g + partial` data bytes that the machine flushed (see `pack_flushed`). Each flush step shifts the head byte
/// right once and moves the write position by two, so the MSBs land `flush_steps` places lower than in a full group.
fn flushed_shift(partial: usize) -> usize {
    let (mut position, mut steps) = (partial, 0);
    while position < GROUP {
        position += 2;
        steps += 1;
    }
    GROUP - partial - steps
}

/// Packs `data` the way the Tempest firmware sends it (byte by byte, then the flush at the end of the message;
/// proved by simulating the firmware's own packer on the image).
/// Whole groups are standard. A last group of `k` bytes (1 to 6) is not: the head byte holds the MSBs shifted
/// right by the number of flush steps (`flushed_shift`), and the slots after the data alternate between a zero and
/// the byte the previous group left there (the flush loop advances the write position by two per step). The model
/// takes zeros for the stale bytes when the message has no full group. The machine's own receiver reads head bit `j`
/// for byte `j`, so it does not read these MSBs back: that layout is the machine's output, never what to send it.
pub fn pack_flushed(data: &[u8]) -> Vec<u8> {
    let whole = data.len() / GROUP * GROUP;
    let mut out = pack(&data[..whole]);
    let tail = &data[whole..];
    if tail.is_empty() {
        return out;
    }
    let mut slots = [0u8; GROUP + 2];
    if whole >= GROUP {
        for (slot, byte) in slots[1..=GROUP].iter_mut().zip(&data[whole - GROUP..whole]) {
            *slot = byte & 0x7F;
        }
    }
    let mut head = 0u8;
    for (j, byte) in tail.iter().enumerate() {
        head = (head | (byte & 0x80)) >> 1;
        slots[j + 1] = byte & 0x7F;
    }
    let mut position = tail.len();
    while position < GROUP {
        position += 2;
        slots[position] = 0;
        head >>= 1;
    }
    out.push(head);
    out.extend_from_slice(&slots[1..=GROUP]);
    out
}

fn check_packed(packed: &[u8]) -> Result<(), PackError> {
    if let Some(at) = packed.iter().position(|byte| *byte >= 0x80) {
        return Err(PackError::HighBit { at });
    }
    let extra = packed.len() % (GROUP + 1);
    if extra != 0 {
        return Err(PackError::PartialGroup { extra });
    }
    Ok(())
}

/// Splits `packed` into its whole groups, checks that `data_len` bytes end in the last group and returns the number
/// of data bytes that group holds (1 to 7).
fn last_group_len(packed: &[u8], data_len: usize) -> Result<usize, PackError> {
    check_packed(packed)?;
    let groups = packed.len() / (GROUP + 1);
    let before = groups.saturating_sub(1) * GROUP;
    if groups == 0 || data_len <= before || data_len > before + GROUP {
        return Err(PackError::BadLength { data_len, groups });
    }
    Ok(data_len - before)
}

/// Unpacks the message of the Tempest `data_len` data bytes long, last group included.
/// `packed` must be 7-bit clean and made of whole groups; the last one must hold the end of the data, whatever the
/// length (the padding is not data and is dropped). Head bits outside the ones the flush can set are refused.
pub fn unpack_flushed(packed: &[u8], data_len: usize) -> Result<Vec<u8>, PackError> {
    let partial = last_group_len(packed, data_len)?;
    let (whole, last) = packed.split_at(packed.len() - (GROUP + 1));
    let mut out = unpack(whole);
    if partial == GROUP {
        out.extend(unpack(last));
        return Ok(out);
    }
    let shift = flushed_shift(partial);
    let window = ((1u8 << partial) - 1) << shift;
    if last[0] & !window != 0 {
        return Err(PackError::TailHead { head: last[0] });
    }
    out.extend(
        last[1..=partial]
            .iter()
            .enumerate()
            .map(|(j, byte)| byte | (((last[0] >> (shift + j)) & 1) << 7)),
    );
    Ok(out)
}

/// `unpack_flushed` when the data length is not known in advance: tries every length the last group allows (longest
/// first) and keeps the one `parse` accepts (it must accept a length only when it consumes the data exactly). The
/// length is nearly always fixed by the data before the last group; if two lengths are accepted with different
/// results the stream is ambiguous and is refused. With none accepted, the error of the longest length is returned.
pub fn unpack_flushed_with<T: PartialEq, E>(
    packed: &[u8],
    parse: impl Fn(&[u8]) -> Result<T, E>,
) -> Result<T, FlushedError<E>> {
    check_packed(packed).map_err(FlushedError::Packing)?;
    let groups = packed.len() / (GROUP + 1);
    if groups == 0 {
        return Err(FlushedError::Packing(PackError::BadLength {
            data_len: 0,
            groups,
        }));
    }
    let before = (groups - 1) * GROUP;
    let mut accepted: Option<T> = None;
    let mut first_error: Option<FlushedError<E>> = None;
    for data_len in (before + 1..=before + GROUP).rev() {
        let outcome = unpack_flushed(packed, data_len)
            .map_err(FlushedError::Packing)
            .and_then(|data| parse(&data).map_err(FlushedError::Parse));
        match (outcome, &accepted) {
            (Ok(value), Some(other)) if *other != value => return Err(FlushedError::Ambiguous),
            (Ok(value), None) => accepted = Some(value),
            (Ok(_), Some(_)) => {}
            (Err(error), _) => {
                first_error.get_or_insert(error);
            }
        }
    }
    accepted.ok_or_else(|| {
        first_error.unwrap_or(FlushedError::Packing(PackError::BadLength {
            data_len: 0,
            groups,
        }))
    })
}

/// Why `unpack_flushed_with` gave nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlushedError<E> {
    Packing(PackError),
    /// No length was accepted: the error `parse` gave for the longest one that unpacked.
    Parse(E),
    /// Two lengths were accepted with different results.
    Ambiguous,
}

/// Reads a message of unknown data length that may be in either layout: the machine's (flushed last group) or the
/// standard one (this app, a DSI tool, an old file). Every length the last group allows is tried in both layouts, and
/// `accept` must take a candidate only when it consumes the data exactly. The data is returned when all the accepted
/// candidates agree. When they differ, the stream is kept apart by its padding: a candidate counts as exact when
/// packing it back (`pack_padded` or `pack_flushed`) gives the stream byte for byte; one exact reading wins. Anything
/// still undecided is `FlushedError::Ambiguous`: refuse rather than guess. With none accepted, the first error is
/// returned.
pub fn unpack_tolerant_with<E>(
    packed: &[u8],
    accept: impl Fn(&[u8]) -> Result<(), E>,
) -> Result<Vec<u8>, FlushedError<E>> {
    check_packed(packed).map_err(FlushedError::Packing)?;
    let groups = packed.len() / (GROUP + 1);
    if groups == 0 {
        return Err(FlushedError::Packing(PackError::BadLength {
            data_len: 0,
            groups,
        }));
    }
    let before = (groups - 1) * GROUP;
    let standard = unpack(packed);
    let mut accepted: Vec<(Vec<u8>, bool)> = Vec::new();
    let mut first_error: Option<FlushedError<E>> = None;
    for data_len in (before + 1..=before + GROUP).rev() {
        let flushed = unpack_flushed(packed, data_len);
        let mut readings = vec![(standard[..data_len].to_vec(), true)];
        let mut flush_error = None;
        match flushed {
            Ok(data) => readings.push((data, false)),
            Err(error) => flush_error = Some(error),
        }
        for (data, is_standard) in readings {
            match accept(&data) {
                Ok(()) => {
                    let again = if is_standard {
                        pack_padded(&data)
                    } else {
                        pack_flushed(&data)
                    };
                    accepted.push((data, again == packed));
                }
                Err(error) => {
                    first_error.get_or_insert(FlushedError::Parse(error));
                }
            }
        }
        if let Some(error) = flush_error {
            first_error.get_or_insert(FlushedError::Packing(error));
        }
    }
    if accepted.is_empty() {
        return Err(
            first_error.unwrap_or(FlushedError::Packing(PackError::BadLength {
                data_len: 0,
                groups,
            })),
        );
    }
    decide(accepted).ok_or(FlushedError::Ambiguous)
}

/// The one reading `accepted` (not empty) leaves: all equal, else the one that packs back exactly; `None` when the
/// candidates disagree and no single one packs back exactly.
fn decide(accepted: Vec<(Vec<u8>, bool)>) -> Option<Vec<u8>> {
    let first = accepted.first()?.0.clone();
    if accepted.iter().all(|(data, _)| *data == first) {
        return Some(first);
    }
    let mut exact = accepted.into_iter().filter(|(_, exact)| *exact);
    let (data, _) = exact.next()?;
    exact.all(|(other, _)| other == data).then_some(data)
}

/// Unpacks `data_len` bytes from a source that may be the machine (flushed last group) or something that packed the
/// standard way (this app, a DSI tool): the head byte tells, because the two layouts use different head bits. Used
/// where both really occur. Refused when the head fits both layouts and they read differently, which for
/// 128-byte sounds (2 bytes in the last group: bits 2-3 against bits 0-1) never happens.
pub fn unpack_either(packed: &[u8], data_len: usize) -> Result<Vec<u8>, PackError> {
    let partial = last_group_len(packed, data_len)?;
    let mut standard = unpack(packed);
    standard.truncate(data_len);
    if partial == GROUP {
        return Ok(standard);
    }
    let head = packed[packed.len() - (GROUP + 1)];
    let standard_fits = head & !((1u8 << partial) - 1) == 0;
    let flushed = unpack_flushed(packed, data_len);
    match (standard_fits, flushed) {
        (true, Ok(flushed)) if flushed != standard => Err(PackError::TailHead { head }),
        (true, _) => Ok(standard),
        (false, flushed) => flushed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Data of the packer simulation (not in the public tree): 7 full-MSB bytes, then `k` more.
    fn sim_data(k: usize) -> Vec<u8> {
        [
            0x91, 0xA2, 0xB3, 0xC4, 0xD5, 0xE6, 0xF7, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD,
        ][..7 + k]
            .to_vec()
    }

    /// Last group on the wire when the image's own packer sends `sim_data(k)` (the packer simulation, k = 1..6).
    const SIM_LAST_GROUP: [[u8; 8]; 6] = [
        [8, 8, 34, 0, 68, 0, 102, 0],
        [12, 8, 25, 51, 0, 85, 0, 119],
        [28, 8, 25, 42, 68, 0, 102, 0],
        [30, 8, 25, 42, 59, 85, 0, 119],
        [62, 8, 25, 42, 59, 76, 102, 0],
        [63, 8, 25, 42, 59, 76, 93, 119],
    ];

    #[test]
    fn test_pack_flushed_when_partial_group_of_k_bytes_returns_what_the_image_sends() {
        for k in 1..=6 {
            let packed = pack_flushed(&sim_data(k));

            assert_eq!(packed.len(), 16, "k={k}");
            assert_eq!(packed[..8], pack(&sim_data(k)[..7]), "k={k}");
            assert_eq!(packed[8..], SIM_LAST_GROUP[k - 1], "k={k}");
        }
    }

    #[test]
    fn test_unpack_flushed_when_image_stream_of_k_bytes_returns_the_data_for_every_k() {
        for k in 1..=6 {
            let packed = pack_flushed(&sim_data(k));

            assert_eq!(unpack_flushed(&packed, 7 + k), Ok(sim_data(k)), "k={k}");
        }
    }

    #[test]
    fn test_unpack_flushed_when_whole_groups_only_returns_standard_unpack() {
        let data: Vec<u8> = (0..21).map(|i| (i * 53 + 7) as u8).collect();

        assert_eq!(unpack_flushed(&pack_flushed(&data), 21), Ok(data));
    }

    #[test]
    fn test_unpack_flushed_when_every_length_returns_the_data() {
        for len in 1..=60usize {
            let data: Vec<u8> = (0..len).map(|i| (i * 37 + 0x81) as u8 | 0x80).collect();

            assert_eq!(unpack_flushed(&pack_flushed(&data), len), Ok(data), "{len}");
        }
    }

    #[test]
    fn test_unpack_when_flushed_group_read_with_the_standard_layout_returns_other_msbs() {
        // Counter-proof of the old rule: the standard reading loses the MSBs of the flushed bytes.
        for k in 1..=5 {
            let packed = pack_flushed(&sim_data(k));

            assert_ne!(unpack(&packed)[7..7 + k], sim_data(k)[7..], "k={k}");
        }
    }

    #[test]
    fn test_unpack_flushed_when_head_has_a_bit_outside_the_data_returns_tail_head_error() {
        let mut packed = pack_flushed(&sim_data(3));
        packed[8] |= 0x01;

        assert_eq!(
            unpack_flushed(&packed, 10),
            Err(PackError::TailHead { head: 0x1D })
        );
    }

    #[test]
    fn test_unpack_flushed_when_length_does_not_end_in_last_group_returns_bad_length() {
        let packed = pack_flushed(&sim_data(3));

        for len in [0, 7, 15] {
            assert_eq!(
                unpack_flushed(&packed, len),
                Err(PackError::BadLength {
                    data_len: len,
                    groups: 2
                })
            );
        }
    }

    #[test]
    fn test_unpack_flushed_when_byte_has_bit_7_returns_high_bit_error_at_its_place() {
        let mut packed = pack_flushed(&[1, 2, 3]);
        packed[2] |= 0x80;

        assert_eq!(
            unpack_flushed(&packed, 3),
            Err(PackError::HighBit { at: 2 })
        );
    }

    #[test]
    fn test_unpack_flushed_when_stream_is_not_whole_groups_returns_partial_group_error() {
        let packed = pack(&[1, 2, 3]);

        assert_eq!(
            unpack_flushed(&packed, 3),
            Err(PackError::PartialGroup { extra: 4 })
        );
    }

    #[test]
    fn test_unpack_flushed_with_when_parser_takes_exact_length_returns_its_value() {
        let data = sim_data(5);
        let packed = pack_flushed(&data);

        let value = unpack_flushed_with(&packed, |bytes: &[u8]| {
            if bytes.len() == data.len() {
                Ok(bytes.to_vec())
            } else {
                Err(bytes.len())
            }
        });

        assert_eq!(value, Ok(data));
    }

    #[test]
    fn test_unpack_flushed_with_when_parser_takes_no_length_returns_first_error() {
        let packed = pack_flushed(&sim_data(2));

        let value = unpack_flushed_with(&packed, |bytes: &[u8]| Err::<(), usize>(bytes.len()));

        assert_eq!(value, Err(FlushedError::Parse(14)));
    }

    #[test]
    fn test_unpack_flushed_with_when_two_lengths_read_differently_returns_ambiguous() {
        let packed = pack_flushed(&sim_data(3));

        let value = unpack_flushed_with(&packed, |bytes: &[u8]| Ok::<_, ()>(bytes.len()));

        assert_eq!(value, Err(FlushedError::Ambiguous));
    }

    #[test]
    fn test_unpack_either_when_128_byte_sound_in_either_layout_returns_the_sound() {
        let data: Vec<u8> = (0..128).map(|i| (i * 11 + 0x80) as u8).collect();
        for packed in [pack_flushed(&data), pack_padded(&data)] {
            assert_eq!(unpack_either(&packed, 128), Ok(data.clone()));
        }
    }

    #[test]
    fn test_unpack_either_when_both_layouts_fit_and_differ_returns_tail_head_error() {
        // 3 bytes in the last group: head bit 2 is byte 2 (standard) or byte 0 (flushed).
        let mut packed = pack_padded(&[1, 2, 3]);
        packed[0] = 0b100;

        assert_eq!(
            unpack_either(&packed, 3),
            Err(PackError::TailHead { head: 0b100 })
        );
    }

    #[test]
    fn test_pack_when_msbs_set_returns_head_byte_with_bit_j_for_byte_j() {
        let data = [0x80, 0x01, 0xFF, 0x00, 0x00, 0x00, 0x80];

        let packed = pack(&data);

        assert_eq!(
            packed,
            [0b100_0101, 0x00, 0x01, 0x7F, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn test_pack_when_head_bits_differ_returns_different_head() {
        let a = pack(&[0x80, 0, 0, 0, 0, 0, 0]);
        let b = pack(&[0, 0x80, 0, 0, 0, 0, 0]);

        assert_ne!(a[0], b[0]);
    }

    #[test]
    fn test_unpack_when_packed_returns_original_for_every_length() {
        for len in 0..=40usize {
            let data: Vec<u8> = (0..len).map(|i| (i * 37 + 11) as u8).collect();

            assert_eq!(unpack(&pack(&data)), data, "length {len}");
            assert_eq!(pack(&data).len(), packed_len(len));
        }
    }

    #[test]
    fn test_pack_when_output_inspected_returns_only_seven_bit_bytes() {
        let data = [0xFF; 50];

        assert!(pack(&data).iter().all(|b| *b < 0x80));
    }

    #[test]
    fn test_pack_padded_when_128_bytes_returns_152_bytes_with_zero_padding() {
        let data = [0xA5; 128];

        let packed = pack_padded(&data);

        assert_eq!(packed.len(), 152);
        assert_eq!(&unpack(&packed)[..128], &data);
        assert_eq!(&unpack(&packed)[128..], &[0u8; 5]);
    }

    #[test]
    fn test_pack_when_not_padded_returns_shorter_than_padded() {
        let data = [1u8; 128];

        assert_eq!(pack(&data).len(), 147);
        assert_ne!(pack(&data).len(), pack_padded(&data).len());
    }

    #[test]
    fn test_unpack_when_head_only_group_returns_nothing() {
        assert!(unpack(&[0x7F]).is_empty());
    }

    /// Last group of `pack_padded(sim_data(k))`: head bit j for byte j (the 6 tail bytes all have their MSB set), the
    /// 7-bit bytes `08 19 2A 3B 4C 5D`, then zeros.
    const STANDARD_LAST_GROUP: [[u8; 8]; 6] = [
        [0x01, 0x08, 0, 0, 0, 0, 0, 0],
        [0x03, 0x08, 0x19, 0, 0, 0, 0, 0],
        [0x07, 0x08, 0x19, 0x2A, 0, 0, 0, 0],
        [0x0F, 0x08, 0x19, 0x2A, 0x3B, 0, 0, 0],
        [0x1F, 0x08, 0x19, 0x2A, 0x3B, 0x4C, 0, 0],
        [0x3F, 0x08, 0x19, 0x2A, 0x3B, 0x4C, 0x5D, 0],
    ];

    #[test]
    fn test_pack_padded_when_partial_group_of_k_bytes_returns_the_standard_golden_group() {
        for k in 1..=6 {
            let packed = pack_padded(&sim_data(k));

            assert_eq!(packed.len(), 16, "k={k}");
            assert_eq!(packed[8..], STANDARD_LAST_GROUP[k - 1], "k={k}");
            assert!(is_standard_for(&packed, &sim_data(k)), "k={k}");
        }
    }

    #[test]
    fn test_unpack_when_standard_group_of_k_bytes_returns_the_original_with_msbs_set() {
        // The machine's receiver rule: head bit j for byte j.
        for k in 1..=6 {
            let packed = pack_padded(&sim_data(k));

            assert_eq!(unpack(&packed)[..7 + k], sim_data(k), "k={k}");
            assert!(sim_data(k)[7..].iter().all(|byte| *byte >= 0x80));
        }
    }

    #[test]
    fn test_is_standard_for_when_the_machines_flushed_bytes_are_kept_returns_false_for_every_k() {
        for k in 1..=6 {
            assert!(
                !is_standard_for(&pack_flushed(&sim_data(k)), &sim_data(k)),
                "k={k}"
            );
        }
    }

    #[test]
    fn test_is_standard_for_when_a_padding_variant_is_used_returns_false() {
        for k in 1..=6 {
            let data = sim_data(k);
            let standard = pack_padded(&data);
            let mut stale = standard.clone();
            stale[15] = 0x77; // stale byte in the last padding slot
            let mut head_in_padding = standard.clone();
            head_in_padding[8] |= 0x40; // MSB bit of a padding byte
            let unpadded = pack(&data);

            assert!(is_standard_for(&standard, &data), "k={k}");
            assert!(!is_standard_for(&stale, &data), "stale padding, k={k}");
            assert!(
                !is_standard_for(&head_in_padding, &data),
                "padding head, k={k}"
            );
            assert!(!is_standard_for(&unpadded, &data), "unpadded, k={k}");
        }
    }

    /// Accepts only data of `len` bytes: a stand-in for a parser that consumes its data exactly.
    fn exactly(len: usize) -> impl Fn(&[u8]) -> Result<(), usize> {
        move |data| {
            if data.len() == len {
                Ok(())
            } else {
                Err(data.len())
            }
        }
    }

    #[test]
    fn test_unpack_tolerant_with_when_either_layout_of_k_bytes_returns_the_data_for_every_k() {
        for k in 1..=6 {
            for packed in [pack_flushed(&sim_data(k)), pack_padded(&sim_data(k))] {
                assert_eq!(
                    unpack_tolerant_with(&packed, exactly(7 + k)),
                    Ok(sim_data(k)),
                    "k={k}"
                );
            }
        }
    }

    #[test]
    fn test_unpack_tolerant_with_when_both_layouts_fit_returns_the_one_that_packs_back_exactly() {
        // Only byte 2 of the tail has its MSB: standard head bit 2, which the flushed layout (window bits 2-4)
        // would read as the MSB of byte 0. Both readings are valid; the padding says which one was written.
        let mut data = sim_data(3);
        data[7] &= 0x7F;
        data[8] &= 0x7F;
        let packed = pack_padded(&data);

        assert_eq!(unpack_tolerant_with(&packed, exactly(10)), Ok(data));
    }

    #[test]
    fn test_unpack_tolerant_with_when_nothing_tells_the_layout_returns_ambiguous() {
        let packed = pack_padded(&sim_data(3));

        let value = unpack_tolerant_with(&packed, |_: &[u8]| Ok::<_, ()>(()));

        assert_eq!(value, Err(FlushedError::Ambiguous));
    }

    #[test]
    fn test_unpack_tolerant_with_when_no_length_fits_returns_the_error_of_the_parser() {
        let packed = pack_flushed(&sim_data(2));

        let value = unpack_tolerant_with(&packed, exactly(99));

        assert!(matches!(value, Err(FlushedError::Parse(_))));
    }
}
