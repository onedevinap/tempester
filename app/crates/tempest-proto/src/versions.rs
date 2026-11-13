//! Tempester Main versions built from the user's own DSI image and the generated recipe tables
//! (`generated/versions.rs`, from `patch/*.json` and `versions/*.json`). Same rules as `tools/build_version.py`:
//! the input image must be DSI Main 1.5.0.2 (SHA-256), no two patches may write the same word, the flasher page
//! `0x9D07F000-0x9D07FFFF` must stay untouched, every word written must change something, and the resulting `.syx` must
//! decode to the image. No DSI byte is embedded: our words are literal, the words derived from DSI's code are copies
//! read from the user's own image (`recipe`).

use std::fmt;

use crate::generated::versions as tables;
use crate::os_image::{self, OsId, OsImageError};
pub use crate::recipe::BASE;
use crate::recipe::{Edit, EditError};
use crate::sha256::sha256_hex;

/// First address of the flasher page, never rewritten by an update.
pub const FLASHER_PAGE: u32 = 0x9D07_F000;
/// Length of what an OS update writes: the image below the flasher page.
pub const WRITTEN_LEN: usize = (FLASHER_PAGE - BASE) as usize;
/// SHA-256 of the decoded DSI Main 1.5.0.2 image every patch is built on.
pub const MAIN_ORIGIN_SHA256: &str = tables::MAIN_ORIGIN_SHA256;

/// The part of a Main image an OS update writes (everything below the flasher page). The machine keeps its own
/// flasher there, which differs a little from the one in DSI's file, so a dump read back from the machine can only
/// be compared over this range.
pub fn written_range(image: &[u8]) -> &[u8] {
    &image[..image.len().min(WRITTEN_LEN)]
}

/// A patch (`patch/*.json`): a recipe over the input image whose SHA-256 is `input_sha256`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Patch {
    pub name: &'static str,
    pub file: &'static str,
    pub input_sha256: &'static str,
    pub edits: &'static [Edit],
}

/// What a version is for the user. Test builds (single-patch proofs, superseded intermediates) are kept in
/// `versions/archive/` and are never in the app's table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A published version.
    Release,
    /// The version with the fixes and small additions only.
    Corrections,
}

impl Kind {
    /// The word the manifest and the page use.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Release => "release",
            Self::Corrections => "corrections",
        }
    }
}

/// What changed since the previous release, as short sentences for musicians.
#[derive(Debug, Clone, Copy, Default)]
pub struct Changes {
    pub fixes: &'static [&'static str],
    pub additions: &'static [&'static str],
    pub changes: &'static [&'static str],
}

/// A Tempester version (`versions/*.json`): an ordered list of patches, and the SHA-256 of the Main image and of the
/// `.syx` file it must build to, and of the written range of its image (recorded by `tools/build_version.py --record`, checked by `tools/verify.sh`).
/// `date`, `summary` and `changes` are the changelog the page shows; `description` is the technical text.
#[derive(Debug, Clone, Copy)]
pub struct Version {
    pub version: &'static str,
    pub name: &'static str,
    pub kind: Kind,
    pub date: &'static str,
    pub summary: &'static str,
    pub changes: Changes,
    pub description: &'static str,
    pub sha256_image: &'static str,
    /// SHA-256 of `written_range` of the image: what a dump read back from the machine must equal.
    pub sha256_image_written: &'static str,
    pub sha256_syx: &'static str,
    pub patches: &'static [&'static Patch],
}

/// All known versions, ordered as in `versions/`.
pub fn versions() -> &'static [Version] {
    &tables::VERSIONS
}

pub fn find_version(version: &str) -> Option<&'static Version> {
    versions().iter().find(|v| v.version == version)
}

/// The version whose `.syx` file has this SHA-256 (lowercase hex).
pub fn find_by_syx_sha256(sha256: &str) -> Option<&'static Version> {
    versions().iter().find(|v| v.sha256_syx == sha256)
}

/// The version whose Main image has this SHA-256 (lowercase hex).
pub fn find_by_image_sha256(sha256: &str) -> Option<&'static Version> {
    versions().iter().find(|v| v.sha256_image == sha256)
}

/// Why a version could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionError {
    /// The file is not an OS file for the Main chip.
    NotMain(OsId),
    Os(OsImageError),
    /// The image is not the one the patches are built on.
    WrongOrigin {
        expected: &'static str,
        found: String,
    },
    /// A patch word written, or read, outside the image.
    OutOfImage {
        patch: &'static str,
        address: u32,
    },
    /// A malformed edit (jump out of its region, field too wide, bad count).
    Malformed {
        patch: &'static str,
        address: u32,
    },
    /// Two patches touch the same word.
    Overlap {
        address: u32,
        first: &'static str,
        second: &'static str,
    },
    /// A patch touches the flasher page.
    FlasherPage {
        patch: &'static str,
        address: u32,
    },
    /// A patch word whose new bytes equal the original.
    NoEffect {
        patch: &'static str,
        address: u32,
    },
    /// The built `.syx` does not decode to the built image.
    RoundTrip,
    /// The build does not give the hash recorded for the version: a stale or tampered manifest or table.
    ReferenceMismatch {
        version: &'static str,
        what: &'static str,
        expected: &'static str,
        found: String,
    },
}

impl fmt::Display for VersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotMain(id) => write!(f, "this is an OS file for {id:?}, not for the Main"),
            Self::Os(e) => write!(f, "{e}"),
            Self::WrongOrigin { found, .. } => {
                write!(f, "this is not DSI Main 1.5.0.2 (SHA-256 {found})")
            }
            Self::OutOfImage { patch, address } => {
                write!(f, "{patch}: {address:#x} is outside the image")
            }
            Self::Malformed { patch, address } => {
                write!(f, "{patch}: the edit at {address:#x} is malformed")
            }
            Self::Overlap {
                address,
                first,
                second,
            } => {
                write!(f, "{first} and {second} both touch {address:#x}")
            }
            Self::FlasherPage { patch, address } => {
                write!(f, "{patch} touches the flasher page ({address:#x})")
            }
            Self::NoEffect { patch, address } => {
                write!(f, "{patch}: word {address:#x} changes nothing")
            }
            Self::RoundTrip => write!(f, "the built file does not decode to the built image"),
            Self::ReferenceMismatch {
                version,
                what,
                expected,
                found,
            } => write!(
                f,
                "the built {what} does not match the reference of Tempester {version} \
                 (SHA-256 expected {expected}, built {found}): nothing will be sent"
            ),
        }
    }
}

impl std::error::Error for VersionError {}

impl From<OsImageError> for VersionError {
    fn from(e: OsImageError) -> Self {
        Self::Os(e)
    }
}

/// A built version: the Main image and its SysEx file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Built {
    pub image: Vec<u8>,
    pub syx: Vec<u8>,
}

/// Builds `version` from the user's DSI Main 1.5.0.2 SysEx file and refuses a result whose image or `.syx` hash
/// differs from the version's recorded reference.
pub fn build(version: &Version, dsi_syx: &[u8]) -> Result<Built, VersionError> {
    let os = os_image::decode(dsi_syx)?;
    if os.id != OsId::Main {
        return Err(VersionError::NotMain(os.id));
    }
    let built = build_from_image(version.patches, &os.image)?;
    check_reference(version, &built)?;
    Ok(built)
}

/// Compares a build with the hashes recorded for its version.
pub fn check_reference(version: &Version, built: &Built) -> Result<(), VersionError> {
    let checks = [
        ("image", version.sha256_image, &built.image[..]),
        (
            "written range of the image",
            version.sha256_image_written,
            written_range(&built.image),
        ),
        (".syx file", version.sha256_syx, &built.syx[..]),
    ];
    for (what, expected, bytes) in checks {
        let found = sha256_hex(bytes);
        if found != expected {
            return Err(VersionError::ReferenceMismatch {
                version: version.version,
                what,
                expected,
                found,
            });
        }
    }
    Ok(())
}

/// Builds from a decoded original image and an explicit patch list.
pub fn build_from_image(patches: &[&Patch], original: &[u8]) -> Result<Built, VersionError> {
    check_patch_list(patches)?;
    let found = sha256_hex(original);
    if let Some(patch) = patches.iter().find(|p| p.input_sha256 != found) {
        return Err(VersionError::WrongOrigin {
            expected: patch.input_sha256,
            found,
        });
    }
    let image = apply(patches, original)?;
    let syx = os_image::encode(OsId::Main, &image)?;
    if os_image::decode(&syx)?.image != image {
        return Err(VersionError::RoundTrip);
    }
    Ok(Built { image, syx })
}

/// Refuses overlaps and the flasher page before reading anything.
fn check_patch_list(patches: &[&Patch]) -> Result<(), VersionError> {
    let mut words: Vec<(u32, &'static str)> = Vec::new();
    for patch in patches {
        for address in patch.edits.iter().flat_map(Edit::addresses) {
            if address >= FLASHER_PAGE {
                return Err(VersionError::FlasherPage {
                    patch: patch.name,
                    address,
                });
            }
            words.push((address, patch.name));
        }
    }
    words.sort_unstable();
    match words.windows(2).find(|w| w[1].0 - w[0].0 < 4) {
        Some(w) => Err(VersionError::Overlap {
            address: w[1].0,
            first: w[0].1,
            second: w[1].1,
        }),
        None => Ok(()),
    }
}

/// Writes every patch word; copies and fields are read from `original`, never from the image being patched.
fn apply(patches: &[&Patch], original: &[u8]) -> Result<Vec<u8>, VersionError> {
    let mut image = original.to_vec();
    for patch in patches {
        for edit in patch.edits {
            let words = edit.words(original).map_err(|e| match e {
                EditError::OutOfImage(address) => VersionError::OutOfImage {
                    patch: patch.name,
                    address,
                },
                EditError::Malformed(address) => VersionError::Malformed {
                    patch: patch.name,
                    address,
                },
            })?;
            for (address, value) in words {
                let at = crate::recipe::offset(address, image.len()).ok_or(
                    VersionError::OutOfImage {
                        patch: patch.name,
                        address,
                    },
                )?;
                let new = value.to_le_bytes();
                if image[at..at + 4] == new {
                    return Err(VersionError::NoEffect {
                        patch: patch.name,
                        address,
                    });
                }
                image[at..at + 4].copy_from_slice(&new);
            }
        }
    }
    Ok(image)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::Op;

    const WORD: [u8; 4] = [1, 2, 3, 4];
    const NEW: u32 = 0x0909_0909;

    fn original() -> Vec<u8> {
        let mut image = vec![0u8; 512 * 1024];
        image[0x100..0x104].copy_from_slice(&WORD);
        image
    }

    fn patch(name: &'static str, edits: Vec<Edit>) -> &'static Patch {
        let hash: &'static str = Box::leak(sha256_hex(&original()).into_boxed_str());
        let edits: &'static [Edit] = Box::leak(edits.into_boxed_slice());
        Box::leak(Box::new(Patch {
            name,
            file: name,
            input_sha256: hash,
            edits,
        }))
    }

    fn word(offset: u32, value: u32) -> Edit {
        Edit {
            address: BASE + offset,
            count: 1,
            op: Op::Word(value),
        }
    }

    #[test]
    fn test_build_from_image_when_patch_valid_returns_image_with_word_changed_and_decodable_syx() {
        let p = patch("p", vec![word(0x100, NEW)]);

        let built = build_from_image(&[p], &original()).unwrap();

        assert_eq!(&built.image[0x100..0x104], &NEW.to_le_bytes());
        assert_eq!(os_image::decode(&built.syx).unwrap().image, built.image);
        let untouched = built
            .image
            .iter()
            .zip(original())
            .filter(|(a, b)| *a != b)
            .count();
        assert_eq!(untouched, 4);
    }

    #[test]
    fn test_build_from_image_when_copy_source_rewritten_by_another_patch_returns_the_original_word()
    {
        let a = patch("a", vec![word(0x100, NEW)]);
        let b = patch(
            "b",
            vec![Edit {
                address: BASE + 0x200,
                count: 1,
                op: Op::Copy {
                    from: BASE + 0x100,
                    set: &[],
                },
            }],
        );

        let built = build_from_image(&[a, b], &original()).unwrap();

        assert_eq!(&built.image[0x200..0x204], &WORD);
        assert_eq!(&built.image[0x100..0x104], &NEW.to_le_bytes());
    }

    #[test]
    fn test_build_from_image_when_two_patches_touch_same_word_returns_overlap() {
        let a = patch("a", vec![word(0x100, NEW)]);
        let b = patch("b", vec![word(0x100, 7)]);

        assert_eq!(
            build_from_image(&[a, b], &original()),
            Err(VersionError::Overlap {
                address: BASE + 0x100,
                first: "a",
                second: "b"
            })
        );
    }

    #[test]
    fn test_build_from_image_when_a_run_covers_another_patch_word_returns_overlap() {
        let a = patch(
            "a",
            vec![Edit {
                address: BASE + 0x100,
                count: 3,
                op: Op::Word(NEW),
            }],
        );
        let b = patch("b", vec![word(0x108, 7)]);

        assert!(matches!(
            build_from_image(&[a, b], &original()),
            Err(VersionError::Overlap {
                address: 0x9D00_0108,
                ..
            })
        ));
    }

    #[test]
    fn test_build_from_image_when_words_overlap_by_half_returns_overlap() {
        let a = patch("a", vec![word(0x100, NEW)]);
        let b = patch("b", vec![word(0x102, NEW)]);

        assert!(matches!(
            build_from_image(&[a, b], &original()),
            Err(VersionError::Overlap { .. })
        ));
    }

    #[test]
    fn test_build_from_image_when_patch_touches_flasher_page_returns_flasher_page() {
        for offset in [0x7F000, 0x7F004, 0x7FFFC] {
            let p = patch("f", vec![word(offset, NEW)]);

            assert!(
                matches!(
                    build_from_image(&[p], &original()),
                    Err(VersionError::FlasherPage { .. })
                ),
                "offset {offset:#x}"
            );
        }
    }

    #[test]
    fn test_build_from_image_when_last_word_before_flasher_page_returns_ok() {
        let p = patch("edge", vec![word(0x7EFFC, NEW)]);

        assert!(build_from_image(&[p], &original()).is_ok());
    }

    #[test]
    fn test_build_from_image_when_word_changes_nothing_returns_no_effect() {
        let p = patch("same", vec![word(0x100, u32::from_le_bytes(WORD))]);

        assert!(matches!(
            build_from_image(&[p], &original()),
            Err(VersionError::NoEffect { .. })
        ));
    }

    #[test]
    fn test_build_from_image_when_address_outside_image_returns_out_of_image() {
        let below = patch(
            "below",
            vec![Edit {
                address: 0x9C00_0000,
                count: 1,
                op: Op::Word(NEW),
            }],
        );
        let above = patch("above", vec![word(512 * 1024, NEW)]);

        assert!(matches!(
            build_from_image(&[below], &original()),
            Err(VersionError::OutOfImage { .. })
        ));
        assert!(matches!(
            build_from_image(&[above], &original()),
            Err(VersionError::FlasherPage { .. })
        ));
    }

    #[test]
    fn test_build_from_image_when_jump_leaves_its_region_returns_malformed() {
        let p = patch(
            "far",
            vec![Edit {
                address: BASE + 0x100,
                count: 1,
                op: Op::Jump {
                    target: 0xBD00_0000,
                    link: true,
                },
            }],
        );

        assert_eq!(
            build_from_image(&[p], &original()),
            Err(VersionError::Malformed {
                patch: "far",
                address: BASE + 0x100
            })
        );
    }

    #[test]
    fn test_build_from_image_when_image_is_another_returns_wrong_origin() {
        let p = patch("p", vec![word(0x100, NEW)]);
        let mut other = original();
        other[5] = 1;

        assert!(matches!(
            build_from_image(&[p], &other),
            Err(VersionError::WrongOrigin { .. })
        ));
    }

    #[test]
    fn test_build_when_file_is_voice_os_returns_not_main() {
        let voice = os_image::encode(OsId::Voice, &vec![0u8; 64 * 1024]).unwrap();
        let version = &versions()[0];

        assert_eq!(
            build(version, &voice),
            Err(VersionError::NotMain(OsId::Voice))
        );
    }

    #[test]
    fn test_build_when_file_is_not_dsi_returns_os_error() {
        assert!(matches!(
            build(&versions()[0], b"nope"),
            Err(VersionError::Os(_))
        ));
    }

    #[test]
    fn test_versions_when_generated_returns_all_manifests_with_unique_numbers() {
        let manifests = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../versions");
        let expected = std::fs::read_dir(manifests)
            .map(|dir| {
                dir.filter_map(Result::ok)
                    .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
                    .count()
            })
            .unwrap_or(0);

        let list = versions();

        assert!(expected > 0);
        assert_eq!(list.len(), expected);
        assert!(find_version("1.5.5.0").is_some());
        assert!(find_version("9.9.9.9").is_none());
        for (i, v) in list.iter().enumerate() {
            assert!(list[i + 1..].iter().all(|w| w.version != v.version));
        }
    }

    #[test]
    fn test_versions_when_generated_returns_releases_and_one_corrections_with_a_changelog() {
        let corrections = versions()
            .iter()
            .filter(|v| v.kind == Kind::Corrections)
            .count();

        assert_eq!(corrections, 1);
        for v in versions() {
            let digits = |part: &str| part.bytes().all(|b| b.is_ascii_digit());
            let date: Vec<&str> = v.date.split('-').collect();
            assert!(
                date.len() == 3 && date.iter().all(|p| digits(p)),
                "{}: date {}",
                v.version,
                v.date
            );
            assert!(
                !v.summary.is_empty() && !v.summary.contains('\n'),
                "{}",
                v.version
            );
            let c = v.changes;
            assert!(
                c.fixes.len() + c.additions.len() + c.changes.len() > 0,
                "{} lists no change",
                v.version
            );
            assert!(!v.description.is_empty(), "{}", v.version);
        }
    }

    /// The author listened to n°52 and kept it: no text of 1.6.5.0 may still call it optional or pending.
    #[test]
    fn test_version_1650_when_patch_52_is_kept_returns_texts_without_optional() {
        let v = find_version("1.6.5.0").expect("1.6.5.0 is in the table");
        let c = v.changes;
        let texts = c
            .fixes
            .iter()
            .chain(c.additions)
            .chain(c.changes)
            .copied()
            .chain([v.summary, v.description]);

        for text in texts {
            assert!(!text.contains("optional"), "still optional: {text}");
            assert!(!text.contains("listening test refuses"), "{text}");
        }
        assert!(v.description.contains("n°52 was kept"));
    }

    #[test]
    fn test_versions_when_manifest_is_archived_returns_it_absent_from_the_table() {
        let archive =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../versions/archive");
        let archived: Vec<String> = std::fs::read_dir(archive)
            .map(|dir| {
                dir.filter_map(Result::ok)
                    .filter_map(|e| {
                        e.path()
                            .file_stem()
                            .map(|s| s.to_string_lossy().into_owned())
                    })
                    .collect()
            })
            .unwrap_or_default();

        assert!(!archived.is_empty(), "the test builds are archived");
        for number in archived {
            assert!(find_version(&number).is_none(), "{number} is a test build");
        }
    }

    #[test]
    fn test_versions_when_generated_returns_patches_valid_by_themselves() {
        for v in versions() {
            assert!(check_patch_list(v.patches).is_ok(), "{}", v.version);
            assert!(v
                .patches
                .iter()
                .all(|p| p.input_sha256 == MAIN_ORIGIN_SHA256));
        }
    }

    #[test]
    fn test_origin_sha256_when_generated_returns_the_dsi_main_1_5_0_2_hash() {
        assert_eq!(
            MAIN_ORIGIN_SHA256,
            "86aaf7ca41be60140f11dd4fed84bffed2590746dcb267d4e9947128974336be"
        );
    }

    /// A synthetic version over the synthetic original: its reference hashes are the honest build's unless given.
    fn synthetic_version(image_hash: Option<&str>, syx_hash: Option<&str>) -> (Version, Vec<u8>) {
        synthetic_version_written(image_hash, None, syx_hash)
    }

    fn synthetic_version_written(
        image_hash: Option<&str>,
        written_hash: Option<&str>,
        syx_hash: Option<&str>,
    ) -> (Version, Vec<u8>) {
        let patch = patch("p", vec![word(0x100, NEW)]);
        let honest = build_from_image(&[patch], &original()).unwrap();
        let leak = |text: String| -> &'static str { Box::leak(text.into_boxed_str()) };
        let version = Version {
            version: "0.0.0.1",
            name: "synthetic",
            kind: Kind::Release,
            date: "2026-01-01",
            summary: "",
            changes: Changes::default(),
            description: "",
            sha256_image: leak(
                image_hash.map_or_else(|| sha256_hex(&honest.image), str::to_string),
            ),
            sha256_image_written: leak(
                written_hash
                    .map_or_else(|| sha256_hex(written_range(&honest.image)), str::to_string),
            ),
            sha256_syx: leak(syx_hash.map_or_else(|| sha256_hex(&honest.syx), str::to_string)),
            patches: Box::leak(Box::new([patch])),
        };
        let dsi = os_image::encode(OsId::Main, &original()).unwrap();
        (version, dsi)
    }

    #[test]
    fn test_build_when_reference_hashes_match_returns_the_built_files() {
        let (version, dsi) = synthetic_version(None, None);

        let built = build(&version, &dsi).unwrap();

        assert_eq!(sha256_hex(&built.image), version.sha256_image);
        assert_eq!(sha256_hex(&built.syx), version.sha256_syx);
    }

    #[test]
    fn test_build_when_reference_image_hash_is_altered_returns_reference_mismatch() {
        let (version, dsi) = synthetic_version(Some(&"0".repeat(64)), None);

        let error = build(&version, &dsi).unwrap_err();

        assert!(
            matches!(
                &error,
                VersionError::ReferenceMismatch { what: "image", .. }
            ),
            "{error}"
        );
        assert!(error.to_string().contains("nothing will be sent"));
    }

    #[test]
    fn test_build_when_reference_written_hash_is_altered_returns_reference_mismatch() {
        let (version, dsi) = synthetic_version_written(None, Some(&"0".repeat(64)), None);

        let error = build(&version, &dsi).unwrap_err();

        assert!(matches!(
            error,
            VersionError::ReferenceMismatch {
                what: "written range of the image",
                ..
            }
        ));
    }

    #[test]
    fn test_written_range_when_image_is_longer_than_the_range_returns_the_part_below_the_flasher() {
        let image = vec![7u8; WRITTEN_LEN + 4096];

        assert_eq!(written_range(&image).len(), WRITTEN_LEN);
        assert_eq!(
            written_range(&image[..100]).len(),
            100,
            "a short image is kept whole"
        );
    }

    #[test]
    fn test_build_when_reference_syx_hash_is_altered_returns_reference_mismatch() {
        let (version, dsi) = synthetic_version(None, Some(&"f".repeat(64)));

        let error = build(&version, &dsi).unwrap_err();

        assert!(matches!(
            error,
            VersionError::ReferenceMismatch {
                what: ".syx file",
                ..
            }
        ));
    }

    #[test]
    fn test_check_reference_when_built_image_has_one_byte_changed_returns_reference_mismatch() {
        let (version, dsi) = synthetic_version(None, None);
        let mut built = build(&version, &dsi).unwrap();
        built.image[7] ^= 1;

        assert!(check_reference(&version, &built).is_err());
    }

    #[test]
    fn test_versions_when_generated_returns_well_formed_distinct_reference_hashes() {
        let well_formed =
            |h: &str| h.len() == 64 && h.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        for (i, v) in versions().iter().enumerate() {
            assert!(
                well_formed(v.sha256_image)
                    && well_formed(v.sha256_image_written)
                    && well_formed(v.sha256_syx),
                "{}",
                v.version
            );
            assert!(versions()[i + 1..]
                .iter()
                .all(|w| w.sha256_image != v.sha256_image && w.sha256_syx != v.sha256_syx));
            let by_syx = find_by_syx_sha256(v.sha256_syx).map(|f| f.version);
            let by_image = find_by_image_sha256(v.sha256_image).map(|f| f.version);
            assert_eq!(by_syx, Some(v.version));
            assert_eq!(by_image, Some(v.version));
        }
        assert!(find_by_syx_sha256(&"0".repeat(64)).is_none());
    }
}
