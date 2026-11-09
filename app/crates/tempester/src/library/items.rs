//! Everything done to items already in the library, except renaming and the trash: listing (with the scan),
//! favourite, tags, note, moving to a folder, duplicating, the history of older versions and its restore, and the
//! auto backups of the machine's current sound or beat.

use std::fs;
use std::path::PathBuf;

use super::inspect::{inspect, Facts};
use super::model::{kind_name, Facets, Item, Version};
use super::scan::{reconcile, stats, Report};
use super::{date_text, folder_of, now_secs, origin, write_replace, Library};
use crate::command::Kind;
use crate::error::AppError;
use crate::storage::names::sanitize_stem;
use crate::storage::stamp::utc_stamp;

const MAX_TAG: usize = 32;
const MAX_NOTE: usize = 2000;

/// The id of the pseudo-item that holds the auto backups of a kind.
pub fn slot_id(kind: Kind) -> String {
    format!("machine-{}", kind_name(kind))
}

fn unknown(id: &str) -> AppError {
    AppError::Invalid(format!("no library item \"{id}\""))
}

impl Library {
    /// The items of `kind` after making the index agree with the files, and what that changed.
    pub fn list(&self, kind: Kind) -> Result<(Vec<Item>, Report), AppError> {
        let (items, report) = self.update(|index| {
            let report = reconcile(self, index)?;
            let items = index
                .items
                .iter()
                .filter(|i| i.kind == kind)
                .cloned()
                .collect();
            Ok::<_, AppError>((items, report))
        })??;
        Ok((items, report))
    }

    pub(crate) fn history_dir(&self, kind: Kind, id: &str) -> Result<PathBuf, AppError> {
        let relative = format!("history/{}/{id}", kind_name(kind));
        let dir = self.data_path(&relative)?;
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Keeps `bytes` as an older version of `item` (`history/<kind>/<id>/<date of that version>.syx`).
    pub(crate) fn keep_version(
        &self,
        item: &Item,
        bytes: &[u8],
        label: &str,
    ) -> Result<Version, AppError> {
        self.keep_raw(item.kind, &item.id, item.modified, &item.hash, bytes, label)
    }

    fn keep_raw(
        &self,
        kind: Kind,
        id: &str,
        date: i64,
        hash: &str,
        bytes: &[u8],
        label: &str,
    ) -> Result<Version, AppError> {
        let dir = self.history_dir(kind, id)?;
        let path = self
            .storage()
            .save_in(&dir, &utc_stamp(date), "syx", bytes)?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(Version {
            file: format!("history/{}/{id}/{name}", kind_name(kind)),
            hash: hash.into(),
            date,
            label: label.into(),
        })
    }

    pub fn set_favourite(&self, ids: &[String], on: bool) -> Result<usize, AppError> {
        self.edit_items(ids, |item| item.favourite = on)
    }

    /// Adds or removes `tag` on each item.
    pub fn set_tag(&self, ids: &[String], add: bool, tag: &str) -> Result<usize, AppError> {
        let tag = valid_tag(tag)?;
        self.edit_items(ids, |item| {
            item.tags.retain(|t| !t.eq_ignore_ascii_case(&tag));
            if add {
                item.tags.push(tag.clone());
                item.tags.sort_by_key(|t| t.to_lowercase());
            }
        })
    }

    pub fn set_note(&self, id: &str, note: &str) -> Result<(), AppError> {
        if note.chars().count() > MAX_NOTE {
            return Err(AppError::Invalid(format!(
                "a note holds at most {MAX_NOTE} characters"
            )));
        }
        let note = note.to_string();
        self.edit_items(&[id.to_string()], |item| item.note.clone_from(&note))
            .map(|_| ())
    }

    fn edit_items(&self, ids: &[String], change: impl Fn(&mut Item)) -> Result<usize, AppError> {
        self.update(|index| {
            if let Some(missing) = ids.iter().find(|id| index.item(id).is_none()) {
                return Err(unknown(missing));
            }
            for id in ids {
                if let Some(item) = index.item_mut(id) {
                    change(item);
                }
            }
            Ok(ids.len())
        })?
    }

    /// Moves the files of `ids` into `folder` of their kind (`""`: the kind's own folder). Refuses everything,
    /// moving nothing, when one name is already taken there.
    pub fn move_items(&self, ids: &[String], folder: &str) -> Result<Vec<String>, AppError> {
        let folder = if folder.trim().is_empty() {
            String::new()
        } else {
            sanitize_stem(folder)
                .ok_or_else(|| AppError::Invalid("the folder name is empty".into()))?
        };
        let index = self.snapshot()?;
        let mut plan = Vec::new();
        for id in ids {
            let item = index.item(id).ok_or_else(|| unknown(id))?;
            if item.slot {
                return Err(AppError::Invalid("auto backups cannot be moved".into()));
            }
            let file_name = item
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&item.path)
                .to_string();
            let target = if folder.is_empty() {
                file_name.clone()
            } else {
                format!("{}/{file_name}", folder)
            };
            if target == item.path {
                continue;
            }
            let to = self.file_of(item.kind, &target)?;
            if to.exists() {
                return Err(AppError::Invalid(format!(
                    "a file named \"{file_name}\" is already in {}: nothing was moved",
                    if folder.is_empty() {
                        "the main folder".to_string()
                    } else {
                        format!("folder \"{folder}\"")
                    }
                )));
            }
            plan.push((item.clone(), target));
        }
        let mut lines = Vec::new();
        for (item, target) in &plan {
            self.move_files(item, target)?;
            lines.push(format!(
                "Moved \"{}\" from {}/{} to {}/{target}. To undo: move it back to its previous folder.",
                item.name,
                kind_name(item.kind),
                item.path,
                kind_name(item.kind)
            ));
        }
        let moves: Vec<(String, String, Option<String>)> = plan
            .iter()
            .map(|(item, target)| {
                let capture = item.capture.as_ref().map(|c| renamed_capture(c, target));
                (item.id.clone(), target.clone(), capture)
            })
            .collect();
        self.update(|index| {
            for (id, target, capture) in &moves {
                if let Some(item) = index.item_mut(id) {
                    item.path.clone_from(target);
                    item.capture.clone_from(capture);
                    if item.origin.starts_with("file:") {
                        let (folder, stem) = split_folder_stem(target);
                        item.origin = origin::file(folder, stem);
                    }
                }
            }
        })?;
        Ok(lines)
    }

    fn move_files(&self, item: &Item, target: &str) -> Result<(), AppError> {
        let from = self.file_of(item.kind, &item.path)?;
        let to = self.file_of(item.kind, target)?;
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&from, &to)?;
        if let Some(capture) = &item.capture {
            let capture_to = self.file_of(item.kind, &renamed_capture(capture, target))?;
            fs::rename(self.file_of(item.kind, capture)?, capture_to)?;
        }
        if let Some(old_folder) = from.parent() {
            // Only succeeds on an empty folder: a left-over empty one is not worth keeping.
            let top = self.storage().folder(folder_of(item.kind))?;
            if old_folder != top {
                let _ = fs::remove_dir(old_folder);
            }
        }
        Ok(())
    }

    /// A second item with the same content, in the same folder, named `<name> copy`.
    pub fn duplicate(&self, id: &str) -> Result<(Item, String), AppError> {
        let source = self.item(id)?;
        if source.slot {
            return Err(AppError::Invalid(
                "auto backups cannot be duplicated".into(),
            ));
        }
        let bytes = fs::read(self.file_of(source.kind, &source.path)?)?;
        let (folder, stem) = split_folder_stem(&source.path);
        let dir = self.dir_of(source.kind, folder)?;
        let path = self
            .storage()
            .save_in(&dir, &format!("{stem} copy"), "syx", &bytes)?;
        let file = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let relative = if folder.is_empty() {
            file
        } else {
            format!("{folder}/{file}")
        };
        let (size, mtime) = stats(&path);
        let created = now_secs();
        let line = format!(
            "Duplicated \"{}\" as {}/{relative}. Both files hold the same data. To undo: delete the copy (it goes to the trash).",
            source.name,
            kind_name(source.kind)
        );
        let added = self.update(|index| {
            let mut item = source.clone();
            item.id = index.next_id(item.kind);
            item.path.clone_from(&relative);
            item.size = size;
            item.mtime = mtime;
            item.created = created;
            item.modified = created;
            item.fetched = None;
            item.history = Vec::new();
            item.capture = None;
            item.favourite = false;
            item.source = format!("duplicate of {}", source.path);
            let (folder, stem) = split_folder_stem(&relative);
            item.origin = origin::file(folder, stem);
            index.items.push(item.clone());
            item
        })?;
        Ok((added, line))
    }

    /// An older version's bytes.
    pub fn version_bytes(&self, id: &str, at: usize) -> Result<(Item, Version, Vec<u8>), AppError> {
        let item = self.item(id)?;
        let version = item.history.get(at).cloned().ok_or_else(|| {
            AppError::Invalid(format!("\"{}\" has no version {}", item.name, at + 1))
        })?;
        let bytes = fs::read(self.data_path(&version.file)?)?;
        Ok((item, version, bytes))
    }

    /// Makes an older version the current one; the current one goes to the history first, so nothing is lost.
    pub fn restore_version(&self, id: &str, at: usize) -> Result<Vec<String>, AppError> {
        let (item, version, bytes) = self.version_bytes(id, at)?;
        if item.slot {
            return Err(AppError::Invalid(
                "auto backups are opened, not restored: open one and send it, or save it".into(),
            ));
        }
        let facts = inspect(item.kind, &bytes, &item.name)?;
        let file = self.file_of(item.kind, &item.path)?;
        let current = fs::read(&file)?;
        let label = format!(
            "replaced by restoring the version of {}",
            date_text(version.date)
        );
        let kept = self.keep_version(&item, &current, &label)?;
        write_replace(&file, &facts.canonical)?;
        let (size, mtime) = stats(&file);
        let now = now_secs();
        self.update(|index| {
            if let Some(entry) = index.item_mut(id) {
                entry.hash.clone_from(&facts.hash);
                entry.name = if facts.name.is_empty() {
                    entry.name.clone()
                } else {
                    facts.name.clone()
                };
                entry.facets.clone_from(&facts.facets);
                entry.size = size;
                entry.mtime = mtime;
                entry.modified = now;
                entry.history.push(kept.clone());
            }
        })?;
        Ok(vec![format!(
            "Restored \"{}\" to the version of {} ({}/{}). The version it replaced (from {}) is in its history. To undo: restore that one.",
            item.name,
            date_text(version.date),
            kind_name(item.kind),
            item.path,
            date_text(item.modified)
        )])
    }

    /// Keeps the machine's current sound or beat in the history of its slot before it is replaced.
    pub fn auto_backup(&self, facts: &Facts) -> Result<String, AppError> {
        let kind = facts.kind;
        let id = slot_id(kind);
        let now = now_secs();
        let label = format!("auto backup {}", utc_stamp(now));
        let version = self.keep_raw(kind, &id, now, &facts.hash, &facts.canonical, &label)?;
        let name = match kind {
            Kind::Sounds => "Machine: current sound",
            Kind::Beats => "Machine: current beat",
            Kind::Projects => "Machine: current project",
        };
        self.update(|index| {
            if index.item(&id).is_none() {
                index.items.push(slot_item(&id, kind, name, now));
            }
            if let Some(slot) = index.item_mut(&id) {
                slot.history.push(version.clone());
                slot.modified = now;
            }
        })?;
        Ok(version.file)
    }
}

fn slot_item(id: &str, kind: Kind, name: &str, now: i64) -> Item {
    Item {
        id: id.into(),
        kind,
        path: String::new(),
        name: name.into(),
        hash: String::new(),
        size: 0,
        mtime: 0,
        created: now,
        modified: now,
        fetched: None,
        source: "auto backup".into(),
        origin: format!("slot:{id}"),
        favourite: false,
        tags: Vec::new(),
        note: "Kept automatically before something was sent to the machine.".into(),
        history: Vec::new(),
        capture: None,
        beats: None,
        facets: Facets::default(),
        slot: true,
    }
}

pub(super) fn split_folder_stem(path: &str) -> (&str, &str) {
    let (folder, file) = path.split_once('/').unwrap_or(("", path));
    (folder, file.strip_suffix(".syx").unwrap_or(file))
}

/// The capture file name that goes with the standard file moved or renamed to `target`.
pub(super) fn renamed_capture(old_capture: &str, target: &str) -> String {
    let (folder, _) = target.split_once('/').unwrap_or(("", target));
    let name = old_capture.rsplit('/').next().unwrap_or(old_capture);
    if folder.is_empty() {
        name.to_string()
    } else {
        format!("{folder}/{name}")
    }
}

fn valid_tag(tag: &str) -> Result<String, AppError> {
    let tag = tag.trim();
    let bad = tag.is_empty()
        || tag.chars().count() > MAX_TAG
        || tag.chars().any(|c| c.is_control() || c == ',');
    if bad {
        return Err(AppError::Invalid(format!(
            "a tag is 1 to {MAX_TAG} characters, without commas"
        )));
    }
    Ok(tag.to_string())
}
