//! What the app remembers about the Voice chips, in the settings file: the Tempester Voice OS it last sent (to compare a
//! full backup with it), the SHA-256 of pages 0-40 of the user's own DSI Voice 1.5 file when the app has built from it
//! (so that a chip still on DSI's OS is named as such), and the calibration pages of the last full backup (page 42 is
//! the chip's own and an update never writes it: it must come back unchanged).

use super::Storage;
use crate::error::AppError;

const KEY_VERSION: &str = "voice_installed_version";
const KEY_AT: &str = "voice_installed_at";
const KEY_PAGES: &str = "voice_installed_pages_sha256";
const KEY_DSI_PAGES: &str = "voice_dsi_pages_sha256";
const KEY_CALIBRATION: &str = "full_backup_calibration_sha256";
const INSTALLED_KEYS: [&str; 3] = [KEY_VERSION, KEY_AT, KEY_PAGES];

/// The Voice OS this app sent to the six chips.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceInstalled {
    pub version: String,
    /// UTC stamp of the end of the send.
    pub at: String,
    /// SHA-256 of pages 0-40 of the image that was sent: what each chip must hold afterwards.
    pub pages_sha256: String,
}

fn is_sha256(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

impl Storage {
    /// The Voice OS last sent from the app; `None` when absent, damaged, or cleared by an update that stopped.
    pub fn voice_installed(&self) -> Option<VoiceInstalled> {
        let settings = self.load_settings();
        let get = |key| settings.get(key).filter(|v| !v.is_empty());
        Some(VoiceInstalled {
            version: get(KEY_VERSION)?.to_string(),
            at: get(KEY_AT)?.to_string(),
            pages_sha256: get(KEY_PAGES).filter(|v| is_sha256(v))?.to_string(),
        })
    }

    /// Keeps `record` as the last Voice OS sent, or forgets it (`None`): other settings are merged, not overwritten.
    pub fn set_voice_installed(&self, record: Option<&VoiceInstalled>) -> Result<(), AppError> {
        self.update_settings(|settings| match record {
            Some(r) => {
                settings.set(KEY_VERSION, &r.version);
                settings.set(KEY_AT, &r.at);
                settings.set(KEY_PAGES, &r.pages_sha256);
            }
            None => INSTALLED_KEYS.iter().for_each(|key| settings.remove(key)),
        })
        .map(|_| ())
    }

    /// SHA-256 of pages 0-40 of the user's DSI Voice 1.5 file, if the app has built from it.
    pub fn voice_dsi_pages(&self) -> Option<String> {
        self.load_settings()
            .get(KEY_DSI_PAGES)
            .filter(|v| is_sha256(v))
            .map(str::to_string)
    }

    pub fn set_voice_dsi_pages(&self, sha256: &str) -> Result<(), AppError> {
        self.update_settings(|settings| settings.set(KEY_DSI_PAGES, sha256))
            .map(|_| ())
    }

    /// SHA-256 of the calibration page (page 42) of each Voice chip in the last saved full backup.
    pub fn last_calibration(&self) -> Option<Vec<String>> {
        let settings = self.load_settings();
        let hashes: Vec<String> = settings
            .get(KEY_CALIBRATION)?
            .split(',')
            .map(str::to_string)
            .collect();
        (hashes.len() == 6 && hashes.iter().all(|h| is_sha256(h))).then_some(hashes)
    }

    pub fn set_last_calibration(&self, hashes: &[String]) -> Result<(), AppError> {
        self.update_settings(|settings| settings.set(KEY_CALIBRATION, &hashes.join(",")))
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::temp_storage;

    fn record() -> VoiceInstalled {
        VoiceInstalled {
            version: "1.5.1.0".into(),
            at: "2026-11-14_100000Z".into(),
            pages_sha256: "a".repeat(64),
        }
    }

    #[test]
    fn test_voice_installed_when_set_then_cleared_returns_the_record_then_none() {
        let storage = temp_storage("voice-installed");
        assert_eq!(storage.voice_installed(), None);

        storage.set_voice_installed(Some(&record())).expect("saved");
        assert_eq!(storage.voice_installed(), Some(record()));
        storage.set_voice_installed(None).expect("cleared");

        assert_eq!(storage.voice_installed(), None);
    }

    #[test]
    fn test_voice_installed_when_the_hash_is_damaged_returns_none() {
        let storage = temp_storage("voice-damaged");
        let bad = VoiceInstalled {
            pages_sha256: "A".repeat(64),
            ..record()
        };
        storage.set_voice_installed(Some(&bad)).expect("saved");

        assert_eq!(storage.voice_installed(), None);
    }

    #[test]
    fn test_last_calibration_when_six_hashes_are_stored_returns_them_and_refuses_other_counts() {
        let storage = temp_storage("voice-calibration");
        let six: Vec<String> = (0..6).map(|k| format!("{k:x}").repeat(64)).collect();

        storage.set_last_calibration(&six).expect("saved");
        assert_eq!(storage.last_calibration(), Some(six.clone()));
        storage.set_last_calibration(&six[..5]).expect("saved");

        assert_eq!(
            storage.last_calibration(),
            None,
            "five hashes are not a backup"
        );
    }

    #[test]
    fn test_settings_when_a_voice_value_is_set_keep_the_installed_main_record() {
        let storage = temp_storage("voice-merge");
        storage
            .update_settings(|s| s.set("installed_label", "kept"))
            .expect("saved");

        storage.set_voice_installed(Some(&record())).expect("saved");
        storage.set_voice_dsi_pages(&"c".repeat(64)).expect("saved");

        assert_eq!(storage.load_settings().get("installed_label"), Some("kept"));
        assert_eq!(storage.voice_dsi_pages(), Some("c".repeat(64)));
    }
}
