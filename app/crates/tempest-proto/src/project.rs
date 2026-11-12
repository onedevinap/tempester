//! The machine's "Send Project" (established from the firmware): one `5E` message holding the
//! project parameters and the Play Lists, then one `5C n` message per beat (`F0 01 28 5C <n> <packed beat> F7`).
//! The packed beat is exactly the body of a `5F` beat message: the beat sender ignores its third argument, the only
//! thing the project send changes.
//!
//! `5E` data (16-bit words, high byte first): 44 parameter words (the first is the version, 5 as sent by 1.5.x),
//! then, from version 4, the Play List version (2) and four lists, each `0xFEFE` when empty or one End word, 10 name
//! words and 100 words of `beat << 8 | repeats` (111 words). The sender and the project file loader agree on that
//! size; the firmware's project receiver reads one word more per list, so it would not take a project with a
//! non-empty list (observed in the decompilation, not tried on a machine).
//!
//! Packing: up to Tempester 1.6.1.0 the machine's packer ends each message with a partial last group laid out by its
//! length (head bits shifted, stale bytes as padding: `pack7::pack_flushed`, `refs/firmware-main.md` 9.1); from
//! Tempester 1.6.1.2 (n°27) it ends it in the standard layout (`pack7::pack_padded`). The length is not in the stream,
//! so each message is read with `pack7::unpack_tolerant_with`: every length the last group allows is tried in both
//! layouts, the one the parser consumes exactly is kept (`read_header_exact`, `beat::read_unpacked`), and readings
//! that disagree are told apart by the one that packs back to the stream byte for byte. Proved on the images
//! (firmware simulations, not in the public tree, and `tests/fixed_export.rs`) and on a real capture
//! (`tests/real_capture.rs`).
//!
//! What the app writes and sends is not what the machine sent: every message goes back out in the standard layout the
//! machine's receivers read (`pack7::pack_padded`), re-encoded from the exactly decoded data. `Project::raw` keeps the
//! messages as received, as evidence of the transfer; `Project::standard` is the file meant to be sent back.
//!
//! Sending a project back (`Project::send_messages`, `Project::with_empty_play_lists`) and checking what the machine
//! holds afterwards (`compare`): the messages go out in the standard layout, `5E` first, then `5C n` in increasing
//! order, and a project read back from the machine is compared per parameter word, Play List and beat.
//!
//! The beats are sent only when their presence flag is set (one per beat, `refs/firmware-main.md` 9.6). The
//! parameter loader sets all 16 and nothing clears them, so a project sent after a load has 16 beats; before any load
//! the flags at power-up are not proved. A project may therefore come with fewer than 16 beats, possibly with gaps
//! (0, 2, 5, 15): the assembler takes beats in strictly increasing order and `Project::absent_beats` says which did
//! not come. A project with absent beats must never pass for a complete one.

use std::fmt;

use crate::beat::{self, BeatDump, BeatError};
use crate::pack7::{pack_flushed, pack_padded, unpack_tolerant_with, FlushedError, PackError};
use crate::quantize::{Quantize, FINE_WORD, QUANTIZE_WORD};
use crate::DSI_PREFIX;

pub const CMD_PROJECT_HEADER: u8 = 0x5E;
pub const CMD_PROJECT_BEAT: u8 = 0x5C;
/// Beats of a project.
pub const BEATS: usize = 16;

const PARAM_WORDS: usize = 44;
const LAST_PARAMS_VERSION: u16 = 5;
const FIRST_VERSION_WITH_LISTS: u16 = 4;
const LISTS: usize = 4;
const EMPTY_LIST: u16 = 0xFEFE;
const LIST_VERSION: u16 = 2;
/// Words after the End word of a non-empty Play List: 10 of name, 100 of steps.
const LIST_BODY_WORDS: usize = 110;
/// Name words that follow the End word.
const LIST_NAME_WORDS: usize = 10;
/// Play Lists of a project.
pub const PLAY_LISTS: usize = LISTS;

mod compare;
pub use compare::{compare, Comparison, Difference};

/// Why a message of a project was refused; each says what is wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectError {
    /// Not `F0 01 28 .. F7`, or too short.
    Malformed,
    Packing(PackError),
    /// Two lengths of the last, partial group read as valid but differently.
    AmbiguousTail,
    /// The header message `5E` arrived twice.
    HeaderTwice,
    /// A beat `5C` came before the header `5E`.
    BeatBeforeHeader {
        beat: u8,
    },
    /// The beat number is not 0 to 15.
    BeatNumber(u8),
    /// Beats come in strictly increasing order: `got` is not above the beat `after` received just before.
    BeatOutOfOrder {
        got: u8,
        after: u8,
    },
    HeaderVersion(u16),
    HeaderTruncated,
    PlayListVersion(u16),
    HeaderTrailing(usize),
    Beat {
        index: u8,
        error: BeatError,
    },
    /// The project ended (or the wait did) without its header.
    NoHeader,
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed => write!(f, "a message is not a complete DSI SysEx"),
            Self::Packing(e) => write!(f, "bad 7-bit packing: {e}"),
            Self::AmbiguousTail => write!(f, "the end of the message reads two valid ways"),
            Self::HeaderTwice => write!(f, "the project header (5E) arrived twice"),
            Self::BeatBeforeHeader { beat } => write!(
                f,
                "beat {} arrived before the project header (5E)",
                beat + 1
            ),
            Self::BeatNumber(n) => write!(f, "beat number {n} is not 0 to 15"),
            Self::BeatOutOfOrder { got, after } => write!(
                f,
                "beat {} arrived after beat {}: beats come once each, in increasing order",
                got + 1,
                after + 1
            ),
            Self::HeaderVersion(v) => write!(f, "project parameters version {v} is not 1 to 5"),
            Self::HeaderTruncated => write!(f, "the project header (5E) is cut short"),
            Self::PlayListVersion(v) => write!(f, "Play List version {v} is not 1 or 2"),
            Self::HeaderTrailing(n) => write!(f, "{n} unexpected byte(s) end the project header"),
            Self::Beat { index, error } => write!(f, "beat {}: {error}", index + 1),
            Self::NoHeader => write!(f, "no project header (5E) was received"),
        }
    }
}

impl std::error::Error for ProjectError {}

/// What the `5E` message announces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectHeader {
    pub params_version: u16,
    /// Play Lists that are not empty (0 to 4).
    pub play_lists_used: usize,
    /// The Play List format version (2), absent before parameters version 4, which have no lists.
    pub play_list_version: Option<u16>,
    /// The name of each of the 4 Play Lists, `None` for an empty list.
    pub play_list_names: [Option<String>; LISTS],
    /// The project's Quantize, the fine values of Tempester 1.6.5.0 (n°55) included; `None` when the parameter word
    /// holds no value 0-7.
    pub quantize: Option<Quantize>,
}

impl ProjectHeader {
    /// Which of the 4 Play Lists are used (none before parameters version 4).
    pub fn lists_used(&self) -> [bool; LISTS] {
        std::array::from_fn(|list| self.play_list_names[list].is_some())
    }
}

/// One beat of a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectBeat {
    /// 0 to 15.
    pub index: u8,
    pub dump: BeatDump,
    /// The decoded bytes of the beat, as the packed message holds them (padding left out).
    data: Vec<u8>,
}

impl ProjectBeat {
    /// The beat as the Beats library keeps it (`F0 01 28 5F ... F7`): the same data as the `5C` message, packed in the
    /// standard layout, which `send_beat` can send back to the machine and its receiver reads exactly.
    pub fn beat_file(&self) -> Vec<u8> {
        beat::standard_dump(&self.data)
    }

    /// The decoded bytes of the beat, as the packed `5C` message holds them.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// The `5C n` message in the standard layout.
    fn standard_message(&self) -> Vec<u8> {
        standard_frame(CMD_PROJECT_BEAT, &[self.index], &self.data)
    }
}

/// A project as received: the header and the beats that came (0 to 16, see `absent_beats`), with the messages as they
/// came.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub header: ProjectHeader,
    pub beats: Vec<ProjectBeat>,
    header_data: Vec<u8>,
    messages: Vec<Vec<u8>>,
}

impl Project {
    /// Numbers (0 to 15) of the beats the machine did not send, in increasing order. Empty for a complete project.
    pub fn absent_beats(&self) -> Vec<u8> {
        (0..BEATS as u8)
            .filter(|n| self.beats.iter().all(|beat| beat.index != *n))
            .collect()
    }

    pub fn is_complete(&self) -> bool {
        self.beats.len() == BEATS
    }

    /// Every message as received, one after the other: a `.syx` that sends the project back as the menu did.
    ///
    /// The machine's packer ends each message with a partial last group that its own receivers misread
    /// (`refs/firmware-main.md` 9.1): this is the evidence of the transfer, not what to send back (see `standard`).
    pub fn raw(&self) -> Vec<u8> {
        self.messages.concat()
    }

    /// The decoded bytes of the `5E` message (parameters, Play List version, Play Lists).
    pub fn header_data(&self) -> &[u8] {
        &self.header_data
    }

    /// The messages to send the project back, in the standard layout: `5E`, then `5C n` of the beats that came, in
    /// increasing order. The decoded data is identical to `raw`'s, and the machine's receivers read it back exactly.
    pub fn send_messages(&self) -> Vec<Vec<u8>> {
        let mut out = vec![standard_frame(CMD_PROJECT_HEADER, &[], &self.header_data)];
        out.extend(self.beats.iter().map(ProjectBeat::standard_message));
        out
    }

    /// The same messages one after the other: the file meant to be sent back.
    pub fn standard(&self) -> Vec<u8> {
        self.send_messages().concat()
    }

    /// The same project with its four Play Lists empty (`0xFEFE` each), what a send that leaves the lists out writes
    /// to the machine. A project whose parameters version has no lists is returned unchanged.
    pub fn with_empty_play_lists(&self) -> Project {
        let mut project = self.clone();
        if self.header.play_list_version.is_some() {
            let mut data = self.header_data[..2 * (PARAM_WORDS + 1)].to_vec();
            for _ in 0..LISTS {
                data.extend(EMPTY_LIST.to_be_bytes());
            }
            project.header.play_lists_used = 0;
            project.header.play_list_names = Default::default();
            project.header_data = data;
        }
        project.messages = project.send_messages();
        project
    }
}

/// What `ProjectAssembler::accept` did with a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accepted {
    Header,
    /// Beat 0 to 15.
    Beat(u8),
    /// Not a project message (another DSI command, foreign SysEx).
    Ignored,
}

/// Collects and checks the messages of one Send Project.
#[derive(Debug, Default)]
pub struct ProjectAssembler {
    messages: Vec<Vec<u8>>,
    header: Option<ProjectHeader>,
    header_data: Vec<u8>,
    beats: Vec<ProjectBeat>,
}

impl ProjectAssembler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn has_header(&self) -> bool {
        self.header.is_some()
    }

    pub fn beats_received(&self) -> usize {
        self.beats.len()
    }

    /// Number (0 to 15) of the last beat accepted.
    pub fn last_beat(&self) -> Option<u8> {
        self.beats.last().map(|beat| beat.index)
    }

    pub fn accept(&mut self, message: &[u8]) -> Result<Accepted, ProjectError> {
        let is_dsi = message.len() >= 5 && message[..3] == DSI_PREFIX;
        match is_dsi.then(|| message[3]) {
            Some(CMD_PROJECT_HEADER) => self.accept_header(message),
            Some(CMD_PROJECT_BEAT) => self.accept_beat(message),
            _ => Ok(Accepted::Ignored),
        }
    }

    fn accept_header(&mut self, message: &[u8]) -> Result<Accepted, ProjectError> {
        if self.header.is_some() {
            return Err(ProjectError::HeaderTwice);
        }
        let (header, data) = unpack_body(message, 4, read_header_exact)?;
        self.header = Some(header);
        self.header_data = data;
        self.messages.push(message.to_vec());
        Ok(Accepted::Header)
    }

    fn accept_beat(&mut self, message: &[u8]) -> Result<Accepted, ProjectError> {
        if message.len() < 6 {
            return Err(ProjectError::Malformed);
        }
        let index = message[4];
        if usize::from(index) >= BEATS {
            return Err(ProjectError::BeatNumber(index));
        }
        if self.header.is_none() {
            return Err(ProjectError::BeatBeforeHeader { beat: index });
        }
        if let Some(after) = self.last_beat().filter(|after| index <= *after) {
            return Err(ProjectError::BeatOutOfOrder { got: index, after });
        }
        let (dump, data) = unpack_body(message, 5, |data| {
            beat::read_unpacked(data).map_err(|error| ProjectError::Beat { index, error })
        })?;
        self.beats.push(ProjectBeat { index, dump, data });
        self.messages.push(message.to_vec());
        Ok(Accepted::Beat(index))
    }

    /// The project with the header and the beats that came, however few; without a header there is none.
    pub fn finish(self) -> Result<Project, ProjectError> {
        let header = self.header.ok_or(ProjectError::NoHeader)?;
        Ok(Project {
            header,
            beats: self.beats,
            header_data: self.header_data,
            messages: self.messages,
        })
    }
}

/// Reads the packed part of a message (from `from` to the final `F7`) with `read`, which must consume the data
/// exactly: the last, partial group is laid out by the machine's flush (`pack7::pack_flushed`, up to 1.6.1.0) or the
/// standard way (from 1.6.1.2), and only the structure of the data tells its length (`pack7::unpack_tolerant_with`).
/// Returns what `read` made of the data, and the data itself.
fn unpack_body<T>(
    message: &[u8],
    from: usize,
    read: impl Fn(&[u8]) -> Result<T, ProjectError>,
) -> Result<(T, Vec<u8>), ProjectError> {
    if message.len() < from + 1 || message[message.len() - 1] != 0xF7 {
        return Err(ProjectError::Malformed);
    }
    let data = unpack_tolerant_with(&message[from..message.len() - 1], |data| {
        read(data).map(|_| ())
    })
    .map_err(|error| match error {
        FlushedError::Packing(e) => ProjectError::Packing(e),
        FlushedError::Parse(e) => e,
        FlushedError::Ambiguous => ProjectError::AmbiguousTail,
    })?;
    Ok((read(&data)?, data))
}

/// `read_header` on data that holds nothing after the header.
fn read_header_exact(data: &[u8]) -> Result<ProjectHeader, ProjectError> {
    let (header, used) = read_header(data)?;
    match data.len() - used {
        0 => Ok(header),
        rest => Err(ProjectError::HeaderTrailing(rest)),
    }
}

/// Reads the header at the start of `data` and returns it with the bytes it takes (the parameters, then the Play
/// Lists as the sender writes them, or as a project file holds them).
pub fn read_header(data: &[u8]) -> Result<(ProjectHeader, usize), ProjectError> {
    let word = |at: usize| -> Option<u16> {
        let pair = data.get(2 * at..2 * at + 2)?;
        Some(u16::from(pair[0]) << 8 | u16::from(pair[1]))
    };
    let params_version = word(0).ok_or(ProjectError::HeaderTruncated)?;
    if !(1..=LAST_PARAMS_VERSION).contains(&params_version) {
        return Err(ProjectError::HeaderVersion(params_version));
    }
    if data.len() < 2 * PARAM_WORDS {
        return Err(ProjectError::HeaderTruncated);
    }
    let mut at = PARAM_WORDS;
    let mut play_list_version = None;
    let mut play_list_names: [Option<String>; LISTS] = Default::default();
    if params_version >= FIRST_VERSION_WITH_LISTS {
        let list_version = word(at).ok_or(ProjectError::HeaderTruncated)?;
        if list_version != LIST_VERSION {
            return Err(ProjectError::PlayListVersion(list_version));
        }
        play_list_version = Some(list_version);
        at += 1;
        for name in &mut play_list_names {
            if word(at).ok_or(ProjectError::HeaderTruncated)? == EMPTY_LIST {
                at += 1;
            } else {
                let name_bytes = data
                    .get(2 * (at + 1)..2 * (at + 1 + LIST_NAME_WORDS))
                    .ok_or(ProjectError::HeaderTruncated)?;
                *name = Some(list_name(name_bytes));
                at += 1 + LIST_BODY_WORDS;
            }
        }
    }
    if 2 * at > data.len() {
        return Err(ProjectError::HeaderTruncated);
    }
    Ok((
        ProjectHeader {
            params_version,
            play_lists_used: play_list_names.iter().flatten().count(),
            play_list_version,
            play_list_names,
            quantize: word(QUANTIZE_WORD)
                .zip(word(FINE_WORD))
                .and_then(|(official, fine)| Quantize::from_words(official, fine)),
        },
        2 * at,
    ))
}

/// A Play List name: ASCII up to the first NUL, anything else shown as `?`.
fn list_name(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take_while(|byte| **byte != 0)
        .map(|byte| {
            if (0x20..0x7F).contains(byte) {
                char::from(*byte)
            } else {
                '?'
            }
        })
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// A synthetic `5E` message (parameters version 5, Play List version 2) with `lists_used` non-empty lists, for tests
/// and the fake machine.
pub fn header_message(lists_used: usize) -> Vec<u8> {
    let mut words = vec![0u16; PARAM_WORDS];
    words[0] = LAST_PARAMS_VERSION;
    words.push(LIST_VERSION);
    for list in 0..LISTS {
        if list < lists_used {
            words.push(1);
            words.extend(std::iter::repeat_n(0, LIST_BODY_WORDS));
        } else {
            words.push(EMPTY_LIST);
        }
    }
    let data: Vec<u8> = words.iter().flat_map(|w| w.to_be_bytes()).collect();
    frame(CMD_PROJECT_HEADER, &[], &data)
}

/// A message of a project as a receiving machine keeps it: its decoded data, whichever layout it came in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Received {
    Header(Vec<u8>),
    Beat { index: u8, data: Vec<u8> },
}

/// Decodes one `5E` or `5C n` message on its own (a lone beat needs no header, as for the machine's receiver);
/// `None` for any other message.
pub fn decode_received(message: &[u8]) -> Result<Option<Received>, ProjectError> {
    let is_dsi = message.len() >= 5 && message[..3] == DSI_PREFIX;
    match is_dsi.then(|| message[3]) {
        Some(CMD_PROJECT_HEADER) => {
            let (_, data) = unpack_body(message, 4, read_header_exact)?;
            Ok(Some(Received::Header(data)))
        }
        Some(CMD_PROJECT_BEAT) if message.len() >= 6 => {
            let index = message[4];
            if usize::from(index) >= BEATS {
                return Err(ProjectError::BeatNumber(index));
            }
            let (_, data) = unpack_body(message, 5, |data| {
                beat::read_unpacked(data).map_err(|error| ProjectError::Beat { index, error })
            })?;
            Ok(Some(Received::Beat { index, data }))
        }
        _ => Ok(None),
    }
}

/// A `5E` message carrying `data`, as the machine sends it (flushed last group).
pub fn header_data_message(data: &[u8]) -> Vec<u8> {
    frame(CMD_PROJECT_HEADER, &[], data)
}

/// A `5C` message for beat `index` carrying `data` (see `beat::synthetic_beat_data`).
pub fn beat_message(index: u8, data: &[u8]) -> Vec<u8> {
    frame(CMD_PROJECT_BEAT, &[index], data)
}

/// A message as the machine sends it: the flushed last group.
fn frame(command: u8, raw: &[u8], data: &[u8]) -> Vec<u8> {
    framed(command, raw, pack_flushed(data))
}

/// A message in the standard layout: the one the machine's receivers read.
fn standard_frame(command: u8, raw: &[u8], data: &[u8]) -> Vec<u8> {
    framed(command, raw, pack_padded(data))
}

fn framed(command: u8, raw: &[u8], packed: Vec<u8>) -> Vec<u8> {
    let mut out = DSI_PREFIX.to_vec();
    out.push(command);
    out.extend_from_slice(raw);
    out.extend(packed);
    out.push(0xF7);
    out
}

#[cfg(test)]
mod tests;
