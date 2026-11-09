//! Making the index agree with the files: a file the index does not know is added (an orphan), an entry whose file
//! is gone is dropped, a file changed outside the app is read again. Everything is reported, nothing is silent.
//! Also builds the whole index when it is absent or damaged.

use std::collections::HashSet;
use std::fs;
use std::time::UNIX_EPOCH;

use super::inspect::{inspect, Facts};
use super::model::{Index, Item, KINDS};
use super::{folder_of, now_secs, origin, Library, AS_RECEIVED};
use crate::command::Kind;
use crate::error::AppError;

/// What a scan changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// Files found on disk and added to the index.
    pub added: Vec<String>,
    /// Entries whose file is missing, dropped from the index.
    pub dropped: Vec<String>,
    /// Files whose content changed outside the app (their hash was updated; the old content is not known).
    pub changed: Vec<String>,
    /// Files that are not a Tempest file of their folder's kind, left alone and not indexed.
    pub unreadable: Vec<String>,
}

impl Report {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.dropped.is_empty()
            && self.changed.is_empty()
            && self.unreadable.is_empty()
    }
}

/// `(size, modification seconds)` of a file.
pub(super) fn stats(path: &std::path::Path) -> (u64, i64) {
    let Ok(meta) = fs::metadata(path) else {
        return (0, 0);
    };
    let seconds = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
    (meta.len(), seconds)
}

/// The item for a file the index did not know.
pub(super) fn new_item(
    index: &Index,
    kind: Kind,
    path: &str,
    facts: &Facts,
    size: u64,
    mtime: i64,
) -> Item {
    let name = if facts.name.is_empty() {
        path.rsplit('/')
            .next()
            .unwrap_or(path)
            .trim_end_matches(".syx")
            .to_string()
    } else {
        facts.name.clone()
    };
    Item {
        id: index.next_id(kind),
        kind,
        path: path.into(),
        name,
        hash: facts.hash.clone(),
        size,
        mtime,
        created: mtime,
        modified: mtime,
        fetched: None,
        source: "found in folder".into(),
        origin: origin::of_path(kind, path, facts),
        favourite: false,
        tags: Vec::new(),
        note: String::new(),
        history: Vec::new(),
        capture: None,
        beats: facts.beats,
        facets: facts.facets.clone(),
        slot: false,
    }
}

/// `Project d (as received) (2).syx` -> `Project d (2).syx`: the standard file a capture belongs to.
fn standard_of_capture(path: &str) -> String {
    path.replacen(AS_RECEIVED, "", 1)
}

/// Brings `index` in line with the files of every folder.
pub fn reconcile(library: &Library, index: &mut Index) -> Result<Report, AppError> {
    let mut report = Report::default();
    for kind in KINDS {
        reconcile_kind(library, index, kind, &mut report)?;
    }
    Ok(report)
}

fn reconcile_kind(
    library: &Library,
    index: &mut Index,
    kind: Kind,
    report: &mut Report,
) -> Result<(), AppError> {
    let on_disk: Vec<String> = library
        .storage()
        .list(folder_of(kind))?
        .into_iter()
        .map(|e| e.name)
        .collect();
    let present: HashSet<&str> = on_disk.iter().map(String::as_str).collect();
    index.items.retain(|item| {
        let keep = item.kind != kind || item.slot || present.contains(item.path.as_str());
        if !keep {
            report
                .dropped
                .push(format!("{} ({})", item.name, item.path));
        }
        keep
    });
    for item in index.items.iter_mut().filter(|i| i.kind == kind) {
        if item
            .capture
            .as_deref()
            .is_some_and(|c| !present.contains(c))
        {
            item.capture = None;
        }
    }
    refresh_changed(library, index, kind, report);
    let known: HashSet<String> = index
        .items
        .iter()
        .filter(|i| i.kind == kind)
        .flat_map(|i| std::iter::once(i.path.clone()).chain(i.capture.clone()))
        .collect();
    let (captures, standard): (Vec<&String>, Vec<&String>) = on_disk
        .iter()
        .filter(|name| !known.contains(*name))
        .partition(|name| kind == Kind::Projects && name.contains(AS_RECEIVED));
    for path in standard {
        add_found(library, index, kind, path, report);
    }
    for path in captures {
        let owner = standard_of_capture(path);
        match index
            .items
            .iter_mut()
            .find(|i| i.kind == kind && i.path == owner && i.capture.is_none())
        {
            Some(item) => item.capture = Some(path.clone()),
            None => add_found(library, index, kind, path, report),
        }
    }
    Ok(())
}

/// Reads again the files whose size or date is not what the index noted.
fn refresh_changed(library: &Library, index: &mut Index, kind: Kind, report: &mut Report) {
    for item in index.items.iter_mut().filter(|i| i.kind == kind && !i.slot) {
        let Ok(file) = library.file_of(kind, &item.path) else {
            continue;
        };
        let (size, mtime) = stats(&file);
        if (size, mtime) == (item.size, item.mtime) {
            continue;
        }
        let facts = fs::read(&file)
            .map_err(AppError::from)
            .and_then(|bytes| inspect(kind, &bytes, &item.name));
        match facts {
            Ok(facts) if facts.hash != item.hash => {
                item.hash = facts.hash;
                item.facets = facts.facets;
                if !facts.name.is_empty() {
                    item.name = facts.name;
                }
                item.modified = mtime;
                report
                    .changed
                    .push(format!("{} ({})", item.name, item.path));
            }
            Ok(_) => {}
            Err(_) => {
                report
                    .unreadable
                    .push(format!("{} (changed and no longer readable)", item.path));
            }
        }
        item.size = size;
        item.mtime = mtime;
    }
}

fn add_found(library: &Library, index: &mut Index, kind: Kind, path: &str, report: &mut Report) {
    let Ok(file) = library.file_of(kind, path) else {
        return;
    };
    let (size, mtime) = stats(&file);
    let facts = fs::read(&file)
        .map_err(AppError::from)
        .and_then(|bytes| inspect(kind, &bytes, path.trim_end_matches(".syx")));
    match facts {
        Ok(facts) => {
            let mut item = new_item(index, kind, path, &facts, size, mtime);
            if item.created == 0 {
                item.created = now_secs();
                item.modified = item.created;
            }
            report.added.push(format!("{} ({})", item.name, item.path));
            index.items.push(item);
        }
        Err(e) => report.unreadable.push(format!("{path}: {e}")),
    }
}
