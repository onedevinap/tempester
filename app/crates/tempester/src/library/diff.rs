//! What differs between two versions of an item, in words, for « compare with current ».

use tempest_proto::beat;
use tempest_proto::sound::params;
use tempest_proto::sound_message::{self, SoundMessage};

use super::inspect::inspect;
use crate::command::Kind;
use crate::error::AppError;

/// Lines shown at most; the rest is counted.
const MAX_LINES: usize = 40;

fn sound_of(bytes: &[u8]) -> Result<tempest_proto::sound::Sound, AppError> {
    match sound_message::decode(bytes).map_err(|e| AppError::Invalid(e.to_string()))? {
        SoundMessage::Current(sound) | SoundMessage::File { sound, .. } => Ok(sound),
    }
}

fn beat_data(bytes: &[u8]) -> Result<Vec<u8>, AppError> {
    let message = beat::standard_message(bytes).map_err(|e| AppError::Invalid(e.to_string()))?;
    beat::read_body(&message[4..message.len() - 1]).map_err(|e| AppError::Invalid(e.to_string()))
}

/// The differences from `older` to `current`; one line says so when there are none.
pub fn describe(kind: Kind, older: &[u8], current: &[u8]) -> Result<Vec<String>, AppError> {
    let a = inspect(kind, older, "")?;
    let b = inspect(kind, current, "")?;
    if a.hash == b.hash {
        return Ok(vec!["Identical to the current version: same data.".into()]);
    }
    let mut lines = Vec::new();
    if a.name != b.name {
        lines.push(format!("Name: \"{}\" -> \"{}\"", a.name, b.name));
    }
    match kind {
        Kind::Sounds => sound_lines(older, current, &mut lines)?,
        Kind::Beats => beat_lines(older, current, &mut lines)?,
        Kind::Projects => lines.push(format!(
            "Beats held: {} -> {}",
            a.beats.unwrap_or(0),
            b.beats.unwrap_or(0)
        )),
    }
    let extra = lines.len().saturating_sub(MAX_LINES);
    lines.truncate(MAX_LINES);
    if extra > 0 {
        lines.push(format!("... and {extra} more difference(s)"));
    }
    Ok(lines)
}

fn sound_lines(older: &[u8], current: &[u8], lines: &mut Vec<String>) -> Result<(), AppError> {
    let (before, after) = (sound_of(older)?, sound_of(current)?);
    for p in params() {
        let (x, y) = (
            before.get(usize::from(p.index)),
            after.get(usize::from(p.index)),
        );
        if let (Ok(x), Ok(y)) = (x, y) {
            if x != y {
                lines.push(format!("{} {}: {x} -> {y}", p.group, p.name));
            }
        }
    }
    Ok(())
}

fn beat_lines(older: &[u8], current: &[u8], lines: &mut Vec<String>) -> Result<(), AppError> {
    let (x, y) = (beat_data(older)?, beat_data(current)?);
    let (a, b) = (
        beat::summarize(&beat::standard_dump(&x)).map_err(|e| AppError::Invalid(e.to_string()))?,
        beat::summarize(&beat::standard_dump(&y)).map_err(|e| AppError::Invalid(e.to_string()))?,
    );
    if a.events != b.events {
        lines.push(format!("Events: {} -> {}", a.events, b.events));
    }
    for pad in 0..beat::PADS {
        if a.voice_assign[pad] != b.voice_assign[pad] {
            lines.push(format!(
                "Pad {} Voice Assign: {} -> {}",
                pad + 1,
                a.voice_assign[pad],
                b.voice_assign[pad]
            ));
        }
    }
    let differing = x.iter().zip(&y).filter(|(p, q)| p != q).count() + x.len().abs_diff(y.len());
    lines.push(format!(
        "{differing} of {} data bytes differ",
        x.len().max(y.len())
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempest_proto::sound::Sound;

    fn file(name: &str, freq: u16) -> Vec<u8> {
        let mut sound = Sound::with_defaults();
        sound.set(0, freq).unwrap();
        sound.set_name(name).unwrap();
        sound_message::encode_file(name, &sound).unwrap()
    }

    #[test]
    fn test_describe_when_one_parameter_and_the_name_differ_returns_both_lines() {
        let lines = describe(Kind::Sounds, &file("Kick", 40), &file("Kick 2", 41)).unwrap();

        assert!(
            lines.contains(&"Name: \"Kick\" -> \"Kick 2\"".to_string()),
            "{lines:?}"
        );
        assert!(
            lines.iter().any(|l| l == "Osc 1 Frequency: 40 -> 41"),
            "{lines:?}"
        );
    }

    #[test]
    fn test_describe_when_same_data_returns_the_identical_line_only() {
        let lines = describe(Kind::Sounds, &file("Kick", 40), &file("Kick", 40)).unwrap();

        assert_eq!(lines, vec!["Identical to the current version: same data."]);
    }
}
