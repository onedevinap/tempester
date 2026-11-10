//! OS update. Two ways to get a file ready: build a Tempester version out of the user's own DSI 1.5.0.2 Main file
//! (SHA-256 checked, patches from the generated tables, and the built image and `.syx` compared with the reference
//! hashes of the version), or validate any `.syx` OS file, which is recognised by its SHA-256 or called an unknown
//! build; plus "back to DSI 1.5.0.2". Nothing is sent until `os_send`, which only runs on a file that is ready, at the
//! cable rate, on its own thread, with progress and a cancel. The page asks for the user's confirmation before
//! `os_send`; an unknown build is refused by `os_send` and needs `os_send_unknown`, behind an extra confirmation.
//! A Main file sent to the end is remembered as the installed OS (`storage::installed`), to be read back later
//! (`os_verify`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use tempest_midi::paced::DEFAULT_RATE;
use tempest_midi::{send_paced, CancelToken, MidiError, Transport};
use tempest_proto::os_image::{self, OsId, OsImage};
use tempest_proto::sha256::sha256_hex;
use tempest_proto::versions::{self, Version, MAIN_ORIGIN_SHA256};

use super::backup::chip_name;
use super::Machine;
use crate::command::Command;
use crate::error::AppError;
use crate::event::{Emitter, Event};
use crate::json::{obj, Json};
use crate::storage::installed::{features_json, InstalledOs};
use crate::storage::stamp::now_stamp;
use crate::storage::Storage;

const DSI_ORIGINAL_LABEL: &str = "DSI 1.5.0.2 (original)";

/// What the reference hashes say a ready file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Origin {
    /// The `.syx` of this Tempester version (built here, or a file whose SHA-256 is the recorded one).
    Reference(&'static str),
    /// The DSI Main 1.5.0.2 image every patch is built on.
    DsiOriginal,
    /// Matches nothing we know: sending it needs an explicit extra confirmation.
    Unknown,
}

impl Origin {
    pub(super) fn is_known(self) -> bool {
        self != Self::Unknown
    }

    pub(super) fn version(self) -> Option<&'static str> {
        match self {
            Self::Reference(version) => Some(version),
            _ => None,
        }
    }

    /// The name the page and the installed record give the file.
    fn label(self, file_name: &str) -> String {
        match self {
            Self::Reference(version) => format!("Tempester {version}"),
            Self::DsiOriginal => DSI_ORIGINAL_LABEL.to_string(),
            Self::Unknown => file_name.to_string(),
        }
    }
}

/// Recognises a Main file by the SHA-256 of its `.syx` (a Tempester version of `table`) or of its image (the DSI
/// original); anything else, and every file for another chip, is unknown.
fn identify(os: &OsImage, syx_sha256: &str, table: &[Version]) -> Origin {
    if os.id != OsId::Main {
        return Origin::Unknown;
    }
    if let Some(version) = table.iter().find(|v| v.sha256_syx == syx_sha256) {
        return Origin::Reference(version.version);
    }
    if sha256_hex(&os.image) == MAIN_ORIGIN_SHA256 {
        return Origin::DsiOriginal;
    }
    Origin::Unknown
}

/// A file that passed every check and waits for the user's go.
pub(super) struct PendingOs {
    label: String,
    id: OsId,
    syx: Arc<Vec<u8>>,
    sha256_image: String,
    sha256_image_written: String,
    origin: Origin,
}

/// Version numbers as integers, so that 1.10.0.0 sorts after 1.9.0.0.
fn version_key(version: &str) -> Vec<u32> {
    version
        .split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

fn sentences(list: &[&str]) -> Json {
    Json::Arr(list.iter().map(|s| (*s).into()).collect())
}

fn versions_event() -> Event {
    let mut all: Vec<_> = versions::versions().iter().collect();
    all.sort_by_key(|v| std::cmp::Reverse(version_key(v.version)));
    let list: Vec<Json> = all
        .iter()
        .map(|v| {
            obj([
                ("version", v.version.into()),
                ("name", v.name.into()),
                ("kind", v.kind.as_str().into()),
                ("date", v.date.into()),
                ("summary", v.summary.into()),
                (
                    "changes",
                    obj([
                        ("fixes", sentences(v.changes.fixes)),
                        ("additions", sentences(v.changes.additions)),
                        ("changes", sentences(v.changes.changes)),
                    ]),
                ),
                ("description", v.description.into()),
                ("sha256_image", v.sha256_image.into()),
                ("sha256_image_written", v.sha256_image_written.into()),
                ("sha256_syx", v.sha256_syx.into()),
            ])
        })
        .collect();
    Event::new("os_versions").with("versions", list)
}

fn seconds_for(bytes: usize, rate: u32) -> usize {
    bytes.div_ceil(rate.max(1) as usize)
}

impl Machine {
    pub(super) fn handle_os(&mut self, command: Command) -> Option<Command> {
        match command {
            Command::OsVersions => self.emit(versions_event()),
            Command::OsBuild { version, data } => {
                self.guard_ready("os_build", |m| m.os_build(&version, &data));
            }
            Command::OsValidate { name, data } => {
                self.guard_ready("os_validate", |m| m.os_validate(&name, &data));
            }
            Command::OsOriginal { data } => {
                self.guard_ready("os_original", |m| m.os_original(&data));
            }
            Command::OsSend => self.guard("os_send", |m| m.os_send(false)),
            Command::OsSendUnknown => self.guard("os_send", |m| m.os_send(true)),
            Command::OsCancel => self.guard("os_cancel", Machine::os_cancel),
            Command::OsVerifyStart => self.guard("os_verify", Machine::verify_start),
            Command::OsVerifyStop => self.verify_stop(),
            other => return Some(other),
        }
        None
    }

    /// Like `guard`, and a file that was refused leaves nothing ready: an older file must not stay sendable behind
    /// a refusal message.
    fn guard_ready(
        &mut self,
        during: &'static str,
        action: impl FnOnce(&mut Self) -> Result<(), AppError>,
    ) {
        self.guard(during, |m| {
            let result = action(m);
            if result.is_err() {
                m.pending_os = None;
            }
            result
        });
    }

    fn os_build(&mut self, version: &str, dsi_file: &[u8]) -> Result<(), AppError> {
        let wanted = versions::find_version(version)
            .ok_or_else(|| AppError::Invalid(format!("unknown Tempester version \"{version}\"")))?;
        self.build_ready(wanted, dsi_file)
    }

    /// Builds `wanted` and keeps it ready only when image and `.syx` equal the version's recorded hashes.
    fn build_ready(&mut self, wanted: &Version, dsi_file: &[u8]) -> Result<(), AppError> {
        let built =
            versions::build(wanted, dsi_file).map_err(|e| AppError::Invalid(e.to_string()))?;
        self.make_ready(
            built.syx,
            Origin::Reference(wanted.version),
            "built",
            String::new(),
        )
    }

    fn os_validate(&mut self, name: &str, file: &[u8]) -> Result<(), AppError> {
        let os = os_image::decode(file).map_err(|e| AppError::Invalid(e.to_string()))?;
        let origin = identify(&os, &sha256_hex(file), versions::versions());
        self.make_ready(file.to_vec(), origin, "file", name.to_string())
    }

    /// "Back to DSI 1.5.0.2": only the user's original Main file is accepted (its image hash is known).
    fn os_original(&mut self, file: &[u8]) -> Result<(), AppError> {
        let os = os_image::decode(file).map_err(|e| AppError::Invalid(e.to_string()))?;
        if os.id != OsId::Main || sha256_hex(&os.image) != MAIN_ORIGIN_SHA256 {
            return Err(AppError::Invalid(
                "this is not the DSI Main 1.5.0.2 file (Tempest_Main_1.5.0.2.syx)".into(),
            ));
        }
        self.make_ready(
            file.to_vec(),
            Origin::DsiOriginal,
            "original",
            String::new(),
        )
    }

    /// Keeps a checked file and tells the page what it is, how big, how long it takes, and whether it matches a
    /// reference (`known`, `version`) with both SHA-256.
    fn make_ready(
        &mut self,
        syx: Vec<u8>,
        origin: Origin,
        source: &'static str,
        file_name: String,
    ) -> Result<(), AppError> {
        let image = os_image::decode(&syx).map_err(|e| AppError::Invalid(e.to_string()))?;
        let label = origin.label(&file_name);
        let sha256_image = sha256_hex(&image.image);
        let sha256_image_written = sha256_hex(versions::written_range(&image.image));
        self.emit(
            Event::new("os_ready")
                .with("label", label.clone())
                .with("source", source)
                .with("chip", chip_name(image.id))
                .with("id", image.id as u8)
                .with("blocks", image.id.blocks())
                .with("checksum", image.checksum)
                .with("size", syx.len())
                .with("seconds", seconds_for(syx.len(), DEFAULT_RATE))
                .with("known", origin.is_known())
                .with("version", origin.version())
                .with("sha256_image", sha256_image.clone())
                .with("sha256_syx", sha256_hex(&syx)),
        );
        self.pending_os = Some(PendingOs {
            label,
            id: image.id,
            syx: Arc::new(syx),
            sha256_image,
            sha256_image_written,
            origin,
        });
        Ok(())
    }

    fn os_send(&mut self, unknown_confirmed: bool) -> Result<(), AppError> {
        if self.clock_running() {
            return Err(AppError::Precondition("stop the clock before an update"));
        }
        let transport = Arc::clone(
            &self
                .session
                .as_ref()
                .ok_or(AppError::NotConnected)?
                .transport,
        );
        let Some(ready) = &self.pending_os else {
            return Err(AppError::Precondition(
                "no OS file is ready: build or validate one first",
            ));
        };
        if !ready.origin.is_known() && !unknown_confirmed {
            return Err(AppError::Precondition(
                "this build matches no known version: sending it needs the extra confirmation",
            ));
        }
        if self.os_active.load(Ordering::SeqCst) || self.project_busy() {
            return Err(AppError::Busy);
        }
        // A key still held when the update starts could not be released any more.
        self.release_held_keys();
        self.release_touched_ribbons();
        self.verify = None;
        self.announced = None;
        self.os_active.store(true, Ordering::SeqCst);
        let Some(pending) = self.pending_os.take() else {
            return Ok(());
        };
        let cancel = CancelToken::new();
        self.os_cancel = Some(cancel.clone());
        let job = OsJob {
            transport,
            pending,
            rate: self.rate,
            cancel,
            emit: self.emit.clone(),
            storage: self.storage.clone(),
            active: ActiveGuard(Arc::clone(&self.os_active)),
        };
        thread::Builder::new()
            .name("tempester-os-send".into())
            .spawn(move || job.run())
            .map(|_| ())
            .map_err(|e| {
                self.os_active.store(false, Ordering::SeqCst);
                e.into()
            })
    }

    fn os_cancel(&mut self) -> Result<(), AppError> {
        match &self.os_cancel {
            Some(cancel) if self.os_active.load(Ordering::SeqCst) => {
                cancel.cancel();
                Ok(())
            }
            _ => Err(AppError::Precondition("no update is running")),
        }
    }
}

/// Clears the "update running" flag when the job ends, however it ends.
pub(super) struct ActiveGuard(pub(super) Arc<AtomicBool>);

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

struct OsJob {
    transport: Arc<dyn Transport>,
    pending: PendingOs,
    rate: u32,
    cancel: CancelToken,
    emit: Emitter,
    storage: Storage,
    active: ActiveGuard,
}

impl OsJob {
    fn run(self) {
        let total = self.pending.syx.len();
        let label = self.pending.label.clone();
        (self.emit)(
            Event::new("os_started")
                .with("label", label.clone())
                .with("chip", chip_name(self.pending.id))
                .with("total", total),
        );
        let result = self.send();
        let installed = self.remember(&result);
        let emit = Arc::clone(&self.emit);
        drop(self.active); // free the port before announcing the end: the page may ask for the identity at once
        if let Some(event) = installed {
            emit(event);
        }
        emit(finished_event(&label, total, result));
    }

    /// Updates the installed-OS record of a Main send: the file when every byte went out, nothing when the send was
    /// cancelled before the first byte, and "unknown" (no record) after a send that stopped half way. A send for
    /// another chip leaves the record alone.
    fn remember(&self, result: &Result<(), MidiError>) -> Option<Event> {
        if self.pending.id != OsId::Main {
            return None;
        }
        let record = match result {
            Ok(()) => Some(InstalledOs {
                label: self.pending.label.clone(),
                version: self.pending.origin.version().map(str::to_string),
                at: now_stamp(),
                sha256_image: self.pending.sha256_image.clone(),
                sha256_image_written: Some(self.pending.sha256_image_written.clone()),
                verified_at: None,
            }),
            Err(MidiError::Cancelled { sent: 0 }) => return None,
            Err(_) => None,
        };
        if let Err(error) = self.storage.set_installed_os(record.as_ref()) {
            (self.emit)(Event::error("os_installed", &error));
            return None;
        }
        let features = features_json(record.as_ref());
        Some(
            Event::new("installed")
                .with("installed", record.map(|r| r.to_json()))
                .with("features", features),
        )
    }

    fn send(&self) -> Result<(), MidiError> {
        let mut last_permille = usize::MAX;
        send_paced(
            self.transport.as_ref(),
            &self.pending.syx,
            self.rate,
            &self.cancel,
            &mut |progress| {
                let permille = progress.sent * 1000 / progress.total.max(1);
                if permille != last_permille {
                    last_permille = permille;
                    (self.emit)(
                        Event::new("os_progress")
                            .with("sent", progress.sent)
                            .with("total", progress.total),
                    );
                }
            },
        )
    }
}

fn finished_event(label: &str, total: usize, result: Result<(), MidiError>) -> Event {
    let event = Event::new("os_finished")
        .with("label", label)
        .with("total", total);
    match result {
        Ok(()) => event.with("state", "done").with("sent", total),
        Err(MidiError::Cancelled { sent }) => event.with("state", "cancelled").with("sent", sent),
        Err(other) => event
            .with("state", "failed")
            .with("message", other.to_string())
            .with("code", AppError::Midi(other).code()),
    }
}

#[cfg(test)]
mod reference_tests;
#[cfg(test)]
mod tests;
