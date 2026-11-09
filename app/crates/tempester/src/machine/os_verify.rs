//! "Verify the installed OS": after an update, the user opens the Debug menu on the machine (both SHIFT and SYSTEM)
//! and chooses "Send Main OS". The app listens for that message, checks it like a backup does (blocks, checksum,
//! constant image), compares the SHA-256 of the range an OS update writes (the image below the flasher page) with the
//! one of the OS it last sent (`storage::installed`), keeps the dump as a dated backup, and, when the two are equal,
//! marks the installed OS as verified.
//! The flasher page after that range is the machine's own: an update never rewrites its own page, and it differs a
//! little from the one in DSI's file. It is reported apart, never compared (proved on the real machine, 2026-11-08:
//! the written range of the 1.6.2.0 dump was identical, 74 words differed, all in the flasher page).
//! The machine's own dump is the only proof of what the machine really runs: Tempester versions all announce the
//! same identity word, so the identity cannot tell them apart.

use tempest_proto::os_image::OsId;

use super::backup::{Piece, Verdict};
use super::Machine;
use crate::error::AppError;
use crate::event::Event;
use crate::storage::installed::{features_json, InstalledOs};
use crate::storage::stamp::now_stamp;

/// Waiting for the Main dump of the machine, to compare with `expected`.
#[derive(Debug)]
pub(super) struct VerifyWatch {
    expected: InstalledOs,
    /// `expected.sha256_image_written`, checked present when the listening starts.
    expected_written: String,
}

/// What a received Main message proves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Outcome {
    /// The written range is exactly the one that was sent. `flasher` is the SHA-256 of the machine's own flasher page.
    Match { sha256: String, flasher: String },
    /// A complete dump whose written range is another one.
    Mismatch { found: String, flasher: String },
    /// Every byte is the same: the machine did not send its real content.
    Constant { byte: u8, found: String },
    /// Blocks or checksum are wrong: the dump is incomplete or damaged.
    Incomplete(String),
}

impl Outcome {
    fn state(&self) -> &'static str {
        match self {
            Self::Match { .. } => "match",
            Self::Mismatch { .. } => "mismatch",
            Self::Constant { .. } => "constant",
            Self::Incomplete(_) => "incomplete",
        }
    }

    fn found(&self) -> Option<&str> {
        match self {
            Self::Match { sha256, .. } => Some(sha256),
            Self::Mismatch { found, .. } | Self::Constant { found, .. } => Some(found),
            Self::Incomplete(_) => None,
        }
    }

    fn flasher(&self) -> Option<&str> {
        match self {
            Self::Match { flasher, .. } | Self::Mismatch { flasher, .. } => Some(flasher),
            Self::Constant { .. } | Self::Incomplete(_) => None,
        }
    }

    /// A damaged message is not a result: the user may send again while we keep listening.
    fn is_final(&self) -> bool {
        !matches!(self, Self::Incomplete(_))
    }
}

/// Compares a received Main message with the SHA-256 of the range an update writes of the image that was sent.
pub(super) fn judge(expected_written_sha256: &str, verdict: &Verdict) -> Outcome {
    match verdict {
        Verdict::Valid {
            written_sha256,
            flasher_sha256,
            ..
        } if written_sha256 == expected_written_sha256 => Outcome::Match {
            sha256: written_sha256.clone(),
            flasher: flasher_sha256.clone(),
        },
        Verdict::Valid {
            written_sha256,
            flasher_sha256,
            ..
        } => Outcome::Mismatch {
            found: written_sha256.clone(),
            flasher: flasher_sha256.clone(),
        },
        Verdict::Constant { byte, sha256 } => Outcome::Constant {
            byte: *byte,
            found: sha256.clone(),
        },
        Verdict::Invalid(why) => Outcome::Incomplete(why.clone()),
    }
}

impl Machine {
    pub(super) fn verify_start(&mut self) -> Result<(), AppError> {
        self.idle_session()?;
        let expected = self.storage.installed_os().ok_or(AppError::Precondition(
            "no installed OS is recorded: send a Main OS from this page first",
        ))?;
        let expected_written = expected.sha256_image_written.clone().ok_or(
            AppError::Precondition(
                "the installed OS was recorded by an older version of the app and is not a known Tempester build: send it again from this page",
            ),
        )?;
        self.emit(listening_event(&expected, &expected_written));
        self.verify = Some(VerifyWatch {
            expected,
            expected_written,
        });
        Ok(())
    }

    pub(super) fn verify_stop(&mut self) {
        self.verify = None;
        self.emit(Event::new("os_verify").with("state", "idle"));
    }

    /// Looks at a message heard while the check is listening.
    pub(super) fn on_verify_piece(&mut self, piece: &Piece) {
        let Some(watch) = &self.verify else { return };
        if piece.id != OsId::Main {
            return;
        }
        let expected = watch.expected.clone();
        let outcome = judge(&watch.expected_written, &piece.verdict);
        let mut event = result_event(&expected, &watch.expected_written, &outcome);
        if outcome.is_final() {
            self.verify = None;
            event = self.keep_result(piece, &expected, &outcome, event);
        }
        self.emit(event);
    }

    /// Saves the dump as a dated backup and, on a match, records the verification.
    fn keep_result(
        &mut self,
        piece: &Piece,
        expected: &InstalledOs,
        outcome: &Outcome,
        event: Event,
    ) -> Event {
        let stamp = now_stamp();
        let only = std::slice::from_ref(piece);
        let event = match self.write_backup(&stamp, only, only) {
            Ok((dir, _)) => event.with("dir", dir),
            Err(error) => {
                self.emit(Event::error("os_verify", &error));
                event
            }
        };
        if matches!(outcome, Outcome::Match { .. }) {
            let verified = InstalledOs {
                verified_at: Some(stamp),
                ..expected.clone()
            };
            match self.storage.set_installed_os(Some(&verified)) {
                Ok(()) => self.emit(
                    Event::new("installed")
                        .with("installed", verified.to_json())
                        .with("features", features_json(Some(&verified))),
                ),
                Err(error) => self.emit(Event::error("os_verify", &error)),
            }
        }
        event
    }
}

fn listening_event(expected: &InstalledOs, expected_written: &str) -> Event {
    Event::new("os_verify")
        .with("state", "listening")
        .with("label", expected.label.as_str())
        .with("version", expected.version.clone())
        .with("expected", expected_written)
}

fn result_event(expected: &InstalledOs, expected_written: &str, outcome: &Outcome) -> Event {
    let event = Event::new("os_verify")
        .with("state", outcome.state())
        .with("label", expected.label.as_str())
        .with("version", expected.version.clone())
        .with("expected", expected_written)
        .with("found", outcome.found().map(str::to_string))
        .with("flasher", outcome.flasher().map(str::to_string));
    match outcome {
        Outcome::Constant { byte, .. } => event.with("byte", *byte),
        Outcome::Incomplete(why) => event.with("detail", why.as_str()),
        _ => event,
    }
}

#[cfg(test)]
mod tests;
