//! The full machine backup (Tempester 1.6.6.0, n°62, `public-src/docs/midi-protocol.md` § Full backup): request
//! `F0 01 28 44 F7`, eight SysEx in answer, in a fixed order (six Voice flashes, the Main boot flash, the System settings
//! with the pad channels). Every message is checked as it arrives (`tempest_proto::full_backup`) and the set is judged
//! as a whole, its order included; only a set in which all eight pass is saved, into `backups/Full backup <UTC date>/`:
//! each message byte for byte as received, the System settings file the machine's own save would write and the 32 pad
//! channels (both from `76`), `manifest.json` (versions, SHA-256 of every file, the calibration of each Voice) and
//! `report.txt`. The Debug menu's Main, Panel and Voice-0 dumps heard while the backup is open join the same folder.
//! While the machine answers it reads no MIDI input: the app sends nothing that waits for an idle session meanwhile.
//!
//! Each Voice row also says which Voice OS the chip holds (`voice_checks`: Tempester Voice 1.5.1.0 when pages 0-40 equal
//! the image the app builds, DSI's Voice 1.5 when the user's file was seen, else an unknown build), whether pages 0-40
//! equal the Voice update the app last sent, and whether page 42 (the chip's calibration) is unchanged since the
//! previous full backup; the manifest and the report carry the same findings. Every Voice row whose message decoded
//! carries its 35 calibration words (`calibration`), refused or not. A calibration of unusual shape gives the row the
//! verdict `warning`: it can be saved, and the row, the manifest and the report say so. Neither the Voice OS update nor
//! anything else of the app relies on that shape: the comparisons with the previous backup and with the update sent
//! are made on the bytes of the pages, whatever their values.
//!
//! These files are backups for a programmer (PICkit) restore: no command of the app sends one back to the machine.

use std::time::{Duration, Instant};

use tempest_proto::full_backup::{
    self, calibration_raw, Backup, BackupError, CalibrationWarning, Chip, Content, SystemSettings,
    CALIBRATION_WORDS, REQUEST_FULL_BACKUP,
};
use tempest_proto::sha256::sha256_hex;

use self::voice_checks::Reference;
use super::backup::{classify, piece_json, Piece, Verdict};
use super::Machine;
use crate::command::Command;
use crate::error::AppError;
use crate::event::Event;
use crate::json::{obj, Json};
use crate::storage::installed::has_feature;
use crate::storage::stamp::now_stamp;

/// How long the machine may take to deliver the first message (Voice 0). The slowest message, as
/// `midi-protocol.md` § Full backup times it, is a Voice message on DIN: 74,951 bytes at 3,125 bytes a second (24 s),
/// after up to 1.6 s of skipping forward to the start of the chip on 1.6.6.0 / 1.6.7.0 (1.6.8.0 never skips). The app
/// gets a SysEx when its `F7` arrives, so each wait covers one whole message, here with more than three times that.
const FIRST_WAIT: Duration = Duration::from_secs(90);
/// Silence allowed between two messages: more than twice the slowest message.
const GAP_WAIT: Duration = Duration::from_secs(60);
const NEEDS: &str = "The full backup needs Tempester 1.6.6.0 or later";
const NEEDS_TEMPESTER: &str =
    "The full backup needs a Tempester OS: the machine answered as the DSI OS";
/// Before 1.6.8.0, n°62 re-aligns a Voice chip by reading forward, past the end of its flash on a second backup in the
/// same power-on: the chips stop (measured 14/11/2026; refs/firmware-voice.md §14.8).
pub(super) const ONE_PER_POWER_ON: &str = "One full backup per power-on on this OS (before Tempester 1.6.8.0): a second \
     one stops the voice chips. Power the machine off and on first, then confirm it (« I powered it off and on ») and \
     request again";

/// Whether a full backup may be requested now. The app cannot see a power cycle (nothing tells it on DIN, and a USB
/// port that comes back may only be a cable moved), so after one backup on an OS without `full_backup_repeat` it waits
/// for the user to say the machine was powered off and on. It cannot see a backup made before it started either: the
/// first request after the app starts is allowed, and the page tells the rule up front.
pub(super) fn second_backup_allowed(repeat_supported: bool, read_since_power_on: bool) -> bool {
    repeat_supported || !read_since_power_on
}

/// A message of the answer and what the checks made of it.
#[derive(Debug, Clone)]
struct Received {
    chip: Chip,
    message: Vec<u8>,
    outcome: Result<Backup, BackupError>,
    /// The image of a refused Voice message that still decoded: shown (its calibration words, its pages compared with
    /// another chip's), never saved.
    refused_image: Option<Vec<u8>>,
}

impl Received {
    fn new(chip: Chip, message: &[u8]) -> Self {
        let outcome = full_backup::decode(message);
        let refused_image = outcome
            .is_err()
            .then(|| full_backup::voice_image_of(message))
            .flatten();
        Self {
            chip,
            message: message.to_vec(),
            outcome,
            refused_image,
        }
    }

    /// The Voice image the row can show, accepted or refused.
    fn voice_image(&self) -> Option<&[u8]> {
        match (&self.outcome, self.chip) {
            (Ok(backup), Chip::Voice(_)) => Some(&backup.image),
            (Ok(_), _) => None,
            (Err(_), _) => self.refused_image.as_deref(),
        }
    }

    /// The calibration shapes the row warns of (none for a refused message or another chip).
    fn warnings(&self) -> Vec<CalibrationWarning> {
        match &self.outcome {
            Ok(Backup {
                content: Content::Voice(c),
                ..
            }) => c.warnings(),
            _ => Vec::new(),
        }
    }
}

/// The backup in progress.
#[derive(Debug)]
pub(super) struct FullBackup {
    received: Vec<Received>,
    /// The Debug menu's dumps heard meanwhile (Main, Voice 0, Panel).
    extras: Vec<Piece>,
    deadline: Instant,
    timed_out: bool,
    /// What the app knew of the Voice chips before the request went out.
    reference: Reference,
}

/// One line of the table the page shows.
struct Row {
    chip: Chip,
    size: usize,
    /// `ok`, `warning` (saved, with what is unusual in `detail`), `refused`, `waiting` or `missing`.
    verdict: &'static str,
    detail: String,
    /// The 35 calibration words of a Voice chip whose message decoded.
    calibration: Option<[u32; CALIBRATION_WORDS]>,
}

/// The verdicts a set can be saved with.
fn passes(verdict: &str) -> bool {
    matches!(verdict, "ok" | "warning")
}

impl FullBackup {
    fn new(now: Instant, reference: Reference) -> Self {
        Self {
            received: Vec::new(),
            extras: Vec::new(),
            deadline: now + FIRST_WAIT,
            timed_out: false,
            reference,
        }
    }

    /// Keeps every message in the order it came: a chip heard twice is an order problem, not a replacement.
    fn add(&mut self, item: Received) {
        self.received.push(item);
    }

    /// Still waiting for the machine: it reads no MIDI input until its last message is out.
    fn running(&self) -> bool {
        !self.timed_out && !self.complete()
    }

    fn add_extra(&mut self, piece: Piece) {
        self.extras.retain(|old| old.id != piece.id);
        self.extras.push(piece);
    }

    fn decoded(&self) -> Vec<Backup> {
        self.received
            .iter()
            .filter_map(|r| r.outcome.as_ref().ok().cloned())
            .collect()
    }

    /// The System settings and pad channels of `76`, once it passed its checks.
    fn system(&self) -> Option<SystemSettings> {
        self.received
            .iter()
            .find_map(|r| match r.outcome.as_ref().map(|b| &b.content) {
                Ok(Content::System(system)) => Some(system.clone()),
                _ => None,
            })
    }

    fn complete(&self) -> bool {
        Chip::all()
            .iter()
            .all(|chip| self.received.iter().any(|r| r.chip == *chip))
    }

    /// The table: every chip, with what its message was judged, the set-wide problems (order included) too.
    fn rows(&self) -> Vec<Row> {
        let arrived: Vec<Chip> = self.received.iter().map(|r| r.chip).collect();
        let mut problems = full_backup::check_order(&arrived);
        problems.extend(full_backup::check_set(&self.decoded()));
        Chip::all()
            .into_iter()
            .map(|chip| {
                let found = self.received.iter().find(|r| r.chip == chip);
                let problem = problems
                    .iter()
                    .find(|p| p.chip == chip && !matches!(p.error, BackupError::Missing(_)));
                let mut row = match (found, problem) {
                    (Some(r), _) if r.outcome.is_err() => self.refused_row(r),
                    (Some(r), Some(p)) => Row {
                        chip,
                        size: r.message.len(),
                        verdict: "refused",
                        detail: p.error.to_string(),
                        calibration: None,
                    },
                    (Some(r), None) => self.passed_row(r),
                    (None, _) => Row {
                        chip,
                        size: 0,
                        verdict: if self.timed_out { "missing" } else { "waiting" },
                        detail: if self.timed_out {
                            BackupError::Missing(chip).to_string()
                        } else {
                            String::new()
                        },
                        calibration: None,
                    },
                };
                row.calibration = found
                    .and_then(Received::voice_image)
                    .and_then(calibration_raw);
                row
            })
            .collect()
    }

    /// A message that passed: `ok`, or `warning` when its calibration has an unusual shape. A Voice row adds what the
    /// chip holds (`voice_checks`) and, when it warns, whether its stream was in step with another chip's.
    fn passed_row(&self, r: &Received) -> Row {
        let warnings = r.warnings();
        let mut parts = vec![ok_detail(&r.outcome)];
        parts.extend(calibration::warning_text(&warnings));
        if let (Ok(backup), Chip::Voice(k)) = (&r.outcome, r.chip) {
            parts.push(self.reference.check(k, &backup.image).sentence());
            if !warnings.is_empty() {
                parts.extend(self.in_step(k, &backup.image));
            }
        }
        Row {
            chip: r.chip,
            size: r.message.len(),
            verdict: if warnings.is_empty() { "ok" } else { "warning" },
            detail: parts.join("; "),
            calibration: None,
        }
    }

    /// A refused message, with its reason. A Voice chip refused for its calibration page, after its alignment, code
    /// pages and boot page passed, also says what its pages 0-41 are: equal to another chip's, they were read in step.
    fn refused_row(&self, r: &Received) -> Row {
        let mut parts = vec![r
            .outcome
            .as_ref()
            .err()
            .map(ToString::to_string)
            .unwrap_or_default()];
        if let (Err(BackupError::Calibration(_)), Chip::Voice(k), Some(image)) =
            (&r.outcome, r.chip, r.voice_image())
        {
            parts.push(self.reference.check(k, image).sentence());
            parts.extend(self.in_step(k, image));
        }
        Row {
            chip: r.chip,
            size: r.message.len(),
            verdict: "refused",
            detail: parts.join("; "),
            calibration: None,
        }
    }

    /// Whether Voice `k`'s pages 0-41 equal those of the first other chip whose message decoded (the six chips run the
    /// same code and boot page, so equal pages say the two streams were read in step up to the calibration page).
    fn in_step(&self, k: u8, image: &[u8]) -> Option<String> {
        let (other, theirs) = self.received.iter().find_map(|r| match r.chip {
            Chip::Voice(j) if j != k => r.voice_image().map(|i| (j, i)),
            _ => None,
        })?;
        Some(
            match full_backup::differences_before_calibration(theirs, image) {
                0 => format!(
                    "pages 0-41 identical to Voice {other}'s: read in step up to the calibration page"
                ),
                n => format!("{n} byte(s) of pages 0-41 differ from Voice {other}'s"),
            },
        )
    }

    fn all_ok(&self) -> bool {
        self.complete() && self.rows().iter().all(|row| passes(row.verdict))
    }

    /// The Voice chips that passed with a calibration warning.
    fn warned(&self) -> usize {
        self.received
            .iter()
            .filter(|r| !r.warnings().is_empty())
            .count()
    }

    /// `waiting`, `receiving`, `ready` (every check passed: it can be saved) or `refused`.
    fn state(&self) -> &'static str {
        if self.all_ok() {
            "ready"
        } else if self.complete() || self.timed_out {
            "refused"
        } else if self.received.is_empty() {
            "waiting"
        } else {
            "receiving"
        }
    }
}

fn ok_detail(outcome: &Result<Backup, BackupError>) -> String {
    match outcome.as_ref().map(|b| &b.content) {
        Ok(Content::Voice(_)) => "aligned, boot page present, calibration read (35 words)".into(),
        Ok(Content::MainBoot(boot)) => format!(
            "12 blocks, checksum OK, configuration words {:08x} {:08x} {:08x} {:08x}",
            boot.devcfg[0], boot.devcfg[1], boot.devcfg[2], boot.devcfg[3]
        ),
        Ok(Content::System(system)) => format!(
            "version {}, {} settings, {} pad channels",
            system.version,
            system.settings.len(),
            system.pad_channels.len()
        ),
        Err(_) => String::new(),
    }
}

fn row_json(row: &Row) -> Json {
    obj([
        ("chip", row.chip.label().into()),
        ("id", row.chip.id().into()),
        ("size", row.size.into()),
        ("verdict", row.verdict.into()),
        ("detail", row.detail.clone().into()),
        (
            "calibration",
            row.calibration
                .as_ref()
                .map_or(Json::Null, calibration::json),
        ),
    ])
}

impl Machine {
    pub(super) fn handle_full_backup(&mut self, command: Command) -> Option<Command> {
        match command {
            Command::FullBackupStart => self.guard("full_backup_start", Machine::full_start),
            Command::FullBackupCancel => self.full_cancel(),
            Command::FullBackupSave => {
                self.guard("full_backup_save", |m| m.full_save(&now_stamp()));
            }
            Command::FullBackupPowerCycled => {
                self.full_backup_read = false;
                self.emit(Event::new("full_backup_power_cycled"));
            }
            other => return Some(other),
        }
        None
    }

    fn require_full_backup(&self) -> Result<(), AppError> {
        let announced = self.announced_text();
        let record = self.storage.installed_os();
        let allowed = has_feature(
            announced.as_deref(),
            record.as_ref(),
            "full_backup",
            self.connector.is_fake(),
            self.dsi_identity,
        );
        match (allowed, self.dsi_identity) {
            (true, _) => Ok(()),
            (false, true) => Err(AppError::Precondition(NEEDS_TEMPESTER)),
            (false, false) => Err(AppError::Precondition(NEEDS)),
        }
    }

    /// Whether a full backup is waiting for the machine's messages: it reads no MIDI input until the last is out.
    pub(super) fn full_backup_running(&self) -> bool {
        self.full_backup.as_ref().is_some_and(FullBackup::running)
    }

    /// Whether the machine's OS reads every Voice dump without reading forward (n°62 in its 1.6.8.0 form).
    fn full_backup_repeatable(&self) -> bool {
        let announced = self.announced_text();
        has_feature(
            announced.as_deref(),
            self.storage.installed_os().as_ref(),
            "full_backup_repeat",
            self.connector.is_fake(),
            self.dsi_identity,
        )
    }

    fn full_start(&mut self) -> Result<(), AppError> {
        self.require_full_backup()?;
        if !second_backup_allowed(self.full_backup_repeatable(), self.full_backup_read) {
            return Err(AppError::Precondition(ONE_PER_POWER_ON));
        }
        let session = self.idle_session()?;
        session.listener.drain();
        self.send_bytes(&REQUEST_FULL_BACKUP)?;
        self.full_backup_read = true;
        self.full_backup = Some(FullBackup::new(
            Instant::now(),
            Reference::load(&self.storage),
        ));
        self.publish_full_backup();
        Ok(())
    }

    fn full_cancel(&mut self) {
        if self.full_backup.take().is_some() {
            self.emit(Event::new("full_backup").with("state", "idle"));
        }
    }

    /// Feeds a SysEx heard on the port to the backup in progress: an answer message is judged, a Debug-menu dump is
    /// kept for the same folder.
    pub(super) fn on_full_backup_sysex(&mut self, message: &[u8]) {
        let Some(backup) = self.full_backup.as_mut() else {
            return;
        };
        let chip = message
            .get(3)
            .filter(|_| message.len() >= 5 && message[..3] == tempest_proto::DSI_PREFIX)
            .and_then(|id| Chip::from_id(*id));
        match chip {
            Some(chip) => {
                backup.add(Received::new(chip, message));
                backup.deadline = Instant::now() + GAP_WAIT;
            }
            None => match classify(message) {
                Some(piece) => backup.add_extra(piece),
                None => return,
            },
        }
        self.publish_full_backup();
    }

    /// Ends the wait when the machine went silent: the chips that never came are named.
    pub(super) fn expire_full_backup(&mut self, now: Instant) {
        let Some(backup) = self.full_backup.as_mut() else {
            return;
        };
        if backup.timed_out || backup.complete() || now < backup.deadline {
            return;
        }
        backup.timed_out = true;
        self.publish_full_backup();
    }

    fn publish_full_backup(&self) {
        let Some(backup) = self.full_backup.as_ref() else {
            return;
        };
        let rows: Vec<Json> = backup.rows().iter().map(row_json).collect();
        let extras: Vec<Json> = backup.extras.iter().map(piece_json).collect();
        self.emit(
            Event::new("full_backup")
                .with("state", backup.state())
                .with("rows", rows)
                .with("extras", extras),
        );
    }

    fn full_save(&mut self, stamp: &str) -> Result<(), AppError> {
        let backup =
            self.full_backup
                .as_ref()
                .filter(|b| b.all_ok())
                .ok_or(AppError::Precondition(
                    "nothing to save: the backup is incomplete or a message was refused",
                ))?;
        let announced = self.announced_text();
        let files = self.write_full_backup(stamp, backup, announced.as_deref())?;
        // The next backup compares each chip's calibration page with this one.
        let calibration: Vec<String> = voice_checks::checks(&backup.reference, &backup.decoded())
            .into_iter()
            .map(|(_, check)| check.calibration)
            .collect();
        self.storage.set_last_calibration(&calibration)?;
        self.full_backup = None;
        self.emit(
            Event::new("full_backup_saved")
                .with("dir", files.dir)
                .with("files", files.names),
        );
        self.emit(Event::new("full_backup").with("state", "idle"));
        Ok(())
    }
}

/// Where a backup went.
struct Written {
    dir: String,
    names: Vec<String>,
}

impl Machine {
    fn write_full_backup(
        &self,
        stamp: &str,
        backup: &FullBackup,
        announced: Option<&str>,
    ) -> Result<Written, AppError> {
        let dir = self
            .storage
            .new_backup_dir(&format!("Full backup {stamp}"))?;
        let mut listed = Vec::new();
        let mut names = Vec::new();
        let mut write = |stem: &str, ext: &str, bytes: &[u8]| -> Result<(), AppError> {
            let path = self.storage.save_in(&dir, stem, ext, bytes)?;
            names.push(path.display().to_string());
            Ok(())
        };
        for chip in Chip::all() {
            let Some(item) = backup.received.iter().find(|r| r.chip == chip) else {
                continue;
            };
            write(&chip.label(), "syx", &item.message)?;
            listed.push(file_json(&chip.label(), &item.message, chip.id()));
        }
        for piece in backup.extras.iter().filter(|p| p.saveable()) {
            let stem = extra_stem(piece);
            write(&stem, "syx", &piece.message)?;
            listed.push(file_json(&stem, &piece.message, piece.id as u8));
        }
        // `all_ok` holds a checked `76`: these two files always join the folder.
        if let Some(system) = backup.system() {
            let file = system.machine_file();
            write(SYSTEM_FILE_STEM, "bin", &file)?;
            listed.push(derived_json(&format!("{SYSTEM_FILE_STEM}.bin"), &file));
            let json = pad_channels_json(&system.pad_channels).render();
            write(PAD_CHANNELS_STEM, "json", json.as_bytes())?;
            listed.push(derived_json(
                &format!("{PAD_CHANNELS_STEM}.json"),
                json.as_bytes(),
            ));
        }
        let manifest = manifest(stamp, announced, listed, backup);
        write("manifest", "json", manifest.render().as_bytes())?;
        write("report", "txt", report(stamp, announced, backup).as_bytes())?;
        Ok(Written {
            dir: dir.display().to_string(),
            names,
        })
    }
}

fn extra_stem(piece: &Piece) -> String {
    let stem = match piece.chip() {
        "Voice" => "Voice (Debug menu - voice 0)",
        other => other,
    };
    match piece.verdict {
        Verdict::Constant { byte, .. } => format!("{stem} (constant {byte:#04x} - suspect)"),
        _ => stem.to_string(),
    }
}

fn file_json(stem: &str, message: &[u8], id: u8) -> Json {
    obj([
        ("name", format!("{stem}.syx").into()),
        ("id", id.into()),
        ("size", message.len().into()),
        ("sha256", sha256_hex(message).into()),
    ])
}

/// A file the app built from `76` rather than received.
fn derived_json(name: &str, bytes: &[u8]) -> Json {
    obj([
        ("name", name.into()),
        ("from_id", Chip::System.id().into()),
        ("size", bytes.len().into()),
        ("sha256", sha256_hex(bytes).into()),
    ])
}

fn hex_words(words: &[u16]) -> Json {
    words
        .iter()
        .map(|w| format!("{w:04x}"))
        .collect::<Vec<_>>()
        .into()
}

fn texts(items: &[CalibrationWarning]) -> Json {
    items
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .into()
}

fn voice_json(backup: &Backup, reference: &Reference) -> Option<Json> {
    let (Chip::Voice(k), Content::Voice(c)) = (backup.chip, &backup.content) else {
        return None;
    };
    let check = reference.check(k, &backup.image);
    let [os, pages, equals, calibration, unchanged] = check.json();
    Some(obj([
        ("voice", k.into()),
        ("image_sha256", sha256_hex(&backup.image).into()),
        os,
        pages,
        equals,
        calibration,
        unchanged,
        ("osc1", hex_words(c.osc1())),
        ("osc2", hex_words(c.osc2())),
        ("linear", hex_words(c.linear())),
        ("scalars", hex_words(c.scalars())),
        ("calibration_warnings", texts(&c.warnings())),
    ]))
}

/// The System settings file rebuilt from `76` (`SystemSettings::machine_file`).
const SYSTEM_FILE_STEM: &str = "System settings file";
const PAD_CHANNELS_STEM: &str = "Pad channels";

/// The 32 pad channels (n°58) as `76` carries them.
fn pad_channels_json(table: &[u8]) -> Json {
    obj([
        ("kind", "pad_channels".into()),
        ("channels", table.to_vec().into()),
        (
            "meaning",
            "0 usual route, 1-16 channel with the played note, 17-32 channel (value - 16) with the pad's MIDI note".into(),
        ),
    ])
}

fn manifest(stamp: &str, announced: Option<&str>, files: Vec<Json>, backup: &FullBackup) -> Json {
    let decoded = backup.decoded();
    let voices: Vec<Json> = decoded
        .iter()
        .filter_map(|b| voice_json(b, &backup.reference))
        .collect();
    let boot = decoded.iter().find_map(|b| match &b.content {
        Content::MainBoot(boot) => Some(obj([
            ("reset_word", format!("{:08x}", boot.reset_word).into()),
            (
                "devcfg3_to_0",
                boot.devcfg
                    .iter()
                    .map(|w| format!("{w:08x}"))
                    .collect::<Vec<_>>()
                    .into(),
            ),
        ])),
        _ => None,
    });
    obj([
        ("kind", "full_backup".into()),
        ("stamp_utc", stamp.into()),
        ("app_version", env!("CARGO_PKG_VERSION").into()),
        ("tempester_version", announced.map(str::to_string).into()),
        (
            "restore",
            "for a programmer (PICkit) restore: never send these files to the machine".into(),
        ),
        ("files", files.into()),
        ("voices", voices.into()),
        ("main_boot_flash", boot.unwrap_or(Json::Null)),
        (
            "pad_channels",
            backup.system().map(|s| s.pad_channels.to_vec()).into(),
        ),
    ])
}

fn report(stamp: &str, announced: Option<&str>, backup: &FullBackup) -> String {
    let mut out = format!("Tempester full backup of the machine, {stamp} (UTC)\n");
    out.push_str(&format!(
        "Tempester version reported by the machine: {}\n",
        announced.unwrap_or("not reported")
    ));
    out.push_str(
        "These files are for a programmer (PICkit) restore. Never send them to the machine.\n",
    );
    out.push_str(
        "The calibration words below are this machine's own measurements: you may quote them in a bug report. Never \
         post the Voice .syx files: they hold DSI's Voice OS.\n",
    );
    if backup.warned() > 0 {
        out.push_str(&format!(
            "WARNING: {} Voice calibration page(s) with an unusual shape, saved exactly as read (see their lines)\n",
            backup.warned()
        ));
    }
    out.push('\n');
    for row in backup.rows() {
        out.push_str(&format!(
            "{} (id {:#04x}): {} bytes of SysEx, {}\n",
            row.chip.label(),
            row.chip.id(),
            row.size,
            row.detail
        ));
    }
    out.push_str(&format!(
        "System settings: {SYSTEM_FILE_STEM}.bin is the 406-byte file the machine's own save writes, rebuilt from the \
         System settings message; the 32 pad channels (n°58) of that message are in {PAD_CHANNELS_STEM}.json\n"
    ));
    let decoded = backup.decoded();
    for voice in decoded.iter().filter(|b| matches!(b.chip, Chip::Voice(_))) {
        out.push_str(&format!(
            "{} image SHA-256 {}\n",
            voice.chip.label(),
            sha256_hex(&voice.image)
        ));
        if let (Chip::Voice(k), Content::Voice(c)) = (voice.chip, &voice.content) {
            out.push_str(&format!(
                "  {}\n",
                backup.reference.check(k, &voice.image).sentence()
            ));
            if let Some(warning) = calibration::warning_text(&c.warnings()) {
                out.push_str(&format!("  {warning}\n"));
            }
            out.extend(calibration_raw(&voice.image).map(|raw| calibration::lines(&raw)));
        }
        let first = decoded.iter().find(|b| b.chip == Chip::Voice(0));
        if let (Some(first), Chip::Voice(k)) = (first, voice.chip) {
            let differ = full_backup::code_differences(first, voice);
            if k > 0 && differ > 0 {
                out.push_str(&format!(
                    "  NOTE: {differ} bytes of pages 0-40 differ from Voice 0 (the chips normally run the same code)\n"
                ));
            }
        }
    }
    for piece in &backup.extras {
        out.push_str(&format!(
            "{} (Debug menu, id {:#04x}): {} bytes, {}\n",
            piece.chip(),
            piece.id as u8,
            piece.message.len(),
            piece.detail()
        ));
    }
    out
}

mod calibration;
mod voice_checks;

#[cfg(test)]
mod tests;
