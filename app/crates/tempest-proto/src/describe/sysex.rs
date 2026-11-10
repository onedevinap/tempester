//! Labels of SysEx messages: the universal identity exchange and the DSI Tempest commands
//! (`refs/firmware-main.md` 6.5). A message that does not fit its command is named for what it is, never guessed at.

use super::{Kind, Line};
use crate::identity::{self, IDENTITY_REQUEST};
use crate::os_image::{OsId, PACKED_BLOCK_SIZE};
use crate::project::{CMD_PROJECT_BEAT, CMD_PROJECT_HEADER};
use crate::sound_message::{self, SoundMessage, CMD_CURRENT_SOUND, CMD_SOUND_FILE};
use crate::{beat::CMD_BEAT, DSI_PREFIX};

/// `F0 01 28 <command> F7`: the shortest DSI message.
const MIN_DSI_LEN: usize = 5;
/// Bytes of the frame around the blocks of an OS file: `F0 01 28 <id>`, the 14-bit checksum, `F7`.
const OS_FRAME_LEN: usize = 7;

/// The line of one complete SysEx message (`F0 ... F7`).
pub(super) fn describe(message: &[u8]) -> Line {
    Line::new(Kind::SysEx, label(message), message)
}

fn label(message: &[u8]) -> String {
    let size = message.len();
    if message.last() != Some(&0xF7) {
        return format!("SysEx without its F7, {size} bytes");
    }
    if message == IDENTITY_REQUEST {
        return "Identity request (all devices)".into();
    }
    if let Ok(found) = identity::parse(message) {
        return format!(
            "Identity reply: device {:#04x}, version word {:#05X}",
            found.device, found.version
        );
    }
    match message {
        [0xF0, 0x7E, device, 0x06, 0x01, 0xF7] => {
            format!("Identity request to device {device:#04x}")
        }
        [0xF0, 0x01, 0x28, ..] => dsi(message),
        _ => format!("SysEx of another maker, {size} bytes"),
    }
}

fn dsi(message: &[u8]) -> String {
    let size = message.len();
    if size < MIN_DSI_LEN {
        return format!("Truncated DSI SysEx, {size} bytes");
    }
    let command = message[DSI_PREFIX.len()];
    if size == MIN_DSI_LEN {
        if let Some(what) = request_name(command) {
            return format!("Request {command:02X}: {what}");
        }
    }
    if let Some(version) = identity::parse_version(message) {
        return format!("Tempester version {version}");
    }
    match command {
        CMD_CURRENT_SOUND => sound(message, "Sound (current)"),
        CMD_SOUND_FILE => sound(message, "Sound file"),
        CMD_BEAT => format!("Beat 5F, {size} bytes"),
        CMD_PROJECT_HEADER => format!("Project header 5E, {size} bytes"),
        CMD_PROJECT_BEAT => project_beat(message),
        0x61 => format!("Project file 61, {size} bytes"),
        0x62 => format!("Beat file 62, {size} bytes"),
        0x70 => format!("Sample bank 70, {size} bytes"),
        _ => match OsId::from_byte(command) {
            Some(id) => os_image(id, size),
            None => format!("DSI command {command:02X}, {size} bytes"),
        },
    }
}

fn request_name(command: u8) -> Option<&'static str> {
    match command {
        0x40 => Some("current sound"),
        0x41 => Some("current beat"),
        0x42 => Some("whole project"),
        0x43 => Some("exact Tempester version"),
        _ => None,
    }
}

fn sound(message: &[u8], what: &str) -> String {
    let size = message.len();
    match sound_message::decode(message) {
        Ok(SoundMessage::Current(sound)) => format!("{what} \"{}\", {size} bytes", sound.name()),
        _ => format!("{what}, {size} bytes"),
    }
}

fn project_beat(message: &[u8]) -> String {
    let size = message.len();
    match message.get(4) {
        Some(n) if size > MIN_DSI_LEN => format!("Project beat 5C n={n}, {size} bytes"),
        _ => format!("Project beat 5C without its number, {size} bytes"),
    }
}

fn os_image(id: OsId, size: usize) -> String {
    let body = size.saturating_sub(OS_FRAME_LEN);
    let (blocks, extra) = (body / PACKED_BLOCK_SIZE, body % PACKED_BLOCK_SIZE);
    let (chip, byte) = (format!("{id:?}"), id as u8);
    if extra == 0 && blocks > 0 {
        let whole = id.blocks();
        format!("OS {chip} image ({byte:02X}): {blocks} of {whole} blocks, {size} bytes")
    } else {
        format!("OS {chip} message ({byte:02X}) cut or damaged: {size} bytes, not whole blocks")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::os_image;
    use crate::sound::Sound;

    fn text(message: &[u8]) -> String {
        describe(message).label
    }

    #[test]
    fn test_describe_when_identity_request_and_reply_returns_version() {
        let reply = [
            0xF0, 0x7E, 0x7F, 0x06, 0x02, 0x01, 0x28, 0x01, 0x00, 0x00, 0x15, 0x02, 0x7F, 0xF7,
        ];

        assert_eq!(text(&IDENTITY_REQUEST), "Identity request (all devices)");
        assert!(
            text(&reply).contains("version word 0x7F2"),
            "{}",
            text(&reply)
        );
        assert_eq!(
            text(&[0xF0, 0x7E, 0x05, 0x06, 0x01, 0xF7]),
            "Identity request to device 0x05"
        );
    }

    #[test]
    fn test_describe_when_dsi_requests_returns_what_they_ask_for() {
        assert_eq!(
            text(&[0xF0, 0x01, 0x28, 0x40, 0xF7]),
            "Request 40: current sound"
        );
        assert_eq!(
            text(&[0xF0, 0x01, 0x28, 0x41, 0xF7]),
            "Request 41: current beat"
        );
        assert_eq!(
            text(&[0xF0, 0x01, 0x28, 0x42, 0xF7]),
            "Request 42: whole project"
        );
        assert_eq!(
            text(&[0xF0, 0x01, 0x28, 0x43, 0xF7]),
            "Request 43: exact Tempester version"
        );
        assert_eq!(
            text(&[0xF0, 0x01, 0x28, 0x43, 0, 1, 0, 6, 0, 3, 0, 0, 0xF7]),
            "Tempester version 1.6.3.0"
        );
    }

    #[test]
    fn test_describe_when_request_has_extra_bytes_returns_the_plain_command() {
        assert_eq!(
            text(&[0xF0, 0x01, 0x28, 0x40, 0x00, 0xF7]),
            "DSI command 40, 6 bytes"
        );
    }

    #[test]
    fn test_describe_when_current_sound_returns_its_name_and_size() {
        let message = sound_message::encode_current(&Sound::with_defaults());

        let label = text(&message);

        assert!(label.starts_with("Sound (current) \""), "{label}");
        assert!(
            label.ends_with(&format!("{} bytes", message.len())),
            "{label}"
        );
    }

    #[test]
    fn test_describe_when_beat_and_project_messages_returns_their_kind() {
        assert_eq!(
            text(&[0xF0, 0x01, 0x28, 0x5F, 1, 2, 0xF7]),
            "Beat 5F, 7 bytes"
        );
        assert_eq!(
            text(&[0xF0, 0x01, 0x28, 0x5E, 1, 0xF7]),
            "Project header 5E, 6 bytes"
        );
        assert_eq!(
            text(&[0xF0, 0x01, 0x28, 0x5C, 7, 1, 0xF7]),
            "Project beat 5C n=7, 7 bytes"
        );
    }

    #[test]
    fn test_describe_when_os_image_returns_block_count() {
        let image = vec![0u8; 64 * os_image::BLOCK_SIZE];
        let message = os_image::encode(OsId::Panel, &image).unwrap();

        let label = text(&message);

        assert!(
            label.starts_with("OS Panel image (73): 64 of 64 blocks"),
            "{label}"
        );
    }

    #[test]
    fn test_describe_when_os_message_is_cut_returns_a_damage_note() {
        let image = vec![0u8; 32 * os_image::BLOCK_SIZE];
        let mut message = os_image::encode(OsId::Sam, &image).unwrap();
        message.truncate(message.len() - 100);
        message.push(0xF7);

        assert!(
            text(&message).contains("cut or damaged"),
            "{}",
            text(&message)
        );
    }

    #[test]
    fn test_describe_when_truncated_or_unterminated_or_foreign_returns_a_plain_label() {
        assert_eq!(
            text(&[0xF0, 0x01, 0x28, 0xF7]),
            "Truncated DSI SysEx, 4 bytes"
        );
        assert_eq!(
            text(&[0xF0, 0x01, 0x28, 0x60, 0x00]),
            "SysEx without its F7, 5 bytes"
        );
        assert_eq!(
            text(&[0xF0, 0x43, 0x10, 0xF7]),
            "SysEx of another maker, 4 bytes"
        );
        assert_eq!(
            text(&[0xF0, 0x01, 0x28, 0x5C, 0xF7]),
            "Project beat 5C without its number, 5 bytes"
        );
    }

    #[test]
    fn test_describe_when_unknown_dsi_command_returns_command_and_size() {
        assert_eq!(
            text(&[0xF0, 0x01, 0x28, 0x6A, 1, 0xF7]),
            "DSI command 6A, 6 bytes"
        );
    }
}
