//! Backup of the machine's own OS: the Debug menu (hold both SHIFT and SYSTEM) sends Main (71), Voice (72) and
//! Panel (73) as SysEx. The app only listens: it checks each message (blocks, checksum), flags images that are a
//! single repeated byte (seen on the real machine for Voice and Panel: the send then carries a constant, not the
//! chip's content), and writes the good ones with a report into `backups/<UTC date>/`.

use tempest_proto::os_image::{self, OsId, OsImageError};
use tempest_proto::sha256::sha256_hex;
use tempest_proto::versions::written_range;
use tempest_proto::DSI_PREFIX;

use super::Machine;
use crate::command::Command;
use crate::error::AppError;
use crate::event::Event;
use crate::json::{obj, Json};
use crate::storage::stamp::now_stamp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Verdict {
    /// Blocks and checksum fit; `sha256` is the hash of the decoded image. An image is split where an OS update
    /// stops writing: `written_sha256` covers what an update writes, `flasher_sha256` the machine's own flasher
    /// page after it (empty for a chip with no flasher page).
    Valid {
        checksum: u16,
        sha256: String,
        written_sha256: String,
        flasher_sha256: String,
    },
    /// Checks pass but every byte of the image is the same: not real chip content.
    Constant {
        byte: u8,
        sha256: String,
    },
    Invalid(String),
}

/// One OS message heard on the cable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Piece {
    pub(super) id: OsId,
    pub(super) message: Vec<u8>,
    pub(super) verdict: Verdict,
}

impl Piece {
    fn chip(&self) -> &'static str {
        chip_name(self.id)
    }

    fn saveable(&self) -> bool {
        !matches!(self.verdict, Verdict::Invalid(_))
    }

    fn detail(&self) -> String {
        match &self.verdict {
            Verdict::Valid { checksum, .. } => {
                format!("{} blocks, checksum {checksum:#06x} OK", self.id.blocks())
            }
            Verdict::Constant { byte, .. } => format!(
                "every byte is {byte:#04x}: the machine sent a constant, not the chip's content"
            ),
            Verdict::Invalid(why) => why.clone(),
        }
    }

    fn verdict_name(&self) -> &'static str {
        match self.verdict {
            Verdict::Valid { .. } => "ok",
            Verdict::Constant { .. } => "constant",
            Verdict::Invalid(_) => "invalid",
        }
    }
}

pub(super) fn chip_name(id: OsId) -> &'static str {
    match id {
        OsId::Main => "Main",
        OsId::Voice => "Voice",
        OsId::Panel => "Panel",
        OsId::Sam => "SAM",
    }
}

/// The pieces heard since the capture started (the latest message of each chip wins).
#[derive(Debug, Default)]
pub(super) struct Capture {
    pieces: Vec<Piece>,
}

impl Capture {
    fn add(&mut self, piece: Piece) {
        self.pieces.retain(|old| old.id != piece.id);
        self.pieces.push(piece);
        self.pieces.sort_by_key(|p| p.id as u8);
    }
}

/// Reads a SysEx as an OS message; `None` when it is not one (another DSI message, foreign SysEx).
pub(super) fn classify(message: &[u8]) -> Option<Piece> {
    if message.len() < 5 || message[..3] != DSI_PREFIX {
        return None;
    }
    let id = OsId::from_byte(message[3])?;
    let verdict = match os_image::decode(message) {
        Ok(os) => verdict_of(&os.image, os.checksum),
        Err(e) => Verdict::Invalid(describe(&e)),
    };
    Some(Piece {
        id,
        message: message.to_vec(),
        verdict,
    })
}

fn verdict_of(image: &[u8], checksum: u16) -> Verdict {
    let sha256 = sha256_hex(image);
    match image.first() {
        Some(&byte) if image.iter().all(|b| *b == byte) => Verdict::Constant { byte, sha256 },
        _ => {
            let (written, flasher) = split_flasher(image);
            Verdict::Valid {
                checksum,
                sha256,
                written_sha256: sha256_hex(written),
                flasher_sha256: sha256_hex(flasher),
            }
        }
    }
}

/// The image cut where an OS update stops writing, and the flasher page after it.
fn split_flasher(image: &[u8]) -> (&[u8], &[u8]) {
    let written = written_range(image);
    (written, &image[written.len()..])
}

fn describe(error: &OsImageError) -> String {
    format!("refused: {error}")
}

/// The text of `report.txt`.
fn report(stamp: &str, pieces: &[Piece]) -> String {
    let mut out = format!("Tempester backup of the machine's OS, {stamp} (UTC)\n\n");
    for piece in pieces {
        out.push_str(&format!(
            "{} (id {:#04x}): {} bytes of SysEx, {}\n",
            piece.chip(),
            piece.id as u8,
            piece.message.len(),
            piece.detail()
        ));
        match &piece.verdict {
            Verdict::Valid {
                sha256,
                written_sha256,
                flasher_sha256,
                ..
            } => {
                out.push_str(&format!("  image SHA-256 {sha256}\n"));
                if piece.id == OsId::Main {
                    out.push_str(&format!(
                        "  written range (below the flasher page) SHA-256 {written_sha256}\n  \
                         flasher page (the machine's own, never rewritten by an update) SHA-256 {flasher_sha256}\n"
                    ));
                }
            }
            Verdict::Constant { sha256, .. } => {
                out.push_str(&format!(
                    "  image SHA-256 {sha256}\n  FLAG: constant image, not a usable backup\n"
                ));
            }
            Verdict::Invalid(_) => out.push_str("  not saved\n"),
        }
    }
    out
}

fn piece_json(piece: &Piece) -> Json {
    obj([
        ("id", (piece.id as u8).into()),
        ("chip", piece.chip().into()),
        ("size", piece.message.len().into()),
        ("verdict", piece.verdict_name().into()),
        ("detail", piece.detail().into()),
    ])
}

impl Machine {
    pub(super) fn handle_backup(&mut self, command: Command) -> Option<Command> {
        match command {
            Command::BackupStart => self.guard("backup_start", Machine::backup_start),
            Command::BackupStop => self.stop_backup(),
            Command::BackupSave => self.guard("backup_save", |m| m.backup_save(&now_stamp())),
            other => return Some(other),
        }
        None
    }

    /// Feeds a SysEx heard on the port to the capture and to the post-install check, whichever is listening.
    pub(super) fn on_sysex(&mut self, message: &[u8]) {
        if self.backup.is_none() && self.verify.is_none() {
            return;
        }
        let Some(piece) = classify(message) else {
            return;
        };
        if let Some(capture) = &mut self.backup {
            capture.add(piece.clone());
            self.publish_backup("listening");
        }
        self.on_verify_piece(&piece);
    }

    fn backup_start(&mut self) -> Result<(), AppError> {
        self.idle_session()?;
        self.backup.get_or_insert_with(Capture::default);
        self.publish_backup("listening");
        Ok(())
    }

    fn stop_backup(&mut self) {
        self.backup = None;
        self.publish_backup("idle");
    }

    fn publish_backup(&self, state: &'static str) {
        let pieces: Vec<Json> = self
            .backup
            .as_ref()
            .map(|c| c.pieces.iter().map(piece_json).collect())
            .unwrap_or_default();
        self.emit(
            Event::new("backup")
                .with("state", state)
                .with("pieces", pieces),
        );
    }

    /// Writes `saved` and a report on everything heard (`all`) into a new `backups/<stamp>/`; returns the folder and
    /// the files written. A constant image is saved under a name that says it is suspect.
    pub(super) fn write_backup(
        &self,
        stamp: &str,
        saved: &[Piece],
        all: &[Piece],
    ) -> Result<(String, Vec<String>), AppError> {
        let dir = self.storage.new_backup_dir(stamp)?;
        let mut files = Vec::new();
        for piece in saved {
            let stem = match piece.verdict {
                Verdict::Constant { byte, .. } => {
                    format!("{} (constant {byte:#04x} - suspect)", piece.chip())
                }
                _ => piece.chip().to_string(),
            };
            let path = self.storage.save_in(&dir, &stem, "syx", &piece.message)?;
            files.push(path.display().to_string());
        }
        self.storage
            .save_in(&dir, "report", "txt", report(stamp, all).as_bytes())?;
        Ok((dir.display().to_string(), files))
    }

    fn backup_save(&mut self, stamp: &str) -> Result<(), AppError> {
        let pieces: Vec<Piece> = self
            .backup
            .as_ref()
            .map(|c| c.pieces.iter().filter(|p| p.saveable()).cloned().collect())
            .unwrap_or_default();
        if pieces.is_empty() {
            return Err(AppError::Precondition(
                "nothing valid has been captured yet",
            ));
        }
        let all: Vec<Piece> = self
            .backup
            .as_ref()
            .map(|c| c.pieces.clone())
            .unwrap_or_default();
        let (dir, files) = self.write_backup(stamp, &pieces, &all)?;
        self.backup = None;
        self.emit(
            Event::new("backup_saved")
                .with("dir", dir)
                .with("files", files)
                .with(
                    "flagged",
                    pieces.iter().any(|p| p.verdict_name() == "constant"),
                ),
        );
        self.publish_backup("idle");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::fake_machine::{debug_send, debug_send_messages};
    use crate::machine::testkit::TestRig;
    use crate::storage::Folder;

    fn listening(label: &str) -> TestRig {
        let mut rig = TestRig::connected(label);
        rig.machine.handle(Command::BackupStart);
        rig.events();
        rig
    }

    fn hear_debug_sends(rig: &mut TestRig) {
        debug_send(&rig.transport());
        rig.machine.poll_incoming();
    }

    #[test]
    fn test_classify_when_debug_sends_returns_main_ok_voice_constant_panel_ok() {
        let pieces: Vec<Piece> = debug_send_messages()
            .iter()
            .filter_map(|m| classify(m))
            .collect();

        let verdicts: Vec<&str> = pieces.iter().map(Piece::verdict_name).collect();
        assert_eq!(verdicts, vec!["ok", "constant", "ok"]);
    }

    #[test]
    fn test_classify_when_checksum_corrupted_returns_invalid() {
        let mut message = debug_send_messages().remove(0);
        let last = message.len() - 3;
        message[last] ^= 0x01;

        let piece = classify(&message).unwrap();

        assert_eq!(piece.verdict_name(), "invalid");
        assert!(piece.detail().contains("checksum"), "{}", piece.detail());
    }

    #[test]
    fn test_classify_when_truncated_or_foreign_returns_invalid_or_none() {
        let message = debug_send_messages().remove(0);

        assert_eq!(
            classify(&message[..message.len() / 2])
                .unwrap()
                .verdict_name(),
            "invalid"
        );
        assert_eq!(classify(&[0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7]), None);
        assert_eq!(classify(&[0xF0, 0x01, 0x28, 0x60, 0x00, 0xF7]), None);
    }

    #[test]
    fn test_poll_incoming_when_debug_sends_arrive_returns_three_pieces_with_voice_flagged() {
        let mut rig = listening("backup-listen");

        hear_debug_sends(&mut rig);

        let events = rig.events();
        let last = events.iter().rev().find(|e| e.name == "backup").unwrap();
        match last.field("pieces") {
            Some(Json::Arr(pieces)) => assert_eq!(pieces.len(), 3),
            other => panic!("unexpected {other:?}"),
        }
        assert!(last.render().contains(r#""chip":"Voice","size""#));
        assert!(last.render().contains(r#""verdict":"constant""#));
    }

    #[test]
    fn test_poll_incoming_when_not_listening_returns_nothing_captured() {
        let mut rig = TestRig::connected("backup-idle");

        hear_debug_sends(&mut rig);

        assert!(rig.machine.backup.is_none());
        assert!(rig.events().is_empty());
    }

    #[test]
    fn test_backup_save_when_captured_returns_files_report_and_constant_flag() {
        let mut rig = listening("backup-save");
        hear_debug_sends(&mut rig);
        rig.events();

        rig.machine.backup_save("2026-11-07_120000Z").unwrap();

        let saved = rig.expect("backup_saved");
        assert_eq!(saved.field("flagged"), Some(&true.into()));
        let dir = rig
            .machine
            .storage
            .folder(Folder::Backups)
            .unwrap()
            .join("2026-11-07_120000Z");
        let mut names: Vec<String> = std::fs::read_dir(dir.clone())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            vec![
                "Main.syx",
                "Panel.syx",
                "Voice (constant 0xff - suspect).syx",
                "report.txt"
            ]
        );
        let report = std::fs::read_to_string(dir.join("report.txt")).unwrap();
        assert!(report.contains("FLAG: constant image"), "{report}");
        assert_eq!(
            std::fs::read(dir.join("Main.syx")).unwrap(),
            debug_send_messages()[0]
        );
    }

    #[test]
    fn test_backup_save_when_only_an_invalid_message_was_heard_returns_error_and_no_folder() {
        let mut rig = listening("backup-invalid");
        let mut bad = debug_send_messages().remove(0);
        let last = bad.len() - 3;
        bad[last] ^= 0x01;
        rig.transport().push_incoming(&bad);
        rig.machine.poll_incoming();
        rig.events();

        rig.machine.handle(Command::BackupSave);

        assert_eq!(
            rig.expect("error").field("code"),
            Some(&"precondition".into())
        );
        let backups = rig.machine.storage.folder(Folder::Backups).unwrap();
        assert_eq!(std::fs::read_dir(backups).unwrap().count(), 0);
    }

    #[test]
    fn test_backup_stop_when_listening_returns_idle_and_drops_the_pieces() {
        let mut rig = listening("backup-stop");
        hear_debug_sends(&mut rig);

        rig.machine.handle(Command::BackupStop);

        assert!(rig.machine.backup.is_none());
        assert_eq!(
            rig.events().last().and_then(|e| e.field("state").cloned()),
            Some("idle".into())
        );
    }

    #[test]
    fn test_backup_start_when_not_connected_returns_not_connected_error() {
        let mut rig = TestRig::new("backup-off");

        rig.machine.handle(Command::BackupStart);

        assert_eq!(
            rig.expect("error").field("code"),
            Some(&"not_connected".into())
        );
    }

    #[test]
    fn test_add_when_same_chip_heard_twice_returns_only_the_latest() {
        let mut capture = Capture::default();
        let main = classify(&debug_send_messages()[0]).unwrap();

        capture.add(main.clone());
        capture.add(main);

        assert_eq!(capture.pieces.len(), 1);
    }
}
