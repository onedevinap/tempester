//! "Verify the installed OS": after an update, the user opens the Debug menu on the machine (both SHIFT and SYSTEM)
//! and chooses "Send Main OS". The app listens for that message, checks it like a backup does (blocks, checksum,
//! constant image), compares the SHA-256 of the range an OS update writes (the image below the flasher page) with the
//! one of the OS it last sent and the one of the release the machine announces (`storage::installed`), keeps the dump
//! as a dated backup, and, when the dump equals the sent OS, marks the installed OS as verified. The announcement is
//! only a number (a test build announces its base), so the dump decides which of the two the machine runs.
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
use crate::storage::installed::{verify_candidates, InstalledOs, VerifyCandidate};
use crate::storage::stamp::now_stamp;

/// Waiting for the Main dump of the machine, to compare with each candidate (never empty).
#[derive(Debug)]
pub(super) struct VerifyWatch {
    candidates: Vec<VerifyCandidate>,
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
        let announced = self.announced.map(|v| v.to_string());
        let candidates = verify_candidates(announced.as_deref(), self.storage.installed_os());
        self.listen_for_dump(candidates)
    }

    /// Starts listening for the machine's Main dump, to compare it with each of `candidates` (the first is shown).
    pub(super) fn listen_for_dump(
        &mut self,
        candidates: Vec<VerifyCandidate>,
    ) -> Result<(), AppError> {
        if candidates.is_empty() {
            return Err(AppError::Precondition(
                "nothing to compare with: the machine reports no version this app knows and no known OS was sent from this page",
            ));
        }
        self.emit(listening_event(&candidates));
        self.verify = Some(VerifyWatch { candidates });
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
        let candidates = watch.candidates.clone();
        let (chosen, outcome) = judge_all(&candidates, &piece.verdict);
        let mut event = result_event(&candidates, chosen, &outcome);
        if outcome.is_final() {
            self.verify = None;
            event = self.keep_result(piece, &candidates[chosen], &outcome, event);
        }
        self.emit(event);
    }

    /// Saves the dump as a dated backup and, on a match with the app's own record, records the verification.
    fn keep_result(
        &mut self,
        piece: &Piece,
        chosen: &VerifyCandidate,
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
        if matches!(outcome, Outcome::Match { .. }) && chosen.from_record {
            let verified = InstalledOs {
                verified_at: Some(stamp),
                ..chosen.os.clone()
            };
            match self.storage.set_installed_os(Some(&verified)) {
                Ok(()) => self.emit(self.installed_event()),
                Err(error) => self.emit(Event::error("os_verify", &error)),
            }
        }
        event
    }
}

/// The candidate the dump matches (the first when none does) and what that comparison proves.
fn judge_all(candidates: &[VerifyCandidate], verdict: &Verdict) -> (usize, Outcome) {
    let hit = candidates
        .iter()
        .position(|c| matches!(judge(&c.written, verdict), Outcome::Match { .. }));
    let chosen = hit.unwrap_or(0);
    (chosen, judge(&candidates[chosen].written, verdict))
}

fn source_of(candidate: &VerifyCandidate) -> &'static str {
    if candidate.from_record {
        "sent"
    } else {
        "reported"
    }
}

fn listening_event(candidates: &[VerifyCandidate]) -> Event {
    let first = &candidates[0];
    Event::new("os_verify")
        .with("state", "listening")
        .with("label", first.os.label.as_str())
        .with("version", first.os.version.clone())
        .with("expected", first.written.as_str())
        .with("source", source_of(first))
}

/// `chosen` is the candidate the dump matched, or the first one when it matched none: a mismatch then names the
/// other candidate too (`also`).
fn result_event(candidates: &[VerifyCandidate], chosen: usize, outcome: &Outcome) -> Event {
    let shown = &candidates[chosen];
    let also = match outcome {
        Outcome::Match { .. } => None,
        _ => candidates.get(1).map(|c| c.os.label.clone()),
    };
    let event = Event::new("os_verify")
        .with("state", outcome.state())
        .with("label", shown.os.label.as_str())
        .with("version", shown.os.version.clone())
        .with("expected", shown.written.as_str())
        .with("source", source_of(shown))
        .with("also", also)
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
