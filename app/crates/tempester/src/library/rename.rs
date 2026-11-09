//! Renaming an item: the internal 20-character name inside the data and the file name change together, and the
//! old version goes to the history.
//!
//! The internal name is what the firmware reads (`refs/firmware-main.md` 9.2): for a sound, 20 characters of 7 bits
//! from byte 110 of its 128 bytes; for a beat, words 12-21 of the header (bytes 24-43 of the data, high byte
//! first). Both are ASCII 0x20-0x7E padded with spaces. A project has no name of its own in its data: only its file
//! (and the name the library shows) changes.

use std::fs;

use tempest_proto::beat;
use tempest_proto::sound::{Sound, NAME_LEN};
use tempest_proto::sound_message::{self, SoundMessage};

use super::inspect::{inspect, sound_type_of};
use super::items::split_folder_stem;
use super::model::{kind_name, Item};
use super::{date_text, origin, scan::stats, write_replace, Library};
use crate::command::Kind;
use crate::error::AppError;
use crate::storage::names::sanitize_stem;

/// Longest file stem of a project (the names the app writes are longer than a machine name).
const MAX_PROJECT_NAME: usize = 80;

/// Why `name` cannot be the internal name of a sound or beat, or `None` when it can.
pub fn name_refusal(name: &str) -> Option<String> {
    if name.is_empty() || name.trim().is_empty() {
        return Some("the name is empty".into());
    }
    if name != name.trim() {
        return Some(
            "the name starts or ends with a space (the machine pads names with spaces itself)"
                .into(),
        );
    }
    if let Some(bad) = name.chars().find(|c| !(' '..='~').contains(c)) {
        return Some(format!(
            "\"{bad}\" is not allowed: only the printable ASCII characters (space to ~) fit in a machine name"
        ));
    }
    let length = name.chars().count();
    (length > NAME_LEN)
        .then(|| format!("the name has {length} characters, the machine keeps {NAME_LEN}"))
}

fn project_name_refusal(name: &str) -> Option<String> {
    if name.is_empty() || name.trim().is_empty() {
        return Some("the name is empty".into());
    }
    if name.chars().count() > MAX_PROJECT_NAME {
        return Some(format!(
            "the name is longer than {MAX_PROJECT_NAME} characters"
        ));
    }
    (sanitize_stem(name).as_deref() != Some(name)).then(|| {
        "the name has characters a file name cannot hold (/ \\ : * ? \" < > |), starts with a dot or with a space"
            .to_string()
    })
}

/// The data of `item` with the internal name `name`, as the file to write.
fn renamed_bytes(item: &Item, current: &[u8], name: &str) -> Result<Vec<u8>, AppError> {
    match item.kind {
        Kind::Sounds => {
            let (mut sound, old_file_name): (Sound, String) = match sound_message::decode(current)
                .map_err(|e| {
                AppError::Invalid(e.to_string())
            })? {
                SoundMessage::File { name, sound } => (sound, name),
                SoundMessage::Current(sound) => (sound, String::new()),
            };
            sound
                .set_name(name)
                .map_err(|e| AppError::Invalid(e.to_string()))?;
            let folder = sound_type_of(&old_file_name);
            let file_name = if folder.is_empty() {
                name.to_string()
            } else {
                format!("/S/{folder}/{name}")
            };
            sound_message::encode_file(&file_name, &sound)
                .map_err(|e| AppError::Invalid(e.to_string()))
        }
        Kind::Beats => {
            let message =
                beat::standard_message(current).map_err(|e| AppError::Invalid(e.to_string()))?;
            let data = beat::read_body(&message[4..message.len() - 1])
                .map_err(|e| AppError::Invalid(e.to_string()))?;
            let renamed = beat::with_name(&data, name)
                .ok_or_else(|| AppError::Invalid("this beat holds no name field".into()))?;
            Ok(beat::standard_dump(&renamed))
        }
        Kind::Projects => Ok(current.to_vec()),
    }
}

impl Library {
    /// Renames `id`; the messages say what changed and where, and how to undo.
    pub fn rename(&self, id: &str, name: &str) -> Result<Vec<String>, AppError> {
        let item = self.item(id)?;
        if item.slot {
            return Err(AppError::Invalid("auto backups cannot be renamed".into()));
        }
        let refusal = if item.kind == Kind::Projects {
            project_name_refusal(name)
        } else {
            name_refusal(name)
        };
        if let Some(why) = refusal {
            return Err(AppError::Invalid(format!("\"{name}\" is refused: {why}")));
        }
        if name == item.name {
            return Err(AppError::Invalid(format!("\"{name}\" is already its name")));
        }
        let (folder, _) = split_folder_stem(&item.path);
        let stem =
            sanitize_stem(name).ok_or_else(|| AppError::Invalid("the name is empty".into()))?;
        let new_path = if folder.is_empty() {
            format!("{stem}.syx")
        } else {
            format!("{folder}/{stem}.syx")
        };
        let to = self.file_of(item.kind, &new_path)?;
        let moves = new_path != item.path;
        if moves && to.exists() {
            return Err(AppError::Invalid(format!(
                "a file named \"{stem}.syx\" already exists there: choose another name, nothing was changed"
            )));
        }
        let current = fs::read(self.file_of(item.kind, &item.path)?)?;
        let bytes = renamed_bytes(&item, &current, name)?;
        let facts = inspect(item.kind, &bytes, name)?;
        if item.kind != Kind::Projects {
            let label = format!("renamed to \"{name}\" on {}", date_text(super::now_secs()));
            let version = self.keep_version(&item, &current, &label)?;
            self.finish_rename(&item, &new_path, &bytes, &facts, Some(version))?;
        } else {
            self.finish_rename(&item, &new_path, &bytes, &facts, None)?;
        }
        Ok(vec![rename_line(&item, name, &new_path)])
    }

    fn finish_rename(
        &self,
        item: &Item,
        new_path: &str,
        bytes: &[u8],
        facts: &super::inspect::Facts,
        version: Option<super::model::Version>,
    ) -> Result<(), AppError> {
        let from = self.file_of(item.kind, &item.path)?;
        let to = self.file_of(item.kind, new_path)?;
        let capture = item.capture.as_ref().map(|_| capture_name_for(new_path));
        if new_path == item.path {
            write_replace(&from, bytes)?;
        } else {
            write_replace(&to, bytes)?;
            if let (Some(old), Some(new)) = (&item.capture, &capture) {
                fs::rename(self.file_of(item.kind, old)?, self.file_of(item.kind, new)?)?;
            }
        }
        let (size, mtime) = stats(&to);
        let item = item.clone();
        let new_path = new_path.to_string();
        let facts = facts.clone();
        self.update(|index| {
            if let Some(entry) = index.item_mut(&item.id) {
                entry.path.clone_from(&new_path);
                entry.capture.clone_from(&capture);
                entry.name = if facts.name.is_empty() {
                    name_of_path(&new_path)
                } else {
                    facts.name.clone()
                };
                entry.hash.clone_from(&facts.hash);
                entry.facets.clone_from(&facts.facets);
                entry.size = size;
                entry.mtime = mtime;
                if version.is_some() {
                    entry.modified = super::now_secs();
                }
                if item.kind != Kind::Projects {
                    entry.origin = origin::of_path(item.kind, &new_path, &facts);
                }
                entry.history.extend(version.clone());
            }
        })?;
        if new_path != item.path {
            // The old file goes last: until now a crash only leaves two files, never none.
            fs::remove_file(from)?;
        }
        Ok(())
    }
}

/// The capture's file name when its standard file is `new_path`: same folder, `<stem> (as received).syx`.
fn capture_name_for(new_path: &str) -> String {
    let (folder, stem) = split_folder_stem(new_path);
    let name = format!("{stem}{}.syx", super::AS_RECEIVED);
    if folder.is_empty() {
        name
    } else {
        format!("{folder}/{name}")
    }
}

fn name_of_path(path: &str) -> String {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .trim_end_matches(".syx")
        .to_string()
}

fn rename_line(item: &Item, name: &str, new_path: &str) -> String {
    let kind = kind_name(item.kind);
    if item.kind == Kind::Projects {
        format!(
            "Renamed project \"{}\" to \"{name}\": its file is now {kind}/{new_path} (was {kind}/{}). A project holds no name of its own, so its data is unchanged. To undo: rename it back.",
            item.name, item.path
        )
    } else {
        format!(
            "Renamed {} \"{}\" to \"{name}\": the name inside the data (20 characters, padded with spaces) and the file changed together, {kind}/{} -> {kind}/{new_path}. The previous version is kept in its history. To undo: rename it back, or restore the previous version from its History.",
            kind.trim_end_matches('s'),
            item.name,
            item.path
        )
    }
}
