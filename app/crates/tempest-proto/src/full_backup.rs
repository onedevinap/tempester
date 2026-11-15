//! The full backup of Tempester 1.6.6.0 (n°62, contract `public-src/docs/midi-protocol.md` § Full backup): the request
//! `F0 01 28 44 F7` and its eight answers, one SysEx per chip, in this order: `78`-`7D` the six Voice flashes (64
//! blocks, the layout of an OS file), `75` the Main boot flash and configuration words (12 blocks), `76` the System
//! settings and the 32 pad channels of n°58 (235 bytes, standard 7-bit packing).
//!
//! These are backups for a programmer (PICkit) restore. Nothing here builds a message to send to the machine, and the
//! ids are outside the ones the machine's receiver knows: a backup file can never start an update.
//!
//! Every message is checked as it arrives (`decode`), and the eight together (`check_set`). A message that fails any
//! check is refused with the reason: a Voice dump that is misaligned (the Voice's dump pointer is never reset, so the
//! second dump of a session is shifted), constant, without its boot page or with a blank calibration page is not a
//! backup, nor is an answer whose messages come out of order. From Tempester 1.6.8.0 a chip the machine cannot read
//! again from its start in this power-on (DSI's Voice OS, already read) answers the empty `F0 01 28 78+k F7`:
//! refused as « not read », with the advice to power the machine off and on.

use std::fmt;

use crate::os_image::{decode_body, encode_with_id, OsImageError, BLOCK_SIZE};
use crate::pack7::{is_standard_for, pack_padded, unpack};
use crate::pad_channel::{PADS, VALUE_MAX};
use crate::sha256::sha256_hex;
use crate::voice_versions::{self, VoiceVersion};
use crate::DSI_PREFIX;

/// The request: the machine answers with the eight messages, in the order of `Chip::all`.
pub const REQUEST_FULL_BACKUP: [u8; 5] = [0xF0, 0x01, 0x28, 0x44, 0xF7];
/// Voice chips of the machine.
pub const VOICES: usize = 6;
/// Messages of a complete answer.
pub const MESSAGES: usize = VOICES + 2;

const ID_VOICE_FIRST: u8 = 0x78;
const ID_MAIN_BOOT: u8 = 0x75;
const ID_SYSTEM: u8 = 0x76;
const EOX: u8 = 0xF7;

/// Bytes of a Voice image: 21,845 instructions of 3 bytes plus one.
pub const VOICE_IMAGE_BYTES: usize = 64 * BLOCK_SIZE;
/// Bytes of one flash page (512 instructions of 3 bytes).
pub const PAGE_BYTES: usize = 512 * 3;
/// Pages 0-40: what an OS update writes (refs/firmware-voice.md §5.1).
pub const CODE_PAGES: usize = 41;
/// Pages that hold code in the DSI image; pages 27-40 are free space, which may legitimately be blank.
const USED_CODE_PAGES: usize = 27;
const BOOT_PAGE_START: usize = CODE_PAGES * PAGE_BYTES;
const CALIBRATION_START: usize = BOOT_PAGE_START + PAGE_BYTES;
/// Words of the calibration page (refs/firmware-voice.md §7.1).
pub const CALIBRATION_WORDS: usize = 35;
/// The largest spread of the steps of calibration table C ("equal steps within +-21").
const LINEAR_SPREAD: i32 = 42;

const MAIN_BOOT_BLOCKS: usize = 12;
/// The four configuration words end the Main boot flash: DEVCFG3, DEVCFG2, DEVCFG1, DEVCFG0.
const DEVCFG_BYTES: usize = 16;
/// Version of the System settings file the machine writes, byte 0 of `76`.
const SYSTEM_VERSION: u8 = 7;
/// The System settings, bytes 1-202 of `76`.
pub const SYSTEM_SETTINGS: usize = 202;
/// The pad channels of n°58, bytes 203-234 of `76`.
pub const PAD_CHANNELS: usize = PADS as usize;
/// The data of `76`: the version byte, the settings, the pad channels.
pub const SYSTEM_PAYLOAD: usize = 1 + SYSTEM_SETTINGS + PAD_CHANNELS;
/// Settings 48-79 are the *MIDI Note: Pad* entries: the file keeps pad `i - 48`'s channel in their high byte.
const PAD_NOTE_SETTINGS: std::ops::Range<usize> = 48..80;
/// Settings 138-201 are stored sign-extended by the machine's save (loaded with `lb`).
const SIGN_EXTENDED_FROM: usize = 138;
/// Words of the System settings file: the version word, then one word per setting.
pub const SYSTEM_FILE_WORDS: usize = 1 + SYSTEM_SETTINGS;

/// One of the eight things the machine answers with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Chip {
    /// Voice chip 0-5.
    Voice(u8),
    MainBoot,
    System,
}

impl Chip {
    /// The eight chips, in the order the machine sends them.
    pub fn all() -> [Chip; MESSAGES] {
        [
            Chip::Voice(0),
            Chip::Voice(1),
            Chip::Voice(2),
            Chip::Voice(3),
            Chip::Voice(4),
            Chip::Voice(5),
            Chip::MainBoot,
            Chip::System,
        ]
    }

    pub fn from_id(id: u8) -> Option<Chip> {
        match id {
            ID_MAIN_BOOT => Some(Chip::MainBoot),
            ID_SYSTEM => Some(Chip::System),
            ID_VOICE_FIRST..=0x7D => Some(Chip::Voice(id - ID_VOICE_FIRST)),
            _ => None,
        }
    }

    /// The command byte of the chip's message.
    pub fn id(self) -> u8 {
        match self {
            Chip::Voice(k) => ID_VOICE_FIRST + k,
            Chip::MainBoot => ID_MAIN_BOOT,
            Chip::System => ID_SYSTEM,
        }
    }

    /// The name the page and the files use.
    pub fn label(self) -> String {
        match self {
            Chip::Voice(k) => format!("Voice {k}"),
            Chip::MainBoot => "Main boot flash".to_string(),
            Chip::System => "System settings".to_string(),
        }
    }

    fn blocks(self) -> Option<usize> {
        match self {
            Chip::Voice(_) => Some(VOICE_IMAGE_BYTES / BLOCK_SIZE),
            Chip::MainBoot => Some(MAIN_BOOT_BLOCKS),
            Chip::System => None,
        }
    }
}

/// Why a message or a set of messages is not a usable backup; each says what is wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupError {
    /// Not `F0 01 28`, a known id and `F7`.
    NotBackupMessage,
    /// The framing, the blocks or the checksum are wrong.
    Image(OsImageError),
    BlockCount {
        expected: usize,
        found: usize,
    },
    /// Every byte of the image is the same.
    Constant(u8),
    /// A Voice image that does not start with the reset `goto`: read from the wrong place of the chip's stream.
    Misaligned,
    /// The empty message `F0 01 28 78+k F7` of Tempester 1.6.8.0: chip `k` was already read since power-on and cannot be
    /// brought back to its first instruction without reading past the end of its flash (DSI's Voice OS, or a chip that did
    /// not answer at start-up), so it was
    /// not read at all.
    NotRead(u8),
    /// A code page (0-26) of a Voice image is blank or constant.
    CodePage(usize),
    BootPageMissing,
    Calibration(CalibrationError),
    /// The Main boot flash ends with configuration words that are all erased or all zero.
    NoConfigBits,
    System(String),
    /// A chip whose message did not come.
    Missing(Chip),
    /// Two chips gave the same image: the chip index was ignored, or one chip was read twice.
    SameImage {
        first: u8,
        second: u8,
    },
    /// The message came at `position` (1-8, or later for a message beyond the eight) where the answer has `expected`
    /// (`None`: the answer was already complete).
    OutOfOrder {
        position: usize,
        expected: Option<Chip>,
    },
}

impl fmt::Display for BackupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotBackupMessage => write!(f, "not a full-backup message"),
            Self::Image(e) => write!(f, "refused: {e}"),
            Self::BlockCount { expected, found } => {
                write!(f, "{found} blocks, expected {expected}")
            }
            Self::Constant(b) => write!(f, "every byte is {b:#04x}: the machine sent a constant, not the chip's content"),
            Self::Misaligned => write!(f, "misaligned dump (it does not start at the chip's first instruction): power the machine off and on, then ask again"),
            Self::NotRead(k) => write!(f, "Voice {k} was not read: its chip was already read since the machine was switched on and did not answer as Tempester Voice at start-up (DSI's Voice OS, or no answer), so it cannot be read again from its start without stopping (the machine refused); power the machine off and on, then ask again"),
            Self::CodePage(p) => write!(f, "code page {p} is blank or constant: the dump is incomplete"),
            Self::BootPageMissing => write!(f, "the boot page (page 41) is blank or constant: the dump is incomplete"),
            Self::Calibration(e) => write!(f, "calibration page: {e}"),
            Self::NoConfigBits => write!(f, "the configuration words at the end are all erased or all zero: not read from the chip"),
            Self::System(why) => write!(f, "System settings: {why}"),
            Self::Missing(chip) => write!(f, "{} did not come", chip.label()),
            Self::SameImage { first, second } => write!(f, "identical to Voice {first}: Voice {second} was not read from its own chip"),
            Self::OutOfOrder { position, expected: Some(chip) } => write!(f, "message {position} of the answer, where the machine sends {}: not the answer of request 44", chip.label()),
            Self::OutOfOrder { position, expected: None } => write!(f, "message {position} of the answer, after the eight: not the answer of request 44"),
        }
    }
}

impl std::error::Error for BackupError {}

impl From<OsImageError> for BackupError {
    fn from(error: OsImageError) -> Self {
        Self::Image(error)
    }
}

/// Why a calibration page was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalibrationError {
    /// The 35 words are all `0xFFFFFF`: the page was never written (or was not read).
    Blank,
    /// A word whose upper byte is not `0xFF`: not a calibration page.
    UpperByte {
        word: usize,
    },
    /// The chip's own test (`cal_load`) would not accept it: A[1] or B[0] is `0xFFFF`.
    HalfWritten,
    NotIncreasing(&'static str),
    NotLinear,
}

impl fmt::Display for CalibrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Blank => write!(
                f,
                "blank (35 words all 0xFFFFFF): nothing was read, or the chip was never calibrated"
            ),
            Self::UpperByte { word } => write!(
                f,
                "word {word} does not have the upper byte 0xFF of a calibration word"
            ),
            Self::HalfWritten => write!(f, "half written (the chip itself would use its defaults)"),
            Self::NotIncreasing(table) => write!(f, "table {table} does not increase"),
            Self::NotLinear => write!(f, "table C is not linear"),
        }
    }
}

/// The 35 calibration words of a Voice chip (refs/firmware-voice.md §7.1; the 16-bit values, the upper byte is 0xFF).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Calibration {
    pub words: [u16; CALIBRATION_WORDS],
}

impl Calibration {
    /// Table A, oscillator 1: 6 + 4 values, each run increasing.
    pub fn osc1(&self) -> &[u16] {
        &self.words[0..10]
    }
    /// Table B, oscillator 2.
    pub fn osc2(&self) -> &[u16] {
        &self.words[10..20]
    }
    /// Table C: 12 near-linear values.
    pub fn linear(&self) -> &[u16] {
        &self.words[20..32]
    }
    /// The three scalars.
    pub fn scalars(&self) -> &[u16] {
        &self.words[32..35]
    }
}

fn increasing(values: &[u16]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

/// Reads the calibration page of a Voice image (at `0xFC00`, as `tools/voice_compare.py --calibration` does).
pub fn read_calibration(image: &[u8]) -> Result<Calibration, CalibrationError> {
    let raw = image
        .get(CALIBRATION_START..CALIBRATION_START + 3 * CALIBRATION_WORDS)
        .ok_or(CalibrationError::Blank)?;
    if raw.iter().all(|b| *b == 0xFF) {
        return Err(CalibrationError::Blank);
    }
    let mut words = [0u16; CALIBRATION_WORDS];
    for (at, (word, bytes)) in words.iter_mut().zip(raw.chunks(3)).enumerate() {
        if bytes[2] != 0xFF {
            return Err(CalibrationError::UpperByte { word: at });
        }
        *word = u16::from(bytes[0]) | u16::from(bytes[1]) << 8;
    }
    let calibration = Calibration { words };
    check_tables(&calibration)?;
    Ok(calibration)
}

fn check_tables(c: &Calibration) -> Result<(), CalibrationError> {
    if c.osc1()[1] == 0xFFFF || c.osc2()[0] == 0xFFFF {
        return Err(CalibrationError::HalfWritten);
    }
    for (name, table) in [("A", c.osc1()), ("B", c.osc2())] {
        if !(increasing(&table[..6]) && increasing(&table[6..])) {
            return Err(CalibrationError::NotIncreasing(name));
        }
    }
    let steps: Vec<i32> = c
        .linear()
        .windows(2)
        .map(|pair| i32::from(pair[0]) - i32::from(pair[1]))
        .collect();
    let (low, high) = steps
        .iter()
        .fold((i32::MAX, i32::MIN), |(l, h), s| (l.min(*s), h.max(*s)));
    let one_direction = low > 0 || high < 0;
    if !one_direction || high - low > LINEAR_SPREAD {
        return Err(CalibrationError::NotLinear);
    }
    Ok(())
}

/// The Main boot flash and its configuration words (little-endian words, `0xBFC00000`...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootFlash {
    /// The word at the reset vector.
    pub reset_word: u32,
    /// DEVCFG3, DEVCFG2, DEVCFG1, DEVCFG0.
    pub devcfg: [u32; 4],
}

/// The System settings as `76` carries them: the version byte, the 202 settings, the 32 pad channels (n°58: 0 the
/// stock route, 1-16 the channel with the played note, 17-32 channel - 16 with the pad's *MIDI Note: Pad*).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemSettings {
    pub version: u8,
    pub settings: Vec<u8>,
    pub pad_channels: [u8; PAD_CHANNELS],
}

impl SystemSettings {
    /// The System settings file the machine's own save writes (406 bytes, 203 little-endian words): word 0 = the
    /// version; word 1 + i = setting i in the low byte and, in the high byte, the channel of pad i - 48 for the
    /// *MIDI Note: Pad* settings 48-79, `FF` for settings 138-201 of 0x80 or above (stored sign-extended), else 0.
    pub fn machine_file(&self) -> Vec<u8> {
        let high = |i: usize, value: u8| {
            if PAD_NOTE_SETTINGS.contains(&i) {
                self.pad_channels[i - PAD_NOTE_SETTINGS.start]
            } else if i >= SIGN_EXTENDED_FROM && value >= 0x80 {
                0xFF
            } else {
                0
            }
        };
        let mut file = Vec::with_capacity(2 * SYSTEM_FILE_WORDS);
        file.extend([self.version, 0]);
        for (i, value) in self.settings.iter().enumerate() {
            file.extend([*value, high(i, *value)]);
        }
        file
    }
}

/// What a chip's message holds, checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    Voice(Calibration),
    MainBoot(BootFlash),
    System(SystemSettings),
}

/// A decoded, checked message of the full backup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backup {
    pub chip: Chip,
    /// The decoded image (the settings payload for the System message).
    pub image: Vec<u8>,
    pub content: Content,
}

/// Decodes and checks one message of the answer.
pub fn decode(message: &[u8]) -> Result<Backup, BackupError> {
    let chip = frame_chip(message)?;
    match chip {
        Chip::System => decode_system(message),
        _ => decode_image(message, chip),
    }
}

fn frame_chip(message: &[u8]) -> Result<Chip, BackupError> {
    let framed =
        message.len() >= 5 && message[..3] == DSI_PREFIX && message[message.len() - 1] == EOX;
    framed
        .then(|| Chip::from_id(message[3]))
        .flatten()
        .ok_or(BackupError::NotBackupMessage)
}

fn decode_image(message: &[u8], chip: Chip) -> Result<Backup, BackupError> {
    if let (Chip::Voice(k), [_, _, _, _, EOX]) = (chip, message) {
        return Err(BackupError::NotRead(k));
    }
    let expected = chip.blocks().unwrap_or_default();
    let (image, _) = decode_body(message, |found| {
        if found == expected {
            Ok(())
        } else {
            Err(BackupError::BlockCount { expected, found })
        }
    })?;
    if let Some(&byte) = image.first().filter(|b| image.iter().all(|x| x == *b)) {
        return Err(BackupError::Constant(byte));
    }
    let content = match chip {
        Chip::Voice(_) => Content::Voice(check_voice(&image)?),
        _ => Content::MainBoot(check_boot_flash(&image)?),
    };
    Ok(Backup {
        chip,
        image,
        content,
    })
}

fn check_voice(image: &[u8]) -> Result<Calibration, BackupError> {
    if image.len() != VOICE_IMAGE_BYTES || !is_aligned(image) {
        return Err(BackupError::Misaligned);
    }
    for page in 0..USED_CODE_PAGES {
        if is_flat(&image[page * PAGE_BYTES..(page + 1) * PAGE_BYTES]) {
            return Err(BackupError::CodePage(page));
        }
    }
    if is_flat(&image[BOOT_PAGE_START..CALIBRATION_START]) {
        return Err(BackupError::BootPageMissing);
    }
    read_calibration(image).map_err(BackupError::Calibration)
}

/// The reset vector of a dsPIC33 is a `goto` (opcode `0x04` in the third byte of the instruction) followed by its
/// second word, whose three bytes are zero for an address below `0x10000`: a stream read from a shifted position
/// does not look like that.
fn is_aligned(image: &[u8]) -> bool {
    image.get(2) == Some(&0x04) && image.get(3..6) == Some(&[0, 0, 0])
}

fn is_flat(bytes: &[u8]) -> bool {
    bytes.first().is_none_or(|b| bytes.iter().all(|x| x == b))
}

fn check_boot_flash(image: &[u8]) -> Result<BootFlash, BackupError> {
    let word = |at: usize| {
        image
            .get(at..at + 4)
            .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let config = &image[image.len() - DEVCFG_BYTES..];
    if is_flat(config) {
        return Err(BackupError::NoConfigBits);
    }
    let tail = image.len() - DEVCFG_BYTES;
    Ok(BootFlash {
        reset_word: word(0),
        devcfg: [word(tail), word(tail + 4), word(tail + 8), word(tail + 12)],
    })
}

/// `76`: whole groups of 8 bytes in the standard 7-bit packing (head bit j for byte j) holding exactly the 235 bytes
/// of the payload, the last group completed with zeros.
fn decode_system(message: &[u8]) -> Result<Backup, BackupError> {
    let body = &message[4..message.len() - 1];
    let data = unpack(body);
    let payload = data.get(..SYSTEM_PAYLOAD).unwrap_or(&data);
    if payload.len() != SYSTEM_PAYLOAD || !is_standard_for(body, payload) {
        return Err(BackupError::System(format!(
            "{} packed bytes, expected the version byte, {SYSTEM_SETTINGS} settings and {PAD_CHANNELS} pad channels \
             ({SYSTEM_PAYLOAD} bytes) in the standard 7-bit packing with zero padding",
            body.len()
        )));
    }
    if payload[0] != SYSTEM_VERSION {
        return Err(BackupError::System(format!(
            "settings version {}, this app reads version {SYSTEM_VERSION}",
            payload[0]
        )));
    }
    let mut pad_channels = [0u8; PAD_CHANNELS];
    pad_channels.copy_from_slice(&payload[1 + SYSTEM_SETTINGS..]);
    if let Some(pad) = pad_channels.iter().position(|c| *c > VALUE_MAX) {
        return Err(BackupError::System(format!(
            "pad {} has the channel value {}, above {VALUE_MAX}",
            pad + 1,
            pad_channels[pad]
        )));
    }
    Ok(Backup {
        chip: Chip::System,
        image: payload.to_vec(),
        content: Content::System(SystemSettings {
            version: payload[0],
            settings: payload[1..=SYSTEM_SETTINGS].to_vec(),
            pad_channels,
        }),
    })
}

/// A problem of the set as a whole, tied to the chip it is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetProblem {
    pub chip: Chip,
    pub error: BackupError,
}

/// The checks across the eight: every chip is there, and no two Voice images are the same (each chip has its own
/// calibration, so identical images mean one chip was read twice).
pub fn check_set(items: &[Backup]) -> Vec<SetProblem> {
    let mut problems: Vec<SetProblem> = Chip::all()
        .into_iter()
        .filter(|chip| items.iter().all(|item| item.chip != *chip))
        .map(|chip| SetProblem {
            chip,
            error: BackupError::Missing(chip),
        })
        .collect();
    let voices: Vec<(u8, &Backup)> = items
        .iter()
        .filter_map(|item| match item.chip {
            Chip::Voice(k) => Some((k, item)),
            _ => None,
        })
        .collect();
    for (at, (second, item)) in voices.iter().enumerate() {
        if let Some((first, _)) = voices[..at]
            .iter()
            .find(|(_, other)| other.image == item.image)
        {
            problems.push(SetProblem {
                chip: item.chip,
                error: BackupError::SameImage {
                    first: *first,
                    second: *second,
                },
            });
        }
    }
    problems
}

/// The order of the answer: the machine sends `Chip::all()` one after the other, so a message `arrived` at another
/// place (a chip twice, one missing in the middle, a stray message beyond the eight) is not its answer to `44`.
pub fn check_order(arrived: &[Chip]) -> Vec<SetProblem> {
    let expected = Chip::all();
    arrived
        .iter()
        .enumerate()
        .filter(|(at, chip)| expected.get(*at) != Some(*chip))
        .map(|(at, chip)| SetProblem {
            chip: *chip,
            error: BackupError::OutOfOrder {
                position: at + 1,
                expected: expected.get(at).copied(),
            },
        })
        .collect()
}

/// Bytes of pages 0-40 in which Voice `k` differs from Voice 0 (the chips run the same code): not a refusal, a note
/// for the report (a chip that kept an older build would show here).
pub fn code_differences(first: &Backup, other: &Backup) -> usize {
    first.image[..BOOT_PAGE_START]
        .iter()
        .zip(&other.image[..BOOT_PAGE_START])
        .filter(|(a, b)| a != b)
        .count()
}

/// Which Voice OS a chip's pages 0-40 hold, as far as the app's tables say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceBuild {
    /// The pages equal the image of this Tempester Voice version (`voice_versions`).
    Tempester(&'static str),
    /// DSI's Voice 1.5: only known when the user gave the app his own DSI file (its pages 0-40 are hashed then; the
    /// app holds no DSI byte and no hash of one).
    Dsi15,
    /// Any other content: a build the app does not know.
    Unknown,
}

impl VoiceBuild {
    /// The name the page, the manifest and the report use.
    pub fn label(self) -> String {
        match self {
            Self::Tempester(version) => format!("Tempester Voice {version}"),
            Self::Dsi15 => "DSI Voice 1.5".to_string(),
            Self::Unknown => "unknown build".to_string(),
        }
    }
}

/// Pages 0-40 of a Voice image: what an OS update writes.
pub fn code_pages(image: &[u8]) -> &[u8] {
    &image[..image.len().min(BOOT_PAGE_START)]
}

/// Page 42 of a Voice image: the chip's own calibration, which an update never writes. The 64-block image ends before
/// the page does (`VOICE_IMAGE_BYTES` is 1,024 bytes past its start): what the dump holds of it.
pub fn calibration_page(image: &[u8]) -> &[u8] {
    let page = image.get(CALIBRATION_START..).unwrap_or_default();
    &page[..page.len().min(PAGE_BYTES)]
}

/// Recognises the Voice OS of a chip's image by the SHA-256 of its pages 0-40. `dsi_pages` is the SHA-256 of pages 0-40
/// of the user's DSI Voice 1.5 file when the app has seen it.
pub fn voice_build(image: &[u8], dsi_pages: Option<&str>) -> VoiceBuild {
    voice_build_among(image, voice_versions::voice_versions(), dsi_pages)
}

/// `voice_build` against an explicit list of versions (the tests give their own: the real images are built from the
/// user's DSI file, which the tests never hold).
pub fn voice_build_among(
    image: &[u8],
    versions: &[VoiceVersion],
    dsi_pages: Option<&str>,
) -> VoiceBuild {
    let sha256 = sha256_hex(code_pages(image));
    if let Some(v) = versions.iter().find(|v| v.sha256_image_written == sha256) {
        return VoiceBuild::Tempester(v.version);
    }
    if dsi_pages == Some(sha256.as_str()) {
        return VoiceBuild::Dsi15;
    }
    VoiceBuild::Unknown
}

/// Synthetic answers for the fake machine and the tests: patterned, never the bytes of a DSI file.
pub mod synthetic {
    use super::*;

    /// A believable Voice image of chip `k`: code pages and a boot page that are the same for every chip, a
    /// calibration page of its own.
    pub fn voice_image(k: u8) -> Vec<u8> {
        let mut image: Vec<u8> = (0..VOICE_IMAGE_BYTES)
            .map(|i| ((i.wrapping_mul(131) ^ (i >> 5) ^ (i / PAGE_BYTES * 17)) as u8) | 1)
            .collect();
        // A reset `goto 0x000200`: the shape of a real reset vector, not the address of DSI's code.
        image[..6].copy_from_slice(&[0x00, 0x02, 0x04, 0, 0, 0]);
        let words = calibration_words(k);
        for (at, word) in words.iter().enumerate() {
            let pos = CALIBRATION_START + 3 * at;
            image[pos..pos + 3].copy_from_slice(&[(*word & 0xFF) as u8, (*word >> 8) as u8, 0xFF]);
        }
        image[CALIBRATION_START + 3 * CALIBRATION_WORDS..].fill(0xFF);
        image
    }

    /// The 35 words of chip `k`: two increasing runs for A and B, a linear C, three scalars.
    pub fn calibration_words(k: u8) -> [u16; CALIBRATION_WORDS] {
        let base = 0x8000 + u16::from(k) * 0x40;
        let mut words = [0u16; CALIBRATION_WORDS];
        for table in [0, 10] {
            for i in 0..10 {
                let run = if i < 6 { i } else { i - 6 + 8 };
                words[table + i] = base + table as u16 * 3 + run as u16 * 0x180;
            }
        }
        for i in 0..12 {
            words[20 + i] = 0xD000 - u16::from(k) * 0x30 - i as u16 * 0xC80;
        }
        words[32..35].copy_from_slice(&[0x0344 + u16::from(k), 0x7EA3, 0x7B90]);
        words
    }

    /// A believable Main boot flash: a patterned 12 KB whose last 16 bytes are configuration words.
    pub fn boot_flash_image() -> Vec<u8> {
        let mut image: Vec<u8> = (0..MAIN_BOOT_BLOCKS * BLOCK_SIZE)
            .map(|i| (i.wrapping_mul(29) ^ (i >> 3)) as u8)
            .collect();
        let tail = image.len() - DEVCFG_BYTES;
        for (at, word) in [0xFFFF_FFF7u32, 0xFFF9_B11B, 0xFF7F_F8D9, 0x7FFF_FFDF]
            .iter()
            .enumerate()
        {
            image[tail + 4 * at..tail + 4 * at + 4].copy_from_slice(&word.to_le_bytes());
        }
        image
    }

    /// The 32 pad channels of a machine whose pads all use the stock route.
    pub const STOCK_ROUTE: [u8; PAD_CHANNELS] = [0; PAD_CHANNELS];

    /// The payload of `76`: version 7, 202 patterned settings (some of 138-201 at 0x80 or above, as the machine
    /// keeps negative values there), then `pad_channels`.
    pub fn system_payload(pad_channels: &[u8; PAD_CHANNELS]) -> Vec<u8> {
        let mut data = vec![SYSTEM_VERSION];
        data.extend((0..SYSTEM_SETTINGS).map(|i| (i * 37 % 256) as u8));
        data.extend(pad_channels);
        data
    }

    /// The message of `chip` for `image` (the payload of `76` for the System message, packed as the machine does).
    pub fn message(chip: Chip, image: &[u8]) -> Vec<u8> {
        if chip == Chip::System {
            let mut out = DSI_PREFIX.to_vec();
            out.push(chip.id());
            out.extend(pack_padded(image));
            out.push(EOX);
            return out;
        }
        // The images built here are whole blocks: a failure is a bug of the builder itself.
        encode_with_id(chip.id(), image).unwrap_or_default()
    }

    /// The eight messages of a healthy machine whose pads use the stock route, in the order of the answer.
    pub fn answer() -> Vec<Vec<u8>> {
        answer_with(&STOCK_ROUTE)
    }

    /// The eight messages of a healthy machine whose pads have `pad_channels`, in the order of the answer.
    pub fn answer_with(pad_channels: &[u8; PAD_CHANNELS]) -> Vec<Vec<u8>> {
        Chip::all()
            .into_iter()
            .map(|chip| match chip {
                Chip::Voice(k) => message(chip, &voice_image(k)),
                Chip::MainBoot => message(chip, &boot_flash_image()),
                Chip::System => message(chip, &system_payload(pad_channels)),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
