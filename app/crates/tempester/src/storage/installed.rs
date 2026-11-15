//! The Main OS this app last sent to the machine, kept in the settings file: what it was, when, the SHA-256 of its
//! image, and when (if ever) a dump read back from the machine proved the machine runs exactly that image.
//! One record at most. An update that stopped half way clears it: the machine then runs nothing we can name.

use super::settings::Settings;
use super::Storage;
use crate::error::AppError;
use crate::json::{obj, Json};
use tempest_proto::identity::{VoiceVersion, VOICE_CHIPS, VOICE_WORD_TEMPESTER_1510};
use tempest_proto::release_cap::{
    cap_of, release_max, ReleaseCap, RELEASES, RELEASE_MAX, TEMPESTER_VOICE_WORDS,
};
use tempest_proto::versions;

const KEY_LABEL: &str = "installed_label";
const KEY_VERSION: &str = "installed_version";
const KEY_AT: &str = "installed_at";
const KEY_SHA256: &str = "installed_sha256";
const KEY_VERIFIED_AT: &str = "installed_verified_at";
const KEY_SHA256_WRITTEN: &str = "installed_sha256_written";
const KEYS: [&str; 6] = [
    KEY_LABEL,
    KEY_VERSION,
    KEY_AT,
    KEY_SHA256,
    KEY_SHA256_WRITTEN,
    KEY_VERIFIED_AT,
];

/// A Main OS sent from the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledOs {
    /// What the page calls it ("Tempester 1.6.1.1", "DSI 1.5.0.2 (original)", a file name).
    pub label: String,
    /// The Tempester version number, when the file is a known Tempester build.
    pub version: Option<String>,
    /// UTC stamp of the end of the send.
    pub at: String,
    /// SHA-256 of the decoded Main image that was sent (what the machine must run afterwards).
    pub sha256_image: String,
    /// SHA-256 of the part of that image an update writes (below the flasher page): what a dump read back from the
    /// machine is compared with, because the machine keeps its own flasher page. `None` for a record saved by an
    /// older app whose image is not a known Tempester version: it cannot be verified until sent again.
    pub sha256_image_written: Option<String>,
    /// UTC stamp of the read-back that matched `sha256_image_written`.
    pub verified_at: Option<String>,
}

fn is_sha256(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

impl InstalledOs {
    /// The record of a settings file; `None` when absent or damaged (a hand-edited hash is not trusted).
    pub fn from_settings(settings: &Settings) -> Option<Self> {
        let get = |key| settings.get(key).filter(|v| !v.is_empty());
        let sha256_image = get(KEY_SHA256).filter(|v| is_sha256(v))?;
        let sha256_image_written = get(KEY_SHA256_WRITTEN)
            .filter(|v| is_sha256(v))
            .or_else(|| {
                versions::find_by_image_sha256(sha256_image).map(|v| v.sha256_image_written)
            });
        Some(Self {
            label: get(KEY_LABEL)?.to_string(),
            version: get(KEY_VERSION).map(str::to_string),
            at: get(KEY_AT)?.to_string(),
            sha256_image: sha256_image.to_string(),
            sha256_image_written: sha256_image_written.map(str::to_string),
            verified_at: get(KEY_VERIFIED_AT).map(str::to_string),
        })
    }

    fn write_to(&self, settings: &mut Settings) {
        settings.set(KEY_LABEL, &self.label);
        settings.set(KEY_VERSION, self.version.as_deref().unwrap_or(""));
        settings.set(KEY_AT, &self.at);
        settings.set(KEY_SHA256, &self.sha256_image);
        settings.set(
            KEY_SHA256_WRITTEN,
            self.sha256_image_written.as_deref().unwrap_or(""),
        );
        settings.set(KEY_VERIFIED_AT, self.verified_at.as_deref().unwrap_or(""));
    }

    /// The record as the page receives it.
    pub fn to_json(&self) -> Json {
        obj([
            ("label", self.label.as_str().into()),
            ("version", self.version.clone().into()),
            ("at", self.at.as_str().into()),
            ("sha256_image", self.sha256_image.as_str().into()),
            (
                "sha256_image_written",
                self.sha256_image_written.clone().into(),
            ),
            ("verified_at", self.verified_at.clone().into()),
        ])
    }
}

/// The Tempester features the app knows, in the order of the release that brings each: its id (what the controls ask
/// for), the name the Machine page shows, and the patch that brings it (`patch/*.json`). The single source of truth:
/// the Machine page's identity card builds its rows from the `features` event field and keeps no list of its own.
pub const FEATURES: [(&str, &str, &str, Option<&str>); 17] = [
    (
        "dump_requests",
        "Sound and beat requests",
        "patch/dump_request.json",
        None,
    ),
    (
        "nrpn_reception",
        "NRPN reception",
        "patch/nrpn_reception.json",
        None,
    ),
    (
        "remote_panel",
        "Remote panel",
        "patch/remote_controls.json",
        None,
    ),
    (
        "project_request",
        "Project request",
        "patch/project_request.json",
        None,
    ),
    (
        "beat_wide",
        "Wide beat positions",
        "patch/beat_wide_emission.json",
        None,
    ),
    ("ribbons", "FX ribbons", "patch/ribbon_remote.json", None),
    ("pedal_pad", "Pedal as pad", "patch/pedal_pad.json", None),
    (
        "probability",
        "Step probability",
        "patch/step_probability.json",
        None,
    ),
    (
        "mixer_reports",
        "Mixer reports",
        "patch/mixer_reports.json",
        None,
    ),
    (
        "long_notes",
        "Long notes",
        "patch/note_length_x4.json",
        None,
    ),
    (
        "quantize_fine",
        "Fine quantize",
        "patch/quantize_fine.json",
        None,
    ),
    (
        "pad_channels",
        "MIDI channel per pad",
        "patch/multichannel_out.json",
        None,
    ),
    (
        "full_backup",
        "Full machine backup",
        "patch/full_backup.json",
        None,
    ),
    (
        "release_cap",
        "Release capped at 123",
        "patch/release_cap.json",
        None,
    ),
    (
        "release_cap_voice",
        "Release up to 127 with Tempester Voice",
        "patch/release_cap_1670.json",
        None,
    ),
    (
        "voice_versions",
        "Voice versions reported",
        "patch/voice_versions.json",
        None,
    ),
    // n°62 in its 1.6.8.0 form: a second full backup in a power-on never reads a Voice chip past its flash. Before it,
    // the second one stops the chips (14/11/2026), so the app allows one per power-on.
    (
        "full_backup_repeat",
        "Repeated full backups",
        "patch/full_backup_1680.json",
        None,
    ),
];

/// The fourth field of a `FEATURES` row is the version a feature is known to start with, for a release whose patch list
/// the app does not hold yet (a release not built here): the feature is on for an announced version of the same
/// `major.minor` line from it on (`1.6.6.0`, `1.6.10.0`), and `since` says it. A number of another line (`1.7.0.0`,
/// `9.9.9.9`) says nothing the app can rely on: as for any unknown number, the record decides. Once the release is in
/// `versions/`, the patch file decides as for every other row and the fourth field goes back to `None`.
fn at_least(version: &str, minimum: &str) -> bool {
    let numbers = |text: &str| -> Option<Vec<u32>> {
        text.split('.').map(|part| part.parse().ok()).collect()
    };
    match (numbers(version), numbers(minimum)) {
        (Some(v), Some(m)) => m.len() >= 2 && v.len() == m.len() && v[..2] == m[..2] && v >= m,
        _ => false,
    }
}

/// For each of `FEATURES`: its page `label`, whether the installed OS has it (`on`, only for a known Tempester build:
/// an unknown file or no record is off) and the first version that brings it (`since`), for the page's tooltips.
pub fn features_json(record: Option<&InstalledOs>) -> Json {
    features_of(version_of(None, record), None, None)
}

/// The Release cap of the Main OS the app can name (announced, else the install record); an OS it cannot name has
/// none: the table's maxima.
pub fn cap_for(announced: Option<&str>, record: Option<&InstalledOs>) -> ReleaseCap {
    version_of(announced, record).map_or(ReleaseCap::Stock, cap_of)
}

/// The maximum of each of the five Release parameters as `[[index, max], ...]`, from the Main OS the app can name
/// (`tempest_proto::release_cap`) and the version words the six Voice chips reported (n°64, `None` when not reported).
/// An OS the app cannot name has no cap: the table's maxima.
pub fn release_max_json(
    announced: Option<&str>,
    record: Option<&InstalledOs>,
    voices: Option<&[u16; VOICE_CHIPS]>,
) -> Json {
    let cap = cap_for(announced, record);
    Json::Arr(
        RELEASES
            .iter()
            .map(|index| {
                Json::Arr(vec![
                    (*index).into(),
                    release_max(*index, cap, voices).into(),
                ])
            })
            .collect(),
    )
}

/// How many of the six Voice chips run each Tempester Voice version, from the words they reported (n°64): the OS
/// page's Voice group line follows it, and a card's `Installed on k/6 chips` when no other version shares the word. `null` when the machine did not report them, `[]`
/// when it did and no chip runs a Tempester version. One entry per version with at least one chip.
pub fn voice_installed_json(words: Option<&[u16; VOICE_CHIPS]>) -> Json {
    let Some(words) = words else {
        return Json::Null;
    };
    let mut counts: Vec<(&'static str, usize, &'static [&'static str])> = Vec::new();
    for word in words {
        let shared = VoiceVersion::from_word(*word).tempester_versions();
        for version in shared {
            match counts.iter_mut().find(|(v, _, _)| v == version) {
                Some((_, count, _)) => *count += 1,
                None => counts.push((version, 1, shared)),
            }
        }
    }
    Json::Arr(
        counts
            .into_iter()
            .map(|(version, count, shared)| {
                let mut fields = vec![
                    ("version", version.into()),
                    ("count", count.into()),
                    ("of", VOICE_CHIPS.into()),
                ];
                if shared.len() > 1 {
                    let same: Vec<Json> = shared.iter().map(|v| (*v).into()).collect();
                    fields.push(("same_word", Json::Arr(same)));
                }
                Json::Obj(fields)
            })
            .collect(),
    )
}

/// The OS the machine says it runs (n°39), as the page receives it: `version` is its number, `label` how the page names
/// it (`Tempester x.y.z.w`), `known` whether the app's versions table has that release (only then can a dump be
/// compared with its image). `null` when the machine did not announce a version.
pub fn running_json(announced: Option<&str>) -> Json {
    let Some(version) = announced else {
        return Json::Null;
    };
    obj([
        ("version", version.into()),
        ("label", format!("Tempester {version}").into()),
        ("known", versions::find_version(version).is_some().into()),
    ])
}

/// An OS a dump read back from the machine may prove: what the app last sent, or the release the machine announced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyCandidate {
    pub os: InstalledOs,
    /// SHA-256 of the written range the dump must equal.
    pub written: String,
    /// True for the app's own record, false for the announced release.
    pub from_record: bool,
}

/// What a dump may be compared with, the record first. The announcement is only a number: a test build keeps the
/// digits of its base, so a machine that runs what the app sent can announce another release. The dump is the proof,
/// hence both candidates; only a match with the record may mark the record verified. A record saved without its
/// written hash borrows the one of the announced release when both are the same image. The announced release is left
/// out when it is the record's own image or is unknown to the app.
pub fn verify_candidates(
    announced: Option<&str>,
    record: Option<InstalledOs>,
) -> Vec<VerifyCandidate> {
    let reported = announced.and_then(versions::find_version);
    let mut candidates = Vec::new();
    if let Some(os) = record {
        let borrowed = reported
            .filter(|v| v.sha256_image == os.sha256_image)
            .map(|v| v.sha256_image_written.to_string());
        if let Some(written) = os.sha256_image_written.clone().or(borrowed) {
            candidates.push(VerifyCandidate {
                os,
                written,
                from_record: true,
            });
        }
    }
    if let Some(v) = reported {
        if !candidates
            .iter()
            .any(|c| c.written == v.sha256_image_written)
        {
            candidates.push(VerifyCandidate {
                os: InstalledOs {
                    label: format!("Tempester {}", v.version),
                    version: Some(v.version.to_string()),
                    at: String::new(),
                    sha256_image: v.sha256_image.to_string(),
                    sha256_image_written: Some(v.sha256_image_written.to_string()),
                    verified_at: None,
                },
                written: v.sha256_image_written.to_string(),
                from_record: false,
            });
        }
    }
    candidates
}

/// The version the machine runs as far as the app knows: the exact one it announced (n°39) when this app knows that
/// release, else the build the install record names. `None` for an OS the app cannot name.
pub(crate) fn version_of(
    announced: Option<&str>,
    record: Option<&InstalledOs>,
) -> Option<&'static versions::Version> {
    announced
        .and_then(versions::find_version)
        .or_else(|| record.and_then(|r| versions::find_by_image_sha256(&r.sha256_image)))
}

/// Whether the app can name the OS the machine runs (announced, else the install record): the `os_known` field of the
/// events that carry `features`, so that the page can say « OS unknown » rather than guess from a feature table that
/// is all off for an OS it cannot name.
pub fn os_known(announced: Option<&str>, record: Option<&InstalledOs>) -> bool {
    version_of(announced, record).is_some()
}

/// The recipe file a patch is a copy of: a release that needs another form of a patch keeps the old recipe and adds
/// `patch/<name>_<digits>.json` (1.6.7.0 has `nrpn_reception_1670`, `version_request_1670`, `release_cap_1670`). Such a
/// copy stands for `patch/<name>.json` so that a feature does not go off on the release that reworks its patch.
fn base_recipe(file: &str) -> String {
    let stem = file.strip_suffix(".json").unwrap_or(file);
    match stem.rsplit_once('_') {
        Some((base, digits))
            if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) =>
        {
            format!("{base}.json")
        }
        _ => file.to_string(),
    }
}

/// The recipe forms a later form replaces while keeping what the earlier one brought, `(later, earlier)`: the 1.6.9.0
/// forms (n°65) take more Voice chips as Tempester Voice and keep the Release lift and the repeated full backups. An
/// explicit list on purpose: a future `release_cap_17xx` / `full_backup_17xx` that dropped the feature must not turn
/// it on (a second full backup stops the chips on a Main without the 1.6.8.0 form), so each successor is added here
/// only once its proof says it keeps the feature.
const SUCCESSORS: [(&str, &str); 2] = [
    ("patch/release_cap_1690.json", "patch/release_cap_1670.json"),
    ("patch/full_backup_1690.json", "patch/full_backup_1680.json"),
];

/// Whether `patched` is a listed successor of the recipe form `file` (`SUCCESSORS`).
fn later_form(patched: &str, file: &str) -> bool {
    SUCCESSORS.contains(&(patched, file))
}

/// Whether the version has the recipe `file`, itself, as a `_<digits>` copy of it, or as a later form of it.
fn has_patch(version: &versions::Version, file: &str) -> bool {
    version
        .patches
        .iter()
        .any(|p| p.file == file || base_recipe(p.file) == file || later_form(p.file, file))
}

/// `features_json` from the exact version the machine announced (n°39) when the app knows that release; from the
/// install record otherwise (no announcement, or a number this app does not know: it says nothing of what it holds).
pub fn features_for(
    announced: Option<&str>,
    record: Option<&InstalledOs>,
    voices: Option<&[u16; VOICE_CHIPS]>,
) -> Json {
    features_of(version_of(announced, record), announced, voices)
}

/// The Tempester Voice versions of the chips whose word is not 0x16, as the identity card names them: the versions
/// their words resolve to (« Tempester Voice 1.5.1.2 »), or « a later Tempester Voice OS » for a word this app does
/// not know yet, so that the note stays true for words after 0x17.
fn newer_voices_named(words: &[u16; VOICE_CHIPS]) -> String {
    let mut names: Vec<&str> = Vec::new();
    let mut unknown = false;
    for word in words.iter().filter(|w| **w != VOICE_WORD_TEMPESTER_1510) {
        match VoiceVersion::from_word(*word).tempester_versions() {
            [] => unknown = true,
            versions => versions.iter().for_each(|v| {
                if !names.contains(v) {
                    names.push(v);
                }
            }),
        }
    }
    let mut parts: Vec<String> = names
        .iter()
        .map(|v| format!("Tempester Voice {v}"))
        .collect();
    if unknown || parts.is_empty() {
        parts.push("a later Tempester Voice OS".into());
    }
    let verb = if parts.len() > 1 { "need" } else { "needs" };
    format!("{} {verb}", parts.join(" and "))
}

/// What the Release cap says about the machine right now, for the two rows that talk about it. The one decision is
/// `release_max` (also what the knobs and the command checks follow): a Release above `RELEASE_MAX` means the cap
/// is lifted. `None` for every other feature, and for a row that has nothing to add.
fn cap_note(name: &str, cap: ReleaseCap, voices: Option<&[u16; VOICE_CHIPS]>) -> Option<Json> {
    let lifted = release_max(RELEASES[0], cap, voices) > RELEASE_MAX;
    let (text, tone) = match (name, lifted) {
        ("release_cap", true) => ("Lifted (six Tempester Voice chips)", "good"),
        ("release_cap_voice", false) if voices.is_none() => {
            ("Inactive: Voice versions not reported", "warn")
        }
        // Tempester Voice chips the Main does not take as such: a word of the range other than 0x16 (1.5.1.2's 0x17,
        // or a later one) on a Main that tests the exact 0x16.
        ("release_cap_voice", false)
            if cap == ReleaseCap::WithTempesterVoice
                && voices.is_some_and(|w| w.iter().all(|x| TEMPESTER_VOICE_WORDS.contains(x))) =>
        {
            let text = format!(
                "Inactive: {} Main 1.6.9.0 or later",
                voices.map(newer_voices_named).unwrap_or_default()
            );
            return Some(obj([("text", text.into()), ("tone", "warn".into())]));
        }
        ("release_cap_voice", false) => {
            ("Inactive: needs Tempester Voice on all six chips", "warn")
        }
        _ => return None,
    };
    Some(obj([("text", text.into()), ("tone", tone.into())]))
}

fn features_of(
    installed: Option<&versions::Version>,
    announced: Option<&str>,
    voices: Option<&[u16; VOICE_CHIPS]>,
) -> Json {
    let cap = installed.map_or(ReleaseCap::Stock, cap_of);
    Json::Obj(
        FEATURES
            .iter()
            .map(|(name, label, file, minimum)| {
                let since = versions::versions()
                    .iter()
                    .find(|v| has_patch(v, file))
                    .map(|v| v.version)
                    .or(*minimum);
                let on = installed.is_some_and(|v| has_patch(v, file))
                    || minimum.zip(announced).is_some_and(|(m, a)| at_least(a, m));
                let mut fields = vec![
                    ("on", on.into()),
                    ("since", since.into()),
                    ("label", (*label).into()),
                ];
                // Only the rows the cap says something about carry a note.
                if let Some(note) = on.then(|| cap_note(name, cap, voices)).flatten() {
                    fields.push(("note", note));
                }
                (*name, Json::Obj(fields))
            })
            .collect(),
    )
}

/// Whether the OS the machine runs (announced, else the install record) has the feature `name` of `FEATURES`:
/// `Some` for an OS the app can name, `None` for one it cannot (nothing announced and no usable record, or an unknown
/// name).
pub fn feature_in_os(
    announced: Option<&str>,
    record: Option<&InstalledOs>,
    name: &str,
) -> Option<bool> {
    let (_, _, file, minimum) = FEATURES.iter().find(|(n, _, _, _)| *n == name)?;
    if minimum.zip(announced).is_some_and(|(m, a)| at_least(a, m)) {
        return Some(true);
    }
    version_of(announced, record).map(|v| has_patch(v, file))
}

/// The one rule for a command that needs a feature, shared with the page (`panel-perform.js` `status`, which greys
/// the control): an OS the app can name decides; an OS it cannot name does not have the feature, except on the fake
/// machine (it plays the newest release and the page enables everything there). An identity answer of the original
/// DSI OS (`dsi`) beats everything, the record and the fake machine included: it has none of them.
pub fn has_feature(
    announced: Option<&str>,
    record: Option<&InstalledOs>,
    name: &str,
    fake: bool,
    dsi: bool,
) -> bool {
    !dsi && feature_in_os(announced, record, name).unwrap_or(fake)
}

impl Storage {
    pub fn installed_os(&self) -> Option<InstalledOs> {
        InstalledOs::from_settings(&self.load_settings())
    }

    /// Keeps `record` as the last installed OS, or forgets it (`None`); other settings are merged, not overwritten.
    pub fn set_installed_os(&self, record: Option<&InstalledOs>) -> Result<(), AppError> {
        self.update_settings(|settings| match record {
            Some(record) => record.write_to(settings),
            None => KEYS.iter().for_each(|key| settings.remove(key)),
        })
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::temp_storage;

    fn record() -> InstalledOs {
        InstalledOs {
            label: "Tempester 1.6.1.1".into(),
            version: Some("1.6.1.1".into()),
            at: "2026-11-08_120000Z".into(),
            sha256_image: "a".repeat(64),
            sha256_image_written: Some("b".repeat(64)),
            verified_at: None,
        }
    }

    #[test]
    fn test_features_when_1_6_3_0_1_6_2_0_or_unknown_returns_what_each_brings_and_since_when() {
        let of = |version: &str| {
            let v = versions::find_version(version).expect("version in the table");
            features_json(Some(&InstalledOs {
                sha256_image: v.sha256_image.into(),
                ..record()
            }))
            .render()
        };
        let line = |on: [bool; 17]| {
            let rows = [
                ("dump_requests", "Sound and beat requests", "1.5.2.0"),
                ("nrpn_reception", "NRPN reception", "1.5.3.0"),
                ("remote_panel", "Remote panel", "1.6.0.0"),
                ("project_request", "Project request", "1.6.1.0"),
                ("beat_wide", "Wide beat positions", "1.6.2.0"),
                ("ribbons", "FX ribbons", "1.6.3.0"),
                ("pedal_pad", "Pedal as pad", "1.6.3.0"),
                ("probability", "Step probability", "1.6.3.0"),
                ("mixer_reports", "Mixer reports", "1.6.4.0"),
                ("long_notes", "Long notes", "1.6.5.0"),
                ("quantize_fine", "Fine quantize", "1.6.5.0"),
                ("pad_channels", "MIDI channel per pad", "1.6.5.0"),
                ("full_backup", "Full machine backup", "1.6.6.0"),
                ("release_cap", "Release capped at 123", "1.6.6.0"),
                (
                    "release_cap_voice",
                    "Release up to 127 with Tempester Voice",
                    "1.6.7.0",
                ),
                ("voice_versions", "Voice versions reported", "1.6.7.0"),
                ("full_backup_repeat", "Repeated full backups", "1.6.8.0"),
            ];
            let body: Vec<String> = rows
                .iter()
                .zip(on)
                .map(|((name, label, since), on)| {
                    // No Voice versions are known here: the Voice lift row says why it is not in force.
                    let note = if *name == "release_cap_voice" && on {
                        r#","note":{"text":"Inactive: Voice versions not reported","tone":"warn"}"#
                    } else {
                        ""
                    };
                    format!(r#""{name}":{{"on":{on},"since":"{since}","label":"{label}"{note}}}"#)
                })
                .collect();
            format!("{{{}}}", body.join(","))
        };
        let upto = |count: usize| -> [bool; 17] { std::array::from_fn(|i| i < count) };

        assert_eq!(
            of("1.6.9.0"),
            line(upto(17)),
            "the 1.6.9.0 forms keep every feature"
        );
        assert_eq!(of("1.6.8.0"), line(upto(17)));
        assert_eq!(of("1.6.7.0"), line(upto(16)));
        assert_eq!(of("1.6.6.0"), line(upto(14)));
        assert_eq!(of("1.6.5.0"), line(upto(12)));
        assert_ne!(
            of("1.6.5.0"),
            line(upto(13)),
            "full backup is not in 1.6.5.0"
        );
        assert_eq!(of("1.6.4.0"), line(upto(9)));
        assert_eq!(of("1.6.3.0"), line(upto(8)));
        assert_eq!(of("1.6.2.0"), line(upto(5)));
        assert_eq!(of("1.6.1.0"), line(upto(4)));
        assert_eq!(features_json(Some(&record())).render(), line(upto(0)));
        assert_eq!(features_json(None).render(), line(upto(0)));
    }

    #[test]
    fn test_feature_in_os_when_full_backup_or_release_cap_returns_what_the_manifest_of_each_version_says(
    ) {
        for name in ["full_backup", "release_cap"] {
            let on = |announced: Option<&str>| feature_in_os(announced, None, name);

            assert_eq!(on(Some("1.6.6.0")), Some(true), "{name}");
            assert_eq!(on(Some("1.6.5.0")), Some(false), "{name}");
            assert_eq!(
                on(Some("1.6.10.0")),
                None,
                "{name}: a number the app has no manifest of says nothing"
            );
            assert_eq!(on(Some("1.7.0.0")), None, "{name}");
            assert_eq!(on(None), None, "{name}: nothing announced, no record");
        }
        let record_1660 = InstalledOs {
            sha256_image: versions::find_version("1.6.6.0")
                .expect("1.6.6.0 in the table")
                .sha256_image
                .into(),
            ..record()
        };
        assert_eq!(
            feature_in_os(None, Some(&record_1660), "full_backup"),
            Some(true)
        );
        let features = features_for(Some("1.6.6.0"), None, None).render();
        assert!(features.contains(r#""full_backup":{"on":true,"since":"1.6.6.0""#));
        assert!(features.contains(r#""release_cap":{"on":true,"since":"1.6.6.0""#));
    }

    #[test]
    fn test_at_least_when_a_minimum_is_given_returns_true_only_from_it_on_the_same_line() {
        assert!(at_least("1.6.6.0", "1.6.6.0"));
        assert!(at_least("1.6.10.0", "1.6.6.0"), "numbers, not text");
        assert!(!at_least("1.6.5.0", "1.6.6.0"));
        assert!(!at_least("1.7.0.0", "1.6.6.0"), "another line");
        assert!(!at_least("1.6.x.0", "1.6.6.0"), "not a number");
        assert!(!at_least("1.6.6", "1.6.6.0"), "not the same length");
    }

    #[test]
    fn test_has_feature_when_full_backup_on_a_real_machine_returns_false_outside_the_manifest() {
        assert!(
            !has_feature(Some("1.6.5.0"), None, "full_backup", true, false),
            "a known older OS, even on the fake"
        );
        assert!(
            !has_feature(Some("1.6.10.0"), None, "full_backup", false, false),
            "unknown number"
        );
        assert!(
            !has_feature(None, None, "full_backup", false, false),
            "nothing known"
        );
        assert!(has_feature(
            Some("1.6.6.0"),
            None,
            "full_backup",
            false,
            false
        ));
    }

    #[test]
    fn test_has_feature_when_full_backup_on_the_original_os_returns_false_even_announced() {
        assert!(!has_feature(
            Some("1.6.6.0"),
            None,
            "full_backup",
            true,
            true
        ));
        assert!(has_feature(
            Some("1.6.6.0"),
            None,
            "full_backup",
            false,
            false
        ));
        assert!(!has_feature(
            Some("1.6.5.0"),
            None,
            "full_backup",
            false,
            false
        ));
    }

    #[test]
    fn test_features_for_when_announced_version_known_returns_its_features_over_the_record() {
        let v1610 = versions::find_version("1.6.1.0").expect("1.6.1.0 in the table");
        let old_record = InstalledOs {
            sha256_image: v1610.sha256_image.into(),
            ..record()
        };

        let announced = features_for(Some("1.6.3.0"), Some(&old_record), None).render();
        let without_record = features_for(Some("1.6.3.0"), None, None).render();

        let all_on = features_json(Some(&InstalledOs {
            sha256_image: versions::find_version("1.6.3.0")
                .expect("1.6.3.0 in the table")
                .sha256_image
                .into(),
            ..record()
        }))
        .render();
        assert_eq!(announced, all_on);
        assert_eq!(without_record, all_on);
        // Every feature 1.6.3.0 brings is on; the later ones (n°38, 1.6.4.0) stay off.
        for name in ["beat_wide", "ribbons", "pedal_pad", "probability"] {
            let on = format!(r#""{name}":{{"on":true"#);
            assert!(announced.contains(&on), "{name}: {announced}");
        }
    }

    #[test]
    fn test_features_for_when_nothing_or_an_unknown_number_announced_returns_the_record_s() {
        let v1620 = versions::find_version("1.6.2.0").expect("1.6.2.0 in the table");
        let record_1620 = InstalledOs {
            sha256_image: v1620.sha256_image.into(),
            ..record()
        };
        let from_record = features_json(Some(&record_1620)).render();

        assert_eq!(
            features_for(None, Some(&record_1620), None).render(),
            from_record
        );
        assert_eq!(
            features_for(Some("9.9.9.9"), Some(&record_1620), None).render(),
            from_record
        );
        assert_eq!(
            features_for(Some("9.9.9.9"), None, None).render(),
            features_json(None).render()
        );
    }

    #[test]
    fn test_has_feature_when_the_os_is_unknown_returns_false_on_a_real_machine_and_true_on_the_fake(
    ) {
        // Nothing announced, a record whose image the app does not know, a number it does not know.
        for announced in [None, Some("9.9.9.9")] {
            assert!(!has_feature(announced, None, "pad_channels", false, false));
            assert!(!has_feature(
                announced,
                Some(&record()),
                "pad_channels",
                false,
                false
            ));
            assert!(has_feature(announced, None, "pad_channels", true, false));
        }
        assert_eq!(feature_in_os(None, None, "pad_channels"), None);
        assert_eq!(
            feature_in_os(Some("1.6.5.0"), None, "no_such_feature"),
            None
        );
    }

    #[test]
    fn test_has_feature_when_the_os_is_known_returns_what_it_has_whatever_the_fake_flag() {
        for fake in [false, true] {
            assert!(has_feature(
                Some("1.6.5.0"),
                None,
                "pad_channels",
                fake,
                false
            ));
            assert!(!has_feature(
                Some("1.6.4.0"),
                None,
                "pad_channels",
                fake,
                false
            ));
            assert!(has_feature(
                Some("1.6.4.0"),
                None,
                "mixer_reports",
                fake,
                false
            ));
        }
    }

    #[test]
    fn test_has_feature_when_the_machine_answered_as_the_dsi_os_returns_false_for_everything() {
        let record_1650 = InstalledOs {
            sha256_image: versions::find_version("1.6.5.0")
                .expect("1.6.5.0 in the table")
                .sha256_image
                .into(),
            ..record()
        };
        for fake in [false, true] {
            for (announced, record) in [
                (Some("1.6.5.0"), None),
                (None, Some(&record_1650)),
                (None, None),
            ] {
                for (name, _, _, _) in FEATURES {
                    assert!(!has_feature(announced, record, name, fake, true), "{name}");
                }
            }
        }
        // Counter-proof: without the DSI answer the same record has the feature.
        assert!(has_feature(
            None,
            Some(&record_1650),
            "pad_channels",
            false,
            false
        ));
    }

    #[test]
    fn test_installed_os_when_record_stored_returns_it_back_with_other_settings_kept() {
        let storage = temp_storage("installed-roundtrip");
        storage.update_settings(|s| s.set("channel", "5")).unwrap();

        storage.set_installed_os(Some(&record())).unwrap();

        assert_eq!(storage.installed_os(), Some(record()));
        assert_eq!(storage.load_settings().channel(), 5);
    }

    #[test]
    fn test_installed_os_when_unknown_build_returns_record_without_version() {
        let storage = temp_storage("installed-unknown");
        let unknown = InstalledOs {
            version: None,
            label: "my build.syx".into(),
            ..record()
        };

        storage.set_installed_os(Some(&unknown)).unwrap();

        assert_eq!(storage.installed_os().map(|i| i.version), Some(None));
    }

    #[test]
    fn test_installed_os_when_verified_then_cleared_returns_verified_then_none() {
        let storage = temp_storage("installed-clear");
        let verified = InstalledOs {
            verified_at: Some("2026-11-08_130000Z".into()),
            ..record()
        };
        storage.set_installed_os(Some(&verified)).unwrap();
        assert_eq!(
            storage.installed_os().and_then(|i| i.verified_at),
            Some("2026-11-08_130000Z".into())
        );

        storage.set_installed_os(None).unwrap();

        assert_eq!(storage.installed_os(), None);
    }

    #[test]
    fn test_from_settings_when_older_record_is_a_known_version_returns_its_written_hash() {
        let known = &versions::versions()[0];
        let text = format!(
            "installed_label=x\ninstalled_at=t\ninstalled_sha256={}",
            known.sha256_image
        );

        let loaded = InstalledOs::from_settings(&Settings::parse(&text)).expect("record");

        assert_eq!(
            loaded.sha256_image_written.as_deref(),
            Some(known.sha256_image_written)
        );
    }

    #[test]
    fn test_from_settings_when_older_record_is_an_unknown_build_returns_no_written_hash() {
        let text = format!(
            "installed_label=x\ninstalled_at=t\ninstalled_sha256={}",
            "9".repeat(64)
        );

        let loaded = InstalledOs::from_settings(&Settings::parse(&text)).expect("record");

        assert_eq!(loaded.sha256_image_written, None);
    }

    fn record_of(version: &str) -> InstalledOs {
        let v = versions::find_version(version).expect("known version");
        InstalledOs {
            label: format!("Tempester {version}"),
            version: Some(version.into()),
            sha256_image: v.sha256_image.into(),
            sha256_image_written: Some(v.sha256_image_written.into()),
            ..record()
        }
    }

    fn written_of(version: &str) -> String {
        record_of(version)
            .sha256_image_written
            .expect("known version")
    }

    #[test]
    fn test_verify_candidates_when_report_differs_from_the_record_returns_the_record_then_the_report(
    ) {
        let candidates = verify_candidates(Some("1.6.5.0"), Some(record_of("1.6.4.0")));

        let from: Vec<(bool, &str)> = candidates
            .iter()
            .map(|c| (c.from_record, c.written.as_str()))
            .collect();
        assert_eq!(
            from,
            vec![
                (true, written_of("1.6.4.0").as_str()),
                (false, written_of("1.6.5.0").as_str())
            ]
        );
    }

    #[test]
    fn test_verify_candidates_when_report_equals_the_record_returns_the_record_only() {
        let candidates = verify_candidates(Some("1.6.4.0"), Some(record_of("1.6.4.0")));

        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].from_record);
    }

    #[test]
    fn test_verify_candidates_when_report_is_absent_or_unknown_returns_the_record_or_nothing() {
        for announced in [None, Some("9.9.9.9")] {
            let candidates = verify_candidates(announced, Some(record_of("1.6.4.0")));
            assert_eq!(candidates.len(), 1);
            assert!(candidates[0].from_record);
            assert!(verify_candidates(announced, None).is_empty());
        }
    }

    #[test]
    fn test_verify_candidates_when_report_is_known_and_nothing_was_sent_returns_the_report() {
        let candidates = verify_candidates(Some("1.6.5.0"), None);

        assert_eq!(candidates.len(), 1);
        assert!(!candidates[0].from_record);
        assert_eq!(candidates[0].written, written_of("1.6.5.0"));
    }

    #[test]
    fn test_verify_candidates_when_old_record_has_no_written_hash_borrows_the_reported_one() {
        let old = InstalledOs {
            sha256_image_written: None,
            ..record_of("1.6.5.0")
        };

        let same_image = verify_candidates(Some("1.6.5.0"), Some(old.clone()));
        let other_image = verify_candidates(Some("1.6.4.0"), Some(old));

        assert_eq!(same_image.len(), 1);
        assert!(same_image[0].from_record);
        assert_eq!(same_image[0].written, written_of("1.6.5.0"));
        assert_eq!(other_image.len(), 1, "no borrowing across images");
        assert!(!other_image[0].from_record);
    }

    #[test]
    fn test_running_json_when_known_unknown_or_absent_returns_label_and_known_flag() {
        assert_eq!(
            running_json(Some("1.6.5.0")).render(),
            r#"{"version":"1.6.5.0","label":"Tempester 1.6.5.0","known":true}"#
        );
        assert!(running_json(Some("9.9.9.9"))
            .render()
            .contains(r#""known":false"#));
        assert_eq!(running_json(None).render(), "null");
    }

    #[test]
    fn test_from_settings_when_hash_is_damaged_or_field_missing_returns_none() {
        let good = "installed_label=x\ninstalled_at=t\ninstalled_sha256=";
        let hash = "b".repeat(64);

        assert!(InstalledOs::from_settings(&Settings::parse(&format!("{good}{hash}"))).is_some());
        for bad in [
            format!("{good}{}", "b".repeat(63)),
            format!("{good}{}", "B".repeat(64)),
            format!("{good}zz"),
            format!("installed_at=t\ninstalled_sha256={hash}"),
            String::new(),
        ] {
            assert_eq!(InstalledOs::from_settings(&Settings::parse(&bad)), None);
        }
    }

    #[test]
    fn test_base_recipe_when_a_copy_with_digits_returns_the_recipe_it_copies() {
        assert_eq!(
            base_recipe("patch/nrpn_reception_1670.json"),
            "patch/nrpn_reception.json"
        );
        assert_eq!(
            base_recipe("patch/release_cap.json"),
            "patch/release_cap.json"
        );
        assert_eq!(
            base_recipe("patch/pad_settings.json"),
            "patch/pad_settings.json"
        );
        assert_eq!(
            base_recipe("patch/a_b_12.json"),
            "patch/a_b.json",
            "only the last group of digits"
        );
        assert_eq!(base_recipe("patch/x_.json"), "patch/x_.json");
    }

    #[test]
    fn test_feature_in_os_when_1_6_7_0_reworks_a_patch_in_a_copy_returns_the_feature_still_on() {
        // 1.6.7.0 replaces patch/nrpn_reception.json and patch/release_cap.json by `_1670` copies.
        let v1670 = versions::find_version("1.6.7.0").expect("1.6.7.0 in the table");
        assert!(v1670
            .patches
            .iter()
            .any(|p| p.file == "patch/nrpn_reception_1670.json"));
        assert!(!v1670
            .patches
            .iter()
            .any(|p| p.file == "patch/nrpn_reception.json"));
        // The wrong variant: an exact file match loses the feature on the release that copies its recipe.
        let exact = |file: &str| v1670.patches.iter().any(|p| p.file == file);
        assert!(
            !exact("patch/nrpn_reception.json"),
            "the exact match is wrong here"
        );

        for name in ["nrpn_reception", "release_cap"] {
            assert_eq!(
                feature_in_os(Some("1.6.7.0"), None, name),
                Some(true),
                "{name}"
            );
        }
        let features = features_for(Some("1.6.7.0"), None, None).render();
        assert!(features.contains(r#""nrpn_reception":{"on":true,"since":"1.5.3.0""#));
        assert!(features.contains(r#""release_cap":{"on":true,"since":"1.6.6.0""#));
    }

    #[test]
    fn test_feature_in_os_when_1_6_9_0_takes_later_forms_returns_the_voice_lift_and_repeated_backups_on(
    ) {
        // 1.6.9.0 replaces release_cap_1670 and full_backup_1680 by their 1690 forms (n°65).
        let v1690 = versions::find_version("1.6.9.0").expect("1.6.9.0 in the table");
        let exact_or_base = |file: &str| {
            v1690
                .patches
                .iter()
                .any(|p| p.file == file || base_recipe(p.file) == file)
        };
        for (name, file) in [
            ("release_cap_voice", "patch/release_cap_1670.json"),
            ("full_backup_repeat", "patch/full_backup_1680.json"),
        ] {
            assert!(
                !exact_or_base(file),
                "{name}: the rule without later forms loses it (the counter-proof)"
            );
            assert_eq!(
                feature_in_os(Some("1.6.9.0"), None, name),
                Some(true),
                "{name}"
            );
        }
        assert!(later_form(
            "patch/release_cap_1690.json",
            "patch/release_cap_1670.json"
        ));
        assert!(!later_form(
            "patch/release_cap_1670.json",
            "patch/release_cap_1690.json"
        ));
        assert!(!later_form(
            "patch/full_backup_1690.json",
            "patch/release_cap_1670.json"
        ));
    }

    #[test]
    fn test_has_patch_when_an_unknown_later_form_returns_false_the_feature_stays_off() {
        // A future 1.7 form that is not in SUCCESSORS (it might drop the feature): no Release lift, no repeated full
        // backups claimed for it. The generic « larger digits » rule (the first form of this code) said yes.
        let v1690 = versions::find_version("1.6.9.0").expect("1.6.9.0 in the table");
        let renamed: Vec<&'static versions::Patch> = v1690
            .patches
            .iter()
            .map(|p| {
                let file = match p.file {
                    "patch/release_cap_1690.json" => "patch/release_cap_1790.json",
                    "patch/full_backup_1690.json" => "patch/full_backup_1790.json",
                    other => other,
                };
                &*Box::leak(Box::new(versions::Patch { file, ..**p }))
            })
            .collect();
        let future = versions::Version {
            patches: Box::leak(renamed.into_boxed_slice()),
            ..*v1690
        };
        for file in ["patch/release_cap_1670.json", "patch/full_backup_1680.json"] {
            assert!(!has_patch(&future, file), "{file}");
            assert!(has_patch(v1690, file), "{file}: the listed successor");
            let generic = |patched: &str| {
                patched.rsplit_once('_').map(|(b, _)| b) == file.rsplit_once('_').map(|(b, _)| b)
                    && patched > file
            };
            assert!(
                future.patches.iter().any(|p| generic(p.file)),
                "{file}: the generic rule would have turned it on (the counter-proof)"
            );
        }
        assert!(
            has_patch(&future, "patch/release_cap.json"),
            "the 1.6.6.0 cap row still follows any copy"
        );
    }

    #[test]
    fn test_feature_in_os_when_the_voice_features_return_on_from_1_6_7_0_only() {
        for name in ["release_cap_voice", "voice_versions"] {
            assert_eq!(
                feature_in_os(Some("1.6.7.0"), None, name),
                Some(true),
                "{name}"
            );
            assert_eq!(
                feature_in_os(Some("1.6.6.0"), None, name),
                Some(false),
                "{name}"
            );
            assert_eq!(
                feature_in_os(Some("1.6.5.0"), None, name),
                Some(false),
                "{name}"
            );
        }
        let features = features_for(Some("1.6.7.0"), None, None).render();
        assert!(features.contains(r#""release_cap_voice":{"on":true,"since":"1.6.7.0""#));
        assert!(features.contains(r#""voice_versions":{"on":true,"since":"1.6.7.0""#));
    }

    fn maxima(announced: Option<&str>, voices: Option<&[u16; 6]>) -> String {
        release_max_json(announced, None, voices).render()
    }

    #[test]
    fn test_release_max_json_when_each_os_and_voice_combination_returns_the_cap_of_that_machine() {
        let tempester = [0x16u16; 6];
        let stock = "[[65,127],[70,127],[75,123],[83,127],[91,127]]";
        let capped = "[[65,123],[70,123],[75,123],[83,123],[91,123]]";
        let lifted = "[[65,127],[70,127],[75,127],[83,127],[91,127]]";

        assert_eq!(maxima(Some("1.6.5.0"), Some(&tempester)), stock);
        assert_eq!(maxima(Some("1.6.6.0"), None), capped);
        assert_eq!(
            maxima(Some("1.6.6.0"), Some(&tempester)),
            capped,
            "1.6.6.0 always caps"
        );
        assert_eq!(maxima(Some("1.6.7.0"), Some(&tempester)), lifted);
        assert_eq!(
            maxima(Some("1.6.7.0"), None),
            capped,
            "versions not reported"
        );
        for chip in 0..6 {
            let mut words = tempester;
            words[chip] = 0x15;
            assert_eq!(maxima(Some("1.6.7.0"), Some(&words)), capped, "chip {chip}");
        }
        assert_eq!(
            maxima(None, Some(&tempester)),
            stock,
            "an OS the app cannot name"
        );
        assert_eq!(maxima(Some("9.9.9.9"), Some(&tempester)), stock);
    }

    #[test]
    fn test_release_max_json_when_only_the_record_names_the_os_returns_its_cap() {
        let v = versions::find_version("1.6.7.0").expect("in the table");
        let record = InstalledOs {
            sha256_image: v.sha256_image.into(),
            ..record()
        };

        assert_eq!(
            release_max_json(None, Some(&record), Some(&[0x16; 6])).render(),
            "[[65,127],[70,127],[75,127],[83,127],[91,127]]"
        );
    }

    type FeaturesFn = fn(Option<&str>, Option<&[u16; 6]>) -> Json;

    /// The `note` text a features table carries for the row `id`, `None` when the row has none.
    fn note_of(features: &Json, id: &str) -> Option<String> {
        let Json::Obj(rows) = features else {
            return None;
        };
        let (_, Json::Obj(fields)) = rows.iter().find(|(name, _)| *name == id)? else {
            return None;
        };
        let (_, Json::Obj(note)) = fields.iter().find(|(name, _)| *name == "note")? else {
            return None;
        };
        match note.iter().find(|(name, _)| *name == "text") {
            Some((_, Json::Str(text))) => Some(text.clone()),
            _ => None,
        }
    }

    const LIFTED: &str = "Lifted (six Tempester Voice chips)";
    const NOT_REPORTED: &str = "Inactive: Voice versions not reported";
    const NEEDS_ALL: &str = "Inactive: needs Tempester Voice on all six chips";
    const NEEDS_1690: &str = "Inactive: Tempester Voice 1.5.1.2 needs Main 1.6.9.0 or later";

    /// Every OS and Voice combination of the identity card, as one judge for any implementation: the row « Release
    /// capped at 123 » says « lifted » exactly when `release_max` gives 127, the row « Release up to 127 with
    /// Tempester Voice » says why it is inactive exactly when it does not, and neither adds a word otherwise.
    fn judge_notes(features: FeaturesFn) -> Result<(), String> {
        let tempester = [0x16u16; 6];
        let v1512 = [0x17u16; 6];
        let mixed = [0x16u16, 0x17, 0x16, 0x17, 0x16, 0x17];
        let mut voices: Vec<Option<[u16; 6]>> =
            vec![None, Some(tempester), Some(v1512), Some(mixed)];
        for chip in 0..6 {
            let mut words = tempester;
            words[chip] = 0x15;
            voices.push(Some(words));
        }
        let all_in_range = |w: &[u16; 6]| w.iter().all(|x| (0x16..=0x79).contains(x));
        for version in ["1.6.5.0", "1.6.6.0", "1.6.7.0", "1.6.8.0", "1.6.9.0"] {
            let exact = matches!(version, "1.6.7.0" | "1.6.8.0");
            let ranged = version == "1.6.9.0";
            for words in &voices {
                let table = features(Some(version), words.as_ref());
                let lifted = (exact && *words == Some(tempester))
                    || (ranged && words.as_ref().is_some_and(all_in_range));
                let expected_cap = lifted.then(|| LIFTED.to_string());
                let expected_voice = match words {
                    _ if lifted || !(exact || ranged) => None,
                    None => Some(NOT_REPORTED.to_string()),
                    Some(w) if exact && all_in_range(w) => Some(NEEDS_1690.to_string()),
                    Some(_) => Some(NEEDS_ALL.to_string()),
                };
                for (id, expected) in [
                    ("release_cap", expected_cap),
                    ("release_cap_voice", expected_voice),
                ] {
                    let found = note_of(&table, id);
                    if found != expected {
                        return Err(format!(
                            "{version}, voices {words:?}, {id}: {found:?}, expected {expected:?}"
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn test_features_for_when_each_os_and_voice_combination_returns_the_notes_release_max_implies()
    {
        assert_eq!(judge_notes(|a, v| features_for(a, None, v)), Ok(()));
    }

    #[test]
    fn test_features_for_when_1_6_8_0_and_a_later_word_names_what_the_word_resolves_to() {
        let note = |words: [u16; 6]| {
            note_of(
                &features_for(Some("1.6.8.0"), None, Some(&words)),
                "release_cap_voice",
            )
        };
        let mut one_0x18 = [0x16u16; 6];
        one_0x18[2] = 0x18;
        assert_eq!(
            note(one_0x18).as_deref(),
            Some("Inactive: a later Tempester Voice OS needs Main 1.6.9.0 or later"),
            "a word this app does not know is not called 1.5.1.2 (the first wording's defect)"
        );
        let mut both = [0x17u16; 6];
        both[5] = 0x18;
        assert_eq!(
            note(both).as_deref(),
            Some("Inactive: Tempester Voice 1.5.1.2 and a later Tempester Voice OS need Main 1.6.9.0 or later")
        );
        assert_eq!(note([0x17; 6]).as_deref(), Some(NEEDS_1690));
        assert_ne!(note(one_0x18).as_deref(), Some(NEEDS_1690));
    }

    #[test]
    fn test_features_for_when_the_cap_is_lifted_agrees_with_release_max_json() {
        let lifted = features_for(Some("1.6.7.0"), None, Some(&[0x16; 6]));
        let capped = features_for(Some("1.6.7.0"), None, None);

        assert_eq!(note_of(&lifted, "release_cap").as_deref(), Some(LIFTED));
        assert!(maxima(Some("1.6.7.0"), Some(&[0x16; 6])).contains("[75,127]"));
        assert_eq!(note_of(&capped, "release_cap"), None);
        assert!(maxima(Some("1.6.7.0"), None).contains("[75,123]"));
    }

    #[test]
    fn test_judge_notes_when_given_deliberately_wrong_tables_returns_the_case_it_gets_wrong() {
        // The voices are never looked at: the contradiction of the real machine.
        let blind: FeaturesFn = |a, _| features_for(a, None, None);
        // « Lifted » as soon as any voice words are reported.
        let any_report: FeaturesFn = |a, v| features_for(a, None, v.map(|_| &[0x16u16; 6]));
        // « Lifted » when only the first five chips are Tempester.
        let five_chips: FeaturesFn = |a, v| {
            let words = v.map(|w| {
                let mut w = *w;
                w[5] = 0x16;
                w
            });
            features_for(a, None, words.as_ref())
        };

        for (name, wrong) in [
            ("blind", blind),
            ("any_report", any_report),
            ("five_chips", five_chips),
        ] {
            assert!(judge_notes(wrong).is_err(), "{name} must be refused");
        }
    }

    type InstalledFn = fn(Option<&[u16; 6]>) -> Json;

    /// The Voice card badge for every way the six chips can answer: the count of chips on the word 0x16, given to both
    /// versions that answer it (1.5.1.0 and 1.5.1.1, each naming the other), `[]` when none, `null` when not reported.
    fn judge_voice_installed(installed: InstalledFn) -> Result<(), String> {
        let entry = |count: usize| {
            let one = |v: &str| {
                format!(
                    r#"{{"version":"{v}","count":{count},"of":6,"same_word":["1.5.1.0","1.5.1.1"]}}"#
                )
            };
            format!("[{},{}]", one("1.5.1.0"), one("1.5.1.1"))
        };
        let own = |count: usize| format!(r#"{{"version":"1.5.1.2","count":{count},"of":6}}"#);
        let mut cases: Vec<(Option<[u16; 6]>, String)> = vec![
            (None, "null".into()),
            (Some([0x16; 6]), entry(6)),
            (Some([0x15; 6]), "[]".into()),
            (Some([0x18; 6]), "[]".into()),
            (Some([0; 6]), "[]".into()),
            // 1.5.1.2 answers 0x17, a word of its own: its count stands alone, no same_word.
            (Some([0x17; 6]), format!("[{}]", own(6))),
            (
                Some([0x16, 0x17, 0x16, 0x17, 0x16, 0x17]),
                format!("{},{}]", entry(3).trim_end_matches(']'), own(3)),
            ),
        ];
        for chip in 0..6 {
            let mut one_dsi = [0x16; 6];
            one_dsi[chip] = 0x15;
            cases.push((Some(one_dsi), entry(5)));
            let mut one_unknown = [0x16; 6];
            one_unknown[chip] = 0x18;
            cases.push((Some(one_unknown), entry(5)));
            let mut one_dsi_1512 = [0x17; 6];
            one_dsi_1512[chip] = 0x15;
            cases.push((Some(one_dsi_1512), format!("[{}]", own(5))));
            let mut only_one = [0x15; 6];
            only_one[chip] = 0x16;
            cases.push((Some(only_one), entry(1)));
        }
        for (words, expected) in cases {
            let found = installed(words.as_ref()).render();
            if found != expected {
                return Err(format!("{words:?}: {found}, expected {expected}"));
            }
        }
        Ok(())
    }

    #[test]
    fn test_voice_installed_json_when_each_chip_combination_returns_the_count_on_1_5_1_0() {
        assert_eq!(judge_voice_installed(voice_installed_json), Ok(()));
    }

    #[test]
    fn test_judge_voice_installed_when_given_deliberately_wrong_counts_returns_the_case_it_gets_wrong(
    ) {
        // Every chip that is not DSI's counts, unknown words included.
        let not_dsi: InstalledFn = |words| {
            words.map_or(Json::Null, |w| {
                let count = w.iter().filter(|word| **word != 0x15).count();
                if count == 0 {
                    Json::Arr(vec![])
                } else {
                    obj_entry(count)
                }
            })
        };
        // All or nothing: a partial count is rounded to zero.
        let all_or_nothing: InstalledFn = |words| match voice_installed_json(words) {
            Json::Arr(list) if !list.is_empty() && words.is_some_and(|w| w != &[0x16; 6]) => {
                Json::Arr(vec![])
            }
            other => other,
        };
        // The word taken for 1.5.1.0 alone (the first form): 1.5.1.1 never marked, nothing says the word is shared.
        let one_version: InstalledFn = |words| {
            words.map_or(Json::Null, |w| {
                let count = w.iter().filter(|word| **word == 0x16).count();
                if count == 0 {
                    return Json::Arr(vec![]);
                }
                Json::Arr(vec![obj([
                    ("version", "1.5.1.0".into()),
                    ("count", count.into()),
                    ("of", 6usize.into()),
                ])])
            })
        };
        // Not reported looks like « no chip ».
        let unreported_is_none: InstalledFn = |words| match words {
            None => Json::Arr(vec![]),
            some => voice_installed_json(some),
        };

        // 1.5.1.2's word read as the shared 0x16 of 1.5.1.0 / 1.5.1.1.
        let shared_1512: InstalledFn = |words| {
            let mapped = words.map(|w| w.map(|x| if x == 0x17 { 0x16 } else { x }));
            voice_installed_json(mapped.as_ref())
        };

        for (name, wrong) in [
            ("shared_1512", shared_1512),
            ("not_dsi", not_dsi),
            ("all_or_nothing", all_or_nothing),
            ("unreported_is_none", unreported_is_none),
            ("one_version", one_version),
        ] {
            assert!(
                judge_voice_installed(wrong).is_err(),
                "{name} must be refused"
            );
        }
    }

    fn obj_entry(count: usize) -> Json {
        let one = |version: &'static str| {
            obj([
                ("version", version.into()),
                ("count", count.into()),
                ("of", 6usize.into()),
                (
                    "same_word",
                    Json::Arr(vec!["1.5.1.0".into(), "1.5.1.1".into()]),
                ),
            ])
        };
        Json::Arr(vec![one("1.5.1.0"), one("1.5.1.1")])
    }
}
