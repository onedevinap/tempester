//! Library tests: all on a disposable data folder (`temp_storage`), each guarantee with its counter-proof.

use std::fs;

use tempest_proto::beat;
use tempest_proto::sha256::sha256_hex;
use tempest_proto::sound::Sound;
use tempest_proto::sound_message;

use super::inspect::inspect;
use super::model::Index;
use super::plan::{evaluate, Candidate, OnIdentical, OnModified, OnNew, Policy, Verdict};
use super::{origin, Library};
use crate::command::Kind;
use crate::storage::{temp_storage, Folder, Storage};

fn rig(label: &str) -> (Storage, Library) {
    let storage = temp_storage(label);
    let library = Library::new(&storage);
    (storage, library)
}

fn sound_file(name: &str, freq: u16) -> Vec<u8> {
    let mut sound = Sound::with_defaults();
    sound.set(0, freq).unwrap();
    sound.set_name(name).unwrap();
    sound_message::encode_file(name, &sound).unwrap()
}

fn beat_file(name: &str) -> Vec<u8> {
    let sounds = std::array::from_fn(|_| Sound::with_defaults());
    beat::standard_dump(&beat::synthetic_beat_data(name, &[(0, 1)], 3, &sounds))
}

fn candidate(kind: Kind, stem: &str, bytes: &[u8], date: i64) -> Candidate {
    let facts = inspect(kind, bytes, stem).unwrap();
    Candidate {
        origin: origin::file("", stem),
        stem: stem.into(),
        folder: String::new(),
        source: "machine".into(),
        date,
        fetched: true,
        capture: None,
        label: stem.into(),
        facts,
    }
}

fn names(storage: &Storage, folder: Folder) -> Vec<String> {
    storage
        .list(folder)
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect()
}

fn save(library: &Library, c: &Candidate) {
    library
        .apply(std::slice::from_ref(c), Policy::default())
        .unwrap();
}

// ---- index: build, rebuild, merge, atomic write -------------------------------------------------------------

#[test]
fn test_list_when_files_exist_and_no_index_returns_every_file_as_an_item_with_its_hash() {
    let (storage, library) = rig("idx-build");
    storage
        .save(Folder::Sounds, "Kick", "syx", &sound_file("Kick", 40))
        .unwrap();
    storage
        .save(Folder::Beats, "Groove", "syx", &beat_file("Groove"))
        .unwrap();

    let (sounds, report) = library.list(Kind::Sounds).unwrap();
    let (beats, _) = library.list(Kind::Beats).unwrap();

    assert_eq!(sounds.len(), 1);
    assert_eq!(beats.len(), 1);
    assert_eq!(sounds[0].name, "Kick");
    assert_eq!(
        sounds[0].hash,
        inspect(Kind::Sounds, &sound_file("Kick", 40), "")
            .unwrap()
            .hash
    );
    assert!(
        report.is_empty(),
        "the rebuild itself already added them: {report:?}"
    );
    assert!(storage.root().join("library.json").is_file());
}

#[test]
fn test_list_when_a_file_is_added_and_another_removed_returns_the_orphan_added_and_the_missing_dropped_and_reported(
) {
    let (storage, library) = rig("idx-reconcile");
    let kick = storage
        .save(Folder::Sounds, "Kick", "syx", &sound_file("Kick", 40))
        .unwrap();
    library.list(Kind::Sounds).unwrap();
    fs::remove_file(kick).unwrap();
    storage
        .save(Folder::Sounds, "Snare", "syx", &sound_file("Snare", 50))
        .unwrap();
    fs::write(
        storage.folder(Folder::Sounds).unwrap().join("junk.syx"),
        b"not tempest",
    )
    .unwrap();

    let (items, report) = library.list(Kind::Sounds).unwrap();

    assert_eq!(
        items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
        ["Snare"]
    );
    assert_eq!(report.added.len(), 1, "{report:?}");
    assert_eq!(report.dropped.len(), 1, "{report:?}");
    assert!(report.dropped[0].contains("Kick"));
    assert_eq!(report.unreadable.len(), 1, "{report:?}");
    assert!(
        storage
            .folder(Folder::Sounds)
            .unwrap()
            .join("junk.syx")
            .exists(),
        "an unreadable file is left alone"
    );
}

#[test]
fn test_list_when_the_index_is_lost_returns_the_items_rebuilt_from_the_files() {
    let (storage, library) = rig("idx-lost");
    storage
        .save(Folder::Sounds, "Kick", "syx", &sound_file("Kick", 40))
        .unwrap();
    storage
        .save(Folder::Sounds, "Snare", "syx", &sound_file("Snare", 50))
        .unwrap();
    library.list(Kind::Sounds).unwrap();
    fs::remove_file(storage.root().join("library.json")).unwrap();

    let (items, _) = library.list(Kind::Sounds).unwrap();

    assert_eq!(items.len(), 2);
}

#[test]
fn test_list_when_the_index_is_damaged_returns_a_rebuilt_index_and_keeps_the_damaged_file_aside() {
    let (storage, library) = rig("idx-damaged");
    storage
        .save(Folder::Sounds, "Kick", "syx", &sound_file("Kick", 40))
        .unwrap();
    library.list(Kind::Sounds).unwrap();
    fs::write(
        storage.root().join("library.json"),
        b"{\"version\":1,\"items\":[",
    )
    .unwrap();

    let aside = library.set_aside_if_damaged().unwrap();
    let (items, _) = library.list(Kind::Sounds).unwrap();

    let aside = aside.expect("the damaged file is set aside");
    assert_eq!(fs::read(aside).unwrap(), b"{\"version\":1,\"items\":[");
    assert_eq!(items.len(), 1);
    assert!(
        Index::parse(&fs::read_to_string(storage.root().join("library.json")).unwrap()).is_ok()
    );
}

#[test]
fn test_update_when_another_writer_changed_the_index_meanwhile_returns_both_changes() {
    let (storage, library) = rig("idx-merge");
    storage
        .save(Folder::Sounds, "Kick", "syx", &sound_file("Kick", 40))
        .unwrap();
    let (items, _) = library.list(Kind::Sounds).unwrap();
    let id = items[0].id.clone();
    let other = library.clone();
    let mut first = true;

    library
        .update(|index| {
            if first {
                first = false;
                // The concurrent writer lands between our read and our write.
                other
                    .set_tag(std::slice::from_ref(&id), true, "from-other-writer")
                    .unwrap();
            }
            index.item_mut(&id).unwrap().note = "mine".into();
        })
        .unwrap();

    let item = library.item(&id).unwrap();
    assert_eq!(
        item.tags,
        ["from-other-writer"],
        "the concurrent change survives"
    );
    assert_eq!(item.note, "mine", "and so does ours");
}

#[test]
fn test_update_when_the_check_is_switched_off_returns_the_concurrent_change_lost() {
    // Counter-proof of the merge test: without the re-read check, stale memory overwrites the other writer.
    let (storage, library) = rig("idx-merge-counter");
    storage
        .save(Folder::Sounds, "Kick", "syx", &sound_file("Kick", 40))
        .unwrap();
    let (items, _) = library.list(Kind::Sounds).unwrap();
    let id = items[0].id.clone();
    let other = library.clone();
    let mut first = true;

    library
        .update_unchecked(|index| {
            if first {
                first = false;
                other
                    .set_tag(std::slice::from_ref(&id), true, "from-other-writer")
                    .unwrap();
            }
            index.item_mut(&id).unwrap().note = "mine".into();
        })
        .unwrap();

    assert!(
        library.item(&id).unwrap().tags.is_empty(),
        "the stale write erased it"
    );
}

// A read-only folder is how the test makes the write fail: Unix permissions. Windows has no equivalent that refuses a
// file creation in a folder (its read-only attribute does not), so the guarantee is proved on the macOS and Linux runs.
#[cfg(unix)]
#[test]
fn test_update_when_the_write_fails_returns_an_error_and_the_old_index_untouched_without_leftovers()
{
    use std::os::unix::fs::PermissionsExt;
    let (storage, library) = rig("idx-atomic");
    storage
        .save(Folder::Sounds, "Kick", "syx", &sound_file("Kick", 40))
        .unwrap();
    let (items, _) = library.list(Kind::Sounds).unwrap();
    let before = fs::read(storage.root().join("library.json")).unwrap();
    let id = items[0].id.clone();
    fs::set_permissions(storage.root(), fs::Permissions::from_mode(0o500)).unwrap();

    let result = library.set_favourite(&[id], true);

    fs::set_permissions(storage.root(), fs::Permissions::from_mode(0o700)).unwrap();
    assert!(result.is_err());
    assert_eq!(
        fs::read(storage.root().join("library.json")).unwrap(),
        before
    );
    let leftovers: Vec<_> = fs::read_dir(storage.root())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn test_update_when_it_succeeds_returns_no_temporary_file_and_a_parsable_index() {
    let (storage, library) = rig("idx-notmp");
    storage
        .save(Folder::Sounds, "Kick", "syx", &sound_file("Kick", 40))
        .unwrap();
    library.list(Kind::Sounds).unwrap();

    let names: Vec<String> = fs::read_dir(storage.root())
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();

    assert!(names.iter().all(|n| !n.contains(".tmp-")), "{names:?}");
}

// ---- duplicates and versions --------------------------------------------------------------------------------

#[test]
fn test_apply_when_the_same_sound_is_saved_twice_returns_one_file_and_says_it_is_already_there() {
    let (storage, library) = rig("dup-skip");
    let c = candidate(Kind::Sounds, "Kick", &sound_file("Kick", 40), 100);
    save(&library, &c);
    let again = Candidate {
        stem: "Another name".into(),
        origin: origin::file("", "Another name"),
        ..c
    };

    let outcome = library.apply(&[again], Policy::default()).unwrap();

    assert_eq!(names(&storage, Folder::Sounds), ["Kick.syx"]);
    assert_eq!((outcome.written, outcome.skipped), (0, 1));
    assert!(
        outcome.lines[0].contains("already in the library as \"Kick\""),
        "{:?}",
        outcome.lines
    );
}

#[test]
fn test_apply_when_a_second_copy_is_asked_returns_two_files_so_the_default_is_what_prevents_them() {
    // Counter-proof of the duplicate guard: the same call with KeepCopy does write two.
    let (storage, library) = rig("dup-copy");
    let c = candidate(Kind::Sounds, "Kick", &sound_file("Kick", 40), 100);
    save(&library, &c);
    let policy = Policy {
        identical: OnIdentical::KeepCopy,
        ..Policy::default()
    };

    library.apply(&[c], policy).unwrap();

    assert_eq!(
        names(&storage, Folder::Sounds),
        ["Kick (2).syx", "Kick.syx"]
    );
}

#[test]
fn test_evaluate_when_same_sound_is_in_another_layout_returns_identical_and_the_raw_bytes_differ() {
    let (_, library) = rig("dup-layout");
    let mut sound = Sound::with_defaults();
    sound.set_name("Kick").unwrap();
    let as_63 = sound_message::encode_file("Kick", &sound).unwrap();
    let as_60 = sound_message::encode_current(&sound);
    save(&library, &candidate(Kind::Sounds, "Kick", &as_63, 1));
    let index = library.snapshot().unwrap();

    let verdicts = evaluate(&index, &[candidate(Kind::Sounds, "Other", &as_60, 2)]);

    assert!(matches!(verdicts[0], Verdict::Identical(_)), "{verdicts:?}");
    assert_ne!(
        sha256_hex(&as_63),
        sha256_hex(&as_60),
        "a hash of the file bytes would not have matched"
    );
}

#[test]
fn test_evaluate_when_beat_comes_in_the_machines_layout_returns_identical_to_the_standard_one() {
    let (_, library) = rig("dup-beat-layout");
    let data = beat::synthetic_tail_beat(3);
    let standard = beat::standard_dump(&data);
    let machine = beat::machine_dump(&data);
    save(&library, &candidate(Kind::Beats, "B", &standard, 1));
    let index = library.snapshot().unwrap();

    let verdicts = evaluate(&index, &[candidate(Kind::Beats, "B2", &machine, 2)]);

    assert!(matches!(verdicts[0], Verdict::Identical(_)), "{verdicts:?}");
    assert_ne!(standard, machine);
}

#[test]
fn test_evaluate_when_same_name_has_other_content_returns_modified_and_other_name_returns_new() {
    let (_, library) = rig("ver-verdicts");
    save(
        &library,
        &candidate(Kind::Sounds, "Kick", &sound_file("Kick", 40), 100),
    );
    let index = library.snapshot().unwrap();

    let modified = evaluate(
        &index,
        &[candidate(
            Kind::Sounds,
            "kick",
            &sound_file("Kick", 41),
            200,
        )],
    );
    let new = evaluate(
        &index,
        &[candidate(
            Kind::Sounds,
            "Snare",
            &sound_file("Snare", 41),
            200,
        )],
    );

    assert!(matches!(modified[0], Verdict::Modified(_)), "{modified:?}");
    assert_eq!(new[0], Verdict::New);
}

#[test]
fn test_apply_when_a_modified_version_is_updated_returns_the_old_one_in_history_and_the_new_one_in_place(
) {
    let (storage, library) = rig("ver-update");
    let old = sound_file("Kick", 40);
    let new = sound_file("Kick", 41);
    save(&library, &candidate(Kind::Sounds, "Kick", &old, 100));

    let outcome = library
        .apply(
            &[candidate(Kind::Sounds, "Kick", &new, 200)],
            Policy::default(),
        )
        .unwrap();

    assert_eq!(outcome.updated, 1);
    assert_eq!(
        names(&storage, Folder::Sounds),
        ["Kick.syx"],
        "updated in place, no second file"
    );
    assert_eq!(storage.read(Folder::Sounds, "Kick.syx").unwrap(), new);
    let item = &library.list(Kind::Sounds).unwrap().0[0];
    assert_eq!(item.history.len(), 1);
    assert_eq!(
        item.history[0].hash,
        inspect(Kind::Sounds, &old, "").unwrap().hash
    );
    assert_eq!(item.history[0].date, 100);
    let kept = fs::read(storage.root().join(&item.history[0].file)).unwrap();
    assert_eq!(
        kept, old,
        "the old bytes are in history/<kind>/<id>/<date>.syx"
    );
    assert!(item.history[0]
        .file
        .starts_with(&format!("history/sounds/{}/", item.id)));
    assert_eq!(item.modified, 200);
    assert!(outcome.lines[0].contains("newer"), "{:?}", outcome.lines);
}

#[test]
fn test_apply_when_the_replacing_version_is_older_returns_a_warning_in_the_message() {
    let (_, library) = rig("ver-older");
    save(
        &library,
        &candidate(Kind::Sounds, "Kick", &sound_file("Kick", 40), 500),
    );

    let outcome = library
        .apply(
            &[candidate(
                Kind::Sounds,
                "Kick",
                &sound_file("Kick", 41),
                100,
            )],
            Policy::default(),
        )
        .unwrap();

    assert!(outcome.lines[0].contains("OLDER"), "{:?}", outcome.lines);
    assert!(outcome.lines[0].contains("WARNING"), "{:?}", outcome.lines);
}

#[test]
fn test_apply_when_a_modified_version_is_skipped_or_saved_as_new_returns_the_choice_obeyed() {
    let (storage, library) = rig("ver-choices");
    let old = sound_file("Kick", 40);
    save(&library, &candidate(Kind::Sounds, "Kick", &old, 100));
    let changed = candidate(Kind::Sounds, "Kick", &sound_file("Kick", 41), 200);

    library
        .apply(
            std::slice::from_ref(&changed),
            Policy {
                modified: OnModified::Skip,
                ..Policy::default()
            },
        )
        .unwrap();
    assert_eq!(
        storage.read(Folder::Sounds, "Kick.syx").unwrap(),
        old,
        "skip leaves the file as it is"
    );
    assert_eq!(names(&storage, Folder::Sounds).len(), 1);

    library
        .apply(
            &[changed],
            Policy {
                modified: OnModified::SaveAsNew,
                ..Policy::default()
            },
        )
        .unwrap();
    assert_eq!(
        names(&storage, Folder::Sounds),
        ["Kick (2).syx", "Kick.syx"]
    );
    assert_eq!(
        storage.read(Folder::Sounds, "Kick.syx").unwrap(),
        old,
        "the original is untouched"
    );
}

#[test]
fn test_apply_when_new_items_are_skipped_by_choice_returns_nothing_written() {
    let (storage, library) = rig("ver-skip-new");

    let outcome = library
        .apply(
            &[candidate(Kind::Sounds, "Kick", &sound_file("Kick", 40), 1)],
            Policy {
                new: OnNew::Skip,
                ..Policy::default()
            },
        )
        .unwrap();

    assert!(names(&storage, Folder::Sounds).is_empty());
    assert_eq!(outcome.skipped, 1);
}

#[test]
fn test_apply_when_two_candidates_of_a_batch_are_identical_returns_one_file() {
    let (storage, library) = rig("dup-batch");
    let bytes = sound_file("Same", 40);
    let a = candidate(Kind::Sounds, "A", &bytes, 1);
    let b = candidate(Kind::Sounds, "B", &bytes, 1);

    library.apply(&[a, b], Policy::default()).unwrap();

    assert_eq!(names(&storage, Folder::Sounds), ["A.syx"]);
}

#[test]
fn test_restore_version_when_called_returns_the_old_bytes_in_place_and_keeps_the_replaced_one() {
    let (storage, library) = rig("ver-restore");
    let old = sound_file("Kick", 40);
    let new = sound_file("Kick", 41);
    save(&library, &candidate(Kind::Sounds, "Kick", &old, 100));
    save(&library, &candidate(Kind::Sounds, "Kick", &new, 200));
    let id = library.list(Kind::Sounds).unwrap().0[0].id.clone();

    let lines = library.restore_version(&id, 0).unwrap();

    assert_eq!(storage.read(Folder::Sounds, "Kick.syx").unwrap(), old);
    let item = library.item(&id).unwrap();
    assert_eq!(
        item.history.len(),
        2,
        "the version the restore replaced is kept too"
    );
    assert_eq!(item.hash, inspect(Kind::Sounds, &old, "").unwrap().hash);
    assert_eq!(
        fs::read(storage.root().join(&item.history[1].file)).unwrap(),
        new
    );
    assert!(lines[0].contains("To undo"), "{lines:?}");
}

#[test]
fn test_restore_version_when_the_version_does_not_exist_returns_an_error_and_changes_nothing() {
    let (storage, library) = rig("ver-restore-bad");
    save(
        &library,
        &candidate(Kind::Sounds, "Kick", &sound_file("Kick", 40), 100),
    );
    let id = library.list(Kind::Sounds).unwrap().0[0].id.clone();

    assert!(library.restore_version(&id, 3).is_err());
    assert_eq!(
        storage.read(Folder::Sounds, "Kick.syx").unwrap(),
        sound_file("Kick", 40)
    );
}

// ---- trash ------------------------------------------------------------------------------------------------------

#[test]
fn test_delete_to_trash_when_called_returns_the_file_in_the_trash_and_restore_puts_it_back() {
    let (storage, library) = rig("trash-roundtrip");
    let bytes = sound_file("Kick", 40);
    save(&library, &candidate(Kind::Sounds, "Kick", &bytes, 100));
    let id = library.list(Kind::Sounds).unwrap().0[0].id.clone();

    library.delete_to_trash(std::slice::from_ref(&id)).unwrap();

    assert!(names(&storage, Folder::Sounds).is_empty());
    assert_eq!(
        fs::read(storage.root().join("trash").join(&id).join("Kick.syx")).unwrap(),
        bytes
    );
    assert!(
        library.list(Kind::Sounds).unwrap().0.is_empty(),
        "a scan does not bring it back"
    );
    assert_eq!(library.trash_list().unwrap().len(), 1);

    library.restore_from_trash(&id).unwrap();

    assert_eq!(storage.read(Folder::Sounds, "Kick.syx").unwrap(), bytes);
    assert!(library.trash_list().unwrap().is_empty());
    assert_eq!(library.item(&id).unwrap().name, "Kick");
}

#[test]
fn test_restore_from_trash_when_the_name_was_taken_returns_another_name_and_overwrites_nothing() {
    let (storage, library) = rig("trash-taken");
    save(
        &library,
        &candidate(Kind::Sounds, "Kick", &sound_file("Kick", 40), 100),
    );
    let id = library.list(Kind::Sounds).unwrap().0[0].id.clone();
    library.delete_to_trash(std::slice::from_ref(&id)).unwrap();
    storage
        .save(Folder::Sounds, "Kick", "syx", &sound_file("Kick", 99))
        .unwrap();

    library.restore_from_trash(&id).unwrap();

    assert_eq!(
        storage.read(Folder::Sounds, "Kick.syx").unwrap(),
        sound_file("Kick", 99)
    );
    assert_eq!(
        storage.read(Folder::Sounds, "Kick (2).syx").unwrap(),
        sound_file("Kick", 40)
    );
}

#[test]
fn test_empty_trash_when_the_count_is_wrong_returns_an_error_and_deletes_nothing() {
    let (storage, library) = rig("trash-empty");
    save(
        &library,
        &candidate(Kind::Sounds, "Kick", &sound_file("Kick", 40), 100),
    );
    let id = library.list(Kind::Sounds).unwrap().0[0].id.clone();
    library.delete_to_trash(std::slice::from_ref(&id)).unwrap();
    let trashed = storage.root().join("trash").join(&id).join("Kick.syx");

    assert!(library.empty_trash(0).is_err());
    assert!(library.empty_trash(2).is_err());
    assert!(
        trashed.exists(),
        "nothing deleted without the confirmed count"
    );

    library.empty_trash(1).unwrap();

    assert!(!trashed.exists());
    assert!(library.trash_list().unwrap().is_empty());
}

#[test]
fn test_empty_trash_when_an_item_had_history_returns_its_history_deleted_with_it() {
    let (storage, library) = rig("trash-history");
    save(
        &library,
        &candidate(Kind::Sounds, "Kick", &sound_file("Kick", 40), 100),
    );
    save(
        &library,
        &candidate(Kind::Sounds, "Kick", &sound_file("Kick", 41), 200),
    );
    let id = library.list(Kind::Sounds).unwrap().0[0].id.clone();
    let history = storage.root().join("history/sounds").join(&id);
    assert!(history.is_dir());
    library.delete_to_trash(std::slice::from_ref(&id)).unwrap();
    assert!(history.is_dir(), "deleting to the trash keeps the history");

    library.empty_trash(1).unwrap();

    assert!(!history.exists());
}

// ---- rename -------------------------------------------------------------------------------------------------------

/// The 20 characters of 7 bits from byte 110 of a sound, read here without the proto crate's own reader.
fn raw_sound_name(sound: &[u8; 128]) -> String {
    (0..20)
        .map(|k| {
            let bit = 110 * 8 + 7 * k;
            let value = (0..7).fold(0u8, |v, j| {
                v | ((sound[(bit + j) / 8] >> ((bit + j) % 8)) & 1) << j
            });
            char::from(value)
        })
        .collect()
}

#[test]
fn test_rename_when_a_sound_returns_the_20_character_field_the_file_and_a_history_entry() {
    let (storage, library) = rig("rename-sound");
    let old = sound_file("Kick", 40);
    save(&library, &candidate(Kind::Sounds, "Kick", &old, 100));
    let id = library.list(Kind::Sounds).unwrap().0[0].id.clone();

    let lines = library.rename(&id, "Big Kick").unwrap();

    assert_eq!(names(&storage, Folder::Sounds), ["Big Kick.syx"]);
    let bytes = storage.read(Folder::Sounds, "Big Kick.syx").unwrap();
    let sound_message::SoundMessage::File { name, sound } = sound_message::decode(&bytes).unwrap()
    else {
        panic!("not a 63 file");
    };
    assert_eq!(name, "Big Kick", "the 63 file name follows");
    assert_eq!(
        raw_sound_name(sound.bytes()),
        "Big Kick            ",
        "20 characters, space padded, from byte 110"
    );
    assert_eq!(sound.get(0), Ok(40), "nothing else changed");
    let item = library.item(&id).unwrap();
    assert_eq!(
        (item.name.as_str(), item.path.as_str()),
        ("Big Kick", "Big Kick.syx")
    );
    assert_eq!(item.history.len(), 1);
    assert_eq!(
        fs::read(storage.root().join(&item.history[0].file)).unwrap(),
        old
    );
    assert!(lines[0].contains("To undo"), "{lines:?}");
}

#[test]
fn test_rename_when_a_beat_returns_the_name_in_bytes_24_to_43_of_the_data() {
    let (storage, library) = rig("rename-beat");
    save(
        &library,
        &candidate(Kind::Beats, "Groove", &beat_file("Groove"), 100),
    );
    let id = library.list(Kind::Beats).unwrap().0[0].id.clone();

    library.rename(&id, "Funky Groove").unwrap();

    let bytes = storage.read(Folder::Beats, "Funky Groove.syx").unwrap();
    let data = beat::read_body(&bytes[4..bytes.len() - 1]).unwrap();
    assert_eq!(&data[24..44], b"Funky Groove        ");
    assert_eq!(beat::summarize(&bytes).unwrap().name, "Funky Groove");
    assert_eq!(
        beat::summarize(&bytes).unwrap().events,
        3,
        "the rest of the beat is unchanged"
    );
    assert_eq!(library.item(&id).unwrap().history.len(), 1);
}

#[test]
fn test_rename_when_the_name_is_invalid_returns_the_reason_and_changes_nothing() {
    let (storage, library) = rig("rename-invalid");
    let bytes = sound_file("Kick", 40);
    save(&library, &candidate(Kind::Sounds, "Kick", &bytes, 100));
    let id = library.list(Kind::Sounds).unwrap().0[0].id.clone();
    let before = fs::read(storage.root().join("library.json")).unwrap();

    for (name, why) in [
        ("", "empty"),
        ("   ", "empty"),
        ("A very long name indeed!", "24 characters"),
        ("Caf\u{e9}", "not allowed"),
        ("tab\there", "not allowed"),
        (" lead", "space"),
        ("trail ", "space"),
        ("Kick", "already its name"),
    ] {
        let error = library.rename(&id, name).unwrap_err().to_string();
        assert!(error.contains(why), "{name:?}: {error}");
    }

    assert_eq!(names(&storage, Folder::Sounds), ["Kick.syx"]);
    assert_eq!(storage.read(Folder::Sounds, "Kick.syx").unwrap(), bytes);
    assert_eq!(
        fs::read(storage.root().join("library.json")).unwrap(),
        before
    );
}

#[test]
fn test_rename_when_a_file_has_that_name_returns_an_error_and_both_files_untouched() {
    let (storage, library) = rig("rename-taken");
    save(
        &library,
        &candidate(Kind::Sounds, "Kick", &sound_file("Kick", 40), 100),
    );
    save(
        &library,
        &candidate(Kind::Sounds, "Snare", &sound_file("Snare", 50), 100),
    );
    let id = library
        .list(Kind::Sounds)
        .unwrap()
        .0
        .iter()
        .find(|i| i.name == "Kick")
        .unwrap()
        .id
        .clone();

    let error = library.rename(&id, "Snare").unwrap_err().to_string();

    assert!(error.contains("already exists"), "{error}");
    assert_eq!(
        storage.read(Folder::Sounds, "Snare.syx").unwrap(),
        sound_file("Snare", 50)
    );
    assert_eq!(
        storage.read(Folder::Sounds, "Kick.syx").unwrap(),
        sound_file("Kick", 40)
    );
}

#[test]
fn test_rename_when_a_sound_came_from_a_sound_type_folder_returns_the_new_name_under_the_same_folder(
) {
    let (storage, library) = rig("rename-type");
    let mut sound = Sound::with_defaults();
    sound.set_name("Zap").unwrap();
    let file = sound_message::encode_file("/S/Kicks/Zap", &sound).unwrap();
    storage.save(Folder::Sounds, "Zap", "syx", &file).unwrap();
    let item = library.list(Kind::Sounds).unwrap().0.remove(0);
    assert_eq!(item.facets.sound_type, "Kicks");

    library.rename(&item.id, "Zip").unwrap();

    let bytes = storage.read(Folder::Sounds, "Zip.syx").unwrap();
    let sound_message::SoundMessage::File { name, .. } = sound_message::decode(&bytes).unwrap()
    else {
        panic!("not a 63 file");
    };
    assert_eq!(name, "/S/Kicks/Zip");
    assert_eq!(library.item(&item.id).unwrap().facets.sound_type, "Kicks");
}

// ---- organising -------------------------------------------------------------------------------------------------

#[test]
fn test_set_tag_and_favourite_and_note_when_applied_returns_them_on_every_selected_item_and_survive_a_scan(
) {
    let (storage, library) = rig("meta");
    storage
        .save(Folder::Sounds, "A", "syx", &sound_file("A", 1))
        .unwrap();
    storage
        .save(Folder::Sounds, "B", "syx", &sound_file("B", 2))
        .unwrap();
    let ids: Vec<String> = library
        .list(Kind::Sounds)
        .unwrap()
        .0
        .iter()
        .map(|i| i.id.clone())
        .collect();

    library.set_tag(&ids, true, "drums").unwrap();
    library.set_favourite(&ids[..1], true).unwrap();
    library.set_note(&ids[1], "hello\nworld").unwrap();
    library.set_tag(&ids[..1], false, "DRUMS").unwrap();
    let items = library.list(Kind::Sounds).unwrap().0;

    assert!(items[0].tags.is_empty() && items[0].favourite);
    assert_eq!(items[1].tags, ["drums"]);
    assert_eq!(items[1].note, "hello\nworld");
    assert!(library.set_tag(&ids, true, "a,b").is_err());
    assert!(library.set_tag(&ids, true, "  ").is_err());
    assert!(library.set_tag(&["nope".into()], true, "x").is_err());
}

#[test]
fn test_move_items_when_target_name_is_taken_returns_an_error_and_moves_nothing() {
    let (storage, library) = rig("move");
    storage
        .save(Folder::Sounds, "A", "syx", &sound_file("A", 1))
        .unwrap();
    storage
        .save(Folder::Sounds, "B", "syx", &sound_file("B", 2))
        .unwrap();
    let dir = storage.new_subfolder(Folder::Sounds, "Pack").unwrap();
    storage
        .save_in(&dir, "B", "syx", &sound_file("B", 9))
        .unwrap();
    let items = library.list(Kind::Sounds).unwrap().0;
    let top: Vec<String> = items
        .iter()
        .filter(|i| !i.path.contains('/'))
        .map(|i| i.id.clone())
        .collect();

    let error = library.move_items(&top, "Pack").unwrap_err().to_string();

    assert!(error.contains("nothing was moved"), "{error}");
    assert_eq!(
        names(&storage, Folder::Sounds),
        ["A.syx", "B.syx", "Pack/B.syx"]
    );
}

#[test]
fn test_move_items_when_free_returns_the_file_and_the_path_moved_and_back() {
    let (storage, library) = rig("move-ok");
    storage
        .save(Folder::Sounds, "A", "syx", &sound_file("A", 1))
        .unwrap();
    let id = library.list(Kind::Sounds).unwrap().0[0].id.clone();

    library
        .move_items(std::slice::from_ref(&id), "Pack")
        .unwrap();
    assert_eq!(names(&storage, Folder::Sounds), ["Pack/A.syx"]);
    assert_eq!(library.item(&id).unwrap().path, "Pack/A.syx");
    library.move_items(std::slice::from_ref(&id), "").unwrap();

    assert_eq!(names(&storage, Folder::Sounds), ["A.syx"]);
    assert!(
        !storage
            .folder(Folder::Sounds)
            .unwrap()
            .join("Pack")
            .exists(),
        "the emptied folder goes"
    );
}

#[test]
fn test_duplicate_when_called_returns_a_second_file_with_the_same_data_and_a_new_id() {
    let (storage, library) = rig("duplicate");
    storage
        .save(Folder::Sounds, "A", "syx", &sound_file("A", 1))
        .unwrap();
    let id = library.list(Kind::Sounds).unwrap().0[0].id.clone();

    let (copy, line) = library.duplicate(&id).unwrap();

    assert_ne!(copy.id, id);
    assert_eq!(copy.hash, library.item(&id).unwrap().hash);
    assert_eq!(names(&storage, Folder::Sounds), ["A copy.syx", "A.syx"]);
    assert!(line.contains("To undo"));
}

// ---- auto backup, projects ----------------------------------------------------------------------------------------

#[test]
fn test_auto_backup_when_called_twice_returns_one_slot_item_with_two_versions_whose_hashes_are_known(
) {
    let (storage, library) = rig("backup-slot");
    let a = inspect(Kind::Sounds, &sound_file("A", 1), "A").unwrap();
    let b = inspect(Kind::Sounds, &sound_file("B", 2), "B").unwrap();

    library.auto_backup(&a).unwrap();
    library.auto_backup(&b).unwrap();

    let index = library.snapshot().unwrap();
    let slot = index.item("machine-sounds").unwrap();
    assert!(slot.slot);
    assert_eq!(slot.history.len(), 2);
    assert_eq!(slot.history[0].hash, a.hash);
    assert!(slot.history[0].label.starts_with("auto backup "));
    assert_eq!(
        fs::read(storage.root().join(&slot.history[1].file)).unwrap(),
        b.canonical
    );
    assert!(
        library.list(Kind::Sounds).unwrap().0.iter().any(|i| i.slot),
        "a scan keeps the slot"
    );
    assert!(index.item("machine-beats").is_none());
}

#[test]
fn test_list_when_a_project_and_its_capture_are_found_returns_one_item_holding_the_capture() {
    let (storage, library) = rig("project-capture");
    let raw = crate::machine::fake_machine::project_messages().concat();
    let facts = inspect(Kind::Projects, &raw, "").unwrap();
    storage
        .save(Folder::Projects, "Project x", "syx", &facts.canonical)
        .unwrap();
    storage
        .save(Folder::Projects, "Project x (as received)", "syx", &raw)
        .unwrap();

    let (items, report) = library.list(Kind::Projects).unwrap();

    assert_eq!(items.len(), 1, "{report:?}");
    assert_eq!(
        items[0].capture.as_deref(),
        Some("Project x (as received).syx")
    );
    assert_eq!(items[0].beats, Some(16));
}
