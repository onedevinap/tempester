//! The app's own trash: a deleted item's files move to `trash/<id>/` and its entry (history included) waits in the
//! index, restorable. Only « empty the trash », with the count the user confirmed, deletes for good.

use std::fs;

use super::items::split_folder_stem;
use super::model::{kind_name, Item, Trashed};
use super::{date_text, now_secs, Library};
use crate::error::AppError;
use crate::storage::names::is_plain_file_name;

fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

impl Library {
    pub fn trash_list(&self) -> Result<Vec<Trashed>, AppError> {
        Ok(self.snapshot()?.trash)
    }

    /// Moves items to the trash.
    pub fn delete_to_trash(&self, ids: &[String]) -> Result<Vec<String>, AppError> {
        let index = self.snapshot()?;
        let mut items: Vec<Item> = Vec::new();
        for id in ids {
            let item = index
                .item(id)
                .ok_or_else(|| AppError::Invalid(format!("no library item \"{id}\"")))?;
            if item.slot {
                return Err(AppError::Invalid(
                    "auto backups are not deleted one by one: they are the safety net".into(),
                ));
            }
            items.push(item.clone());
        }
        let now = now_secs();
        let mut entries = Vec::new();
        let mut lines = Vec::new();
        for item in items {
            let dir = self.data_path(&format!("trash/{}", item.id))?;
            fs::create_dir_all(&dir)?;
            let file = format!("trash/{}/{}", item.id, base_name(&item.path));
            fs::rename(self.file_of(item.kind, &item.path)?, self.data_path(&file)?)?;
            let capture_file = match &item.capture {
                Some(capture) => {
                    let moved = format!("trash/{}/{}", item.id, base_name(capture));
                    fs::rename(self.file_of(item.kind, capture)?, self.data_path(&moved)?)?;
                    Some(moved)
                }
                None => None,
            };
            lines.push(format!(
                "Moved \"{}\" ({}/{}) to the trash. Nothing is deleted for good: restore it from the trash, or empty the trash to delete.",
                item.name,
                kind_name(item.kind),
                item.path
            ));
            entries.push(Trashed {
                item,
                trashed: now,
                file,
                capture_file,
            });
        }
        self.update(|index| {
            for entry in &entries {
                index.items.retain(|i| i.id != entry.item.id);
                index.trash.push(entry.clone());
            }
        })?;
        Ok(lines)
    }

    /// Puts a trashed item back where it was (under another name if that one is taken meanwhile).
    pub fn restore_from_trash(&self, id: &str) -> Result<Vec<String>, AppError> {
        let index = self.snapshot()?;
        let entry = index
            .trash
            .iter()
            .find(|t| t.item.id == id)
            .cloned()
            .ok_or_else(|| AppError::Invalid(format!("no trashed item \"{id}\"")))?;
        let item = &entry.item;
        let (folder, stem) = split_folder_stem(&item.path);
        let dir = self.dir_of(item.kind, folder)?;
        let (path, capture) = free_names(&dir, folder, stem, item.capture.is_some());
        fs::rename(
            self.data_path(&entry.file)?,
            self.file_of(item.kind, &path)?,
        )?;
        if let (Some(from), Some(to)) = (&entry.capture_file, &capture) {
            fs::rename(self.data_path(from)?, self.file_of(item.kind, to)?)?;
        }
        let restored_path = path.clone();
        self.update(|index| {
            index.trash.retain(|t| t.item.id != id);
            let mut item = entry.item.clone();
            item.path.clone_from(&restored_path);
            item.capture.clone_from(&capture);
            index.items.push(item);
        })?;
        let note = if path == item.path {
            String::new()
        } else {
            format!(" (its old name was taken, so it is now {path})")
        };
        Ok(vec![format!(
            "Restored \"{}\" from the trash to {}/{path}{note}, with its history (trashed {}).",
            item.name,
            kind_name(item.kind),
            date_text(entry.trashed)
        )])
    }

    /// Deletes the trash for good. `expected` is the number of items the user was shown: when the trash changed
    /// since, nothing is deleted.
    pub fn empty_trash(&self, expected: usize) -> Result<Vec<String>, AppError> {
        let entries = self.trash_list()?;
        if entries.len() != expected {
            return Err(AppError::Invalid(format!(
                "the trash holds {} item(s), not the {expected} you confirmed: look again before emptying",
                entries.len()
            )));
        }
        let ids: Vec<String> = entries.iter().map(|t| t.item.id.clone()).collect();
        self.update(|index| index.trash.retain(|t| !ids.contains(&t.item.id)))?;
        for entry in &entries {
            let id = &entry.item.id;
            if is_plain_file_name(id) {
                let _ = fs::remove_dir_all(self.data_path(&format!("trash/{id}"))?);
                let history = format!("history/{}/{id}", kind_name(entry.item.kind));
                let _ = fs::remove_dir_all(self.data_path(&history)?);
            }
        }
        Ok(vec![format!(
            "Emptied the trash: {} item(s) and their history deleted for good. This cannot be undone.",
            entries.len()
        )])
    }
}

/// A free `(file, capture)` pair of names for `stem` in `dir`.
fn free_names(
    dir: &std::path::Path,
    folder: &str,
    stem: &str,
    with_capture: bool,
) -> (String, Option<String>) {
    let prefix = |name: String| {
        if folder.is_empty() {
            name
        } else {
            format!("{folder}/{name}")
        }
    };
    for n in 1..1000 {
        let suffix = if n == 1 {
            String::new()
        } else {
            format!(" ({n})")
        };
        let main = format!("{stem}{suffix}.syx");
        let capture = format!("{stem}{}{suffix}.syx", super::AS_RECEIVED);
        let taken = dir.join(&main).exists() || (with_capture && dir.join(&capture).exists());
        if !taken {
            return (prefix(main), with_capture.then(|| prefix(capture)));
        }
    }
    (prefix(format!("{stem} (restored).syx")), None)
}
