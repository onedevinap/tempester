//! The Main OS this app last sent to the machine, kept in the settings file: what it was, when, the SHA-256 of its
//! image, and when (if ever) a dump read back from the machine proved the machine runs exactly that image.
//! One record at most. An update that stopped half way clears it: the machine then runs nothing we can name.

use super::settings::Settings;
use super::Storage;
use crate::error::AppError;
use crate::json::{obj, Json};
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

/// The Tempester features the app has controls for, each with the patch that brings it (`patch/*.json`).
pub const FEATURES: [(&str, &str); 5] = [
    ("beat_wide", "patch/beat_wide_emission.json"),
    ("ribbons", "patch/ribbon_remote.json"),
    ("pedal_pad", "patch/pedal_pad.json"),
    ("probability", "patch/step_probability.json"),
    ("mixer_reports", "patch/mixer_reports.json"),
];

/// For each of `FEATURES`: whether the installed OS has it (`on`, only for a known Tempester build: an unknown file or
/// no record is off) and the first version that brings it (`since`), for the page's tooltips.
pub fn features_json(record: Option<&InstalledOs>) -> Json {
    features_of(record.and_then(|r| versions::find_by_image_sha256(&r.sha256_image)))
}

/// `features_json` from the exact version the machine announced (n°39) when the app knows that release; from the
/// install record otherwise (no announcement, or a number this app does not know: it says nothing of what it holds).
pub fn features_for(announced: Option<&str>, record: Option<&InstalledOs>) -> Json {
    match announced.and_then(versions::find_version) {
        Some(version) => features_of(Some(version)),
        None => features_json(record),
    }
}

fn features_of(installed: Option<&versions::Version>) -> Json {
    Json::Obj(
        FEATURES
            .iter()
            .map(|(name, file)| {
                let has = |v: &versions::Version| v.patches.iter().any(|p| p.file == *file);
                let since = versions::versions()
                    .iter()
                    .find(|v| has(v))
                    .map(|v| v.version);
                let on = installed.is_some_and(has);
                (*name, obj([("on", on.into()), ("since", since.into())]))
            })
            .collect(),
    )
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
        let line = |a, b, c, d, e| {
            format!(
                r#"{{"beat_wide":{{"on":{a},"since":"1.6.2.0"}},"ribbons":{{"on":{b},"since":"1.6.3.0"}},"pedal_pad":{{"on":{c},"since":"1.6.3.0"}},"probability":{{"on":{d},"since":"1.6.3.0"}},"mixer_reports":{{"on":{e},"since":"1.6.4.0"}}}}"#
            )
        };

        assert_eq!(of("1.6.4.0"), line(true, true, true, true, true));
        assert_eq!(of("1.6.3.0"), line(true, true, true, true, false));
        assert_eq!(of("1.6.2.0"), line(true, false, false, false, false));
        assert_eq!(of("1.6.1.0"), line(false, false, false, false, false));
        assert_eq!(
            features_json(Some(&record())).render(),
            line(false, false, false, false, false)
        );
        assert_eq!(
            features_json(None).render(),
            line(false, false, false, false, false)
        );
    }

    #[test]
    fn test_features_for_when_announced_version_known_returns_its_features_over_the_record() {
        let v1610 = versions::find_version("1.6.1.0").expect("1.6.1.0 in the table");
        let old_record = InstalledOs {
            sha256_image: v1610.sha256_image.into(),
            ..record()
        };

        let announced = features_for(Some("1.6.3.0"), Some(&old_record)).render();
        let without_record = features_for(Some("1.6.3.0"), None).render();

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

        assert_eq!(features_for(None, Some(&record_1620)).render(), from_record);
        assert_eq!(
            features_for(Some("9.9.9.9"), Some(&record_1620)).render(),
            from_record
        );
        assert_eq!(
            features_for(Some("9.9.9.9"), None).render(),
            features_json(None).render()
        );
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
}
