//! What a full backup says of each Voice chip beyond "the dump is sound": which Voice OS its pages 0-40 hold (a Tempester
//! Voice version of the app's table, DSI's Voice 1.5 when the user's DSI file was seen, else an unknown build), whether
//! they equal the Voice update this app last sent (an update goes to the six chips at once and nothing reads a chip
//! back, so this is the only proof it reached each one), and whether the calibration page (page 42, the chip's own,
//! which an update never writes) is unchanged since the previous full backup.

use tempest_proto::full_backup::{
    calibration_page, code_pages, voice_build, Backup, Chip, VoiceBuild,
};
use tempest_proto::sha256::sha256_hex;

use crate::json::Json;
use crate::storage::voice_state::VoiceInstalled;
use crate::storage::Storage;

/// What the app knew before the backup started.
#[derive(Debug, Clone, Default)]
pub(super) struct Reference {
    installed: Option<VoiceInstalled>,
    /// SHA-256 of pages 0-40 of the user's DSI Voice 1.5 file, when the app has built from it.
    dsi_pages: Option<String>,
    /// SHA-256 of each chip's calibration page in the previous saved full backup.
    previous_calibration: Option<Vec<String>>,
}

impl Reference {
    pub(super) fn load(storage: &Storage) -> Self {
        Self {
            installed: storage.voice_installed(),
            dsi_pages: storage.voice_dsi_pages(),
            previous_calibration: storage.last_calibration(),
        }
    }

    pub(super) fn check(&self, voice: u8, image: &[u8]) -> VoiceCheck {
        let pages = sha256_hex(code_pages(image));
        let calibration = sha256_hex(calibration_page(image));
        VoiceCheck {
            build: voice_build(image, self.dsi_pages.as_deref()),
            sent: self.installed.as_ref().map(|sent| Sent {
                version: sent.version.clone(),
                equal: sent.pages_sha256 == pages,
            }),
            calibration_unchanged: self
                .previous_calibration
                .as_ref()
                .and_then(|all| all.get(usize::from(voice)))
                .map(|before| *before == calibration),
            pages,
            calibration,
        }
    }
}

/// How a chip's pages 0-40 stand against the Voice update the app sent.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Sent {
    version: String,
    equal: bool,
}

/// The findings for one Voice chip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct VoiceCheck {
    pub(super) build: VoiceBuild,
    sent: Option<Sent>,
    calibration_unchanged: Option<bool>,
    pub(super) pages: String,
    pub(super) calibration: String,
}

impl VoiceCheck {
    /// The sentence for the row and the report.
    pub(super) fn sentence(&self) -> String {
        let mut parts = vec![format!("Voice OS: {}", self.build.label())];
        match &self.sent {
            Some(sent) if sent.equal => parts.push(format!(
                "pages 0-40 equal the Tempester Voice {} update sent from this app",
                sent.version
            )),
            Some(sent) => parts.push(format!(
                "pages 0-40 DIFFER from the Tempester Voice {} update sent from this app: it did not reach this chip",
                sent.version
            )),
            None => {}
        }
        parts.push(
            match self.calibration_unchanged {
                Some(true) => "calibration page unchanged since the previous full backup",
                Some(false) => "calibration page CHANGED since the previous full backup (was the machine calibrated meanwhile?)",
                None => "no previous full backup to compare the calibration page with",
            }
            .to_string(),
        );
        parts.join("; ")
    }

    pub(super) fn json(&self) -> [(&'static str, Json); 5] {
        [
            ("voice_os", self.build.label().into()),
            ("pages_0_40_sha256", self.pages.as_str().into()),
            (
                "equals_update_sent",
                self.sent.as_ref().map(|s| s.equal).into(),
            ),
            ("calibration_sha256", self.calibration.as_str().into()),
            ("calibration_unchanged", self.calibration_unchanged.into()),
        ]
    }
}

/// The check of every Voice message of a set, in chip order.
pub(super) fn checks(reference: &Reference, items: &[Backup]) -> Vec<(u8, VoiceCheck)> {
    items
        .iter()
        .filter_map(|b| match b.chip {
            Chip::Voice(k) => Some((k, reference.check(k, &b.image))),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::obj;
    use tempest_proto::full_backup::synthetic::voice_image;

    fn reference(sent: Option<&Vec<u8>>, previous: Option<Vec<String>>) -> Reference {
        Reference {
            installed: sent.map(|image| VoiceInstalled {
                version: "1.5.1.0".into(),
                at: "2026-11-14_100000Z".into(),
                pages_sha256: sha256_hex(code_pages(image)),
            }),
            dsi_pages: None,
            previous_calibration: previous,
        }
    }

    fn calibrations(images: &[Vec<u8>]) -> Vec<String> {
        images
            .iter()
            .map(|i| sha256_hex(calibration_page(i)))
            .collect()
    }

    #[test]
    fn test_check_when_pages_equal_the_update_sent_and_calibration_unchanged_says_both() {
        let images: Vec<Vec<u8>> = (0..6).map(voice_image).collect();
        let reference = reference(Some(&images[0]), Some(calibrations(&images)));

        let check = reference.check(2, &images[2]);

        assert_eq!(check.sent.as_ref().map(|s| s.equal), Some(true));
        assert_eq!(check.calibration_unchanged, Some(true));
        assert!(check
            .sentence()
            .contains("equal the Tempester Voice 1.5.1.0"));
        assert!(check.sentence().contains("calibration page unchanged"));
    }

    #[test]
    fn test_check_when_a_chip_kept_other_code_says_it_differs_from_the_update_sent() {
        let images: Vec<Vec<u8>> = (0..6).map(voice_image).collect();
        let reference = reference(Some(&images[0]), None);
        let mut stale = images[3].clone();
        stale[100] ^= 0x55;

        let check = reference.check(3, &stale);

        assert_eq!(check.sent.as_ref().map(|s| s.equal), Some(false));
        assert!(check
            .sentence()
            .contains("DIFFER from the Tempester Voice 1.5.1.0"));
        assert!(check.sentence().contains("did not reach this chip"));
        assert_eq!(check.calibration_unchanged, None);
        assert!(check.sentence().contains("no previous full backup"));
    }

    #[test]
    fn test_check_when_a_calibration_page_changed_says_so_for_that_chip_only() {
        let images: Vec<Vec<u8>> = (0..6).map(voice_image).collect();
        let mut now = images.clone();
        now[4][tempest_proto::full_backup::VOICE_IMAGE_BYTES - 40] ^= 0x01;
        let reference = reference(None, Some(calibrations(&images)));

        for (k, image) in now.iter().enumerate() {
            let check = reference.check(k as u8, image);
            assert_eq!(check.calibration_unchanged, Some(k != 4), "chip {k}");
        }
        assert!(reference
            .check(4, &now[4])
            .sentence()
            .contains("calibration page CHANGED"));
    }

    #[test]
    fn test_check_when_nothing_was_sent_and_the_build_is_not_known_says_unknown_build() {
        let image = voice_image(0);

        let check = Reference::default().check(0, &image);

        assert_eq!(check.build, VoiceBuild::Unknown);
        assert!(check.sentence().starts_with("Voice OS: unknown build"));
        assert!(!check.sentence().contains("update sent"));
    }

    #[test]
    fn test_check_when_the_pages_equal_the_users_dsi_file_says_dsi_voice() {
        let image = voice_image(0);
        let reference = Reference {
            dsi_pages: Some(sha256_hex(code_pages(&image))),
            ..Reference::default()
        };

        assert_eq!(reference.check(0, &image).build, VoiceBuild::Dsi15);
    }

    #[test]
    fn test_json_when_a_check_is_made_returns_the_hashes_and_the_findings() {
        let image = voice_image(1);
        let check = reference(Some(&image), None).check(1, &image);

        let json = obj(check.json()).render();

        assert!(json.contains(r#""voice_os":"unknown build""#), "{json}");
        assert!(json.contains(r#""equals_update_sent":true"#));
        assert!(json.contains(r#""calibration_unchanged":null"#));
        assert!(json.contains(&sha256_hex(code_pages(&image))));
    }
}
