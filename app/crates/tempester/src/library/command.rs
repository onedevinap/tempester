//! The library commands the page sends (`lib_*`): one line each, arguments after tabs, lists of ids joined by
//! commas. Everything is checked here, at the entrance: an id is a short plain word, a number is a number.

use super::model::kind_from_name;
use super::plan::{OnIdentical, OnModified, OnNew, Policy};
use crate::command::{CommandError, Kind};
use crate::hex;
use tempest_proto::beat::PADS;

const MAX_IDS: usize = 5000;
const MAX_ID_LEN: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibCommand {
    /// A `.syx` file picked in the page, to put in the library (what it is is read from its bytes).
    Import {
        name: String,
        data: Vec<u8>,
    },
    /// The user's choices for the batch waiting in the review dialog.
    Apply(Policy),
    CancelPlan,
    Open {
        id: String,
    },
    OpenVersion {
        id: String,
        version: usize,
    },
    Favourite {
        ids: Vec<String>,
        on: bool,
    },
    Tag {
        ids: Vec<String>,
        add: bool,
        tag: String,
    },
    Note {
        id: String,
        text: String,
    },
    Move {
        ids: Vec<String>,
        folder: String,
    },
    Duplicate {
        id: String,
    },
    Delete {
        ids: Vec<String>,
    },
    RestoreTrash {
        id: String,
    },
    EmptyTrash {
        expected: usize,
    },
    Trash,
    History {
        id: String,
    },
    Compare {
        id: String,
        version: usize,
    },
    RestoreVersion {
        id: String,
        version: usize,
    },
    Rename {
        id: String,
        name: String,
    },
    /// Looks at what the machine holds before something replaces it. An empty `id` means the item opened in the
    /// current sound or beat card.
    SendCheck {
        kind: Kind,
        id: String,
    },
    /// Looks at the machine's project before a project replaces it; `version` picks an older version of the item
    /// (an auto backup's), `None` the current file.
    ProjectCheck {
        id: String,
        version: Option<usize>,
    },
    /// Sends a project after the user confirmed what the check said; `machine` is the hash it saw (`none`: no answer).
    ProjectSend {
        id: String,
        version: Option<usize>,
        machine: String,
        mode: ProjectMode,
    },
    /// Stops the project send that is running.
    ProjectCancel,
    /// Fetches the machine's project and compares it with what the last send wrote.
    ProjectVerify,
    /// Writes a beat as a Standard MIDI File in the exports folder; an empty `id` means the beat fetched in the card.
    ExportMidi {
        id: String,
    },
    /// Looks at the machine's beat before the library's sounds replace some of its pads (`sounds`: one item id per pad,
    /// `None` to keep the pad's sound).
    KitCheck {
        sounds: Vec<Option<String>>,
    },
    /// Writes the kit after the user confirmed what the check said; `machine` is the hash it saw.
    KitApply {
        sounds: Vec<Option<String>>,
        machine: String,
    },
    /// Sends after the user confirmed what the check said; `machine` is the hash the check saw (`none`: no answer).
    SendApply {
        kind: Kind,
        id: String,
        machine: String,
    },
}

/// How a project goes to the machine (`lib_project_send`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectMode {
    /// Refused when the machine's OS may misread Play Lists (the page must have shown the warning).
    Normal,
    /// The same bytes, after the user read the warning and chose to send anyway.
    Anyway,
    /// The four Play Lists go out empty: the machine's own lists are emptied, none can be misread.
    WithoutLists,
}

struct Line<'a> {
    name: &'a str,
    args: &'a [&'a str],
}

impl Line<'_> {
    fn bad(&self, why: String) -> CommandError {
        CommandError::BadArgument {
            command: self.name.to_string(),
            why,
        }
    }

    fn expect(&self, count: usize) -> Result<(), CommandError> {
        if self.args.len() == count {
            Ok(())
        } else {
            Err(CommandError::Arguments {
                command: self.name.to_string(),
                expected: count,
            })
        }
    }

    fn id(&self, at: usize, allow_empty: bool) -> Result<String, CommandError> {
        let text = self.args[at];
        let plain = text.len() <= MAX_ID_LEN
            && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
        if (text.is_empty() && allow_empty) || (!text.is_empty() && plain) {
            Ok(text.to_string())
        } else {
            Err(self.bad(format!("\"{text}\" is not an item id")))
        }
    }

    fn ids(&self, at: usize) -> Result<Vec<String>, CommandError> {
        let ids: Vec<&str> = self.args[at].split(',').collect();
        if ids.len() > MAX_IDS {
            return Err(self.bad(format!("more than {MAX_IDS} items")));
        }
        let each = |id: &str| {
            let plain = !id.is_empty()
                && id.len() <= MAX_ID_LEN
                && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
            plain
                .then(|| id.to_string())
                .ok_or_else(|| self.bad(format!("\"{id}\" is not an item id")))
        };
        ids.into_iter().map(each).collect()
    }

    fn number(&self, at: usize) -> Result<usize, CommandError> {
        let text = self.args[at];
        text.parse::<usize>()
            .map_err(|_| self.bad(format!("\"{text}\" is not a number")))
    }

    fn kind(&self, at: usize) -> Result<Kind, CommandError> {
        kind_from_name(self.args[at])
            .filter(|kind| *kind != Kind::Projects)
            .ok_or_else(|| self.bad("expected sounds or beats".into()))
    }

    /// An item's version number, or `-` for the current file.
    fn version(&self, at: usize) -> Result<Option<usize>, CommandError> {
        if self.args[at] == "-" {
            Ok(None)
        } else {
            self.number(at).map(Some)
        }
    }

    /// A hash the check saw (64 hex digits), or `none` when the machine did not answer.
    fn machine_hash(&self, at: usize) -> Result<String, CommandError> {
        let hash = self.args[at];
        let plain =
            hash == "none" || (hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()));
        if plain {
            Ok(hash.to_string())
        } else {
            Err(self.bad("expected the hash the check saw, or none".into()))
        }
    }

    /// One item id per pad, `_` where the pad keeps its sound: exactly `PADS` entries.
    fn kit(&self, at: usize) -> Result<Vec<Option<String>>, CommandError> {
        let entries: Vec<&str> = self.args[at].split(',').collect();
        if entries.len() != PADS {
            return Err(self.bad(format!("expected {PADS} entries, got {}", entries.len())));
        }
        entries
            .into_iter()
            .map(|entry| match entry {
                "_" => Ok(None),
                id if !id.is_empty()
                    && id.len() <= MAX_ID_LEN
                    && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') =>
                {
                    Ok(Some(id.to_string()))
                }
                other => Err(self.bad(format!("\"{other}\" is not an item id"))),
            })
            .collect()
    }

    fn word<T: Copy>(&self, at: usize, choices: &[(&str, T)]) -> Result<T, CommandError> {
        let text = self.args[at];
        choices
            .iter()
            .find(|(name, _)| *name == text)
            .map(|(_, value)| *value)
            .ok_or_else(|| {
                let all: Vec<&str> = choices.iter().map(|(n, _)| *n).collect();
                self.bad(format!("\"{text}\" is none of {}", all.join(", ")))
            })
    }
}

/// Parses a `lib_*` line.
pub fn parse(name: &str, args: &[&str]) -> Result<LibCommand, CommandError> {
    let line = Line { name, args };
    match name {
        "lib_import" => {
            line.expect(2)?;
            let data = hex::decode(args[1]).map_err(|e| line.bad(e.to_string()))?;
            Ok(LibCommand::Import {
                name: args[0].to_string(),
                data,
            })
        }
        "lib_apply" => {
            line.expect(3)?;
            Ok(LibCommand::Apply(Policy {
                new: line.word(0, &[("write", OnNew::Write), ("skip", OnNew::Skip)])?,
                identical: line.word(
                    1,
                    &[("skip", OnIdentical::Skip), ("copy", OnIdentical::KeepCopy)],
                )?,
                modified: line.word(
                    2,
                    &[
                        ("update", OnModified::Update),
                        ("new", OnModified::SaveAsNew),
                        ("skip", OnModified::Skip),
                    ],
                )?,
            }))
        }
        "lib_cancel" | "lib_trash" => {
            line.expect(0)?;
            Ok(if name == "lib_cancel" {
                LibCommand::CancelPlan
            } else {
                LibCommand::Trash
            })
        }
        "lib_open" | "lib_duplicate" | "lib_restore_trash" | "lib_history" => {
            line.expect(1)?;
            let id = line.id(0, false)?;
            Ok(match name {
                "lib_open" => LibCommand::Open { id },
                "lib_duplicate" => LibCommand::Duplicate { id },
                "lib_restore_trash" => LibCommand::RestoreTrash { id },
                _ => LibCommand::History { id },
            })
        }
        "lib_open_version" | "lib_compare" | "lib_restore" => {
            line.expect(2)?;
            let (id, version) = (line.id(0, false)?, line.number(1)?);
            Ok(match name {
                "lib_open_version" => LibCommand::OpenVersion { id, version },
                "lib_compare" => LibCommand::Compare { id, version },
                _ => LibCommand::RestoreVersion { id, version },
            })
        }
        "lib_favourite" => {
            line.expect(2)?;
            let on = line.word(1, &[("1", true), ("0", false)])?;
            Ok(LibCommand::Favourite {
                ids: line.ids(0)?,
                on,
            })
        }
        "lib_tag" => {
            line.expect(3)?;
            let add = line.word(1, &[("add", true), ("remove", false)])?;
            Ok(LibCommand::Tag {
                ids: line.ids(0)?,
                add,
                tag: args[2].to_string(),
            })
        }
        "lib_note" => {
            line.expect(2)?;
            Ok(LibCommand::Note {
                id: line.id(0, false)?,
                text: args[1].to_string(),
            })
        }
        "lib_move" => {
            line.expect(2)?;
            Ok(LibCommand::Move {
                ids: line.ids(0)?,
                folder: args[1].to_string(),
            })
        }
        "lib_delete" => {
            line.expect(1)?;
            Ok(LibCommand::Delete { ids: line.ids(0)? })
        }
        "lib_empty_trash" => {
            line.expect(1)?;
            Ok(LibCommand::EmptyTrash {
                expected: line.number(0)?,
            })
        }
        "lib_rename" => {
            line.expect(2)?;
            Ok(LibCommand::Rename {
                id: line.id(0, false)?,
                name: args[1].to_string(),
            })
        }
        "lib_send_check" => {
            line.expect(2)?;
            Ok(LibCommand::SendCheck {
                kind: line.kind(0)?,
                id: line.id(1, true)?,
            })
        }
        "lib_project_check" => {
            line.expect(2)?;
            Ok(LibCommand::ProjectCheck {
                id: line.id(0, false)?,
                version: line.version(1)?,
            })
        }
        "lib_project_send" => {
            line.expect(4)?;
            let mode = line.word(
                3,
                &[
                    ("normal", ProjectMode::Normal),
                    ("anyway", ProjectMode::Anyway),
                    ("no_lists", ProjectMode::WithoutLists),
                ],
            )?;
            Ok(LibCommand::ProjectSend {
                id: line.id(0, false)?,
                version: line.version(1)?,
                machine: line.machine_hash(2)?,
                mode,
            })
        }
        "lib_project_cancel" | "lib_project_verify" => {
            line.expect(0)?;
            Ok(if name == "lib_project_cancel" {
                LibCommand::ProjectCancel
            } else {
                LibCommand::ProjectVerify
            })
        }
        "lib_export_midi" => {
            line.expect(1)?;
            Ok(LibCommand::ExportMidi {
                id: line.id(0, true)?,
            })
        }
        "lib_kit_check" => {
            line.expect(1)?;
            Ok(LibCommand::KitCheck {
                sounds: line.kit(0)?,
            })
        }
        "lib_kit_apply" => {
            line.expect(2)?;
            Ok(LibCommand::KitApply {
                sounds: line.kit(0)?,
                machine: line.machine_hash(1)?,
            })
        }
        "lib_send_apply" => {
            line.expect(3)?;
            Ok(LibCommand::SendApply {
                kind: line.kind(0)?,
                id: line.id(1, true)?,
                machine: line.machine_hash(2)?,
            })
        }
        other => Err(CommandError::Unknown(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(line: &str) -> Result<LibCommand, CommandError> {
        let mut parts = line.split('\t');
        let name = parts.next().unwrap_or_default();
        let args: Vec<&str> = parts.collect();
        parse(name, &args)
    }

    #[test]
    fn test_parse_when_commands_are_well_formed_returns_them() {
        assert_eq!(
            run("lib_open\ts12"),
            Ok(LibCommand::Open { id: "s12".into() })
        );
        assert_eq!(
            run("lib_favourite\ts1,b2\t1"),
            Ok(LibCommand::Favourite {
                ids: vec!["s1".into(), "b2".into()],
                on: true
            })
        );
        assert_eq!(
            run("lib_apply\twrite\tskip\tupdate"),
            Ok(LibCommand::Apply(Policy::default()))
        );
        assert_eq!(
            run("lib_send_check\tsounds\t"),
            Ok(LibCommand::SendCheck {
                kind: Kind::Sounds,
                id: String::new()
            })
        );
        assert_eq!(
            run("lib_empty_trash\t3"),
            Ok(LibCommand::EmptyTrash { expected: 3 })
        );
        assert_eq!(
            run("lib_project_check\tp2\t-"),
            Ok(LibCommand::ProjectCheck {
                id: "p2".into(),
                version: None
            })
        );
        assert_eq!(
            run(&format!(
                "lib_project_send\tmachine-projects\t3\t{}\tno_lists",
                "a".repeat(64)
            )),
            Ok(LibCommand::ProjectSend {
                id: "machine-projects".into(),
                version: Some(3),
                machine: "a".repeat(64),
                mode: ProjectMode::WithoutLists
            })
        );
        assert_eq!(run("lib_project_cancel"), Ok(LibCommand::ProjectCancel));
        assert_eq!(run("lib_project_verify"), Ok(LibCommand::ProjectVerify));
    }

    #[test]
    fn test_parse_when_arguments_are_wrong_returns_an_error() {
        for line in [
            "lib_open",
            "lib_open\t",
            "lib_open\t../x",
            "lib_open\ts1\textra",
            "lib_favourite\ts1\tyes",
            "lib_favourite\ts1,,b\t1",
            "lib_apply\twrite\tskip\tmaybe",
            "lib_send_check\tprojects\ts1",
            "lib_send_apply\tsounds\ts1\tabc",
            "lib_compare\ts1\t-1",
            "lib_import\tx.syx\tzz",
            "lib_project_check\tp1",
            "lib_project_check\t../x\t-",
            "lib_project_check\tp1\tx",
            "lib_project_send\tp1\t-\tabc\tnormal",
            "lib_project_send\tp1\t-\tnone\tforce",
            "lib_project_verify\tp1",
            "lib_nothing",
        ] {
            assert!(run(line).is_err(), "{line}");
        }
    }

    #[test]
    fn test_parse_when_too_many_ids_returns_an_error() {
        let ids = vec!["s1"; MAX_IDS + 1].join(",");

        assert!(run(&format!("lib_delete\t{ids}")).is_err());
    }

    #[test]
    fn test_parse_when_kit_has_32_entries_returns_ids_and_kept_pads() {
        let mut entries = vec!["_"; 32];
        entries[0] = "ab12";
        entries[31] = "c-3";
        let line = format!("lib_kit_check\t{}", entries.join(","));

        let Ok(LibCommand::KitCheck { sounds }) = run(&line) else {
            panic!("not a kit check");
        };

        assert_eq!(sounds.len(), 32);
        assert_eq!(sounds[0].as_deref(), Some("ab12"));
        assert_eq!(sounds[1], None);
        assert_eq!(sounds[31].as_deref(), Some("c-3"));
    }

    #[test]
    fn test_parse_when_kit_is_malformed_returns_an_error() {
        let short = vec!["_"; 31].join(",");
        let long = vec!["_"; 33].join(",");
        let mut bad = vec!["_"; 32];
        bad[3] = "../x";
        let mut empty = vec!["_"; 32];
        empty[4] = "";
        for entries in [short, long, bad.join(","), empty.join(",")] {
            assert!(
                run(&format!("lib_kit_check\t{entries}")).is_err(),
                "{entries}"
            );
        }
        let all = vec!["_"; 32].join(",");
        assert!(run(&format!("lib_kit_apply\t{all}\tnot-a-hash")).is_err());
        assert!(run(&format!("lib_kit_apply\t{all}\t{}", "a".repeat(64))).is_ok());
    }

    #[test]
    fn test_parse_when_export_midi_returns_the_item_or_the_current_beat() {
        assert_eq!(
            run("lib_export_midi\tab12"),
            Ok(LibCommand::ExportMidi { id: "ab12".into() })
        );
        assert_eq!(
            run("lib_export_midi\t"),
            Ok(LibCommand::ExportMidi { id: String::new() })
        );
        assert!(run("lib_export_midi\t../x").is_err());
    }
}
