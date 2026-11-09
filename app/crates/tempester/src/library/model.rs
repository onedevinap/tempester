//! What the index holds: items (one per sound, beat or project file), their older versions, and the trash. Reading
//! and writing the index as JSON is here too (`library.json`).

use crate::command::Kind;
use crate::json::{obj, Json};
use crate::json_read::{self, Value};

/// Version of the index layout, written in the file.
const INDEX_VERSION: i64 = 1;

pub fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Sounds => "sounds",
        Kind::Beats => "beats",
        Kind::Projects => "projects",
    }
}

pub fn kind_from_name(name: &str) -> Option<Kind> {
    KINDS.into_iter().find(|kind| kind_name(*kind) == name)
}

pub const KINDS: [Kind; 3] = [Kind::Sounds, Kind::Beats, Kind::Projects];

/// An older version of an item, kept as a file under `history/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// Path relative to the data folder.
    pub file: String,
    pub hash: String,
    /// When that version was the current one (seconds since the epoch).
    pub date: i64,
    /// Why it was kept (`replaced by an update`, `renamed`, `auto backup 2026-...`).
    pub label: String,
}

/// What is read from a sound's data, for the filters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facets {
    /// The Sound Type folder of the `63` file name (`/S/Kicks/...`), empty when there is none.
    pub sound_type: String,
    /// One text per oscillator, `Shape 57` (Osc 1 and 2) or `Sample/Noise 13` (Osc 3 and 4); empty for a beat.
    pub osc: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    pub kind: Kind,
    /// Relative to the folder of its kind: `Kick.syx` or `Pack/Kick.syx`. Empty for the auto-backup slot.
    pub path: String,
    /// The internal name (a project has none: its file name).
    pub name: String,
    /// SHA-256 of the DECODED data, whichever 7-bit layout the file uses.
    pub hash: String,
    pub size: u64,
    /// Modification time of the file when the index last looked at it (change detection).
    pub mtime: i64,
    pub created: i64,
    pub modified: i64,
    /// When it was last fetched from the machine (the machine stores no dates).
    pub fetched: Option<i64>,
    /// Where it came from: `machine`, `project`, `file import`, `found in folder`, `duplicate`...
    pub source: String,
    /// What makes two files "the same thing" with different content (same name in the same folder, same beat slot).
    pub origin: String,
    pub favourite: bool,
    pub tags: Vec<String>,
    pub note: String,
    pub history: Vec<Version>,
    /// A project's exact capture (`... (as received).syx`), path relative to `projects/`.
    pub capture: Option<String>,
    /// Beats held by a project.
    pub beats: Option<usize>,
    pub facets: Facets,
    /// The pseudo-item that only holds the auto backups of the machine's current sound or beat.
    pub slot: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trashed {
    pub item: Item,
    pub trashed: i64,
    /// Where the file went, relative to the data folder (`trash/<id>/<file>`).
    pub file: String,
    pub capture_file: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Index {
    pub items: Vec<Item>,
    pub trash: Vec<Trashed>,
}

impl Index {
    pub fn item(&self, id: &str) -> Option<&Item> {
        self.items.iter().find(|item| item.id == id)
    }

    pub fn item_mut(&mut self, id: &str) -> Option<&mut Item> {
        self.items.iter_mut().find(|item| item.id == id)
    }

    /// The next free id of a kind: `s12`, `b3`, `p1`; never one that is in the index or the trash.
    pub fn next_id(&self, kind: Kind) -> String {
        let letter = &kind_name(kind)[..1];
        let used = self
            .items
            .iter()
            .map(|i| i.id.as_str())
            .chain(self.trash.iter().map(|t| t.item.id.as_str()));
        let top = used
            .filter_map(|id| id.strip_prefix(letter)?.parse::<u64>().ok())
            .max()
            .unwrap_or(0);
        format!("{letter}{}", top + 1)
    }

    pub fn render(&self) -> String {
        let trash = self.trash.iter().map(|t| {
            obj([
                ("item", item_json(&t.item)),
                ("trashed", t.trashed.into()),
                ("file", t.file.clone().into()),
                ("capture_file", t.capture_file.clone().into()),
            ])
        });
        obj([
            ("version", Json::Int(INDEX_VERSION)),
            (
                "items",
                Json::Arr(self.items.iter().map(item_json).collect()),
            ),
            ("trash", Json::Arr(trash.collect())),
        ])
        .render()
    }

    /// Reads the text of `library.json`; any damage is an error (the index is then rebuilt from the files).
    pub fn parse(text: &str) -> Result<Self, String> {
        let root = json_read::parse(text)?;
        if root.get("version").and_then(Value::as_int) != Some(INDEX_VERSION) {
            return Err("unknown index version".into());
        }
        let list = |key: &str| {
            root.get(key)
                .and_then(Value::as_array)
                .ok_or_else(|| format!("no \"{key}\" list"))
        };
        let items = list("items")?
            .iter()
            .map(item_from)
            .collect::<Result<_, _>>()?;
        let trash = list("trash")?
            .iter()
            .map(trashed_from)
            .collect::<Result<_, _>>()?;
        Ok(Self { items, trash })
    }
}

fn strings(list: &[String]) -> Json {
    Json::Arr(list.iter().map(|s| s.clone().into()).collect())
}

fn item_json(item: &Item) -> Json {
    let history = item.history.iter().map(|v| {
        obj([
            ("file", v.file.clone().into()),
            ("hash", v.hash.clone().into()),
            ("date", v.date.into()),
            ("label", v.label.clone().into()),
        ])
    });
    obj([
        ("id", item.id.clone().into()),
        ("kind", kind_name(item.kind).into()),
        ("path", item.path.clone().into()),
        ("name", item.name.clone().into()),
        ("hash", item.hash.clone().into()),
        ("size", i64::try_from(item.size).unwrap_or(i64::MAX).into()),
        ("mtime", item.mtime.into()),
        ("created", item.created.into()),
        ("modified", item.modified.into()),
        ("fetched", item.fetched.into()),
        ("source", item.source.clone().into()),
        ("origin", item.origin.clone().into()),
        ("favourite", item.favourite.into()),
        ("tags", strings(&item.tags)),
        ("note", item.note.clone().into()),
        ("history", Json::Arr(history.collect())),
        ("capture", item.capture.clone().into()),
        ("beats", item.beats.into()),
        ("sound_type", item.facets.sound_type.clone().into()),
        ("osc", strings(&item.facets.osc)),
        ("slot", item.slot.into()),
    ])
}

fn missing(key: &str) -> String {
    format!("item field \"{key}\" is missing")
}

fn text(value: &Value, key: &str) -> Result<String, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| missing(key))
}

fn number(value: &Value, key: &str) -> Result<i64, String> {
    value
        .get(key)
        .and_then(Value::as_int)
        .ok_or_else(|| missing(key))
}

fn flag(value: &Value, key: &str) -> Result<bool, String> {
    value
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| missing(key))
}

fn optional_text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn string_list(value: &Value, key: &str) -> Result<Vec<String>, String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| missing(key))?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("item field \"{key}\" holds a non-text"))
        })
        .collect()
}

fn version_from(value: &Value) -> Result<Version, String> {
    Ok(Version {
        file: text(value, "file")?,
        hash: text(value, "hash")?,
        date: number(value, "date")?,
        label: text(value, "label")?,
    })
}

fn item_from(value: &Value) -> Result<Item, String> {
    let kind = kind_from_name(&text(value, "kind")?).ok_or("unknown kind")?;
    let history = value
        .get("history")
        .and_then(Value::as_array)
        .ok_or_else(|| missing("history"))?
        .iter()
        .map(version_from)
        .collect::<Result<_, _>>()?;
    Ok(Item {
        id: text(value, "id")?,
        kind,
        path: text(value, "path")?,
        name: text(value, "name")?,
        hash: text(value, "hash")?,
        size: u64::try_from(number(value, "size")?).unwrap_or(0),
        mtime: number(value, "mtime")?,
        created: number(value, "created")?,
        modified: number(value, "modified")?,
        fetched: value.get("fetched").and_then(Value::as_int),
        source: text(value, "source")?,
        origin: text(value, "origin")?,
        favourite: flag(value, "favourite")?,
        tags: string_list(value, "tags")?,
        note: text(value, "note")?,
        history,
        capture: optional_text(value, "capture"),
        beats: value
            .get("beats")
            .and_then(Value::as_int)
            .and_then(|n| usize::try_from(n).ok()),
        facets: Facets {
            sound_type: text(value, "sound_type")?,
            osc: string_list(value, "osc")?,
        },
        slot: flag(value, "slot")?,
    })
}

fn trashed_from(value: &Value) -> Result<Trashed, String> {
    Ok(Trashed {
        item: item_from(value.get("item").ok_or("trash entry without item")?)?,
        trashed: number(value, "trashed")?,
        file: text(value, "file")?,
        capture_file: optional_text(value, "capture_file"),
    })
}

/// A plain item for tests.
#[cfg(test)]
pub fn sample_item(id: &str, kind: Kind, path: &str, hash: &str) -> Item {
    Item {
        id: id.into(),
        kind,
        path: path.into(),
        name: path.trim_end_matches(".syx").into(),
        hash: hash.into(),
        size: 10,
        mtime: 5,
        created: 1,
        modified: 2,
        fetched: Some(3),
        source: "machine".into(),
        origin: format!("file:{path}"),
        favourite: false,
        tags: vec![],
        note: String::new(),
        history: vec![],
        capture: None,
        beats: None,
        facets: Facets::default(),
        slot: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_when_parsed_back_returns_the_same_index_with_awkward_text() {
        let mut item = sample_item("s1", Kind::Sounds, "Pack/Ké \"x\".syx", "ab");
        item.tags = vec!["kick".into(), "a\tb".into()];
        item.note = "line1\nline2 \u{2028}".into();
        item.history.push(Version {
            file: "history/sounds/s1/x.syx".into(),
            hash: "cd".into(),
            date: 9,
            label: "replaced".into(),
        });
        item.facets = Facets {
            sound_type: "Kicks".into(),
            osc: vec!["Shape 1".into()],
        };
        let mut project = sample_item("p1", Kind::Projects, "P.syx", "ef");
        project.capture = Some("P (as received).syx".into());
        project.beats = Some(14);
        let index = Index {
            items: vec![item, project],
            trash: vec![Trashed {
                item: sample_item("b2", Kind::Beats, "B.syx", "01"),
                trashed: 77,
                file: "trash/b2/B.syx".into(),
                capture_file: None,
            }],
        };

        let back = Index::parse(&index.render());

        assert_eq!(back, Ok(index));
    }

    #[test]
    fn test_parse_when_text_is_damaged_or_foreign_returns_an_error() {
        for text in [
            "",
            "{",
            "[]",
            r#"{"version":2,"items":[],"trash":[]}"#,
            r#"{"version":1,"items":[{}],"trash":[]}"#,
        ] {
            assert!(Index::parse(text).is_err(), "{text}");
        }
    }

    #[test]
    fn test_next_id_when_items_and_trash_hold_numbers_returns_one_above_all_of_them() {
        let mut index = Index::default();
        index
            .items
            .push(sample_item("s3", Kind::Sounds, "a.syx", "1"));
        index.trash.push(Trashed {
            item: sample_item("s7", Kind::Sounds, "b.syx", "2"),
            trashed: 1,
            file: "trash/s7/b.syx".into(),
            capture_file: None,
        });

        assert_eq!(index.next_id(Kind::Sounds), "s8");
        assert_eq!(index.next_id(Kind::Beats), "b1");
    }
}
