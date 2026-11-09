//! Bringing the whole project home: the menu's Send Project (`5E` then one `5C n` per beat present on the machine,
//! normally all 16), either heard (the author starts it on the machine) or asked for (`42`, Tempester 1.6.1.0 or
//! later). Every message is checked as it arrives. The project is complete at once when beat 15 arrives (no later beat
//! can come); otherwise, when the silence between messages lasts `GAP_WAIT` with the header in, it ends with the beats
//! that came. The machine ends each message with a 7-bit group its own receivers misread (`refs/firmware-main.md`
//! 9.1), so nothing it sent is written back as it came, except the one file kept as evidence. A finished project is
//! first judged against the library (new, identical, modified version) and shown in a review; only what the user
//! chooses is written (`manager`, `library::plan`). The files below are what a « write everything » produces:
//! - the project as received (`projects/Project <date> (as received).syx`, every message one after the other, byte for
//!   byte: the proof of the transfer, not meant to be sent back),
//! - the same project in the standard layout (`projects/Project <date>.syx`): decoded exactly, re-encoded with
//!   `pack_padded`, the one meant to be sent back,
//! - each beat as a standard-layout `5F` beat file, the format of the Beats library (`beats/Project <date>/NN <beat
//!   name>.syx`),
//! - the beats' sounds as `63` sound files, the format of the Sounds library (`sounds/Project <date>/...`), a sound
//!   identical to one already saved from the same project being left out: a project copies kits from beat to beat, and
//!   every beat file still holds all its sounds. 16 x 32 near-identical entries would bury the library.
//!
//! A project with absent beats is saved as partial and never passes for a complete one: the names carry
//! `(14 of 16 beats)`, the event says `partial: true` and lists the absent beats (numbered from 1, as shown).
//! Nothing is saved from a refused project, or when the header never came. Nothing here ever overwrites a file in
//! silence: a modified version replaces the old one only after the old one went to the library's history.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use tempest_proto::project::{Accepted, Project, ProjectAssembler, BEATS};
use tempest_proto::sound::{Sound, SOUND_BYTES};
use tempest_proto::sound_message::{self, REQUEST_PROJECT};

use super::{fake_machine, Machine, PendingPlan};
use crate::command::{Command, Kind};
use crate::error::AppError;
use crate::event::Event;
use crate::library::plan::Outcome;
use crate::library::{now_secs, origin};
use crate::storage::names::sanitize_stem;
use crate::storage::stamp::now_stamp;

/// How long the app listens for a Send Project the author starts by hand.
const LISTEN_WAIT: Duration = Duration::from_secs(120);
/// How long it waits for an answer to the request `42` before saying the machine's OS may be too old.
pub(super) const REQUEST_WAIT: Duration = Duration::from_secs(5);
/// The last beat number: once it arrived, no later beat can come.
pub(super) const LAST_BEAT: u8 = BEATS as u8 - 1;
/// Silence between two messages of a project that began; its end ends a project that stopped short of beat 15. A big
/// beat takes about 10 s on a DIN cable, so 30 s tolerates a slow beat without cutting it.
pub(super) const GAP_WAIT: Duration = Duration::from_secs(30);

/// A project being received.
pub(super) struct ProjectCapture {
    assembler: ProjectAssembler,
    deadline: Instant,
    asked: bool,
}

/// What the review of a project says about it, kept until the user's choice is applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProjectNote {
    label: String,
    beats: usize,
    /// Absent beats, numbered from 1 as the author sees them; empty for a complete project.
    absent: Vec<u8>,
    sounds: usize,
    duplicates: usize,
}

/// The name stem of a project's files: the date, plus a mark when beats are absent.
fn project_label(stamp: &str, project: &Project) -> String {
    if project.is_complete() {
        format!("Project {stamp}")
    } else {
        format!("Project {stamp} ({} of {BEATS} beats)", project.beats.len())
    }
}

/// A sound to save with the name its file gets.
#[derive(Debug, PartialEq, Eq)]
struct PlannedSound<'a> {
    stem: String,
    sound: &'a Sound,
    beat: u8,
    pad: usize,
}

fn beat_name(index: u8, name: &str) -> String {
    if name.is_empty() {
        format!("Beat {:02}", index + 1)
    } else {
        name.to_string()
    }
}

/// The sounds of the project worth a file of their own, in beat then pad order, with their names.
fn plan_sounds(project: &Project) -> (Vec<PlannedSound<'_>>, usize) {
    let mut seen: HashSet<[u8; SOUND_BYTES]> = HashSet::new();
    let (mut planned, mut duplicates) = (Vec::new(), 0);
    for beat in &project.beats {
        let beat_label = beat_name(beat.index, &beat.dump.summary.name);
        for (pad, sound) in beat.dump.sounds.iter().enumerate() {
            let Some(sound) = sound else { continue };
            if !seen.insert(*sound.bytes()) {
                duplicates += 1;
                continue;
            }
            let raw = format!(
                "{:02} {} pad {:02} {}",
                beat.index + 1,
                beat_label,
                pad + 1,
                sound.name().trim()
            );
            let stem = sanitize_stem(&raw).unwrap_or_else(|| format!("sound {pad:02}"));
            planned.push(PlannedSound {
                stem,
                sound,
                beat: beat.index,
                pad,
            });
        }
    }
    (planned, duplicates)
}

impl Machine {
    pub(super) fn handle_project(&mut self, command: Command) -> Option<Command> {
        match command {
            Command::ReceiveProject => self.guard("receive_project", |m| m.listen_project(false)),
            Command::FetchProject => self.guard("fetch_project", |m| m.listen_project(true)),
            Command::ProjectCancel => self.cancel_project(),
            Command::FakeProjectDefect(on) => {
                self.connector.set_fake_project_defect(on);
                self.emit(Event::new("fake_project_defect").with("on", on));
            }
            Command::FakeProjectSend => {
                self.guard("fake_project_send", |m| m.fake_project_send(false));
            }
            Command::FakeProjectSendPartial => {
                self.guard("fake_project_send", |m| m.fake_project_send(true));
            }
            other => return Some(other),
        }
        None
    }

    /// Starts listening; with `ask`, sends the request `42` first.
    fn listen_project(&mut self, ask: bool) -> Result<(), AppError> {
        let session = self.idle_session()?;
        session.listener.drain();
        let sent = ask.then(|| session.transport.send(&REQUEST_PROJECT));
        let wait = if ask { REQUEST_WAIT } else { LISTEN_WAIT };
        self.project = Some(ProjectCapture {
            assembler: ProjectAssembler::new(),
            deadline: Instant::now() + wait,
            asked: ask,
        });
        if let Some(Err(error)) = sent {
            self.project = None;
            return Err(self.failed(error.into()));
        }
        self.publish_project("listening");
        Ok(())
    }

    fn cancel_project(&mut self) {
        if self.project.take().is_some() {
            self.publish_project("idle");
        }
    }

    /// Feeds a SysEx heard on the port to the project being received, if any.
    pub(super) fn on_project_sysex(&mut self, message: &[u8]) {
        let Some(mut capture) = self.project.take() else {
            return;
        };
        match capture.assembler.accept(message) {
            Ok(Accepted::Ignored) => self.project = Some(capture),
            Ok(_) => {
                capture.deadline = Instant::now() + GAP_WAIT;
                if capture.assembler.last_beat() == Some(LAST_BEAT) {
                    self.complete_project(capture.assembler);
                } else {
                    self.project = Some(capture);
                    self.publish_project("receiving");
                }
            }
            Err(error) => {
                let beats = capture.assembler.beats_received();
                self.fail_project(
                    "refused",
                    beats,
                    &format!("Project refused: {error}. Nothing was saved."),
                );
            }
        }
    }

    /// Ends a project whose wait ran out at `now`: with the header in, with the beats that came; otherwise nothing.
    pub(super) fn expire_project(&mut self, now: Instant) {
        let Some(capture) = &self.project else { return };
        if now < capture.deadline {
            return;
        }
        if capture.assembler.has_header() {
            if let Some(capture) = self.project.take() {
                self.complete_project(capture.assembler);
            }
            return;
        }
        let (code, message) = if capture.asked {
            ("no_answer", NO_ANSWER)
        } else {
            ("timeout", NOTHING_HEARD)
        };
        self.fail_project(code, 0, message);
    }

    fn complete_project(&mut self, assembler: ProjectAssembler) {
        let outcome = assembler
            .finish()
            .map_err(|e| AppError::Invalid(e.to_string()))
            .and_then(|project| self.plan_project(&project, &now_stamp()));
        if let Err(error) = outcome {
            let message = format!("Project received but not fully saved: {error}.");
            self.fail_project(error.code(), 0, &message);
        }
    }

    /// Says what the project would add to the library (new, identical, modified) and waits for the user's choice;
    /// nothing is written yet.
    fn plan_project(&mut self, project: &Project, stamp: &str) -> Result<(), AppError> {
        let label = project_label(stamp, project);
        let date = now_secs();
        let mut candidates = Vec::new();
        let standard = project.standard();
        let mut file = Self::candidate(Kind::Projects, &label, &standard, "project", date, true)?;
        file.capture = Some(project.raw());
        candidates.push(file);
        for beat in &project.beats {
            let name = beat_name(beat.index, &beat.dump.summary.name);
            let stem = format!("{:02} {name}", beat.index + 1);
            let mut c =
                Self::candidate(Kind::Beats, &stem, &beat.beat_file(), "project", date, true)?;
            c.folder.clone_from(&label);
            c.origin = origin::project_beat(beat.index, &beat.dump.summary.name);
            candidates.push(c);
        }
        let (planned, duplicates) = plan_sounds(project);
        for item in &planned {
            let message = sound_message::encode_file(&item.stem, item.sound)
                .map_err(|e| AppError::Invalid(e.to_string()))?;
            let mut c = Self::candidate(Kind::Sounds, &item.stem, &message, "project", date, true)?;
            c.folder.clone_from(&label);
            c.origin = origin::project_sound(item.beat, item.pad, &item.sound.name());
            candidates.push(c);
        }
        self.project_note = Some(ProjectNote {
            label: label.clone(),
            beats: project.beats.len(),
            absent: project.absent_beats().iter().map(|n| n + 1).collect(),
            sounds: planned.len(),
            duplicates,
        });
        self.submit(candidates, &format!("Project {stamp}"), "project", true)?;
        self.publish_project("review");
        Ok(())
    }

    /// After the user's choice was applied: the `project_saved` event with where the project's files went.
    pub(super) fn report_project(&mut self, plan: &PendingPlan, outcome: &Outcome) {
        let Some(note) = plan.project.as_ref() else {
            return;
        };
        let file_of = |at: Option<&crate::library::plan::EntryResult>| {
            at.and_then(|r| r.path.clone()).unwrap_or_default()
        };
        let project = outcome.results.first();
        self.emit(
            Event::new("project_saved")
                .with("folder", note.label.clone())
                .with("project_file", file_of(project))
                .with(
                    "received_file",
                    project.and_then(|r| r.capture.clone()).unwrap_or_default(),
                )
                .with("beats", note.beats)
                .with("total", BEATS)
                .with("partial", !note.absent.is_empty())
                .with("absent_beats", note.absent.clone())
                .with("sounds", note.sounds)
                .with("duplicates", note.duplicates)
                .with("written", outcome.written)
                .with("updated", outcome.updated)
                .with("skipped", outcome.skipped),
        );
        self.publish_project("saved");
    }

    fn fail_project(&mut self, code: &'static str, beats: usize, message: &str) {
        self.project = None;
        self.emit(
            Event::new("project")
                .with("state", "failed")
                .with("beats", beats)
                .with("total", BEATS)
                .with("code", code)
                .with("message", message),
        );
    }

    pub(super) fn publish_project(&self, state: &'static str) {
        let (header, beats, last, asked) =
            self.project.as_ref().map_or((false, 0, None, false), |c| {
                (
                    c.assembler.has_header(),
                    c.assembler.beats_received(),
                    c.assembler.last_beat(),
                    c.asked,
                )
            });
        self.emit(
            Event::new("project")
                .with("state", state)
                .with("mode", if asked { "fetch" } else { "receive" })
                .with("header", header)
                .with("beats", beats)
                // Number (from 1) of the last beat received, 0 before the first: progress is a position, not a
                // count, since beats may be absent.
                .with("beat", last.map_or(0, |n| n + 1))
                .with("total", BEATS),
        );
    }

    fn fake_project_send(&mut self, partial: bool) -> Result<(), AppError> {
        let fake = self
            .connector
            .fake()
            .filter(|_| self.session.is_some())
            .ok_or(AppError::Precondition(
                "only available with the fake machine, once connected",
            ))?;
        if partial {
            fake_machine::project_send_partial(&fake);
        } else {
            fake_machine::project_send(&fake);
        }
        Ok(())
    }
}

const NO_ANSWER: &str = "The machine did not answer. Check that it runs Tempester 1.6.1.0 or later and that the Remote/MIDI cable in System is set to USB. Otherwise click \"Wait for Send Project from the machine's menu\", then choose Send Project in the machine's menu.";
const NOTHING_HEARD: &str = "Nothing heard in 2 minutes. On the machine, open the menu and choose Send Project, and check that the MIDI cable set in System is the one this app uses.";

#[cfg(test)]
use crate::library::AS_RECEIVED;
#[cfg(test)]
use crate::storage::Folder;
#[cfg(test)]
mod standard_tests;
#[cfg(test)]
pub(super) mod support;
#[cfg(test)]
mod tests;
