//! Everything the app writes lives under the data folder (`data_dir`): `sounds/`, `beats/`, `projects/`,
//! `backups/<date>/`, `os/` and `settings.txt` (which also holds the last installed OS, see `installed`). A received project puts its beats and sounds in a sub-folder of
//! `beats/` and `sounds/`, which the listing shows one level deep (`<folder>/<file>.syx`). A file is never overwritten: a name already taken gets a ` (2)`, ` (3)`... suffix.

pub mod data_dir;
pub mod installed;
pub mod names;
pub mod settings;
pub mod stamp;

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::command::Place;
use crate::error::AppError;
use names::{is_plain_file_name, sanitize_stem};
use settings::Settings;

const SETTINGS_FILE: &str = "settings.txt";
/// Largest file read back from the library (a sound is 0.2 KB, a beat a few KB).
const MAX_READ: u64 = 8 * 1024 * 1024;

/// A library folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Folder {
    Sounds,
    Beats,
    Projects,
    Backups,
    Os,
    /// Files written for other programs (Standard MIDI Files), never read back by the library.
    Exports,
}

impl Folder {
    fn dir_name(self) -> &'static str {
        match self {
            Self::Sounds => "sounds",
            Self::Beats => "beats",
            Self::Projects => "projects",
            Self::Backups => "backups",
            Self::Os => "os",
            Self::Exports => "exports",
        }
    }
}

/// A `.syx` file of a library folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub size: u64,
}

/// The data folder.
#[derive(Debug, Clone)]
pub struct Storage {
    root: PathBuf,
}

impl Storage {
    /// The user's data folder, created if needed.
    pub fn open() -> Result<Self, AppError> {
        let root = data_dir::data_dir().map_err(|e| AppError::Io(e.to_string()))?;
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// The data folder of the fake machine (`TEMPESTER_FAKE_MIDI=1`): a sub-folder, so that demo sounds and
    /// backups never mix with the user's real library.
    pub fn open_fake() -> Result<Self, AppError> {
        let root = data_dir::data_dir()
            .map_err(|e| AppError::Io(e.to_string()))?
            .join("fake-midi");
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// A storage in a disposable folder, for tests; refuses a place outside the temporary folder.
    #[cfg(test)]
    pub fn at(root: PathBuf) -> Self {
        let check = data_dir::ensure_disposable(&root, &std::env::temp_dir());
        assert!(
            check.is_ok(),
            "test storage outside the temporary folder: {root:?}"
        );
        Self { root }
    }

    /// The data folder itself (the library keeps its index, history and trash under it).
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn place_path(&self, place: Place) -> Result<PathBuf, AppError> {
        match place {
            Place::Data => Ok(self.root.clone()),
            Place::Sounds => self.folder(Folder::Sounds),
            Place::Beats => self.folder(Folder::Beats),
            Place::Projects => self.folder(Folder::Projects),
            Place::Backups => self.folder(Folder::Backups),
            Place::Os => self.folder(Folder::Os),
            Place::Exports => self.folder(Folder::Exports),
        }
    }

    pub fn folder(&self, folder: Folder) -> Result<PathBuf, AppError> {
        let path = self.root.join(folder.dir_name());
        fs::create_dir_all(&path)?;
        Ok(path)
    }

    /// Writes `bytes` as `<stem>.<ext>` in `folder` and returns the path actually used.
    #[cfg(test)]
    pub fn save(
        &self,
        folder: Folder,
        stem: &str,
        ext: &str,
        bytes: &[u8],
    ) -> Result<PathBuf, AppError> {
        let dir = self.folder(folder)?;
        save_unique(&dir, stem, ext, bytes)
    }

    /// Same as `save`, inside a directory made by `new_backup_dir`.
    pub fn save_in(
        &self,
        dir: &Path,
        stem: &str,
        ext: &str,
        bytes: &[u8],
    ) -> Result<PathBuf, AppError> {
        if !dir.starts_with(&self.root) {
            return Err(AppError::Invalid(
                "refusing to write outside the data folder".into(),
            ));
        }
        save_unique(dir, stem, ext, bytes)
    }

    /// The `.syx` files of a folder, by name, and those of its sub-folders as `<sub-folder>/<file>`.
    pub fn list(&self, folder: Folder) -> Result<Vec<Entry>, AppError> {
        let mut entries = Vec::new();
        let top = self.folder(folder)?;
        for item in fs::read_dir(&top)? {
            let item = item?;
            let name = item.file_name().to_string_lossy().into_owned();
            let meta = item.metadata()?;
            if meta.is_file() {
                push_syx(&mut entries, name, meta.len());
            } else if meta.is_dir() && is_plain_file_name(&name) {
                for inner in fs::read_dir(item.path())? {
                    let inner = inner?;
                    let meta = inner.metadata()?;
                    if meta.is_file() {
                        let inner_name = inner.file_name().to_string_lossy().into_owned();
                        push_syx(&mut entries, format!("{name}/{inner_name}"), meta.len());
                    }
                }
            }
        }
        entries.sort_by_key(|entry| entry.name.to_lowercase());
        Ok(entries)
    }

    /// Reads a library file by its plain name, or `<sub-folder>/<file>` as `list` shows it.
    pub fn read(&self, folder: Folder, name: &str) -> Result<Vec<u8>, AppError> {
        let parts: Vec<&str> = name.split('/').collect();
        if parts.len() > 2 || !parts.iter().all(|part| is_plain_file_name(part)) {
            return Err(AppError::Invalid(format!(
                "\"{name}\" is not a plain file name"
            )));
        }
        let path = parts
            .iter()
            .fold(self.folder(folder)?, |path, part| path.join(part));
        if fs::metadata(&path)?.len() > MAX_READ {
            return Err(AppError::Invalid(format!(
                "\"{name}\" is too large for the library"
            )));
        }
        Ok(fs::read(path)?)
    }

    /// A fresh `backups/<UTC date>/` folder (a second one in the same second gets a suffix).
    pub fn new_backup_dir(&self, stamp: &str) -> Result<PathBuf, AppError> {
        self.new_subfolder(Folder::Backups, stamp)
    }

    /// A fresh sub-folder `name` of `folder`: a name already taken gets a `-1`, `-2`... suffix, so that nothing
    /// existing is ever mixed with or overwritten.
    pub fn new_subfolder(&self, folder: Folder, stamp: &str) -> Result<PathBuf, AppError> {
        let stamp =
            sanitize_stem(stamp).ok_or_else(|| AppError::Invalid("the name is empty".into()))?;
        let parent = self.folder(folder)?;
        for attempt in 0..100 {
            let name = if attempt == 0 {
                stamp.to_string()
            } else {
                format!("{stamp}-{attempt}")
            };
            let path = parent.join(name);
            match fs::create_dir(&path) {
                Ok(()) => return Ok(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Err(AppError::Io("cannot find a free folder name".into()))
    }

    pub fn load_settings(&self) -> Settings {
        fs::read_to_string(self.root.join(SETTINGS_FILE))
            .map(|text| Settings::parse(&text))
            .unwrap_or_default()
    }

    /// Re-reads the file, applies `change`, writes: another change made meanwhile is merged, not overwritten.
    pub fn update_settings(
        &self,
        change: impl FnOnce(&mut Settings),
    ) -> Result<Settings, AppError> {
        let mut settings = self.load_settings();
        change(&mut settings);
        let path = self.root.join(SETTINGS_FILE);
        let temp = self.root.join(format!("{SETTINGS_FILE}.tmp"));
        fs::write(&temp, settings.render())?;
        fs::rename(temp, path)?;
        Ok(settings)
    }
}

fn push_syx(entries: &mut Vec<Entry>, name: String, size: u64) {
    if name.to_ascii_lowercase().ends_with(".syx") {
        entries.push(Entry { name, size });
    }
}

fn save_unique(dir: &Path, stem: &str, ext: &str, bytes: &[u8]) -> Result<PathBuf, AppError> {
    let stem = sanitize_stem(stem).ok_or_else(|| AppError::Invalid("the name is empty".into()))?;
    for attempt in 1..1000 {
        let file = if attempt == 1 {
            format!("{stem}.{ext}")
        } else {
            format!("{stem} ({attempt}).{ext}")
        };
        let path = dir.join(file);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut handle) => {
                handle.write_all(bytes)?;
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(AppError::Io(format!("too many files named \"{stem}\"")))
}

/// A disposable storage for tests.
#[cfg(test)]
pub fn temp_storage(label: &str) -> Storage {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let root =
        std::env::temp_dir().join(format!("tempester-test-{}-{label}-{n}", std::process::id()));
    let storage = Storage::at(root);
    // A leftover from an earlier crashed run with the same pid would break "starts empty".
    let _ = fs::remove_dir_all(&storage.root);
    assert!(fs::create_dir_all(&storage.root).is_ok());
    storage
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_save_when_name_taken_returns_new_file_and_keeps_the_old_one() {
        let storage = temp_storage("save");

        let first = storage.save(Folder::Sounds, "Kick", "syx", b"one").unwrap();
        let second = storage.save(Folder::Sounds, "Kick", "syx", b"two").unwrap();

        assert_ne!(first, second);
        assert!(second.ends_with("Kick (2).syx"));
        assert_eq!(fs::read(first).unwrap(), b"one");
        assert_eq!(fs::read(second).unwrap(), b"two");
    }

    #[test]
    fn test_save_when_name_has_path_characters_returns_file_inside_the_folder() {
        let storage = temp_storage("traversal");

        let path = storage
            .save(Folder::Beats, "../../escape", "syx", b"x")
            .unwrap();

        assert_eq!(
            path.parent(),
            Some(storage.folder(Folder::Beats).unwrap().as_path())
        );
    }

    #[test]
    fn test_save_when_name_is_empty_returns_error_and_writes_nothing() {
        let storage = temp_storage("empty");

        assert!(storage.save(Folder::Sounds, "  ..", "syx", b"x").is_err());
        assert!(storage.list(Folder::Sounds).unwrap().is_empty());
    }

    #[test]
    fn test_list_when_mixed_files_returns_only_syx_sorted_without_case() {
        let storage = temp_storage("list");
        for name in ["b.syx", "A.SYX", "notes.txt", "c.syx"] {
            fs::write(storage.folder(Folder::Sounds).unwrap().join(name), b"1234").unwrap();
        }

        let names: Vec<String> = storage
            .list(Folder::Sounds)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();

        assert_eq!(names, vec!["A.SYX", "b.syx", "c.syx"]);
    }

    #[test]
    fn test_read_when_name_escapes_the_folder_returns_error() {
        let storage = temp_storage("read");
        fs::write(storage.root.join("secret.txt"), b"s").unwrap();

        assert!(storage.read(Folder::Sounds, "../secret.txt").is_err());
        assert!(storage.read(Folder::Sounds, "/etc/passwd").is_err());
    }

    #[test]
    fn test_read_when_saved_file_returns_its_bytes() {
        let storage = temp_storage("roundtrip");
        storage
            .save(Folder::Sounds, "Snare", "syx", &[1, 2, 3])
            .unwrap();

        assert_eq!(
            storage.read(Folder::Sounds, "Snare.syx").unwrap(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn test_new_backup_dir_when_same_stamp_twice_returns_two_distinct_folders() {
        let storage = temp_storage("backup");

        let a = storage.new_backup_dir("2026-11-07_000000Z").unwrap();
        let b = storage.new_backup_dir("2026-11-07_000000Z").unwrap();

        assert_ne!(a, b);
        assert!(a.is_dir() && b.is_dir());
    }

    #[test]
    fn test_save_in_when_directory_is_outside_the_data_folder_returns_error() {
        let storage = temp_storage("outside");
        let other = temp_storage("other");

        let result = storage.save_in(&other.root, "x", "syx", b"x");

        assert!(result.is_err());
        assert!(other.list(Folder::Sounds).unwrap().is_empty());
    }

    #[test]
    fn test_update_settings_when_file_changed_meanwhile_returns_merged_values() {
        let storage = temp_storage("settings");
        storage.update_settings(|s| s.set("port", "A")).unwrap();
        fs::write(storage.root.join(SETTINGS_FILE), "port=A\nextra=kept\n").unwrap();

        let merged = storage.update_settings(|s| s.set("channel", "5")).unwrap();

        assert_eq!(merged.get("extra"), Some("kept"));
        assert_eq!(storage.load_settings().channel(), 5);
    }

    #[test]
    fn test_at_when_root_is_outside_temp_returns_panic() {
        let result =
            std::panic::catch_unwind(|| Storage::at(PathBuf::from("/Users/someone/Tempester")));

        assert!(result.is_err());
    }

    #[test]
    fn test_list_when_sub_folder_holds_files_returns_them_as_folder_slash_file() {
        let storage = temp_storage("list-sub");
        storage.save(Folder::Sounds, "Top", "syx", b"t").unwrap();
        let dir = storage.new_subfolder(Folder::Sounds, "Project A").unwrap();
        storage.save_in(&dir, "Inner", "syx", b"i").unwrap();
        storage.save_in(&dir, "notes", "txt", b"n").unwrap();

        let names: Vec<String> = storage
            .list(Folder::Sounds)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();

        assert_eq!(names, vec!["Project A/Inner.syx", "Top.syx"]);
    }

    #[test]
    fn test_read_when_name_is_folder_slash_file_returns_the_bytes() {
        let storage = temp_storage("read-sub");
        let dir = storage.new_subfolder(Folder::Beats, "P").unwrap();
        storage.save_in(&dir, "B", "syx", b"beat").unwrap();

        assert_eq!(storage.read(Folder::Beats, "P/B.syx").unwrap(), b"beat");
    }

    #[test]
    fn test_read_when_path_climbs_or_goes_too_deep_returns_invalid_error() {
        let storage = temp_storage("read-deep");
        for name in [
            "P/../x.syx",
            "../x.syx",
            "a/b/c.syx",
            "/etc/passwd",
            "P//x.syx",
        ] {
            assert!(
                matches!(storage.read(Folder::Beats, name), Err(AppError::Invalid(_))),
                "{name}"
            );
        }
    }

    #[test]
    fn test_new_subfolder_when_name_taken_returns_a_new_one_and_keeps_the_old_files() {
        let storage = temp_storage("subfolder");
        let first = storage.new_subfolder(Folder::Projects, "P").unwrap();
        storage.save_in(&first, "a", "syx", b"keep").unwrap();

        let second = storage.new_subfolder(Folder::Projects, "P").unwrap();

        assert_ne!(first, second);
        assert_eq!(fs::read(first.join("a.syx")).unwrap(), b"keep");
        assert_eq!(fs::read_dir(second).unwrap().count(), 0);
    }
}
