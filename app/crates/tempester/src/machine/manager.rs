//! The library manager's side of the worker: saving what comes from the machine through the duplicate check, the
//! review of a batch before anything is written, the actions on library items, and sending an item to the machine
//! after looking at what it would replace. Pure library logic lives in `crate::library`; this file turns it into
//! commands and events, and talks to the machine.
//!
//! Nothing is written without saying what and where; nothing is sent without fetching the machine's own current
//! sound or beat first.

use std::fs;

use tempest_midi::MidiError;
use tempest_proto::sound_message::{self, SoundMessage};

use super::{Machine, PendingPlan};
use crate::command::{Command, Kind};
use crate::error::AppError;
use crate::event::Event;
use crate::json::{obj, Json};
use crate::library::command::LibCommand;
use crate::library::inspect::{detect_kind, inspect, Facts};
use crate::library::model::{kind_name, Index, Item, KINDS};
use crate::library::plan::{evaluate, Candidate, Outcome, Policy, Verdict};
use crate::library::{date_text, diff, now_secs, origin, Library};

/// What `lib_*` errors are filed under in the `error` event.
const DURING: &str = "lib";

impl Machine {
    pub(super) fn library(&self) -> Library {
        Library::new(&self.storage)
    }

    pub(super) fn handle_manager(&mut self, command: Command) -> Option<Command> {
        match command {
            Command::Lib(lib) => self.guard(DURING, |m| m.run_lib(lib)),
            other => return Some(other),
        }
        None
    }

    fn run_lib(&mut self, command: LibCommand) -> Result<(), AppError> {
        match command {
            LibCommand::Import { name, data } => self.import(&name, &data),
            LibCommand::Apply(policy) => self.apply_pending(policy),
            LibCommand::CancelPlan => self.cancel_plan(),
            LibCommand::Open { id } => self.open_item(&id),
            LibCommand::OpenVersion { id, version } => self.open_version(&id, version),
            LibCommand::Trash => self.publish_trash(),
            LibCommand::History { id } => self.publish_history(&id),
            LibCommand::Compare { id, version } => self.publish_compare(&id, version),
            LibCommand::SendCheck { kind, id } => self.send_check(kind, &id),
            LibCommand::SendApply { kind, id, machine } => self.send_apply(kind, &id, &machine),
            LibCommand::ProjectCheck { id, version } => self.project_check(&id, version),
            LibCommand::ProjectSend {
                id,
                version,
                machine,
                mode,
            } => self.project_send(&id, version, &machine, mode),
            LibCommand::ExportMidi { id } => self.export_midi(&id),
            LibCommand::KitCheck { sounds } => self.kit_check(&sounds),
            LibCommand::KitApply { sounds, machine } => self.kit_apply(&sounds, &machine),
            LibCommand::ProjectCancel => self.project_cancel(),
            LibCommand::ProjectVerify => self.project_verify(),
            other => self.edit(other),
        }
    }

    /// The commands that change items: each says what it did, where, and how to undo.
    fn edit(&mut self, command: LibCommand) -> Result<(), AppError> {
        let library = self.library();
        let (title, lines, touched): (&str, Vec<String>, Vec<Kind>) = match command {
            LibCommand::Favourite { ids, on } => {
                let n = library.set_favourite(&ids, on)?;
                let what = if on { "Marked" } else { "Unmarked" };
                (
                    "Favourite",
                    vec![format!("{what} {n} item(s) as favourite.")],
                    self.kinds_of(&ids)?,
                )
            }
            LibCommand::Tag { ids, add, tag } => {
                let n = library.set_tag(&ids, add, &tag)?;
                let what = if add {
                    "Added the tag to"
                } else {
                    "Removed the tag from"
                };
                (
                    "Tags",
                    vec![format!("{what} \"{}\" on {n} item(s).", tag.trim())],
                    self.kinds_of(&ids)?,
                )
            }
            LibCommand::Note { id, text } => {
                library.set_note(&id, &text)?;
                (
                    "Note",
                    vec!["Note saved.".to_string()],
                    self.kinds_of(&[id])?,
                )
            }
            LibCommand::Move { ids, folder } => {
                let touched = self.kinds_of(&ids)?;
                ("Move", library.move_items(&ids, &folder)?, touched)
            }
            LibCommand::Duplicate { id } => {
                let (copy, line) = library.duplicate(&id)?;
                ("Duplicate", vec![line], vec![copy.kind])
            }
            LibCommand::Delete { ids } => {
                let touched = self.kinds_of(&ids)?;
                ("Trash", library.delete_to_trash(&ids)?, touched)
            }
            LibCommand::RestoreTrash { id } => {
                let kind = library
                    .trash_list()?
                    .iter()
                    .find(|t| t.item.id == id)
                    .map(|t| t.item.kind);
                (
                    "Trash",
                    library.restore_from_trash(&id)?,
                    kind.into_iter().collect(),
                )
            }
            LibCommand::EmptyTrash { expected } => {
                ("Trash", library.empty_trash(expected)?, vec![])
            }
            LibCommand::RestoreVersion { id, version } => {
                let touched = self.kinds_of(std::slice::from_ref(&id))?;
                ("History", library.restore_version(&id, version)?, touched)
            }
            LibCommand::Rename { id, name } => {
                let touched = self.kinds_of(std::slice::from_ref(&id))?;
                ("Rename", library.rename(&id, &name)?, touched)
            }
            _ => {
                return Err(AppError::Invalid(
                    "this library command has no handler".into(),
                ))
            }
        };
        self.emit(done_event(title, &lines));
        for kind in touched {
            self.publish_library(kind)?;
        }
        self.publish_trash()
    }

    fn kinds_of(&self, ids: &[String]) -> Result<Vec<Kind>, AppError> {
        let index = self.library().snapshot()?;
        let mut kinds = Vec::new();
        for id in ids {
            let item = index
                .item(id)
                .ok_or_else(|| AppError::Invalid(format!("no library item \"{id}\"")))?;
            if !kinds.contains(&item.kind) {
                kinds.push(item.kind);
            }
        }
        Ok(kinds)
    }

    // ---- saving through the duplicate check ------------------------------------------------------------------

    /// A candidate for the library from bytes that are a file of `kind`.
    pub(super) fn candidate(
        kind: Kind,
        stem: &str,
        bytes: &[u8],
        source: &str,
        date: i64,
        fetched: bool,
    ) -> Result<Candidate, AppError> {
        let facts = inspect(kind, bytes, stem.trim())?;
        Ok(Candidate {
            origin: if kind == Kind::Projects {
                origin::project(&facts.signature)
            } else {
                origin::file("", stem)
            },
            stem: stem.to_string(),
            folder: String::new(),
            source: source.into(),
            date,
            fetched,
            capture: None,
            label: stem.to_string(),
            facts,
        })
    }

    /// Judges the candidates. Without `always_ask`, a batch of only new items is written at once; anything else
    /// (a duplicate, a modified version, every project) waits for the user's choice in a review.
    pub(super) fn submit(
        &mut self,
        candidates: Vec<Candidate>,
        title: &str,
        source: &'static str,
        always_ask: bool,
    ) -> Result<(), AppError> {
        let library = self.library();
        let index = library.snapshot()?;
        let verdicts = evaluate(&index, &candidates);
        let all_new = verdicts.iter().all(|v| *v == Verdict::New);
        let plan = PendingPlan {
            candidates,
            title: title.to_string(),
            source,
            project: self.project_note.take(),
        };
        if all_new && !always_ask {
            return self.run_plan(plan, Policy::default());
        }
        if let Some(dropped) = self.plan.replace(plan) {
            let lines = vec![format!(
                "{}: a newer review replaced it, nothing was written.",
                dropped.title
            )];
            self.emit(done_event(&dropped.title, &lines));
        }
        if let Some(waiting) = &self.plan {
            self.emit(plan_event(waiting, &verdicts));
        }
        Ok(())
    }

    fn apply_pending(&mut self, policy: Policy) -> Result<(), AppError> {
        let plan = self
            .plan
            .take()
            .ok_or(AppError::Precondition("nothing is waiting for a choice"))?;
        self.run_plan(plan, policy)
    }

    fn run_plan(&mut self, plan: PendingPlan, policy: Policy) -> Result<(), AppError> {
        let outcome = match self.library().apply(&plan.candidates, policy) {
            Ok(outcome) => outcome,
            Err(error) => {
                self.plan = Some(plan);
                return Err(error);
            }
        };
        self.report_plan(&plan, &outcome)
    }

    fn cancel_plan(&mut self) -> Result<(), AppError> {
        if let Some(plan) = self.plan.take() {
            let lines = vec![format!("{}: cancelled, nothing was written.", plan.title)];
            self.emit(done_event(&plan.title, &lines));
            if plan.source == "project" {
                self.publish_project("idle");
            }
        }
        Ok(())
    }

    fn report_plan(&mut self, plan: &PendingPlan, outcome: &Outcome) -> Result<(), AppError> {
        self.emit(
            done_event(&plan.title, &outcome.lines)
                .with("written", outcome.written)
                .with("updated", outcome.updated)
                .with("skipped", outcome.skipped),
        );
        if plan.source == "project" {
            self.report_project(plan, outcome);
        } else if let [one] = outcome.results.as_slice() {
            if let Some(path) = &one.path {
                self.emit(
                    Event::new("saved")
                        .with("kind", plan.source)
                        .with("file", path.rsplit('/').next().unwrap_or(path))
                        .with("path", path.clone()),
                );
            }
        }
        let kinds: Vec<Kind> = if plan.source == "project" {
            KINDS.to_vec()
        } else {
            plan.candidates.iter().map(|c| c.facts.kind).collect()
        };
        for kind in kinds {
            self.publish_library(kind)?;
        }
        Ok(())
    }

    fn import(&mut self, name: &str, data: &[u8]) -> Result<(), AppError> {
        let kind = detect_kind(data).ok_or_else(|| {
            AppError::Invalid(format!(
                "\"{name}\" is not a Tempest sound, beat or project file"
            ))
        })?;
        let stem = name
            .strip_suffix(".syx")
            .or_else(|| name.strip_suffix(".SYX"))
            .unwrap_or(name);
        let mut candidate = Self::candidate(kind, stem, data, "file import", now_secs(), false)?;
        if kind == Kind::Projects {
            candidate.capture = Some(data.to_vec());
        }
        let title = format!("Import {name}");
        self.submit(vec![candidate], &title, "import", false)
    }

    // ---- listing and opening ----------------------------------------------------------------------------------

    /// Emits the `library` event of a kind (and `lib_report` when the scan found something).
    pub(super) fn publish_library(&mut self, kind: Kind) -> Result<(), AppError> {
        let library = self.library();
        if let Some(aside) = library.set_aside_if_damaged()? {
            self.emit(
                Event::new("lib_report")
                    .with("kind", kind_name(kind))
                    .with("damaged", aside.display().to_string()),
            );
        }
        let (items, report) = library.list(kind)?;
        let trash = library.trash_list()?.len();
        self.emit(
            Event::new("library")
                .with("kind", kind_name(kind))
                .with("files", items.iter().map(item_json).collect::<Vec<_>>())
                .with("trash", trash),
        );
        if !report.is_empty() {
            self.emit(
                Event::new("lib_report")
                    .with("kind", kind_name(kind))
                    .with("added", report.added)
                    .with("dropped", report.dropped)
                    .with("changed", report.changed)
                    .with("unreadable", report.unreadable),
            );
        }
        Ok(())
    }

    fn publish_trash(&mut self) -> Result<(), AppError> {
        let items: Vec<Json> = self
            .library()
            .trash_list()?
            .iter()
            .map(|t| {
                obj([
                    ("id", t.item.id.clone().into()),
                    ("kind", kind_name(t.item.kind).into()),
                    ("name", t.item.name.clone().into()),
                    ("path", t.item.path.clone().into()),
                    ("trashed", t.trashed.into()),
                ])
            })
            .collect();
        self.emit(Event::new("lib_trash").with("items", items));
        Ok(())
    }

    fn open_item(&mut self, id: &str) -> Result<(), AppError> {
        let library = self.library();
        let item = library.item(id)?;
        let open: fn(String, Vec<u8>) -> Command = match item.kind {
            Kind::Sounds => |name, data| Command::OpenSound { name, data },
            Kind::Beats => |name, data| Command::OpenBeat { name, data },
            Kind::Projects => return Err(AppError::Precondition(
                "a project cannot be sent back yet: the machine's project receiver is not verified",
            )),
        };
        if item.slot {
            return Err(AppError::Precondition(
                "open one of its versions from its history",
            ));
        }
        let data = fs::read(library.file_of(item.kind, &item.path)?)?;
        self.handle(open(item.path, data));
        Ok(())
    }

    fn open_version(&mut self, id: &str, version: usize) -> Result<(), AppError> {
        let (item, found, bytes) = self.library().version_bytes(id, version)?;
        let name = format!("{} (version of {})", item.name, date_text(found.date));
        match item.kind {
            Kind::Sounds => self.handle(Command::OpenSound { name, data: bytes }),
            Kind::Beats => self.handle(Command::OpenBeat { name, data: bytes }),
            Kind::Projects => return Err(AppError::Precondition(
                "a project cannot be sent back yet: the machine's project receiver is not verified",
            )),
        }
        Ok(())
    }

    fn publish_history(&mut self, id: &str) -> Result<(), AppError> {
        let item = self.library().item(id)?;
        let versions: Vec<Json> = item
            .history
            .iter()
            .enumerate()
            .map(|(at, v)| {
                obj([
                    ("index", at.into()),
                    ("date", v.date.into()),
                    ("label", v.label.clone().into()),
                    ("same_as_current", (v.hash == item.hash).into()),
                ])
            })
            .collect();
        self.emit(
            Event::new("lib_history")
                .with("id", item.id.clone())
                .with("kind", kind_name(item.kind))
                .with("name", item.name.clone())
                .with("slot", item.slot)
                .with("modified", item.modified)
                .with("versions", versions),
        );
        Ok(())
    }

    fn publish_compare(&mut self, id: &str, version: usize) -> Result<(), AppError> {
        let library = self.library();
        let (item, found, older) = library.version_bytes(id, version)?;
        if item.slot {
            return Err(AppError::Precondition(
                "an auto backup has no current version to compare with",
            ));
        }
        let current = fs::read(library.file_of(item.kind, &item.path)?)?;
        let lines = diff::describe(item.kind, &older, &current)?;
        self.emit(
            Event::new("lib_compare")
                .with("id", item.id)
                .with("version", version)
                .with("date", found.date)
                .with("label", found.label)
                .with("lines", lines),
        );
        Ok(())
    }

    // ---- library -> machine -----------------------------------------------------------------------------------

    /// What would go to the machine: the item (or the opened card) and its message.
    fn send_subject(&mut self, kind: Kind, id: &str) -> Result<Subject, AppError> {
        let (facts, name, source) = if id.is_empty() {
            let message = match kind {
                Kind::Sounds => self
                    .opened_sound
                    .as_ref()
                    .map(sound_message::encode_current)
                    .ok_or(AppError::Precondition("open a sound file first"))?,
                _ => self
                    .opened_beat
                    .as_ref()
                    .map(|b| b.message.clone())
                    .ok_or(AppError::Precondition("open a beat file first"))?,
            };
            let facts = inspect(kind, &message, "opened")?;
            let name = facts.name.clone();
            (facts, name, "the file opened in the card".to_string())
        } else {
            let library = self.library();
            let item = library.item(id)?;
            if item.kind != kind || item.slot {
                return Err(AppError::Invalid("this item cannot be sent as that".into()));
            }
            let bytes = fs::read(library.file_of(kind, &item.path)?)?;
            let facts = inspect(kind, &bytes, &item.name)?;
            (
                facts,
                item.name.clone(),
                format!("{}/{}", kind_name(kind), item.path),
            )
        };
        let message = match kind {
            Kind::Sounds => match sound_message::decode(&facts.canonical) {
                Ok(SoundMessage::File { sound, .. } | SoundMessage::Current(sound)) => {
                    sound_message::encode_current(&sound)
                }
                Err(e) => return Err(AppError::Invalid(e.to_string())),
            },
            _ => facts.canonical.clone(),
        };
        Ok(Subject {
            name,
            facts,
            message,
            source,
        })
    }

    /// The machine's own current sound or beat; `None` when it does not answer (an OS without the requests).
    fn query_machine(&mut self, kind: Kind) -> Result<Option<Facts>, AppError> {
        let found = match kind {
            Kind::Sounds => self
                .query_sound()
                .and_then(|s| inspect(kind, &sound_message::encode_current(&s), "machine")),
            _ => self
                .query_beat()
                .and_then(|b| inspect(kind, &b.message, "machine")),
        };
        match found {
            Ok(facts) => Ok(Some(facts)),
            Err(AppError::Midi(MidiError::Timeout)) => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn send_check(&mut self, kind: Kind, id: &str) -> Result<(), AppError> {
        let subject = self.send_subject(kind, id)?;
        self.idle_session()?;
        let machine = self.query_machine(kind)?;
        let index = self.library().snapshot()?;
        self.emit(send_plan_event(
            kind,
            id,
            &subject,
            machine.as_ref(),
            &index,
            false,
        ));
        Ok(())
    }

    fn send_apply(&mut self, kind: Kind, id: &str, token: &str) -> Result<(), AppError> {
        let subject = self.send_subject(kind, id)?;
        let library = self.library();
        let mut lines = Vec::new();
        if token == "none" {
            lines.push(format!(
                "No answer from the machine: nothing was backed up, as you confirmed. The machine's current {} is replaced.",
                singular(kind)
            ));
        } else {
            let Some(machine) = self.query_machine(kind)? else {
                return Err(AppError::Invalid(
                    "the machine no longer answers: nothing was sent".into(),
                ));
            };
            let index = library.snapshot()?;
            if machine.hash != token {
                self.emit(send_plan_event(
                    kind,
                    id,
                    &subject,
                    Some(&machine),
                    &index,
                    true,
                ));
                return Ok(());
            }
            if machine.hash == subject.facts.hash {
                self.emit(done_event("Send", &[identical_text(kind, &subject.name)]));
                return Ok(());
            }
            if known_as(&index, &machine.hash).is_none() {
                let kept = library.auto_backup(&machine)?;
                lines.push(format!(
                    "The machine's {} \"{}\" was not in the library: it was saved as an auto backup first ({kept}; see Library > Auto backups, it can be opened and sent back).",
                    singular(kind),
                    machine.name
                ));
            }
        }
        self.send_sysex(singular(kind), &subject.message)?;
        self.emit(Event::new("sent").with("kind", singular(kind)));
        lines.insert(
            0,
            format!(
                "Sent \"{}\" ({}) to the machine as its current {}. To undo: open the auto backup or the item you replaced and send it.",
                subject.name, subject.source, singular(kind)
            ),
        );
        self.emit(done_event("Send to the machine", &lines));
        self.publish_library(kind)
    }
}

/// What a send is about.
struct Subject {
    name: String,
    facts: Facts,
    message: Vec<u8>,
    source: String,
}

fn singular(kind: Kind) -> &'static str {
    match kind {
        Kind::Sounds => "sound",
        Kind::Beats => "beat",
        Kind::Projects => "project",
    }
}

fn identical_text(kind: Kind, name: &str) -> String {
    format!(
        "The machine already holds exactly this {} (\"{name}\"): nothing to send, nothing replaced.",
        singular(kind)
    )
}

/// Where a content is already in the library: an item, or an older version of one (an auto backup included).
pub(super) fn known_as(index: &Index, hash: &str) -> Option<String> {
    if let Some(item) = index.items.iter().find(|i| !i.slot && i.hash == hash) {
        return Some(format!(
            "\"{}\" ({}/{})",
            item.name,
            kind_name(item.kind),
            item.path
        ));
    }
    index.items.iter().find_map(|item| {
        item.history.iter().find(|v| v.hash == hash).map(|v| {
            if item.slot {
                format!("an auto backup of {}", date_text(v.date))
            } else {
                format!("the version of {} of \"{}\"", date_text(v.date), item.name)
            }
        })
    })
}

fn send_plan_event(
    kind: Kind,
    id: &str,
    subject: &Subject,
    machine: Option<&Facts>,
    index: &Index,
    changed: bool,
) -> Event {
    let noun = singular(kind);
    let (status, lines, token) = match machine {
        None => (
            "no_answer",
            vec![format!(
                "The machine did not answer, so its current {noun} cannot be backed up (an original OS does not answer requests). Sending replaces it, and it is lost unless you saved it. Send only if you are sure."
            )],
            "none".to_string(),
        ),
        Some(m) if m.hash == subject.facts.hash => (
            "identical",
            vec![identical_text(kind, &subject.name)],
            m.hash.clone(),
        ),
        Some(m) => match known_as(index, &m.hash) {
            Some(place) => (
                "known",
                vec![format!(
                    "Sending replaces the machine's current {noun} \"{}\". It is already in your library as {place}: nothing is lost.",
                    m.name
                )],
                m.hash.clone(),
            ),
            None => (
                "unknown",
                vec![format!(
                    "Sending replaces the machine's current {noun} \"{}\", which is NOT in your library. It is first saved as \"auto backup <date>\" in the history of the machine's current {noun} (Library > Auto backups), then \"{}\" is sent.",
                    m.name, subject.name
                )],
                m.hash.clone(),
            ),
        },
    };
    let mut lines = lines;
    if changed {
        lines.insert(
            0,
            format!("The machine's {noun} changed since you looked: check again."),
        );
    }
    Event::new("lib_send_plan")
        .with("kind", kind_name(kind))
        .with("id", id)
        .with("status", status)
        .with("changed", changed)
        .with("item", subject.name.clone())
        .with("from", subject.source.clone())
        .with("machine", machine.map(|m| m.name.clone()))
        .with("token", token)
        .with("lines", lines)
}

pub(super) fn done_event(title: &str, lines: &[String]) -> Event {
    Event::new("lib_done")
        .with("title", title)
        .with("lines", lines.to_vec())
}

fn verdict_name(verdict: &Verdict) -> &'static str {
    match verdict {
        Verdict::New => "new",
        Verdict::Identical(_) => "identical",
        Verdict::Modified(_) => "modified",
    }
}

fn plan_event(plan: &PendingPlan, verdicts: &[Verdict]) -> Event {
    let count = |name: &str| verdicts.iter().filter(|v| verdict_name(v) == name).count();
    let entries: Vec<Json> = plan
        .candidates
        .iter()
        .zip(verdicts)
        .map(|(c, v)| {
            let existing = match v {
                Verdict::Identical(e) | Verdict::Modified(e) => Some(e),
                Verdict::New => None,
            };
            obj([
                ("label", c.label.clone().into()),
                ("kind", kind_name(c.facts.kind).into()),
                ("verdict", verdict_name(v).into()),
                ("name", existing.map(|e| e.name.clone()).into()),
                ("place", existing.map(|e| e.path.clone()).into()),
                ("in_batch", existing.is_some_and(|e| e.in_batch).into()),
                ("existing_date", existing.map(|e| e.modified).into()),
                ("candidate_date", c.date.into()),
                (
                    "newer",
                    existing.is_none_or(|e| c.date >= e.modified).into(),
                ),
            ])
        })
        .collect();
    Event::new("lib_plan")
        .with("title", plan.title.clone())
        .with("source", plan.source)
        .with("new", count("new"))
        .with("identical", count("identical"))
        .with("modified", count("modified"))
        .with("entries", entries)
}

pub(super) fn item_json(item: &Item) -> Json {
    let (folder, _) = item.path.split_once('/').unwrap_or(("", &item.path));
    obj([
        ("id", item.id.clone().into()),
        ("kind", kind_name(item.kind).into()),
        ("name", item.path.clone().into()),
        ("display", item.name.clone().into()),
        ("folder", folder.into()),
        ("size", i64::try_from(item.size).unwrap_or(i64::MAX).into()),
        ("hash", item.hash.clone().into()),
        ("favourite", item.favourite.into()),
        ("tags", item.tags.clone().into()),
        ("note", item.note.clone().into()),
        ("source", item.source.clone().into()),
        ("created", item.created.into()),
        ("modified", item.modified.into()),
        ("fetched", item.fetched.into()),
        ("history", item.history.len().into()),
        ("capture", item.capture.clone().into()),
        ("beats", item.beats.into()),
        ("sound_type", item.facets.sound_type.clone().into()),
        ("osc", item.facets.osc.clone().into()),
        ("slot", item.slot.into()),
    ])
}

#[cfg(test)]
mod tests;
