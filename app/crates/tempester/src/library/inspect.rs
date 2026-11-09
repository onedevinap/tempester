//! Reading a `.syx` file as a library item: its internal name, the hash of its DECODED data (so that the packing
//! layout never makes two copies of the same sound look different), its filter facets, and the same file re-encoded
//! in the standard 7-bit layout the machine reads back.

use tempest_proto::beat;
use tempest_proto::pack7;
use tempest_proto::project::{self, BEATS};
use tempest_proto::sha256::sha256_hex;
use tempest_proto::sound::Sound;
use tempest_proto::sound_message::{self, SoundMessage};
use tempest_proto::DSI_PREFIX;

use super::model::Facets;
use crate::command::Kind;
use crate::error::AppError;

/// The 14 Sound Type folders of the machine's file system (`refs/firmware-main.md` 3.3).
const SOUND_TYPES: [&str; 14] = [
    "Kicks",
    "Snares",
    "HiHats",
    "Toms",
    "Cymbals",
    "Percussion",
    "Noise",
    "Bass",
    "Lead",
    "Misc",
    "User 1",
    "User 2",
    "User 3",
    "User 4",
];

/// Parameters read for the oscillator facets: (label, parameter index, second index for a bank).
const OSCILLATORS: [(&str, usize, Option<usize>); 4] = [
    ("Shape", 2, None),
    ("Shape", 8, None),
    ("Sample/Noise", 14, Some(15)),
    ("Sample/Noise", 20, Some(21)),
];

/// What a library file holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    pub kind: Kind,
    /// Internal name (empty for a project).
    pub name: String,
    pub hash: String,
    /// The file in the standard layout; this is what the library writes.
    pub canonical: Vec<u8>,
    pub facets: Facets,
    /// Beats of a project.
    pub beats: Option<usize>,
    /// For a project: which beat slots hold which beat names (its identity across fetches).
    pub signature: String,
}

/// The kind a DSI message is, from its command byte.
pub fn detect_kind(bytes: &[u8]) -> Option<Kind> {
    if bytes.len() < 5 || bytes[..3] != DSI_PREFIX {
        return None;
    }
    match bytes[3] {
        sound_message::CMD_CURRENT_SOUND | sound_message::CMD_SOUND_FILE => Some(Kind::Sounds),
        beat::CMD_BEAT => Some(Kind::Beats),
        tempest_proto::project::CMD_PROJECT_HEADER => Some(Kind::Projects),
        _ => None,
    }
}

fn invalid(what: &str, why: impl std::fmt::Display) -> AppError {
    AppError::Invalid(format!("not a {what} file: {why}"))
}

/// Reads `bytes` as an item of `kind`. `fallback_name` names the `63` file written from a bare `60` message.
pub fn inspect(kind: Kind, bytes: &[u8], fallback_name: &str) -> Result<Facts, AppError> {
    match kind {
        Kind::Sounds => inspect_sound(bytes, fallback_name),
        Kind::Beats => inspect_beat(bytes),
        Kind::Projects => inspect_project(bytes),
    }
}

/// The folder of a `63` file name such as `/S/Kicks/Electro Zap`, when it is one of the machine's Sound Types.
pub fn sound_type_of(file_name: &str) -> String {
    let mut parts = file_name.split('/').filter(|p| !p.is_empty());
    match (parts.next(), parts.next(), parts.next()) {
        (Some("S"), Some(folder), Some(_)) if SOUND_TYPES.contains(&folder) => folder.into(),
        _ => String::new(),
    }
}

/// One text per oscillator, read from the sound's own data.
pub fn oscillators_of(sound: &Sound) -> Vec<String> {
    OSCILLATORS
        .iter()
        .map(|(label, index, bank)| {
            let value = sound.get(*index).unwrap_or(0);
            match bank.map(|b| sound.get(b).unwrap_or(0)) {
                Some(bank) if bank != 0 => format!("{label} {value} bank {bank}"),
                _ => format!("{label} {value}"),
            }
        })
        .collect()
}

fn inspect_sound(bytes: &[u8], fallback_name: &str) -> Result<Facts, AppError> {
    let (sound, file_name) = match sound_message::decode(bytes).map_err(|e| invalid("sound", e))? {
        SoundMessage::File { name, sound } => (sound, name),
        SoundMessage::Current(sound) => (sound, fallback_name.trim().to_string()),
    };
    let canonical =
        sound_message::encode_file(&file_name, &sound).map_err(|e| invalid("sound", e))?;
    Ok(Facts {
        kind: Kind::Sounds,
        name: sound.name(),
        hash: sha256_hex(sound.bytes()),
        canonical,
        facets: Facets {
            sound_type: sound_type_of(&file_name),
            osc: oscillators_of(&sound),
        },
        beats: None,
        signature: String::new(),
    })
}

fn inspect_beat(bytes: &[u8]) -> Result<Facts, AppError> {
    let canonical = beat::standard_message(bytes).map_err(|e| invalid("beat", e))?;
    let data =
        beat::read_body(&canonical[4..canonical.len() - 1]).map_err(|e| invalid("beat", e))?;
    let summary = beat::summarize(&canonical).map_err(|e| invalid("beat", e))?;
    Ok(Facts {
        kind: Kind::Beats,
        name: summary.name,
        hash: sha256_hex(&data),
        canonical,
        facets: Facets::default(),
        beats: None,
        signature: String::new(),
    })
}

/// The messages of a project file: every `F0 ... F7` run.
pub(crate) fn messages_of(bytes: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut start = None;
    for (at, byte) in bytes.iter().enumerate() {
        match (*byte, start) {
            (0xF0, _) => start = Some(at),
            (0xF7, Some(from)) => {
                out.push(&bytes[from..=at]);
                start = None;
            }
            _ => {}
        }
    }
    out
}

/// The data of a `5E` message in either layout (the machine's flushed last group, or the standard one), read
/// exactly.
fn header_data(message: &[u8]) -> Result<Vec<u8>, AppError> {
    let exact = |data: &[u8]| match project::read_header(data) {
        Ok((_, used)) if used == data.len() => Ok(()),
        Ok((_, used)) => Err(project::ProjectError::HeaderTrailing(data.len() - used)),
        Err(e) => Err(e),
    };
    pack7::unpack_tolerant_with(&message[4..message.len() - 1], exact)
        .map_err(|e| invalid("project", format!("header: {e:?}")))
}

/// A project file in either layout, decoded message by message; the hash covers the decoded header and beats, so
/// the exact capture and the standard file of one project have the same hash.
fn inspect_project(bytes: &[u8]) -> Result<Facts, AppError> {
    let messages = messages_of(bytes);
    let first = messages
        .first()
        .filter(|m| detect_kind(m) == Some(Kind::Projects))
        .ok_or_else(|| invalid("project", "no project header (5E)"))?;
    let header = header_data(first)?;
    let mut canonical = standard_message(project::CMD_PROJECT_HEADER, &[], &header);
    let mut decoded = header;
    let mut signature = Vec::new();
    let mut last: Option<u8> = None;
    for message in &messages[1..] {
        let (index, data, name) = beat_of(message)?;
        if usize::from(index) >= BEATS || last.is_some_and(|l| index <= l) {
            return Err(invalid(
                "project",
                format!("beat {} is out of order", index + 1),
            ));
        }
        last = Some(index);
        canonical.extend(standard_message(project::CMD_PROJECT_BEAT, &[index], &data));
        decoded.push(index);
        decoded.extend(&data);
        signature.push(format!("{index}:{name}"));
    }
    Ok(Facts {
        kind: Kind::Projects,
        name: String::new(),
        hash: sha256_hex(&decoded),
        canonical,
        facets: Facets::default(),
        beats: Some(signature.len()),
        signature: format!("{BEATS}|{}", signature.join("|")),
    })
}

/// A `5C n` message: its beat number, decoded data and beat name.
fn beat_of(message: &[u8]) -> Result<(u8, Vec<u8>, String), AppError> {
    let is_beat =
        message.len() >= 6 && message[..3] == DSI_PREFIX && message[3] == project::CMD_PROJECT_BEAT;
    if !is_beat {
        return Err(invalid(
            "project",
            "a message that is not a beat follows the header",
        ));
    }
    let data = beat::read_body(&message[5..message.len() - 1]).map_err(|e| {
        invalid(
            "project",
            format!("beat {}: {e}", message[4].wrapping_add(1)),
        )
    })?;
    let dump = beat::read_unpacked(&data).map_err(|e| invalid("project", e))?;
    Ok((message[4], data, dump.summary.name))
}

/// A message in the standard layout (`pack_padded`), the one the machine's receivers read.
fn standard_message(command: u8, raw: &[u8], data: &[u8]) -> Vec<u8> {
    let mut out = DSI_PREFIX.to_vec();
    out.push(command);
    out.extend_from_slice(raw);
    out.extend(pack7::pack_padded(data));
    out.push(0xF7);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempest_proto::pack7::pack_padded;

    fn sound(name: &str, freq: u16) -> Sound {
        let mut sound = Sound::with_defaults();
        sound.set(0, freq).unwrap();
        sound.set_name(name).unwrap();
        sound
    }

    #[test]
    fn test_inspect_when_same_sound_in_60_and_63_returns_the_same_hash() {
        let s = sound("Kick", 40);
        let current = sound_message::encode_current(&s);
        let file = sound_message::encode_file("/S/Kicks/Kick", &s).unwrap();

        let a = inspect(Kind::Sounds, &current, "Kick").unwrap();
        let b = inspect(Kind::Sounds, &file, "x").unwrap();

        assert_eq!(a.hash, b.hash);
        assert_eq!(a.name, "Kick");
        assert_eq!(b.facets.sound_type, "Kicks");
        assert_eq!(a.facets.sound_type, "");
    }

    #[test]
    fn test_inspect_when_the_sound_differs_by_one_parameter_returns_another_hash() {
        let a = inspect(
            Kind::Sounds,
            &sound_message::encode_file("K", &sound("K", 40)).unwrap(),
            "",
        )
        .unwrap();
        let b = inspect(
            Kind::Sounds,
            &sound_message::encode_file("K", &sound("K", 41)).unwrap(),
            "",
        )
        .unwrap();

        assert_ne!(a.hash, b.hash);
    }

    #[test]
    fn test_inspect_when_beat_in_either_layout_returns_the_same_hash_and_the_standard_file() {
        let data = beat::synthetic_tail_beat(3);
        let machine = beat::machine_dump(&data);
        let standard = beat::standard_dump(&data);

        let a = inspect(Kind::Beats, &machine, "").unwrap();
        let b = inspect(Kind::Beats, &standard, "").unwrap();

        assert_eq!(a.hash, b.hash);
        assert_eq!(a.canonical, standard);
        assert_ne!(machine, standard);
    }

    #[test]
    fn test_inspect_when_project_in_either_layout_returns_the_same_hash_and_the_standard_file() {
        let messages = crate::machine::fake_machine::project_messages();
        let raw = messages.concat();

        let facts = inspect(Kind::Projects, &raw, "").unwrap();
        let again = inspect(Kind::Projects, &facts.canonical, "").unwrap();

        assert_eq!(facts.hash, again.hash);
        assert_eq!(facts.canonical, again.canonical);
        assert_eq!(facts.beats, Some(16));
        assert!(facts.signature.contains("0:Fake beat 01"));
    }

    #[test]
    fn test_inspect_when_not_a_tempest_file_returns_an_error() {
        for kind in [Kind::Sounds, Kind::Beats, Kind::Projects] {
            assert!(inspect(kind, b"hello", "").is_err());
            assert!(inspect(kind, &pack_padded(&[1, 2, 3]), "").is_err());
        }
    }

    #[test]
    fn test_detect_kind_when_command_bytes_known_returns_the_kinds() {
        let s = sound("K", 1);
        assert_eq!(
            detect_kind(&sound_message::encode_current(&s)),
            Some(Kind::Sounds)
        );
        assert_eq!(
            detect_kind(&beat::standard_dump(&beat::synthetic_tail_beat(0))),
            Some(Kind::Beats)
        );
        assert_eq!(
            detect_kind(&tempest_proto::project::header_message(1)),
            Some(Kind::Projects)
        );
        assert_eq!(detect_kind(&[0xF0, 0x01, 0x28, 0x10, 0xF7]), None);
        assert_eq!(detect_kind(b"x"), None);
    }

    #[test]
    fn test_sound_type_of_when_path_names_are_given_returns_the_folder_or_nothing() {
        assert_eq!(sound_type_of("/S/Kicks/Electro Zap"), "Kicks");
        assert_eq!(sound_type_of("/S/User 3/x"), "User 3");
        assert_eq!(sound_type_of("/S/Nope/x"), "");
        assert_eq!(sound_type_of("Kick"), "");
        assert_eq!(sound_type_of("/S/Kicks"), "");
    }

    #[test]
    fn test_oscillators_of_when_values_are_set_returns_one_text_each() {
        let mut s = Sound::with_defaults();
        s.set(2, 57).unwrap();
        s.set(14, 20).unwrap();
        s.set(15, 2).unwrap();

        let osc = oscillators_of(&s);

        assert_eq!(osc.len(), 4);
        assert_eq!(osc[0], "Shape 57");
        assert_eq!(osc[2], "Sample/Noise 20 bank 2");
    }
}
