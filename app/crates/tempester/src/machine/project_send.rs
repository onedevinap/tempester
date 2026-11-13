//! Library -> machine for a whole project: look at the machine's own project first, save it as an auto backup when the
//! library does not know it, send `5E` then `5C n` in the standard layout (paced, with progress and a cancel), then
//! offer to fetch it back and compare. Nothing here sends without the user's confirmation of what the check said, and
//! the same rules as for a sound or a beat apply (`manager.rs`): the machine's project is fetched (request `42`), it
//! is backed up when it is different and unknown, a silent machine needs an explicit « send without a backup ».
//!
//! The stock receiver (and every Tempester before n°28, 1.6.1.3) reads Play Lists one word late after a used list
//! (`Docs/analyses/bug-project-receive.md`). The guard uses the exact version the machine reports (n°39) when the app
//! knows that release, else the version the OS update page recorded (`storage::installed`): if it includes n°28 the
//! send is plain; if not (or if neither is known), and the
//! project uses a Play List that can be misread, the page must show the warning and the user must choose
//! « send anyway » or « send without Play Lists » (the four lists go out empty: all four are then read correctly, the
//! machine's own lists are emptied). Rust refuses a plain send in that case, whatever the page does.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use tempest_midi::{send_paced, CancelToken, MidiError, Transport};
use tempest_proto::project::{self, compare, Project, ProjectAssembler, BEATS};
use tempest_proto::sound_message::REQUEST_PROJECT;
use tempest_proto::versions;

use super::manager::{done_event, known_as};
use super::os_update::ActiveGuard;
use super::project::{GAP_WAIT, LAST_BEAT};
use super::Machine;
use crate::command::Kind;
use crate::error::AppError;
use crate::event::{Emitter, Event};
use crate::library::command::ProjectMode;
use crate::library::inspect::{inspect, messages_of, Facts};
use crate::library::model::{kind_name, Index};
use crate::storage::installed::{feature_in_os, version_of, InstalledOs};
use tempest_midi::message::Message;

/// The patch that makes the SysEx receiver read each Play List with the sender's length.
const FIX_PATCH_FILE: &str = "patch/project_receive_lists.json";
/// The first release that has it, for a version this build does not list.
const FIRST_RELEASE_WITH_FIX: [u32; 4] = [1, 6, 2, 0];
/// The Play Lists that can be misread when used: after list 4 nothing follows, so a project using only list 4 is read
/// right (`bug-project-receive.md` 1, simulated).
const LISTS_AT_RISK: usize = 3;

/// Whether the receiver of the installed OS may misread the Play Lists of a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Guard {
    /// The recorded installed version includes n°28.
    Fixed,
    /// The project uses no Play List that the defect can touch: it cannot occur.
    NoRisk,
    /// The OS may misread them.
    AtRisk,
}

impl Guard {
    fn name(self) -> &'static str {
        match self {
            Self::Fixed => "fixed",
            Self::NoRisk => "no_risk",
            Self::AtRisk => "at_risk",
        }
    }
}

/// True when a Main OS of this version has n°28: by its patch list when the build knows the version, else by being
/// at least 1.6.2.0.
pub(super) fn includes_play_list_fix(version: &str) -> bool {
    if let Some(known) = versions::find_version(version) {
        return known.patches.iter().any(|p| p.file == FIX_PATCH_FILE);
    }
    let parts: Option<Vec<u32>> = version.split('.').map(|p| p.parse().ok()).collect();
    parts.is_some_and(|parts| parts.len() == 4 && parts[..] >= FIRST_RELEASE_WITH_FIX[..])
}

/// The guard for `project` on the OS the machine runs: the exact version it announced (n°39) when the app knows that
/// release, else the build the install record names (the same choice as `features_for`), else the record's version
/// number.
pub(super) fn guard_of(
    announced: Option<&str>,
    installed: Option<&InstalledOs>,
    project: &Project,
) -> Guard {
    let used = project.header.lists_used();
    if !used[..LISTS_AT_RISK].contains(&true) {
        return Guard::NoRisk;
    }
    let fixed = version_of(announced, installed)
        .map(|v| v.version)
        .or_else(|| installed.and_then(|i| i.version.as_deref()))
        .is_some_and(includes_play_list_fix);
    if fixed {
        Guard::Fixed
    } else {
        Guard::AtRisk
    }
}

/// What the last send wrote, to compare with what the machine says it holds. `complete` is set when every byte went
/// out: a cancelled or failed send leaves nothing to verify.
pub(super) struct SentProject {
    reference: Project,
    complete: Arc<AtomicBool>,
}

/// The project a send is about.
struct Subject {
    name: String,
    project: Project,
    hash: String,
    source: String,
}

fn numbers(list: &[u8]) -> String {
    list.iter()
        .map(|n| (n + 1).to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn used_lists(project: &Project) -> String {
    let used: Vec<String> = project
        .header
        .lists_used()
        .iter()
        .enumerate()
        .filter(|(_, used)| **used)
        .map(|(at, _)| (at + 1).to_string())
        .collect();
    used.join(", ")
}

impl Machine {
    pub(super) fn project_busy(&self) -> bool {
        self.project_sending.load(Ordering::SeqCst)
    }

    /// The project of a library item (its file, or one of its older versions: an auto backup's).
    fn project_subject(&self, id: &str, version: Option<usize>) -> Result<Subject, AppError> {
        let library = self.library();
        let (item, bytes, source) = match version {
            None => {
                let item = library.item(id)?;
                if item.kind != Kind::Projects || item.slot {
                    return Err(AppError::Invalid(
                        "this item is not a project file: open one of the versions of an auto backup instead"
                            .into(),
                    ));
                }
                let bytes = std::fs::read(library.file_of(Kind::Projects, &item.path)?)?;
                let source = format!("projects/{}", item.path);
                (item, bytes, source)
            }
            Some(at) => {
                let (item, found, bytes) = library.version_bytes(id, at)?;
                if item.kind != Kind::Projects {
                    return Err(AppError::Invalid("this item is not a project".into()));
                }
                let source = format!(
                    "{}, version of {}",
                    item.name,
                    crate::library::date_text(found.date)
                );
                (item, bytes, source)
            }
        };
        let facts = inspect(Kind::Projects, &bytes, &item.name)?;
        let mut assembler = ProjectAssembler::new();
        for message in messages_of(&facts.canonical) {
            assembler
                .accept(message)
                .map_err(|e| AppError::Invalid(e.to_string()))?;
        }
        let project = assembler
            .finish()
            .map_err(|e| AppError::Invalid(e.to_string()))?;
        Ok(Subject {
            name: item.name,
            project,
            hash: facts.hash,
            source,
        })
    }

    /// The machine's own project (request `42`), or `None` when it stays silent (an OS without the request). The
    /// wait is `fetch_wait` for the first message, then `GAP_WAIT` between messages; a project that stops after
    /// its header ends with the beats that came.
    pub(super) fn query_project(&mut self) -> Result<Option<Project>, AppError> {
        let session = self.idle_session()?;
        session.listener.drain();
        session
            .transport
            .send(&REQUEST_PROJECT)
            .map_err(AppError::from)?;
        let mut assembler = ProjectAssembler::new();
        let mut deadline = Instant::now() + self.fetch_wait;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match session.listener.recv_timeout(left) {
                Ok(Message::SysEx(message)) => {
                    let accepted = assembler.accept(&message).map_err(|e| {
                        AppError::Invalid(format!("the machine's project could not be read: {e}"))
                    })?;
                    if accepted != project::Accepted::Ignored {
                        deadline = Instant::now() + GAP_WAIT;
                    }
                    if assembler.last_beat() == Some(LAST_BEAT) {
                        break;
                    }
                }
                Ok(_) => {}
                Err(MidiError::Timeout) if assembler.has_header() => break,
                Err(MidiError::Timeout) => return Ok(None),
                Err(other) => return Err(other.into()),
            }
        }
        assembler
            .finish()
            .map(Some)
            .map_err(|e| AppError::Invalid(e.to_string()))
    }

    fn machine_project_facts(project: &Project) -> Result<Facts, AppError> {
        inspect(Kind::Projects, &project.standard(), "machine")
    }

    pub(super) fn project_check(
        &mut self,
        id: &str,
        version: Option<usize>,
    ) -> Result<(), AppError> {
        let subject = self.project_subject(id, version)?;
        self.idle_session()?;
        let machine = self.query_project()?;
        let machine = machine
            .as_ref()
            .map(Self::machine_project_facts)
            .transpose()?;
        let index = self.library().snapshot()?;
        let guard = guard_of(
            self.announced_text().as_deref(),
            self.storage.installed_os().as_ref(),
            &subject.project,
        );
        self.emit(self.plan_event(
            id,
            version,
            &subject,
            machine.as_ref(),
            &index,
            guard,
            false,
        ));
        Ok(())
    }

    pub(super) fn project_send(
        &mut self,
        id: &str,
        version: Option<usize>,
        token: &str,
        mode: ProjectMode,
    ) -> Result<(), AppError> {
        if self.project_busy() {
            return Err(AppError::Busy);
        }
        if self.clock_running() {
            return Err(AppError::Precondition(
                "stop the clock before sending a project",
            ));
        }
        let subject = self.project_subject(id, version)?;
        let installed = self.storage.installed_os();
        let guard = guard_of(
            self.announced_text().as_deref(),
            installed.as_ref(),
            &subject.project,
        );
        if guard == Guard::AtRisk && mode == ProjectMode::Normal {
            return Err(AppError::Precondition(
                "the machine's OS may misread Play Lists: choose Send anyway or Send without Play Lists",
            ));
        }
        let mut lines = Vec::new();
        if token == "none" {
            lines.push("No answer from the machine: nothing was backed up, as you confirmed. The machine's current project is replaced.".to_string());
        } else {
            let Some(machine) = self.query_project()? else {
                return Err(AppError::Invalid(
                    "the machine no longer answers: nothing was sent".into(),
                ));
            };
            let facts = Self::machine_project_facts(&machine)?;
            let index = self.library().snapshot()?;
            if facts.hash != token {
                self.emit(self.plan_event(
                    id,
                    version,
                    &subject,
                    Some(&facts),
                    &index,
                    guard,
                    true,
                ));
                return Ok(());
            }
            if facts.hash == subject.hash {
                self.emit(done_event("Send", &[identical_text(&subject.name)]));
                return Ok(());
            }
            if known_as(&index, &facts.hash).is_none() {
                let kept = self.library().auto_backup(&facts)?;
                lines.push(format!(
                    "The machine's project was not in the library: it was saved as an auto backup first ({kept}; see Library > Projects > Auto backups, it can be sent back)."
                ));
                self.publish_library(Kind::Projects)?;
            }
        }
        self.start_project_send(&subject, mode, lines)
    }

    fn start_project_send(
        &mut self,
        subject: &Subject,
        mode: ProjectMode,
        lines: Vec<String>,
    ) -> Result<(), AppError> {
        let transport = Arc::clone(
            &self
                .session
                .as_ref()
                .ok_or(AppError::NotConnected)?
                .transport,
        );
        let reference = match mode {
            ProjectMode::WithoutLists => subject.project.with_empty_play_lists(),
            _ => subject.project.clone(),
        };
        let complete = Arc::new(AtomicBool::new(false));
        let cancel = CancelToken::new();
        self.release_held_keys();
        self.release_touched_ribbons();
        self.project_sending.store(true, Ordering::SeqCst);
        self.edit_forget();
        self.project_cancel = Some(cancel.clone());
        self.sent_project = Some(SentProject {
            reference: reference.clone(),
            complete: Arc::clone(&complete),
        });
        let job = SendJob {
            transport,
            messages: reference.send_messages(),
            rate: self.rate,
            cancel,
            emit: self.emit.clone(),
            complete,
            active: ActiveGuard(Arc::clone(&self.project_sending)),
            name: subject.name.clone(),
            source: subject.source.clone(),
            lines,
            without_lists: mode == ProjectMode::WithoutLists,
        };
        thread::Builder::new()
            .name("tempester-project-send".into())
            .spawn(move || job.run())
            .map(|_| ())
            .map_err(|e| {
                self.project_sending.store(false, Ordering::SeqCst);
                e.into()
            })
    }

    pub(super) fn project_cancel(&mut self) -> Result<(), AppError> {
        match &self.project_cancel {
            Some(cancel) if self.project_busy() => {
                cancel.cancel();
                Ok(())
            }
            _ => Err(AppError::Precondition("no project is being sent")),
        }
    }

    /// Fetches the machine's project and compares it with what the last send wrote.
    pub(super) fn project_verify(&mut self) -> Result<(), AppError> {
        let reference = match &self.sent_project {
            Some(sent) if sent.complete.load(Ordering::SeqCst) && !self.project_busy() => {
                sent.reference.clone()
            }
            _ => {
                return Err(AppError::Precondition(
                    "no complete project send to verify: send a project first",
                ))
            }
        };
        let event = match self.query_project()? {
            None => Event::new("project_verify")
                .with("state", "no_answer")
                .with("lines", vec![NO_ANSWER_TEXT.to_string()]),
            Some(back) => {
                let result = compare(&reference, &back);
                let differences: Vec<String> =
                    result.differences.iter().map(ToString::to_string).collect();
                Event::new("project_verify")
                    .with("state", if result.matches() { "match" } else { "differ" })
                    .with("differences", differences)
                    .with("notes", result.notes)
                    .with("beats_compared", result.beats_compared)
                    .with(
                        "lines",
                        verify_lines(result.beats_compared, result.differences.len()),
                    )
            }
        };
        self.emit(event);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn plan_event(
        &self,
        id: &str,
        version: Option<usize>,
        subject: &Subject,
        machine: Option<&Facts>,
        index: &Index,
        guard: Guard,
        changed: bool,
    ) -> Event {
        let (status, backup, token) = match machine {
            None => ("no_answer", NO_BACKUP_TEXT.to_string(), "none".to_string()),
            Some(m) if m.hash == subject.hash => {
                ("identical", identical_text(&subject.name), m.hash.clone())
            }
            Some(m) => match known_as(index, &m.hash) {
                Some(place) => (
                    "known",
                    format!("The machine's current project is already in your library as {place}: nothing is lost."),
                    m.hash.clone(),
                ),
                None => (
                    "unknown",
                    "The machine's current project is NOT in your library. It is first saved as \"auto backup <date>\" (Library > Projects > Auto backups), then the project is sent.".to_string(),
                    m.hash.clone(),
                ),
            },
        };
        let announced = self.announced_text();
        let installed = self.storage.installed_os();
        let fine_quantize =
            feature_in_os(announced.as_deref(), installed.as_ref(), "quantize_fine");
        let mut lines = replaced_lines(&subject.project, fine_quantize);
        lines.push(backup);
        if changed {
            lines.insert(
                0,
                "The machine's project changed since you looked: check again.".to_string(),
            );
        }
        Event::new("lib_project_plan")
            .with("kind", kind_name(Kind::Projects))
            .with("id", id)
            .with("version", version)
            .with("status", status)
            .with("changed", changed)
            .with("item", subject.name.clone())
            .with("from", subject.source.clone())
            .with("token", token)
            .with("beats", subject.project.beats.len())
            .with("total", BEATS)
            .with("lists_used", subject.project.header.lists_used().to_vec())
            .with(
                "quantize",
                subject
                    .project
                    .header
                    .quantize
                    .map(|q| q.label().to_string()),
            )
            .with("guard", guard.name())
            .with("installed", installed.as_ref().map(|i| i.label.clone()))
            .with("lines", lines)
            .with(
                "warning",
                warning_lines(
                    guard,
                    announced.as_deref(),
                    installed.as_ref(),
                    &subject.project,
                ),
            )
            .with("without_lists", without_lists_lines())
    }
}

const NO_ANSWER_TEXT: &str =
    "The machine did not answer the request for its project: nothing could be compared.";
const NO_BACKUP_TEXT: &str = "The machine did not answer, so its current project cannot be backed up (an original OS does not answer requests). Sending replaces it, and it is lost unless you saved it. Send only if you are sure.";

fn identical_text(name: &str) -> String {
    format!("The machine already holds exactly this project (\"{name}\"): nothing to send, nothing replaced.")
}

/// What a send replaces, and which beats it leaves alone. `fine_quantize`: whether the machine's OS keeps the fine
/// Quantize values (`None`: the OS is not known).
fn replaced_lines(project: &Project, fine_quantize: Option<bool>) -> Vec<String> {
    let mut lines = vec![
        "Sending replaces the machine's whole project in RAM: its beats, their sounds and its 4 Play Lists. It is not saved to the machine's flash until you save it there.".to_string(),
    ];
    let absent = project.absent_beats();
    if !absent.is_empty() {
        lines.push(format!(
            "The file holds {} of {BEATS} beats: beat(s) {} are not sent and keep their current content on the machine.",
            project.beats.len(),
            numbers(&absent)
        ));
    }
    if let Some(quantize) = project.header.quantize.filter(|q| q.needs_tempester()) {
        let verdict = match fine_quantize {
            Some(true) => "this machine's OS keeps it",
            Some(false) => "this machine's OS does not keep it: it loads the project with Quantize Off",
            None => "the machine keeps it only with that OS or a later one (any other loads the project with Quantize Off)",
        };
        lines.push(format!(
            "The project records in Quantize {quantize}, a Tempester 1.6.5.0 or later value: {verdict}."
        ));
    }
    lines
}

fn warning_lines(
    guard: Guard,
    announced: Option<&str>,
    installed: Option<&InstalledOs>,
    project: &Project,
) -> Vec<String> {
    if guard != Guard::AtRisk {
        return Vec::new();
    }
    let reported = announced.filter(|v| versions::find_version(v).is_some());
    let installed = match (reported, installed) {
        (Some(version), _) => format!("The machine reports Tempester {version}, which does not include the Play List fix (n°28, Tempester 1.6.2.0 or later)."),
        (None, Some(i)) => format!("The OS the app last installed is \"{}\", which does not include the Play List fix (n°28, Tempester 1.6.2.0 or later).", i.label),
        (None, None) => "The app does not know the OS of the machine: it reports no version it knows, and the OS update page has no record (it records the OS after a send).".to_string(),
    };
    vec![
        "The machine's OS may misread Play Lists: lists after a used one can come back wrong; beats and sounds are not affected.".to_string(),
        installed,
        format!("This project uses Play List(s) {}.", used_lists(project)),
    ]
}

/// What « send without Play Lists » does, and why it is safe: with four empty lists the receiver reads four markers
/// and nothing shifts (`bug-project-receive.md` 1, simulated); it also writes them, so the machine's lists go.
fn without_lists_lines() -> Vec<String> {
    vec![
        "Send without Play Lists: the project goes with its 4 Play Lists empty. The receiver reads empty lists correctly, so nothing can shift, but it also writes them: the machine's own 4 Play Lists become empty (the machine then fills an empty Play List 1 with the selected beat as step 1).".to_string(),
    ]
}

fn verify_lines(beats: usize, differences: usize) -> Vec<String> {
    if differences == 0 {
        vec![format!(
            "The machine holds exactly what was sent: the parameters, the Play Lists and {beats} beat(s) with their sounds and events are identical."
        )]
    } else {
        vec![format!(
            "The machine does NOT hold exactly what was sent: {differences} difference(s)."
        )]
    }
}

/// What a send job ended with.
enum Ended {
    Done,
    Cancelled {
        message: usize,
        sent: usize,
    },
    Failed {
        message: usize,
        text: String,
        code: &'static str,
    },
}

struct SendJob {
    transport: Arc<dyn Transport>,
    messages: Vec<Vec<u8>>,
    rate: u32,
    cancel: CancelToken,
    emit: Emitter,
    complete: Arc<AtomicBool>,
    active: ActiveGuard,
    name: String,
    source: String,
    lines: Vec<String>,
    without_lists: bool,
}

impl SendJob {
    fn run(self) {
        let total: usize = self.messages.iter().map(Vec::len).sum();
        let beats = self.messages.len() - 1;
        self.progress("started", 0, 0, total);
        let ended = self.send_all(total);
        if matches!(ended, Ended::Done) {
            self.complete.store(true, Ordering::SeqCst);
        }
        let emit = Arc::clone(&self.emit);
        let (state, lines) = self.report(&ended, beats);
        let sent = match &ended {
            Ended::Done => total,
            Ended::Cancelled { sent, .. } => *sent,
            Ended::Failed { .. } => 0,
        };
        // Free the port before announcing the end: the page may ask for the project back at once.
        drop(self.active);
        emit(done_event("Send project to the machine", &lines));
        let mut end = Event::new("project_send")
            .with("state", state)
            .with("beats", beats)
            .with("sent", sent)
            .with("total", total);
        if let Ended::Failed { text, code, .. } = &ended {
            end = end.with("message", text.clone()).with("code", *code);
        }
        emit(end);
    }

    fn progress(&self, state: &'static str, message: usize, sent: usize, total: usize) {
        (self.emit)(
            Event::new("project_send")
                .with("state", state)
                .with("message", message)
                .with("beats", self.messages.len() - 1)
                .with("sent", sent)
                .with("total", total),
        );
    }

    fn send_all(&self, total: usize) -> Ended {
        let mut done = 0;
        for (at, message) in self.messages.iter().enumerate() {
            self.progress("sending", at, done, total);
            let mut last_percent = usize::MAX;
            let result = send_paced(
                self.transport.as_ref(),
                message,
                self.rate,
                &self.cancel,
                &mut |p| {
                    let percent = (done + p.sent) * 100 / total.max(1);
                    if percent != last_percent {
                        last_percent = percent;
                        self.progress("sending", at, done + p.sent, total);
                    }
                },
            );
            match result {
                Ok(()) => done += message.len(),
                Err(MidiError::Cancelled { sent }) => {
                    return Ended::Cancelled {
                        message: at,
                        sent: done + sent,
                    };
                }
                Err(other) => {
                    let code = AppError::Midi(other.clone()).code();
                    return Ended::Failed {
                        message: at,
                        text: other.to_string(),
                        code,
                    };
                }
            }
        }
        Ended::Done
    }

    /// Which message of the send, as the user reads it: the header, or a beat numbered from 1.
    fn message_name(&self, message: usize) -> String {
        match self.messages.get(message).filter(|_| message > 0) {
            Some(beat) => format!("beat {}", beat[4] + 1),
            None => "the 5E header".to_string(),
        }
    }

    fn report(&self, ended: &Ended, beats: usize) -> (&'static str, Vec<String>) {
        let mut lines = self.lines.clone();
        match ended {
            Ended::Done => {
                lines.insert(0, format!(
                    "Sent \"{}\" ({}) to the machine: the 5E header and {beats} beat(s), in order{}. The machine's project in RAM is replaced; it is not saved to the machine's flash until you save it there. To undo: send the auto backup or the project you replaced.",
                    self.name,
                    self.source,
                    if self.without_lists { ", with the 4 Play Lists empty" } else { "" }
                ));
                ("done", lines)
            }
            Ended::Cancelled { message, sent } => {
                lines.insert(0, format!(
                    "Cancelled after {sent} bytes, during {}. The machine already took what was complete: its project is now a mix of the sent project and its previous one. The cut message may stay half received on the machine: restart it if it looks stuck.",
                    self.message_name(*message)
                ));
                ("cancelled", lines)
            }
            Ended::Failed { message, text, .. } => {
                lines.insert(0, format!(
                    "The send stopped during {}: {text}. The machine's project may be a mix of the sent project and its previous one.",
                    self.message_name(*message)
                ));
                ("failed", lines)
            }
        }
    }
}

#[cfg(test)]
mod tests;
