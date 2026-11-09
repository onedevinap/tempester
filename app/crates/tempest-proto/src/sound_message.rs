//! Sound SysEx messages (`tools/tempest_sysex.py`). No checksum; data packed with the zero-padded 7-bit form.
//!
//! - current sound: `F0 01 28 60 <pack(128)> F7` (answer to the request `40`)
//! - sound file:    `F0 01 28 63 <len> <pack(name + NUL + 128)> F7`, `len` counting the NUL ending the name

use crate::pack7::{pack_padded, unpack_tolerant_with, FlushedError, PackError};
use crate::sound::{Sound, SoundError, SOUND_BYTES};
use crate::DSI_PREFIX;

/// Request for the current sound (answered by `60`; needs a Tempester Main 1.5.2.0 or later).
pub const REQUEST_SOUND: [u8; 5] = [0xF0, 0x01, 0x28, 0x40, 0xF7];
/// Request for the current beat (answered by `5F`; needs a Tempester Main 1.5.2.0 or later).
pub const REQUEST_BEAT: [u8; 5] = [0xF0, 0x01, 0x28, 0x41, 0xF7];
/// Request for the whole project (answered by the menu's Send Project: `5E` then 16 x `5C n`; needs Tempester Main
/// 1.6.1.0 or later). Older versions stay silent.
pub const REQUEST_PROJECT: [u8; 5] = [0xF0, 0x01, 0x28, 0x42, 0xF7];
pub const CMD_CURRENT_SOUND: u8 = 0x60;
pub const CMD_SOUND_FILE: u8 = 0x63;
const MAX_NAME_LEN: usize = 126;

/// A decoded sound message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SoundMessage {
    Current(Sound),
    File { name: String, sound: Sound },
}

fn frame(command: u8, header: Option<u8>, payload: &[u8]) -> Vec<u8> {
    let mut out = DSI_PREFIX.to_vec();
    out.push(command);
    out.extend(header);
    out.extend(pack_padded(payload));
    out.push(0xF7);
    out
}

/// `F0 01 28 60 … F7`: the sound the machine sends from its menu and answers to `40`.
pub fn encode_current(sound: &Sound) -> Vec<u8> {
    frame(CMD_CURRENT_SOUND, None, sound.bytes())
}

/// `F0 01 28 63 …`: a named sound file; `name` is the path shown by the machine (7-bit ASCII, at most 126 bytes).
pub fn encode_file(name: &str, sound: &Sound) -> Result<Vec<u8>, SoundError> {
    if name.len() > MAX_NAME_LEN || !name.is_ascii() || name.contains('\0') {
        return Err(SoundError::BadName);
    }
    let mut payload = name.as_bytes().to_vec();
    payload.push(0);
    let header = payload.len() as u8;
    payload.extend(sound.bytes());
    Ok(frame(CMD_SOUND_FILE, Some(header), &payload))
}

/// Decodes a `60` or `63` message.
pub fn decode(message: &[u8]) -> Result<SoundMessage, SoundError> {
    let (command, body) = split(message)?;
    match command {
        CMD_CURRENT_SOUND => Ok(SoundMessage::Current(sound_at(&unpack_current(body)?, 0)?)),
        CMD_SOUND_FILE => decode_file(body),
        _ => Err(SoundError::BadMessage),
    }
}

/// The 128 bytes of a `60` message (`unpack_exact`).
fn unpack_current(body: &[u8]) -> Result<Vec<u8>, SoundError> {
    unpack_exact(body, SOUND_BYTES)
}

/// `len` data bytes of a message whose last group is laid out either by the machine's flush (head bits shifted, up to
/// Tempester 1.6.1.0: `pack7::pack_flushed`) or the standard way (from 1.6.1.2, this app, DSI files): both readings
/// of that length are tried; when they differ, the one that packs back to the stream byte for byte is kept, and a
/// stream that neither does is refused (`pack7::unpack_tolerant_with`).
fn unpack_exact(body: &[u8], len: usize) -> Result<Vec<u8>, SoundError> {
    unpack_tolerant_with(
        body,
        |data| if data.len() == len { Ok(()) } else { Err(()) },
    )
    .map_err(|error| match error {
        FlushedError::Packing(PackError::BadLength { .. } | PackError::PartialGroup { .. })
        | FlushedError::Parse(()) => SoundError::TooShort,
        _ => SoundError::BadMessage,
    })
}

fn split(message: &[u8]) -> Result<(u8, &[u8]), SoundError> {
    let valid =
        message.len() >= 5 && message[..3] == DSI_PREFIX && message[message.len() - 1] == 0xF7;
    if !valid {
        return Err(SoundError::BadMessage);
    }
    Ok((message[3], &message[4..message.len() - 1]))
}

fn decode_file(body: &[u8]) -> Result<SoundMessage, SoundError> {
    let (&len, packed) = body.split_first().ok_or(SoundError::TooShort)?;
    let len = usize::from(len);
    let data = unpack_exact(packed, len + SOUND_BYTES)?;
    let name_bytes = data.get(..len).ok_or(SoundError::TooShort)?;
    let end = name_bytes.iter().position(|b| *b == 0).unwrap_or(len);
    let name = name_bytes[..end].iter().map(|b| char::from(*b)).collect();
    Ok(SoundMessage::File {
        name,
        sound: sound_at(&data, len)?,
    })
}

fn sound_at(data: &[u8], start: usize) -> Result<Sound, SoundError> {
    let bytes = data
        .get(start..start + SOUND_BYTES)
        .ok_or(SoundError::TooShort)?;
    let mut array = [0u8; SOUND_BYTES];
    array.copy_from_slice(bytes);
    Ok(Sound::from_bytes(array))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Sound {
        let mut sound = Sound::with_defaults();
        sound.set(34, 100).unwrap();
        sound.set_name("Test").unwrap();
        sound
    }

    #[test]
    fn test_encode_current_when_sound_returns_152_packed_bytes_framed() {
        let msg = encode_current(&sample());

        assert_eq!(&msg[..4], &[0xF0, 0x01, 0x28, 0x60]);
        assert_eq!(msg.len(), 4 + 152 + 1);
        assert_eq!(msg[msg.len() - 1], 0xF7);
        assert!(msg[1..msg.len() - 1].iter().all(|b| *b < 0x80));
    }

    #[test]
    fn test_decode_when_current_encoded_returns_same_sound() {
        let sound = sample();

        assert_eq!(
            decode(&encode_current(&sound)),
            Ok(SoundMessage::Current(sound))
        );
    }

    #[test]
    fn test_decode_when_file_encoded_returns_name_and_sound() {
        let sound = sample();

        let decoded = decode(&encode_file("/S/Kicks/Electro Zap", &sound).unwrap());

        assert_eq!(
            decoded,
            Ok(SoundMessage::File {
                name: "/S/Kicks/Electro Zap".into(),
                sound
            })
        );
    }

    /// A `63` message whose packed part is laid out by `pack` (the machine's flush up to 1.6.1.0, or standard).
    fn file_message(name: &str, sound: &Sound, pack: fn(&[u8]) -> Vec<u8>) -> Vec<u8> {
        let mut payload = name.as_bytes().to_vec();
        payload.push(0);
        let mut out = vec![0xF0, 0x01, 0x28, CMD_SOUND_FILE, payload.len() as u8];
        payload.extend(sound.bytes());
        out.extend(pack(&payload));
        out.push(0xF7);
        out
    }

    #[test]
    fn test_decode_when_file_in_either_layout_for_every_last_group_size_returns_name_and_sound() {
        // A file name of n bytes gives n + 1 + 128 data bytes: n = 1..7 covers every size of the last group. The last
        // group holds the sound's last bytes, its name bits: a name of '~' (7 bits set) puts MSBs there.
        let mut sound = sample();
        sound.set_name("~~~~~~~~~~~~~~~~~~~~").unwrap();
        assert!(sound.bytes()[120..].iter().any(|b| b & 0x80 != 0));
        for n in 1..=7 {
            let name = "N".repeat(n);
            let expected = Ok(SoundMessage::File {
                name: name.clone(),
                sound: sound.clone(),
            });
            for pack in [crate::pack7::pack_flushed, pack_padded] {
                assert_eq!(
                    decode(&file_message(&name, &sound, pack)),
                    expected,
                    "n={n}"
                );
            }
        }
    }

    #[test]
    fn test_encode_file_when_name_is_20_chars_returns_length_21_and_154_packed_bytes() {
        let msg = encode_file("/S/Kicks/Electro Zap", &sample()).unwrap();

        assert_eq!(msg[4], 21);
        assert_eq!(msg.len(), 5 + 176 + 1);
    }

    #[test]
    fn test_encode_file_when_name_not_ascii_or_too_long_returns_bad_name() {
        let sound = sample();

        assert_eq!(encode_file("é", &sound), Err(SoundError::BadName));
        assert_eq!(
            encode_file(&"x".repeat(127), &sound),
            Err(SoundError::BadName)
        );
        assert!(encode_file(&"x".repeat(126), &sound).is_ok());
    }

    #[test]
    fn test_decode_when_other_command_returns_bad_message() {
        assert_eq!(decode(&REQUEST_SOUND), Err(SoundError::BadMessage));
        assert_eq!(
            decode(&[0xF0, 0x01, 0x28, 0x60]),
            Err(SoundError::BadMessage)
        );
    }

    #[test]
    fn test_decode_when_payload_too_short_returns_too_short() {
        let mut msg = encode_current(&sample());
        msg.truncate(50);
        msg.push(0xF7);

        assert_eq!(decode(&msg), Err(SoundError::TooShort));
    }

    #[test]
    fn test_decode_when_current_sound_ends_with_the_machines_flush_returns_the_msbs_of_the_last_bytes(
    ) {
        let mut bytes = *sample().bytes();
        bytes[126] = 0xFF;
        bytes[127] = 0xA5;
        let sound = Sound::from_bytes(bytes);
        let mut message = DSI_PREFIX.to_vec();
        message.push(CMD_CURRENT_SOUND);
        message.extend(crate::pack7::pack_flushed(sound.bytes()));
        message.push(0xF7);

        assert_eq!(decode(&message), Ok(SoundMessage::Current(sound)));
    }

    #[test]
    fn test_decode_when_current_sound_ends_with_the_standard_layout_returns_the_msbs_of_the_last_bytes(
    ) {
        let mut bytes = *sample().bytes();
        bytes[126] = 0xFF;
        bytes[127] = 0xA5;
        let sound = Sound::from_bytes(bytes);

        assert_eq!(
            decode(&encode_current(&sound)),
            Ok(SoundMessage::Current(sound))
        );
    }

    #[test]
    fn test_requests_when_built_return_exact_bytes() {
        assert_eq!(REQUEST_SOUND, [0xF0, 0x01, 0x28, 0x40, 0xF7]);
        assert_eq!(REQUEST_BEAT, [0xF0, 0x01, 0x28, 0x41, 0xF7]);
    }
}
