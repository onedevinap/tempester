//! Summary of a beat dump `F0 01 28 5F <pack(beat)> F7` (`tools/beat_fx_check.py`, `tools/beat_voice_assign.py`).
//!
//! The packed stream is a list of 16-bit words, high byte first: a 538-word header (word 0 is the format word,
//! 32 pad blocks of 15 words start at word 26), then event chunks `[N, (step, w0, w1, w2, w3) * (N - 1) / 5]`
//! repeated while `N > 2000`, then the 128-byte sounds. `summarize` reads the header and events only; `read_unpacked`
//! reads everything, strictly. Words 12-21 hold the beat name, 20 ASCII characters padded with spaces (read on a real
//! dump of the author's machine: `NoSpoon-1905` + 8 spaces).
//!
//! Sounds (established from the firmware): the beat sender writes each of them with the same function as the sound
//! sender of the `60` message, so a sound in a beat is the same 128 bytes as the payload of `60`. The header
//! deserializer marks every pad as holding a sound (two exceptions in versions below 7: pads 16-31 have none),
//! whatever the header says; the sender always writes version 10, so 32.

use std::fmt;

use crate::pack7::{pack_padded, unpack, unpack_tolerant_with, FlushedError, PackError};
use crate::sound::{Sound, SOUND_BYTES};
use crate::DSI_PREFIX;

pub const PADS: usize = 32;
const HEADER_WORDS: usize = 538;
const PAD0_WORD: usize = 26;
const PAD_WORDS: usize = 15;
const CHUNK_LIMIT: usize = 2000;
/// Word of a pad block holding the play-probability drop (n°36).
const DROP_WORD: usize = 5;
const MAX_DROP: u16 = 100;
const NAME_WORD: usize = 12;
const NAME_WORDS: usize = 10;
pub const CMD_BEAT: u8 = 0x5F;

/// Why a beat dump was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeatError {
    NotBeatMessage,
    /// Fewer words than the header needs.
    TruncatedHeader(usize),
    /// An event chunk announces more words than the dump holds, or a size that is not `1 + 5k`.
    BadChunk {
        at_word: usize,
        count: usize,
    },
    /// A format word the firmware does not accept (it takes 1 to 10).
    UnsupportedVersion(u16),
    /// The data ends before the sound of this pad (0-based).
    TruncatedSounds {
        pad: usize,
    },
    /// Bytes after the last sound.
    TrailingData(usize),
    /// The 7-bit stream is not a well-formed packed message.
    Packing(PackError),
    /// The end of the message reads as two different valid beats (the layout of its last group is not decidable).
    AmbiguousTail,
    /// A sound was given for a pad this beat's format version holds no sound for (pads 17-32 below version 7).
    NoSoundSlot {
        pad: usize,
    },
    /// The time signature unit of the header is outside the firmware's table (0-5).
    UnsupportedTimeUnit(u8),
}

impl fmt::Display for BeatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotBeatMessage => write!(f, "not a beat dump (F0 01 28 5F ... F7)"),
            Self::TruncatedHeader(n) => {
                write!(f, "beat dump has {n} words, header needs {HEADER_WORDS}")
            }
            Self::BadChunk { at_word, count } => {
                write!(f, "bad event chunk of {count} words at word {at_word}")
            }
            Self::UnsupportedVersion(v) => write!(f, "beat format version {v} is not 1 to 10"),
            Self::TruncatedSounds { pad } => {
                write!(f, "the data ends before the sound of pad {}", pad + 1)
            }
            Self::TrailingData(n) => write!(f, "{n} unexpected byte(s) after the last sound"),
            Self::Packing(e) => write!(f, "bad 7-bit packing: {e}"),
            Self::AmbiguousTail => write!(
                f,
                "the end of the message reads as two different valid beats: refusing to guess"
            ),
            Self::NoSoundSlot { pad } => {
                write!(f, "this beat's format holds no sound for pad {}", pad + 1)
            }
            Self::UnsupportedTimeUnit(unit) => {
                write!(f, "time signature unit {unit} is not 0 to 5")
            }
        }
    }
}

impl std::error::Error for BeatError {}

/// What a beat dump holds, at a glance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeatSummary {
    pub format_word: u16,
    /// The beat name without its trailing spaces and NULs; a byte outside 0x20-0x7E shows as `?`.
    pub name: String,
    /// Voice Assign of each pad: 0 none, 1-6 a voice (low byte of word 1 of the pad block).
    pub voice_assign: [u8; PADS],
    /// Play probability of each pad in percent (Tempester n°36): word 5 of the pad block holds the drop `100 - p`;
    /// 0 (every file older than n°36) is 100 %, and a drop above 100 is read as 0, as the firmware does.
    pub play_probability: [u8; PADS],
    pub events: usize,
}

/// Reads the summary out of a complete dump message.
pub fn summarize(message: &[u8]) -> Result<BeatSummary, BeatError> {
    let valid = message.len() >= 5
        && message[..3] == DSI_PREFIX
        && message[3] == CMD_BEAT
        && message[message.len() - 1] == 0xF7;
    if !valid {
        return Err(BeatError::NotBeatMessage);
    }
    let words = to_words(&unpack(&message[4..message.len() - 1]));
    if words.len() < HEADER_WORDS {
        return Err(BeatError::TruncatedHeader(words.len()));
    }
    Ok(summary_of(&words, count_events(&words)?))
}

fn summary_of(words: &[u16], events: usize) -> BeatSummary {
    BeatSummary {
        format_word: words[0],
        name: name_of(words),
        voice_assign: std::array::from_fn(|pad| {
            (words[PAD0_WORD + PAD_WORDS * pad + 1] & 0xFF) as u8
        }),
        play_probability: std::array::from_fn(|pad| {
            let drop = words[PAD0_WORD + PAD_WORDS * pad + DROP_WORD];
            let drop = if drop > MAX_DROP { 0 } else { drop };
            (MAX_DROP - drop) as u8
        }),
        events,
    }
}

/// A beat read in full: its summary and its sounds (`None` for a pad the format version has no sound for).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeatDump {
    pub summary: BeatSummary,
    pub sounds: [Option<Sound>; PADS],
}

/// Words of the header by format version, as the firmware's beat receiver reads them (1 and 2 are the short legacy
/// header).
pub(crate) fn header_words_of(version: u16) -> Result<usize, BeatError> {
    match version {
        1 | 2 => Ok(530),
        3..=10 => Ok(HEADER_WORDS),
        other => Err(BeatError::UnsupportedVersion(other)),
    }
}

/// Pads that carry a sound: all 32 from version 7, the first 16 before.
fn sound_pads_of(version: u16) -> usize {
    if version >= 7 {
        PADS
    } else {
        PADS / 2
    }
}

/// Reads one beat at the start of `data` (the unpacked bytes of a `5F` or `5C` message, or of a beat inside a project
/// file) and returns it with the number of bytes it takes. The reading follows the firmware's beat receiver: header,
/// event chunks, sounds; anything that would make the firmware wait for more (or hang) is an error.
pub fn read_unpacked_prefix(data: &[u8]) -> Result<(BeatDump, usize), BeatError> {
    let version = data
        .get(..2)
        .map(|pair| u16::from(pair[0]) << 8 | u16::from(pair[1]))
        .ok_or(BeatError::TruncatedHeader(data.len() / 2))?;
    let header_bytes = header_words_of(version)? * 2;
    let header = data
        .get(..header_bytes)
        .ok_or(BeatError::TruncatedHeader(data.len() / 2))?;
    let words = to_words(header);
    let (events, mut at) = skip_event_chunks(data, header_bytes)?;
    let mut sounds: [Option<Sound>; PADS] = std::array::from_fn(|_| None);
    for (pad, slot) in sounds.iter_mut().enumerate().take(sound_pads_of(version)) {
        let bytes = data
            .get(at..at + SOUND_BYTES)
            .ok_or(BeatError::TruncatedSounds { pad })?;
        let mut array = [0u8; SOUND_BYTES];
        array.copy_from_slice(bytes);
        *slot = Some(Sound::from_bytes(array));
        at += SOUND_BYTES;
    }
    Ok((
        BeatDump {
            summary: summary_of(&words, events),
            sounds,
        },
        at,
    ))
}

/// Reads a whole beat message body: `read_unpacked_prefix`, with nothing left over (the padding of the last packed
/// group is not data: `pack7::unpack_flushed` leaves it out).
pub fn read_unpacked(data: &[u8]) -> Result<BeatDump, BeatError> {
    let (dump, used) = read_unpacked_prefix(data)?;
    match data.len() - used {
        0 => Ok(dump),
        rest => Err(BeatError::TrailingData(rest)),
    }
}

/// The data of a beat message body (`5F`, or `5C` without its beat number) whichever way its last 7-bit group is laid
/// out (the machine's flush, or the standard layout), read in full with `read_unpacked`. Refused when the layout
/// cannot be decided (`pack7::unpack_tolerant_with`).
pub fn read_body(packed: &[u8]) -> Result<Vec<u8>, BeatError> {
    unpack_tolerant_with(packed, |data| read_unpacked(data).map(|_| ())).map_err(
        |error| match error {
            FlushedError::Packing(e) => BeatError::Packing(e),
            FlushedError::Parse(e) => e,
            FlushedError::Ambiguous => BeatError::AmbiguousTail,
        },
    )
}

/// A `5F` message in the standard layout (`pack_padded`: head bit j for byte j, last group completed with zeros), the
/// one the machine's receivers read, for beat `data` (the unpacked bytes).
pub fn standard_dump(data: &[u8]) -> Vec<u8> {
    let mut out = DSI_PREFIX.to_vec();
    out.push(CMD_BEAT);
    out.extend(pack_padded(data));
    out.push(0xF7);
    out
}

/// A `5F` message as the machine sends it (`pack7::pack_flushed`: the broken tail), for the fake machine and tests.
pub fn machine_dump(data: &[u8]) -> Vec<u8> {
    let mut out = DSI_PREFIX.to_vec();
    out.push(CMD_BEAT);
    out.extend(crate::pack7::pack_flushed(data));
    out.push(0xF7);
    out
}

/// The same beat message in the standard layout, whichever layout `message` (a `5F`) came in: what the app saves and
/// sends. The decoded data is identical. Fails on a message that is not a complete beat or whose tail is ambiguous.
pub fn standard_message(message: &[u8]) -> Result<Vec<u8>, BeatError> {
    let valid = message.len() >= 5
        && message[..3] == DSI_PREFIX
        && message[3] == CMD_BEAT
        && message[message.len() - 1] == 0xF7;
    if !valid {
        return Err(BeatError::NotBeatMessage);
    }
    read_body(&message[4..message.len() - 1]).map(|data| standard_dump(&data))
}

/// The unpacked beat `data` with the sound of each pad that has `Some` replaced and every other byte kept: the header,
/// the events and the other sounds are not touched (the sounds are fixed-size records after the last event chunk).
/// Fails when `data` is not exactly one beat, or for a pad the format version holds no sound for.
pub fn with_sounds(data: &[u8], sounds: &[Option<Sound>; PADS]) -> Result<Vec<u8>, BeatError> {
    let version = u16::from(data.first().copied().unwrap_or(0)) << 8
        | u16::from(data.get(1).copied().unwrap_or(0));
    let (_, used) = read_unpacked_prefix(data)?;
    if used != data.len() {
        return Err(BeatError::TrailingData(data.len() - used));
    }
    let slots = sound_pads_of(version);
    if let Some(pad) = (slots..PADS).find(|pad| sounds[*pad].is_some()) {
        return Err(BeatError::NoSoundSlot { pad });
    }
    let first = used - slots * SOUND_BYTES;
    let mut out = data.to_vec();
    for (pad, sound) in sounds.iter().enumerate().take(slots) {
        if let Some(sound) = sound {
            let at = first + pad * SOUND_BYTES;
            out[at..at + SOUND_BYTES].copy_from_slice(sound.bytes());
        }
    }
    Ok(out)
}

/// Event chunks from `start` (a byte offset): their event count and the offset after the last one.
fn skip_event_chunks(data: &[u8], start: usize) -> Result<(usize, usize), BeatError> {
    let mut events = 0;
    let at = walk_events(data, start, |_| events += 1)?;
    Ok((events, at))
}

/// Walks the event chunks from `start` (a byte offset), handing each `[step, w0, w1, w2, w3]` record to `visit`, and
/// returns the offset after the last chunk. The checks are those of the firmware's beat receiver (see
/// `read_unpacked_prefix`).
pub(crate) fn walk_events(
    data: &[u8],
    start: usize,
    mut visit: impl FnMut([u16; 5]),
) -> Result<usize, BeatError> {
    let mut at = start;
    loop {
        let bad = |count| BeatError::BadChunk {
            at_word: at / 2,
            count,
        };
        let count = data
            .get(at..at + 2)
            .map(|pair| usize::from(pair[0]) << 8 | usize::from(pair[1]))
            .ok_or_else(|| bad(0))?;
        // The firmware reads `count - 1` words and loops on while that is 2000 or more; it hangs on 0 or above 0x2000.
        let payload = count.checked_sub(1).ok_or_else(|| bad(count))?;
        if payload > 0x2000 || payload % 5 != 0 || at + 2 * count > data.len() {
            return Err(bad(count));
        }
        let records = data[at + 2..at + 2 * count].as_chunks::<10>().0;
        for record in records {
            visit(std::array::from_fn(|k| {
                u16::from_be_bytes([record[2 * k], record[2 * k + 1]])
            }));
        }
        at += 2 * count;
        if count <= CHUNK_LIMIT {
            return Ok(at);
        }
    }
}

fn name_of(words: &[u16]) -> String {
    let text: String = words[NAME_WORD..NAME_WORD + NAME_WORDS]
        .iter()
        .flat_map(|word| word.to_be_bytes())
        .map(|byte| {
            if (0x20..0x7F).contains(&byte) || byte == 0 {
                char::from(byte)
            } else {
                '?'
            }
        })
        .collect();
    text.trim_end_matches([' ', '\0']).to_string()
}

/// The unpacked beat `data` with the play probability of every pad set (word 5 of each pad block = `100 - p`), every
/// other byte kept. Used by the fake machine, which keeps what CC 14 sets as the firmware does. Fails when `data` has
/// no full header or a probability is above 100.
pub fn with_play_probability(data: &[u8], probability: &[u8; PADS]) -> Result<Vec<u8>, BeatError> {
    let version = u16::from(data.first().copied().unwrap_or(0)) << 8
        | u16::from(data.get(1).copied().unwrap_or(0));
    let header_bytes = header_words_of(version)? * 2;
    if data.len() < header_bytes {
        return Err(BeatError::TruncatedHeader(data.len() / 2));
    }
    let mut out = data.to_vec();
    for (pad, percent) in probability.iter().enumerate() {
        let drop = MAX_DROP.saturating_sub(u16::from(*percent));
        let at = (PAD0_WORD + PAD_WORDS * pad + DROP_WORD) * 2;
        if at + 2 > header_bytes {
            break;
        }
        out[at..at + 2].copy_from_slice(&drop.to_be_bytes());
    }
    Ok(out)
}

/// Bytes of the beat name in the unpacked data: words 12-21 of the header, two bytes per word, high byte first.
pub const NAME_BYTES: std::ops::Range<usize> = NAME_WORD * 2..(NAME_WORD + NAME_WORDS) * 2;

/// The unpacked beat `data` with another name: at most 20 characters of 0x20-0x7E, padded with spaces as the
/// firmware reads it. `None` for a name that does not fit or data too short to hold a header.
pub fn with_name(data: &[u8], name: &str) -> Option<Vec<u8>> {
    let valid = name.len() <= NAME_BYTES.len() && name.bytes().all(|b| (0x20..0x7F).contains(&b));
    if !valid || data.len() < NAME_BYTES.end {
        return None;
    }
    let mut out = data.to_vec();
    let padded = name.bytes().chain(std::iter::repeat(b' '));
    for (slot, byte) in out[NAME_BYTES].iter_mut().zip(padded) {
        *slot = byte;
    }
    Some(out)
}

/// Builds a well-formed beat dump out of nothing, for tests and the fake machine: `name` (at most 20 printable
/// ASCII characters, else it is cut or shown as `?`), a Voice Assign per `(pad, voice)` and one chunk of `events`
/// events. It is not a playable beat: the sounds that follow the events in a real dump are absent.
pub fn synthetic_dump(name: &str, assigns: &[(usize, u8)], events: usize) -> Vec<u8> {
    let mut words = vec![0u16; HEADER_WORDS];
    words[0] = 10;
    let mut chars = name.bytes().chain(std::iter::repeat(b' '));
    for word in &mut words[NAME_WORD..NAME_WORD + NAME_WORDS] {
        let high = chars.next().unwrap_or(b' ');
        let low = chars.next().unwrap_or(b' ');
        *word = u16::from(high) << 8 | u16::from(low);
    }
    for (pad, voice) in assigns.iter().filter(|(pad, _)| *pad < PADS) {
        words[PAD0_WORD + PAD_WORDS * pad + 1] = u16::from(*voice);
    }
    if events > 0 {
        words.push((1 + 5 * events) as u16);
        words.extend((0..events * 5).map(|i| i as u16));
    }
    let data: Vec<u8> = words.iter().flat_map(|w| w.to_be_bytes()).collect();
    let mut out = DSI_PREFIX.to_vec();
    out.push(CMD_BEAT);
    out.extend(crate::pack7::pack_padded(&data));
    out.push(0xF7);
    out
}

/// The unpacked bytes of a complete synthetic beat (format 10): like `synthetic_dump` but with the chunk header of a
/// real beat (also for zero events) and one sound per pad, so it reads back with `read_unpacked`.
pub fn synthetic_beat_data(
    name: &str,
    assigns: &[(usize, u8)],
    events: usize,
    sounds: &[Sound; PADS],
) -> Vec<u8> {
    let dump = synthetic_dump(name, assigns, 0);
    let mut data = unpack(&dump[4..dump.len() - 1]);
    data.truncate(HEADER_WORDS * 2);
    let mut chunk = vec![(1 + 5 * events) as u16];
    chunk.extend((0..events * 5).map(|i| i as u16));
    data.extend(chunk.iter().flat_map(|w| w.to_be_bytes()));
    for sound in sounds {
        data.extend_from_slice(sound.bytes());
    }
    data
}

/// The unpacked bytes of a complete synthetic beat whose length leaves `tail` bytes (0 to 6) in the last 7-bit group
/// (`pack7::pack_flushed` lays that group out by its length), and whose last bytes all have their MSB set: the case
/// where the machine's broken tail and the standard layout differ. For tests and the fake machine.
pub fn synthetic_tail_beat(tail: usize) -> Vec<u8> {
    let sounds: [Sound; PADS] = std::array::from_fn(|pad| {
        let mut bytes = [0x11 + pad as u8; SOUND_BYTES];
        if pad == PADS - 1 {
            for (at, byte) in bytes[SOUND_BYTES - 8..].iter_mut().enumerate() {
                *byte = 0x80 | (0x31 + at as u8);
            }
        }
        Sound::from_bytes(bytes)
    });
    let events = (0..7)
        .find(|events| synthetic_beat_data("Tail", &[(0, 1)], *events, &sounds).len() % 7 == tail)
        .unwrap_or(0);
    synthetic_beat_data("Tail", &[(0, 1)], events, &sounds)
}

fn to_words(data: &[u8]) -> Vec<u16> {
    data.as_chunks::<2>()
        .0
        .iter()
        .map(|&pair| u16::from_be_bytes(pair))
        .collect()
}

fn count_events(words: &[u16]) -> Result<usize, BeatError> {
    let (mut position, mut events) = (HEADER_WORDS, 0);
    while position < words.len() {
        let count = usize::from(words[position]);
        if count == 0 {
            break; // zero padding of the last 7-byte group: no more chunks
        }
        let well_formed = count >= 1 && (count - 1) % 5 == 0 && position + count <= words.len();
        if !well_formed {
            return Err(BeatError::BadChunk {
                at_word: position,
                count,
            });
        }
        events += (count - 1) / 5;
        position += count;
        if count <= CHUNK_LIMIT {
            break;
        }
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack7::pack_padded;

    fn message(words: &[u16]) -> Vec<u8> {
        let data: Vec<u8> = words.iter().flat_map(|w| w.to_be_bytes()).collect();
        let mut out = vec![0xF0, 0x01, 0x28, CMD_BEAT];
        out.extend(pack_padded(&data));
        out.push(0xF7);
        out
    }

    fn header(assigns: &[(usize, u16)]) -> Vec<u16> {
        let mut words = vec![0u16; HEADER_WORDS];
        words[0] = 10;
        for (pad, value) in assigns {
            words[PAD0_WORD + PAD_WORDS * pad + 1] = *value;
        }
        words
    }

    fn chunk(events: usize) -> Vec<u16> {
        let mut words = vec![(1 + 5 * events) as u16];
        words.extend((0..events * 5).map(|i| i as u16));
        words
    }

    #[test]
    fn test_summarize_when_header_and_one_chunk_returns_format_assign_and_events() {
        let mut words = header(&[(0, 3), (31, 6)]);
        words.extend(chunk(4));

        let summary = summarize(&message(&words)).unwrap();

        assert_eq!(summary.format_word, 10);
        assert_eq!(summary.voice_assign[0], 3);
        assert_eq!(summary.voice_assign[31], 6);
        assert_eq!(summary.voice_assign[1], 0);
        assert_eq!(summary.events, 4);
    }

    #[test]
    fn test_summarize_when_word_5_holds_drops_returns_100_minus_drop_and_100_above_100() {
        let mut words = header(&[]);
        for (pad, drop) in [(0, 25), (5, 100), (31, 1), (7, 101), (8, 0xFFFF)] {
            words[PAD0_WORD + PAD_WORDS * pad + 5] = drop;
        }
        words.extend(chunk(0));

        let summary = summarize(&message(&words)).unwrap();

        assert_eq!(summary.play_probability[0], 75);
        assert_eq!(summary.play_probability[5], 0);
        assert_eq!(summary.play_probability[31], 99);
        assert_eq!(summary.play_probability[7], 100);
        assert_eq!(summary.play_probability[8], 100);
        assert_eq!(summary.play_probability[1], 100);
    }

    #[test]
    fn test_with_play_probability_when_set_returns_word_5_only_changed_and_read_back() {
        let sounds: [Sound; PADS] = std::array::from_fn(|_| Sound::with_defaults());
        let data = synthetic_beat_data("P", &[(0, 1)], 3, &sounds);
        let mut wanted = [100u8; PADS];
        wanted[2] = 40;
        wanted[31] = 0;

        let out = with_play_probability(&data, &wanted).unwrap();

        let changed: Vec<usize> = (0..data.len()).filter(|i| data[*i] != out[*i]).collect();
        let word5 = |pad: usize| (PAD0_WORD + PAD_WORDS * pad + 5) * 2;
        assert_eq!(changed, vec![word5(2) + 1, word5(31) + 1]);
        assert_eq!(
            read_unpacked(&out).unwrap().summary.play_probability,
            wanted
        );
        assert!(with_play_probability(&data[..100], &wanted).is_err());
    }

    #[test]
    fn test_summarize_when_voice_assign_has_high_byte_returns_low_byte_only() {
        let words = header(&[(2, 0xAB05)]);

        assert_eq!(summarize(&message(&words)).unwrap().voice_assign[2], 5);
    }

    #[test]
    fn test_summarize_when_chunk_over_limit_returns_events_of_all_chunks() {
        let mut words = header(&[]);
        words.extend(chunk(401)); // 2006 words: more chunks follow
        words.extend(chunk(3));

        assert_eq!(summarize(&message(&words)).unwrap().events, 404);
    }

    #[test]
    fn test_summarize_when_no_chunk_returns_zero_events() {
        assert_eq!(summarize(&message(&header(&[]))).unwrap().events, 0);
    }

    #[test]
    fn test_summarize_when_header_truncated_returns_error() {
        let words = vec![0u16; 100];

        assert!(matches!(
            summarize(&message(&words)),
            Err(BeatError::TruncatedHeader(_))
        ));
    }

    #[test]
    fn test_summarize_when_chunk_size_not_one_plus_five_k_returns_bad_chunk() {
        let mut words = header(&[]);
        words.extend([7, 1, 2, 3, 4, 5, 6]);

        assert!(matches!(
            summarize(&message(&words)),
            Err(BeatError::BadChunk { count: 7, .. })
        ));
    }

    #[test]
    fn test_summarize_when_chunk_runs_past_the_end_returns_bad_chunk() {
        let mut words = header(&[]);
        words.extend([11, 1, 2]);

        assert!(matches!(
            summarize(&message(&words)),
            Err(BeatError::BadChunk { count: 11, .. })
        ));
    }

    #[test]
    fn test_summarize_when_name_words_hold_text_returns_trimmed_name() {
        let mut words = header(&[]);
        for (k, pair) in b"NoSpoon-1905        ".chunks(2).enumerate() {
            words[NAME_WORD + k] = u16::from(pair[0]) << 8 | u16::from(pair[1]);
        }

        assert_eq!(summarize(&message(&words)).unwrap().name, "NoSpoon-1905");
    }

    #[test]
    fn test_summarize_when_name_word_neighbours_change_returns_same_name() {
        let mut words = header(&[]);
        words[NAME_WORD - 1] = 0x4141;
        words[NAME_WORD + NAME_WORDS] = 0x4242;

        assert_eq!(summarize(&message(&words)).unwrap().name, "");
    }

    #[test]
    fn test_summarize_when_name_has_control_byte_returns_question_mark() {
        let mut words = header(&[]);
        words[NAME_WORD] = 0x4101;

        assert_eq!(summarize(&message(&words)).unwrap().name, "A?");
    }

    #[test]
    fn test_synthetic_dump_when_summarized_returns_what_was_asked() {
        let dump = synthetic_dump("Demo beat", &[(0, 2), (31, 6), (40, 1)], 7);

        let summary = summarize(&dump).unwrap();

        assert_eq!(summary.name, "Demo beat");
        assert_eq!((summary.voice_assign[0], summary.voice_assign[31]), (2, 6));
        assert_eq!(summary.events, 7);
    }

    #[test]
    fn test_summarize_when_not_a_beat_message_returns_error() {
        let mut msg = message(&header(&[]));
        msg[3] = 0x60;

        assert_eq!(summarize(&msg), Err(BeatError::NotBeatMessage));
    }

    #[test]
    fn test_standard_message_when_machine_dump_ends_in_every_partial_group_returns_the_standard_layout(
    ) {
        for tail in 0..=6 {
            let data = synthetic_tail_beat(tail);
            assert_eq!(data.len() % 7, tail);
            let dump = machine_dump(&data);

            let standard = standard_message(&dump).unwrap();

            let body = &standard[4..standard.len() - 1];
            assert_eq!(standard, standard_dump(&data), "tail {tail}");
            assert!(crate::pack7::is_standard_for(body, &data), "tail {tail}");
            // The machine's receiver rule reads back the MSBs of the last bytes.
            assert_eq!(unpack(body)[..data.len()], data[..], "tail {tail}");
        }
    }

    #[test]
    fn test_standard_message_when_already_standard_returns_it_unchanged() {
        for tail in 0..=6 {
            let standard = standard_dump(&synthetic_tail_beat(tail));

            assert_eq!(
                standard_message(&standard).unwrap(),
                standard,
                "tail {tail}"
            );
        }
    }

    #[test]
    fn test_standard_message_when_flushed_bytes_are_kept_returns_something_the_receiver_misreads() {
        // Counter-proof: the machine's dump itself fails the standard check for tails of 1 to 6.
        for tail in 1..=6 {
            let data = synthetic_tail_beat(tail);
            let dump = machine_dump(&data);
            let body = &dump[4..dump.len() - 1];

            assert!(!crate::pack7::is_standard_for(body, &data), "tail {tail}");
            if tail < 6 {
                assert_ne!(unpack(body)[..data.len()], data[..], "tail {tail}");
            }
        }
    }

    #[test]
    fn test_standard_message_when_not_a_complete_beat_returns_error() {
        let mut sound = standard_dump(&synthetic_tail_beat(2));
        sound[3] = 0x60;
        let mut cut = standard_dump(&synthetic_tail_beat(2));
        cut.drain(100..120);

        assert_eq!(standard_message(&sound), Err(BeatError::NotBeatMessage));
        assert!(standard_message(&cut).is_err());
        assert!(standard_message(&synthetic_dump("x", &[], 3)).is_err());
    }

    #[test]
    fn test_with_name_when_valid_returns_data_whose_summary_reads_the_new_name_and_nothing_else_changed(
    ) {
        let data = synthetic_beat_data(
            "Old name",
            &[(0, 1)],
            3,
            &std::array::from_fn(|_| Sound::with_defaults()),
        );

        let renamed = with_name(&data, "New").unwrap();

        let before = summarize(&standard_dump(&data)).unwrap();
        let after = summarize(&standard_dump(&renamed)).unwrap();
        assert_eq!(after.name, "New");
        assert_eq!(&renamed[NAME_BYTES], b"New                 ");
        assert_eq!(
            (after.events, after.voice_assign),
            (before.events, before.voice_assign)
        );
        let changed: Vec<usize> = (0..data.len())
            .filter(|i| data[*i] != renamed[*i])
            .collect();
        assert!(
            changed.iter().all(|i| NAME_BYTES.contains(i)),
            "{changed:?}"
        );
        assert_eq!(read_unpacked(&renamed).map(|d| d.sounds.len()), Ok(PADS));
    }

    #[test]
    fn test_with_name_when_too_long_or_not_printable_or_data_too_short_returns_none() {
        let data = vec![0u8; 100];

        assert!(with_name(&data, &"x".repeat(21)).is_none());
        assert!(with_name(&data, "caf\u{e9}").is_none());
        assert!(with_name(&data, "tab\there").is_none());
        assert!(with_name(&data[..43], "ok").is_none());
        assert!(with_name(&data, &"x".repeat(20)).is_some());
    }

    fn beat_with_sounds() -> (Vec<u8>, [Sound; PADS]) {
        let sounds: [Sound; PADS] =
            std::array::from_fn(|pad| Sound::from_bytes([0x11 + pad as u8; SOUND_BYTES]));
        (synthetic_beat_data("Kit", &[(0, 1)], 7, &sounds), sounds)
    }

    #[test]
    fn test_with_sounds_when_two_pads_replaced_returns_only_those_128_byte_ranges_changed() {
        let (data, _) = beat_with_sounds();
        let mut new: [Option<Sound>; PADS] = std::array::from_fn(|_| None);
        new[3] = Some(Sound::from_bytes([0xEE; SOUND_BYTES]));
        new[31] = Some(Sound::from_bytes([0xDD; SOUND_BYTES]));

        let out = with_sounds(&data, &new).unwrap();

        let first = data.len() - PADS * SOUND_BYTES;
        let changed: Vec<usize> = (0..data.len()).filter(|i| out[*i] != data[*i]).collect();
        assert_eq!(out.len(), data.len());
        assert!(changed.iter().all(|i| {
            let pad = (i - first) / SOUND_BYTES;
            *i >= first && (pad == 3 || pad == 31)
        }));
        assert_eq!(changed.len(), 2 * SOUND_BYTES);
        assert_eq!(out[..first], data[..first], "header and events untouched");
        let read = read_unpacked(&out).unwrap();
        assert_eq!(read.sounds[3].as_ref().map(|s| s.bytes()[0]), Some(0xEE));
        assert_eq!(read.sounds[4].as_ref().map(|s| s.bytes()[0]), Some(0x15));
    }

    #[test]
    fn test_with_sounds_when_nothing_replaced_returns_the_same_bytes() {
        let (data, _) = beat_with_sounds();

        let out = with_sounds(&data, &std::array::from_fn(|_| None)).unwrap();

        assert_eq!(out, data);
    }

    #[test]
    fn test_with_sounds_when_pad_has_no_slot_in_this_version_returns_error() {
        // Counter-proof: a version 5 beat holds 16 sounds; a sound for pad 17 must be refused, not dropped.
        let sounds: [Sound; PADS] = std::array::from_fn(|_| Sound::from_bytes([1; SOUND_BYTES]));
        let mut data = synthetic_beat_data("Old", &[], 0, &sounds);
        data.truncate(data.len() - 16 * SOUND_BYTES);
        data[1] = 5;
        let mut new: [Option<Sound>; PADS] = std::array::from_fn(|_| None);
        new[16] = Some(Sound::from_bytes([9; SOUND_BYTES]));

        assert_eq!(
            with_sounds(&data, &new),
            Err(BeatError::NoSoundSlot { pad: 16 })
        );
    }

    #[test]
    fn test_with_sounds_when_data_has_trailing_bytes_returns_error() {
        let (mut data, _) = beat_with_sounds();
        data.push(0);

        assert_eq!(
            with_sounds(&data, &std::array::from_fn(|_| None)),
            Err(BeatError::TrailingData(1))
        );
    }
}
