//! The list and open commands of the Library page: `library <kind>` publishes the items of a folder (the manager
//! in `manager.rs` builds the event), `open_library <kind> <name>` opens a saved sound or beat as if it came from the
//! file picker. A project is only listed: sending a whole project back waits for a verified fix of the machine's
//! `5E` receiver (`refs/firmware-main.md` 9.6).

use super::Machine;
use crate::command::{Command, Kind};
use crate::error::AppError;
use crate::library::folder_of;

impl Machine {
    pub(super) fn handle_library(&mut self, command: Command) -> Option<Command> {
        match command {
            Command::Library(kind) => self.guard("library", |m| m.publish_library(kind)),
            Command::OpenLibrary { kind, name } => {
                self.guard("open_library", |m| m.open_library(kind, &name));
            }
            other => return Some(other),
        }
        None
    }

    fn open_library(&mut self, kind: Kind, name: &str) -> Result<(), AppError> {
        let open: fn(String, Vec<u8>) -> Command = match kind {
            Kind::Sounds => |name, data| Command::OpenSound { name, data },
            Kind::Beats => |name, data| Command::OpenBeat { name, data },
            Kind::Projects => return Err(AppError::Precondition(
                "a project cannot be sent back yet: the machine's project receiver is not verified",
            )),
        };
        let data = self.storage.read(folder_of(kind), name)?;
        self.handle(open(name.to_string(), data));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::Json;
    use crate::library::model::kind_name;
    use crate::machine::testkit::TestRig;

    #[test]
    fn test_library_when_two_files_saved_returns_their_names_and_sizes() {
        let mut rig = TestRig::connected("lib-list");
        rig.machine.handle(Command::FetchSound);
        rig.machine.handle(Command::SaveSound("B".into()));
        rig.machine.handle(Command::SaveSound("A".into()));
        rig.events();

        rig.machine.handle(Command::Library(Kind::Sounds));

        let files = rig.expect("library").field("files").cloned();
        match files {
            Some(Json::Arr(list)) => {
                assert_eq!(list.len(), 1, "the second save is the same sound: skipped")
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn test_library_when_empty_returns_an_empty_list() {
        let mut rig = TestRig::new("lib-empty");

        rig.machine.handle(Command::Library(Kind::Beats));

        assert_eq!(
            rig.expect("library").field("files"),
            Some(&Json::Arr(vec![]))
        );
    }

    #[test]
    fn test_open_library_when_saved_sound_returns_sound_opened_event() {
        let mut rig = TestRig::connected("lib-open");
        rig.machine.handle(Command::FetchSound);
        rig.machine.handle(Command::SaveSound("Kick".into()));
        rig.events();

        rig.machine.handle(Command::OpenLibrary {
            kind: Kind::Sounds,
            name: "Kick.syx".into(),
        });

        assert_eq!(
            rig.expect("sound_opened").field("file"),
            Some(&"Kick.syx".into())
        );
    }

    #[test]
    fn test_open_library_when_name_climbs_out_returns_error_and_opens_nothing() {
        let mut rig = TestRig::new("lib-escape");

        rig.machine.handle(Command::OpenLibrary {
            kind: Kind::Sounds,
            name: "../settings.txt".into(),
        });

        assert_eq!(rig.expect("error").field("code"), Some(&"invalid".into()));
        assert!(rig.machine.opened_sound.is_none());
    }

    /// The `library` event of `kind`: one object per item.
    fn listed(rig: &mut TestRig, kind: Kind) -> Vec<Vec<(&'static str, Json)>> {
        rig.machine.handle(Command::Library(kind));
        let event = rig.expect("library");
        assert_eq!(event.field("kind"), Some(&kind_name(kind).into()));
        let Some(Json::Arr(files)) = event.field("files").cloned() else {
            panic!("no file list");
        };
        files
            .into_iter()
            .map(|file| match file {
                Json::Obj(fields) => fields,
                other => panic!("not an object: {other:?}"),
            })
            .collect()
    }

    fn field<'a>(item: &'a [(&'static str, Json)], key: &str) -> &'a Json {
        &item.iter().find(|(k, _)| *k == key).expect("field").1
    }

    #[test]
    fn test_library_when_a_project_was_saved_returns_one_item_holding_the_capture_and_no_sound_or_beat_in_it(
    ) {
        let mut rig = crate::machine::project::support::rig_with_everything("lib-projects");

        let projects = listed(&mut rig, Kind::Projects);
        let sounds = listed(&mut rig, Kind::Sounds);
        let beats = listed(&mut rig, Kind::Beats);

        assert_eq!(projects.len(), 1);
        assert!(
            matches!(field(&projects[0], "capture"), Json::Str(c) if c.ends_with(" (as received).syx"))
        );
        assert_eq!(field(&projects[0], "beats"), &Json::Int(16));
        assert!(sounds
            .iter()
            .any(|s| field(s, "display") == &Json::from("Fake Kick")));
        assert!(beats
            .iter()
            .any(|b| field(b, "display") == &Json::from("Groove")
                || field(b, "name") == &Json::from("Groove.syx")));
        assert!(sounds
            .iter()
            .chain(&beats)
            .all(|i| field(i, "capture") == &Json::Null));
    }

    #[test]
    fn test_open_library_when_a_project_is_asked_returns_error_and_opens_nothing() {
        let mut rig = TestRig::new("lib-open-project");

        rig.machine.handle(Command::OpenLibrary {
            kind: Kind::Projects,
            name: "Project x.syx".into(),
        });

        let error = rig.expect("error");
        assert_eq!(error.field("code"), Some(&"precondition".into()));
        assert!(rig.machine.opened_beat.is_none() && rig.machine.opened_sound.is_none());
    }
}
