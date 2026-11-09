//! The identity of an item across fetches: two files with the same origin and different content are two versions of
//! one thing (« modified version »); the same content under any origin is a duplicate.
//!
//! - a file: the same name in the same folder (`file:<folder>/<name>`, case-insensitive);
//! - a beat of a project: the same slot and the same beat name;
//! - a sound of a project: the same beat slot, the same pad and the same sound name;
//! - a project: the same beat slots holding beats of the same names (a project has no name of its own).
//!
//! `scan` derives the origin of a file found on disk from its path with the same rules, so a file the app wrote and
//! the same file found by a rebuild agree.

use tempest_proto::sha256::sha256_hex;

use super::inspect::Facts;
use crate::command::Kind;

pub fn file(folder: &str, stem: &str) -> String {
    format!("file:{}/{}", folder.to_lowercase(), stem.to_lowercase())
}

pub fn project_beat(slot: u8, beat_name: &str) -> String {
    format!("project-beat:{:02}:{beat_name}", slot + 1)
}

pub fn project_sound(slot: u8, pad: usize, sound_name: &str) -> String {
    format!("project-sound:{:02}:{:02}:{sound_name}", slot + 1, pad + 1)
}

pub fn project(signature: &str) -> String {
    format!("project:{}", &sha256_hex(signature.as_bytes())[..16])
}

/// The origin of a file found at `path` (`Kick.syx` or `Folder/Kick.syx`) with these facts.
pub fn of_path(kind: Kind, path: &str, facts: &Facts) -> String {
    let (folder, file_name) = path.split_once('/').unwrap_or(("", path));
    let stem = file_name.strip_suffix(".syx").unwrap_or(file_name);
    if kind == Kind::Projects {
        return project(&facts.signature);
    }
    if folder.starts_with("Project ") {
        if let Some(origin) = project_origin(kind, stem, &facts.name) {
            return origin;
        }
    }
    file(folder, stem)
}

/// `03 Beat name` (a beat) or `03 Beat name pad 05 Sound name` (a sound), as `save_project` names the files.
fn project_origin(kind: Kind, stem: &str, name: &str) -> Option<String> {
    let slot: u8 = stem
        .get(..2)?
        .parse()
        .ok()
        .filter(|n| (1..=16).contains(n))?;
    if stem.as_bytes().get(2) != Some(&b' ') {
        return None;
    }
    match kind {
        Kind::Beats => Some(project_beat(slot - 1, name)),
        Kind::Sounds => {
            let (_, after) = stem.rsplit_once(" pad ")?;
            let pad: usize = after
                .get(..2)?
                .parse()
                .ok()
                .filter(|n| (1..=32).contains(n))?;
            Some(project_sound(slot - 1, pad - 1, name))
        }
        Kind::Projects => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::model::Facets;

    fn facts(name: &str) -> Facts {
        Facts {
            kind: Kind::Sounds,
            name: name.into(),
            hash: String::new(),
            canonical: vec![],
            facets: Facets::default(),
            beats: None,
            signature: "16|0:A".into(),
        }
    }

    #[test]
    fn test_of_path_when_files_follow_the_save_project_names_returns_the_slot_origins() {
        assert_eq!(
            of_path(
                Kind::Beats,
                "Project 2026-11-08_100000Z/03 Fake beat 03.syx",
                &facts("Fake beat 03")
            ),
            project_beat(2, "Fake beat 03")
        );
        assert_eq!(
            of_path(
                Kind::Sounds,
                "Project x/01 Fake beat 01 pad 05 Fake 5 v1.syx",
                &facts("Fake 5 v1")
            ),
            project_sound(0, 4, "Fake 5 v1")
        );
    }

    #[test]
    fn test_of_path_when_plain_file_returns_folder_and_lowercase_stem() {
        assert_eq!(of_path(Kind::Sounds, "Kick.syx", &facts("K")), "file:/kick");
        assert_eq!(
            of_path(Kind::Sounds, "Pack/Kick.syx", &facts("K")),
            "file:pack/kick"
        );
        assert_eq!(
            of_path(Kind::Sounds, "Project x/not numbered.syx", &facts("K")),
            "file:project x/not numbered"
        );
    }

    #[test]
    fn test_of_path_when_project_returns_the_signature_origin_whatever_the_file_name() {
        let a = of_path(Kind::Projects, "Project 1.syx", &facts(""));
        let b = of_path(Kind::Projects, "Project 2.syx", &facts(""));

        assert_eq!(a, b);
        assert!(a.starts_with("project:"));
    }
}
