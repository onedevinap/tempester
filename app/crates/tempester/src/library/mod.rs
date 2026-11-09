//! The library: the `.syx` files of `sounds/`, `beats/` and `projects/` plus ONE index, `library.json`, in the data
//! folder. The files stay plain files in the standard 7-bit layout; the index adds what files cannot say (hash of the
//! decoded data, dates, source, favourite, tags, note, older versions).
//!
//! - The index is never held in memory between operations: every change reads it again, applies the change to what
//!   it read, and writes it (temp file + rename). If the file changed between the read and the write (another
//!   writer), the change is applied again to the new content: nothing is overwritten with stale memory.
//! - An absent or damaged index is rebuilt from the files (a damaged one is set aside, not deleted).
//! - Older versions go to `history/<kind>/<id>/`, deleted items to `trash/<id>/`; nothing is deleted for good
//!   except by emptying the trash.
//!
//! Files are written before the index mentions them and removed after it stopped mentioning them, so a crash leaves
//! at worst an orphan file (found and added by the next scan), never an entry without its file.

pub mod command;
pub mod diff;
pub mod inspect;
pub mod model;
pub mod origin;
pub mod plan;
pub mod scan;

mod items;
mod rename;
mod trash;

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::command::Kind;
use crate::error::AppError;
use crate::storage::names::is_plain_file_name;
use crate::storage::stamp::utc_stamp;
use crate::storage::{Folder, Storage};
use model::{Index, Item};

/// Ends the name of the file that holds a project exactly as the machine sent it.
pub const AS_RECEIVED: &str = " (as received)";

const INDEX_FILE: &str = "library.json";
/// Times a change is applied again because the index changed under it.
const TRIES: usize = 8;

pub fn folder_of(kind: Kind) -> Folder {
    match kind {
        Kind::Sounds => Folder::Sounds,
        Kind::Beats => Folder::Beats,
        Kind::Projects => Folder::Projects,
    }
}

/// Seconds since the epoch.
pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| i64::try_from(since.as_secs()).unwrap_or(0))
}

/// `2026-11-08 10:00 UTC`-style text for the messages.
pub fn date_text(seconds: i64) -> String {
    let stamp = utc_stamp(seconds);
    format!("{} {}:{} UTC", &stamp[..10], &stamp[11..13], &stamp[13..15])
}

/// The library of one data folder.
#[derive(Debug, Clone)]
pub struct Library {
    storage: Storage,
}

impl Library {
    pub fn new(storage: &Storage) -> Self {
        Self {
            storage: storage.clone(),
        }
    }

    fn index_path(&self) -> PathBuf {
        self.storage.root().join(INDEX_FILE)
    }

    fn read_text(&self) -> Result<Option<String>, AppError> {
        match fs::read_to_string(self.index_path()) {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            // A file that is not UTF-8 is a damaged index like any other.
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => Ok(Some(String::new())),
            Err(e) => Err(e.into()),
        }
    }

    /// The index as the file holds it, or rebuilt from the files when the file is absent or damaged.
    fn index_from(&self, text: Option<&str>) -> Result<Index, AppError> {
        if let Some(index) = text.and_then(|text| Index::parse(text).ok()) {
            return Ok(index);
        }
        let mut index = Index::default();
        scan::reconcile(self, &mut index)?;
        Ok(index)
    }

    /// Reads the index, applies `change` to it and writes it back; see the module comment for the merge rule.
    pub fn update<R>(&self, mut change: impl FnMut(&mut Index) -> R) -> Result<R, AppError> {
        self.update_checked(true, &mut change)
    }

    /// `update`, with the check that the file did not change meanwhile switched off: only tests use it, to show
    /// what the check prevents.
    #[cfg(test)]
    fn update_unchecked<R>(&self, mut change: impl FnMut(&mut Index) -> R) -> Result<R, AppError> {
        self.update_checked(false, &mut change)
    }

    fn update_checked<R>(
        &self,
        check: bool,
        change: &mut dyn FnMut(&mut Index) -> R,
    ) -> Result<R, AppError> {
        for _ in 0..TRIES {
            let before = self.read_text()?;
            let mut index = self.index_from(before.as_deref())?;
            let result = change(&mut index);
            let text = index.render();
            if check && self.read_text()? != before {
                continue;
            }
            if before.as_deref() != Some(text.as_str()) {
                self.write_index(&text)?;
            }
            return Ok(result);
        }
        Err(AppError::Io(
            "the library index keeps changing: try again".into(),
        ))
    }

    /// Writes the text to a temporary file next to the index, then renames it over the index: the index is either
    /// the old one or the new one, never half written.
    fn write_index(&self, text: &str) -> Result<(), AppError> {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let temp = self
            .storage
            .root()
            .join(format!("{INDEX_FILE}.tmp-{}-{n}", std::process::id()));
        let written = fs::write(&temp, text).and_then(|()| fs::rename(&temp, self.index_path()));
        if let Err(e) = written {
            // The temporary file is ours: a failure must not leave it behind.
            let _ = fs::remove_file(&temp);
            return Err(e.into());
        }
        Ok(())
    }

    /// Sets a damaged index aside (never deleted) and says so; `None` when the index is absent or fine.
    pub fn set_aside_if_damaged(&self) -> Result<Option<PathBuf>, AppError> {
        let Some(text) = self.read_text()? else {
            return Ok(None);
        };
        if Index::parse(&text).is_ok() {
            return Ok(None);
        }
        let aside = self.storage.root().join(format!(
            "{INDEX_FILE}.damaged-{}",
            crate::storage::stamp::now_stamp()
        ));
        fs::rename(self.index_path(), &aside)?;
        Ok(Some(aside))
    }

    /// The index now (rebuilt first when absent or damaged).
    pub fn snapshot(&self) -> Result<Index, AppError> {
        self.update(|index| index.clone())
    }

    /// The absolute path of an item's file, refusing a path that is not a plain `file` or `folder/file`.
    pub(crate) fn file_of(&self, kind: Kind, relative: &str) -> Result<PathBuf, AppError> {
        let parts: Vec<&str> = relative.split('/').collect();
        if parts.len() > 2 || !parts.iter().all(|p| is_plain_file_name(p)) {
            return Err(AppError::Invalid(format!(
                "\"{relative}\" is not a plain library path"
            )));
        }
        let base = self.storage.folder(folder_of(kind))?;
        Ok(parts.iter().fold(base, |path, part| path.join(part)))
    }

    /// An absolute path under the data folder from a relative one the index holds (history, trash), refusing
    /// anything that climbs out.
    pub(crate) fn data_path(&self, relative: &str) -> Result<PathBuf, AppError> {
        if relative.split('/').any(|p| !is_plain_file_name(p)) {
            return Err(AppError::Invalid(format!(
                "\"{relative}\" is not a plain data path"
            )));
        }
        Ok(relative
            .split('/')
            .fold(self.storage.root().to_path_buf(), |path, part| {
                path.join(part)
            }))
    }

    /// The folder of a kind, or one of its sub-folders (`""` for the kind's own folder), created if needed.
    pub(crate) fn dir_of(&self, kind: Kind, folder: &str) -> Result<PathBuf, AppError> {
        let base = self.storage.folder(folder_of(kind))?;
        if folder.is_empty() {
            return Ok(base);
        }
        if !is_plain_file_name(folder) {
            return Err(AppError::Invalid(format!(
                "\"{folder}\" is not a folder name"
            )));
        }
        let dir = base.join(folder);
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    pub(crate) fn storage(&self) -> &Storage {
        &self.storage
    }

    pub fn item(&self, id: &str) -> Result<Item, AppError> {
        self.snapshot()?
            .item(id)
            .cloned()
            .ok_or_else(|| AppError::Invalid(format!("no library item \"{id}\"")))
    }
}

/// Replaces the content of `path` without ever leaving it half written (temp file in the same folder + rename).
pub(crate) fn write_replace(path: &std::path::Path, bytes: &[u8]) -> Result<(), AppError> {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp = path.with_file_name(format!(".{name}.tmp-{}-{n}", std::process::id()));
    let written = fs::write(&temp, bytes).and_then(|()| fs::rename(&temp, path));
    if let Err(e) = written {
        // The temporary file is ours: a failure must not leave it behind.
        let _ = fs::remove_file(&temp);
        return Err(e.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
