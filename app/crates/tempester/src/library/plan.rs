//! Saving into the library without ever duplicating or overwriting in silence. A batch of candidates (one sound, one
//! beat, a whole project's files) is first judged against the index:
//!
//! - same content (hash of the decoded data) as something already there: **identical**, whatever its name or place;
//! - else same origin (same name in the same folder, same beat slot of the same project) with other content:
//!   **modified version**, with both dates and which one is newer;
//! - else **new**.
//!
//! The user chooses one policy per group; `apply` then writes files first and the index last (one merged update).

use std::collections::HashMap;
use std::fs;

use super::inspect::Facts;
use super::model::{kind_name, Index, Item, Version};
use super::scan::stats;
use super::{date_text, write_replace, Library, AS_RECEIVED};
use crate::error::AppError;
use crate::storage::names::sanitize_stem;
use crate::storage::stamp::utc_stamp;

/// A file about to enter the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub facts: Facts,
    /// The wanted file name without extension.
    pub stem: String,
    /// `""` for the kind's folder itself, else the sub-folder (a project's batch).
    pub folder: String,
    pub origin: String,
    pub source: String,
    /// When this version exists: the fetch time, or the date of the imported file.
    pub date: i64,
    /// Whether it comes from the machine just now (sets the « last fetched » date).
    pub fetched: bool,
    /// A project's exact capture, written next to its standard file.
    pub capture: Option<Vec<u8>>,
    /// How the summary names it (`03 Fake beat 03`).
    pub label: String,
}

/// An item already in the library (or earlier in the same batch) that a candidate matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Existing {
    pub id: Option<String>,
    pub name: String,
    pub path: String,
    pub modified: i64,
    pub in_batch: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    New,
    Identical(Existing),
    Modified(Existing),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnNew {
    Write,
    Skip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnIdentical {
    Skip,
    KeepCopy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnModified {
    Update,
    SaveAsNew,
    Skip,
}

/// One choice per group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    pub new: OnNew,
    pub identical: OnIdentical,
    pub modified: OnModified,
}

impl Default for Policy {
    /// Write what is new, skip what is identical, update what changed (the old version goes to the history).
    fn default() -> Self {
        Self {
            new: OnNew::Write,
            identical: OnIdentical::Skip,
            modified: OnModified::Update,
        }
    }
}

fn existing_of(item: &Item) -> Existing {
    Existing {
        id: Some(item.id.clone()),
        name: item.name.clone(),
        path: item.path.clone(),
        modified: item.modified,
        in_batch: false,
    }
}

/// Judges each candidate against `index` and against the candidates before it in the batch.
pub fn evaluate(index: &Index, candidates: &[Candidate]) -> Vec<Verdict> {
    let mut seen: HashMap<(&str, &str), usize> = HashMap::new();
    let mut verdicts = Vec::with_capacity(candidates.len());
    for (at, c) in candidates.iter().enumerate() {
        let kind = c.facts.kind;
        let live = index.items.iter().filter(|i| i.kind == kind && !i.slot);
        let same = live.clone().find(|i| i.hash == c.facts.hash);
        let verdict = if let Some(item) = same {
            Verdict::Identical(existing_of(item))
        } else if let Some(&earlier) = seen.get(&(kind_name(kind), c.facts.hash.as_str())) {
            Verdict::Identical(Existing {
                id: None,
                name: candidates[earlier].label.clone(),
                path: String::new(),
                modified: candidates[earlier].date,
                in_batch: true,
            })
        } else if let Some(item) = live
            .filter(|i| i.origin == c.origin)
            .max_by_key(|i| i.modified)
        {
            Verdict::Modified(existing_of(item))
        } else {
            Verdict::New
        };
        seen.entry((kind_name(kind), c.facts.hash.as_str()))
            .or_insert(at);
        verdicts.push(verdict);
    }
    verdicts
}

/// What `apply` did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    /// One sentence per candidate: what was done, where, older or newer, how to undo.
    pub lines: Vec<String>,
    pub written: usize,
    pub updated: usize,
    pub skipped: usize,
    /// One per candidate, in order: where its file went, when one was written or updated.
    pub results: Vec<EntryResult>,
}

/// What became of one candidate.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntryResult {
    pub path: Option<String>,
    pub capture: Option<String>,
}

/// A change of the index, decided after the files are written; applying it twice to the same index is not needed
/// (each retry of the merged update starts from the file's current content).
enum Change {
    Add(Box<Item>),
    Replace(Box<Replace>),
}

struct Replace {
    id: String,
    facts: Facts,
    size: u64,
    mtime: i64,
    date: i64,
    source: String,
    fetched: bool,
    version: Version,
}

fn relative(folder: &str, file: &str) -> String {
    if folder.is_empty() {
        file.to_string()
    } else {
        format!("{folder}/{file}")
    }
}

impl Library {
    /// Writes what the policy says for each candidate: files first, then one merged update of the index.
    pub fn apply(&self, candidates: &[Candidate], policy: Policy) -> Result<Outcome, AppError> {
        let index = self.snapshot()?;
        let verdicts = evaluate(&index, candidates);
        let mut outcome = Outcome::default();
        let mut changes = Vec::new();
        for (candidate, verdict) in candidates.iter().zip(&verdicts) {
            let before = changes.len();
            self.apply_one(
                &index,
                candidate,
                verdict,
                policy,
                &mut changes,
                &mut outcome,
            )?;
            outcome.results.push(match changes.get(before) {
                Some(Change::Add(item)) => EntryResult {
                    path: Some(item.path.clone()),
                    capture: item.capture.clone(),
                },
                Some(Change::Replace(r)) => EntryResult {
                    path: index.item(&r.id).map(|i| i.path.clone()),
                    capture: None,
                },
                None => EntryResult::default(),
            });
        }
        self.update(|index| {
            for change in &changes {
                match change {
                    Change::Add(item) => {
                        let mut item = (**item).clone();
                        item.id = index.next_id(item.kind);
                        index.items.push(item);
                    }
                    Change::Replace(r) => {
                        if let Some(item) = index.item_mut(&r.id) {
                            replace_in(item, r);
                        }
                    }
                }
            }
        })?;
        Ok(outcome)
    }

    fn apply_one(
        &self,
        index: &Index,
        c: &Candidate,
        verdict: &Verdict,
        policy: Policy,
        changes: &mut Vec<Change>,
        outcome: &mut Outcome,
    ) -> Result<(), AppError> {
        let kind = kind_name(c.facts.kind);
        match verdict {
            Verdict::New if policy.new == OnNew::Write => {
                let (item, line) = self.write_new(index, c, "saved")?;
                outcome.lines.push(line);
                outcome.written += 1;
                changes.push(Change::Add(Box::new(item)));
            }
            Verdict::New => {
                outcome.lines.push(format!(
                    "Skipped {} ({kind}): new, not saved as you chose.",
                    c.label
                ));
                outcome.skipped += 1;
            }
            Verdict::Identical(_) if policy.identical == OnIdentical::KeepCopy => {
                let (item, line) = self.write_new(index, c, "second copy kept")?;
                outcome.lines.push(line);
                outcome.written += 1;
                changes.push(Change::Add(Box::new(item)));
            }
            Verdict::Identical(e) => {
                outcome.lines.push(identical_line(c, e));
                outcome.skipped += 1;
            }
            Verdict::Modified(e) => match policy.modified {
                OnModified::Update => {
                    let item = e
                        .id
                        .as_deref()
                        .and_then(|id| index.item(id))
                        .ok_or_else(|| AppError::Invalid("the item to update is gone".into()))?;
                    let (replace, line) = self.write_update(item, c)?;
                    outcome.lines.push(line);
                    outcome.updated += 1;
                    changes.push(Change::Replace(Box::new(replace)));
                }
                OnModified::SaveAsNew => {
                    let (item, line) = self.write_new(index, c, "saved as a new item")?;
                    outcome.lines.push(line);
                    outcome.written += 1;
                    changes.push(Change::Add(Box::new(item)));
                }
                OnModified::Skip => {
                    outcome.lines.push(format!(
                        "Skipped {} ({kind}): a modified version of \"{}\" ({}), kept as it is as you chose; the library still holds the version of {}.",
                        c.label,
                        e.name,
                        e.path,
                        date_text(e.modified)
                    ));
                    outcome.skipped += 1;
                }
            },
        }
        Ok(())
    }

    /// Writes a candidate as a new file (never over an existing one) and returns the item that describes it.
    fn write_new(
        &self,
        index: &Index,
        c: &Candidate,
        how: &str,
    ) -> Result<(Item, String), AppError> {
        let kind = c.facts.kind;
        let stem =
            sanitize_stem(&c.stem).ok_or_else(|| AppError::Invalid("the name is empty".into()))?;
        let dir = self.dir_of(kind, &c.folder)?;
        let (main, capture) =
            save_with_capture(&dir, &stem, &c.facts.canonical, c.capture.as_deref())?;
        let path = relative(&c.folder, &main);
        let (size, mtime) = stats(&dir.join(&main));
        let mut item = super::scan::new_item(index, kind, &path, &c.facts, size, mtime);
        item.origin.clone_from(&c.origin);
        item.source.clone_from(&c.source);
        item.created = c.date;
        item.modified = c.date;
        item.fetched = c.fetched.then_some(c.date);
        item.capture = capture.map(|name| relative(&c.folder, &name));
        let line = format!(
            "{} {} as {}/{} ({}). To undo: select it in the list and delete it (it goes to the trash).",
            capitalise(how),
            c.label,
            kind_name(kind),
            path,
            date_text(c.date)
        );
        Ok((item, line))
    }

    /// Replaces the file of `item` by the candidate; the old version (and the old capture) go to the history first.
    fn write_update(&self, item: &Item, c: &Candidate) -> Result<(Replace, String), AppError> {
        let file = self.file_of(item.kind, &item.path)?;
        let old = fs::read(&file)?;
        let label = format!("replaced by an update on {}", date_text(super::now_secs()));
        let version = self.keep_version(item, &old, &label)?;
        if let (Some(old_capture), Some(new_capture)) = (&item.capture, &c.capture) {
            let capture_file = self.file_of(item.kind, old_capture)?;
            let kept = fs::read(&capture_file)?;
            let dir = self.history_dir(item.kind, &item.id)?;
            let stamp = utc_stamp(item.modified);
            self.storage()
                .save_in(&dir, &format!("{stamp}{AS_RECEIVED}"), "syx", &kept)?;
            write_replace(&capture_file, new_capture)?;
        }
        write_replace(&file, &c.facts.canonical)?;
        let (size, mtime) = stats(&file);
        let newer = if c.date >= item.modified {
            "newer"
        } else {
            "OLDER"
        };
        let warning = if c.date < item.modified {
            " WARNING: the version that replaced it is OLDER than the one it replaced."
        } else {
            ""
        };
        let line = format!(
            "Updated {}/{} (\"{}\"): the new version ({}) is {newer} than the previous one ({}), which is kept in its history. To undo: open its History and restore that version.{warning}",
            kind_name(item.kind),
            item.path,
            item.name,
            date_text(c.date),
            date_text(item.modified)
        );
        let replace = Replace {
            id: item.id.clone(),
            facts: c.facts.clone(),
            size,
            mtime,
            date: c.date,
            source: c.source.clone(),
            fetched: c.fetched,
            version,
        };
        Ok((replace, line))
    }
}

fn replace_in(item: &mut Item, r: &Replace) {
    item.hash.clone_from(&r.facts.hash);
    if !r.facts.name.is_empty() {
        item.name.clone_from(&r.facts.name);
    }
    item.facets.clone_from(&r.facts.facets);
    item.beats = r.facts.beats;
    item.size = r.size;
    item.mtime = r.mtime;
    item.modified = r.date;
    item.source.clone_from(&r.source);
    if r.fetched {
        item.fetched = Some(r.date);
    }
    item.history.push(r.version.clone());
}

fn identical_line(c: &Candidate, e: &Existing) -> String {
    let kind = kind_name(c.facts.kind);
    if e.in_batch {
        format!(
            "Skipped {}: identical to {} earlier in the same batch.",
            c.label, e.name
        )
    } else {
        format!(
            "Skipped {} ({kind}): already in the library as \"{}\" ({kind}/{}, {}); nothing was written.",
            c.label,
            e.name,
            e.path,
            date_text(e.modified)
        )
    }
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Writes `bytes` as `<stem>.syx` in `dir`, and the capture as `<stem> (as received).syx`, with the same ` (n)`
/// suffix when the name is taken; nothing existing is touched.
fn save_with_capture(
    dir: &std::path::Path,
    stem: &str,
    bytes: &[u8],
    capture: Option<&[u8]>,
) -> Result<(String, Option<String>), AppError> {
    for n in 1..1000 {
        let suffix = if n == 1 {
            String::new()
        } else {
            format!(" ({n})")
        };
        let main = format!("{stem}{suffix}.syx");
        let cap = format!("{stem}{AS_RECEIVED}{suffix}.syx");
        if dir.join(&main).exists() || (capture.is_some() && dir.join(&cap).exists()) {
            continue;
        }
        write_new_file(&dir.join(&main), bytes)?;
        if let Some(capture) = capture {
            write_new_file(&dir.join(&cap), capture)?;
            return Ok((main, Some(cap)));
        }
        return Ok((main, None));
    }
    Err(AppError::Io(format!("too many files named \"{stem}\"")))
}

fn write_new_file(path: &std::path::Path, bytes: &[u8]) -> Result<(), AppError> {
    use std::io::Write;
    let mut handle = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    handle.write_all(bytes)?;
    Ok(())
}
