//! Tempester Voice OS versions built from the user's own DSI Voice 1.5 file and the generated recipe tables
//! (`generated/voice.rs`, from `patch/voice/*.json` and `versions/voice/*.json`). Same rules as the Voice branch of
//! `tools/build_version.py`: the input image must be DSI Voice 1.5 (SHA-256), no two recipes may own the same word,
//! the protected zones of `voice_recipe` stay as DSI has them (checked once per edit and once more on the built
//! image), every word written must change something, the built `.syx` (command `72`, broadcast by the machine to the
//! six chips at once) must decode to the image, and the image, the part an update sends and the `.syx` must give the
//! hashes recorded in the manifest. No DSI byte is embedded.

use std::fmt;

use crate::generated::voice as tables;
use crate::os_image::{self, OsId, OsImageError};
use crate::recipe::EditError;
use crate::sha256::sha256_hex;
use crate::versions::Changes;
use crate::voice_recipe::{self, VoiceEdit};

/// Bytes of a Voice image an update sends: 41 pages of 768 instructions of 3 bytes (PC 0 to 0xA3FF).
pub const SENT_LEN: usize = 0xF600;
/// The reset vector every Voice image must keep: `goto 0x15F0`.
const RESET: [u32; 2] = [0x04_15F0, 0];
/// SHA-256 of the decoded DSI Voice 1.5 image every Voice recipe is built on.
pub const VOICE_ORIGIN_SHA256: &str = tables::VOICE_ORIGIN_SHA256;

/// The part of a Voice image an update sends (pages 0-40). The boot page and the calibration page are the chip's own:
/// a backup read from the machine can only be compared over this range.
pub fn sent_range(image: &[u8]) -> &[u8] {
    &image[..image.len().min(SENT_LEN)]
}

/// A Voice recipe (`patch/voice/*.json`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoicePatch {
    pub name: &'static str,
    pub file: &'static str,
    pub input_sha256: &'static str,
    pub edits: &'static [VoiceEdit],
}

/// How far a Voice version has been through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceKind {
    /// Proved on the simulator, not installed yet.
    Test,
    Release,
    /// Replaced by a newer version that holds the same fixes: kept for its history row and to name what a chip holds
    /// (identity, full backup), never offered for building or sending again.
    Superseded,
}

impl VoiceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Test => "test",
            Self::Release => "release",
            Self::Superseded => "superseded",
        }
    }

    /// Whether the app may build and send a version of this kind.
    pub fn is_offered(self) -> bool {
        self != Self::Superseded
    }
}

/// A Tempester Voice version (`versions/voice/*.json`). `date`, `summary` and `changes` are the changelog the OS page
/// shows, as for a Main version; `description` is the technical text.
#[derive(Debug, Clone, Copy)]
pub struct VoiceVersion {
    pub version: &'static str,
    pub name: &'static str,
    pub kind: VoiceKind,
    pub date: &'static str,
    pub summary: &'static str,
    pub changes: Changes,
    pub description: &'static str,
    pub sha256_image: &'static str,
    /// SHA-256 of `sent_range` of the image: what pages 0-40 of every chip must equal after the update.
    pub sha256_image_written: &'static str,
    pub sha256_syx: &'static str,
    pub patches: &'static [&'static VoicePatch],
}

/// All known Voice versions, ordered as in `versions/voice/`.
pub fn voice_versions() -> &'static [VoiceVersion] {
    &tables::VOICE_VERSIONS
}

pub fn find_voice_version(version: &str) -> Option<&'static VoiceVersion> {
    voice_versions().iter().find(|v| v.version == version)
}

/// The version whose `.syx` has this SHA-256.
pub fn find_voice_by_syx_sha256(sha256: &str) -> Option<&'static VoiceVersion> {
    voice_versions().iter().find(|v| v.sha256_syx == sha256)
}

/// The version whose pages 0-40 have this SHA-256: what a chip read back after an update holds.
pub fn find_voice_by_written_sha256(sha256: &str) -> Option<&'static VoiceVersion> {
    voice_versions()
        .iter()
        .find(|v| v.sha256_image_written == sha256)
}

/// Why a Voice version could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceError {
    /// The file is not an OS file for the Voice chips.
    NotVoice(OsId),
    Os(OsImageError),
    /// The image is not DSI Voice 1.5.
    WrongOrigin {
        expected: &'static str,
        found: String,
    },
    OutOfImage {
        patch: &'static str,
        pc: u32,
    },
    Malformed {
        patch: &'static str,
        pc: u32,
    },
    /// Two recipes (or two edits) own the same word.
    Overlap {
        pc: u32,
        first: &'static str,
        second: &'static str,
    },
    /// A recipe writes a protected PC (reset goto, update entry, boot page, calibration).
    Protected {
        patch: &'static str,
        pc: u32,
        why: String,
    },
    /// A word whose new value equals the original.
    NoEffect {
        patch: &'static str,
        pc: u32,
    },
    /// The built image does not start with `goto 0x15F0`.
    ResetChanged,
    /// The built `.syx` does not decode to the built image.
    RoundTrip,
    ReferenceMismatch {
        version: &'static str,
        what: &'static str,
        expected: &'static str,
        found: String,
    },
}

impl fmt::Display for VoiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotVoice(id) => write!(f, "this is an OS file for {id:?}, not for the Voice"),
            Self::Os(e) => write!(f, "{e}"),
            Self::WrongOrigin { found, .. } => {
                write!(f, "this is not DSI Voice 1.5 (SHA-256 {found})")
            }
            Self::OutOfImage { patch, pc } => write!(f, "{patch}: PC {pc:#x} is outside the image"),
            Self::Malformed { patch, pc } => {
                write!(f, "{patch}: the edit at PC {pc:#x} is malformed")
            }
            Self::Overlap { pc, first, second } => {
                write!(f, "{first} and {second} both own PC {pc:#x}")
            }
            Self::Protected { patch, pc, why } => write!(f, "{patch} writes PC {pc:#x}: {why}"),
            Self::NoEffect { patch, pc } => write!(f, "{patch}: PC {pc:#x} changes nothing"),
            Self::ResetChanged => write!(f, "PC 0 does not hold goto 0x15F0"),
            Self::RoundTrip => write!(f, "the built file does not decode to the built image"),
            Self::ReferenceMismatch {
                version,
                what,
                expected,
                found,
            } => write!(
                f,
                "the built {what} does not match the reference of Tempester Voice {version} \
                 (SHA-256 expected {expected}, built {found}): nothing will be sent"
            ),
        }
    }
}

impl std::error::Error for VoiceError {}

impl From<OsImageError> for VoiceError {
    fn from(e: OsImageError) -> Self {
        Self::Os(e)
    }
}

/// A built Voice version: the image and its SysEx file (command `72`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceBuilt {
    pub image: Vec<u8>,
    pub syx: Vec<u8>,
}

/// Builds `version` from the user's DSI Voice 1.5 SysEx file and refuses a result whose image, sent range or `.syx`
/// differs from the version's recorded hashes.
pub fn build_voice(version: &VoiceVersion, dsi_syx: &[u8]) -> Result<VoiceBuilt, VoiceError> {
    let os = os_image::decode(dsi_syx)?;
    if os.id != OsId::Voice {
        return Err(VoiceError::NotVoice(os.id));
    }
    let built = build_voice_from_image(version.patches, &os.image)?;
    check_voice_reference(version, &built)?;
    Ok(built)
}

/// Compares a build with the hashes recorded for its version.
pub fn check_voice_reference(version: &VoiceVersion, built: &VoiceBuilt) -> Result<(), VoiceError> {
    let checks = [
        ("image", version.sha256_image, &built.image[..]),
        (
            "part of the image an update sends",
            version.sha256_image_written,
            sent_range(&built.image),
        ),
        (".syx file", version.sha256_syx, &built.syx[..]),
    ];
    for (what, expected, bytes) in checks {
        let found = sha256_hex(bytes);
        if found != expected {
            return Err(VoiceError::ReferenceMismatch {
                version: version.version,
                what,
                expected,
                found,
            });
        }
    }
    Ok(())
}

/// Builds from a decoded original image and an explicit recipe list.
pub fn build_voice_from_image(
    patches: &[&'static VoicePatch],
    original: &[u8],
) -> Result<VoiceBuilt, VoiceError> {
    check_ownership(patches)?;
    let found = sha256_hex(original);
    if let Some(patch) = patches.iter().find(|p| p.input_sha256 != found) {
        return Err(VoiceError::WrongOrigin {
            expected: patch.input_sha256,
            found,
        });
    }
    let image = apply(patches, original)?;
    check_built_image(original, &image)?;
    let syx = os_image::encode(OsId::Voice, &image)?;
    if os_image::decode(&syx)?.image != image {
        return Err(VoiceError::RoundTrip);
    }
    Ok(VoiceBuilt { image, syx })
}

/// Refuses protected PCs and overlaps before reading anything: every PC an edit owns, written or not.
fn check_ownership(patches: &[&'static VoicePatch]) -> Result<(), VoiceError> {
    let mut owned: Vec<(u32, &'static str)> = Vec::new();
    for patch in patches {
        for pc in patch.edits.iter().flat_map(VoiceEdit::owned) {
            if let Some(why) = voice_recipe::protected(pc) {
                return Err(VoiceError::Protected {
                    patch: patch.name,
                    pc,
                    why,
                });
            }
            owned.push((pc, patch.name));
        }
    }
    owned.sort_unstable();
    match owned.windows(2).find(|w| w[0].0 == w[1].0) {
        Some(w) => Err(VoiceError::Overlap {
            pc: w[1].0,
            first: w[0].1,
            second: w[1].1,
        }),
        None => Ok(()),
    }
}

fn apply(patches: &[&'static VoicePatch], original: &[u8]) -> Result<Vec<u8>, VoiceError> {
    let mut image = original.to_vec();
    for patch in patches {
        for edit in patch.edits {
            let words = edit.words(original).map_err(|e| match e {
                EditError::OutOfImage(pc) => VoiceError::OutOfImage {
                    patch: patch.name,
                    pc,
                },
                EditError::Malformed(pc) => VoiceError::Malformed {
                    patch: patch.name,
                    pc,
                },
            })?;
            for (pc, value) in words {
                if voice_recipe::word_at(original, pc) == Some(value) {
                    return Err(VoiceError::NoEffect {
                        patch: patch.name,
                        pc,
                    });
                }
                if !voice_recipe::put_word(&mut image, pc, value) {
                    return Err(VoiceError::OutOfImage {
                        patch: patch.name,
                        pc,
                    });
                }
            }
        }
    }
    Ok(image)
}

/// The rules of a Voice image, checked on the built image itself and independently of the recipes' own checks: PC 0-2
/// is still `goto 0x15F0` and no changed instruction is in a protected zone.
fn check_built_image(original: &[u8], built: &[u8]) -> Result<(), VoiceError> {
    let reset = [0, 2].map(|pc| voice_recipe::word_at(built, pc));
    if reset != RESET.map(Some) {
        return Err(VoiceError::ResetChanged);
    }
    let changed = original
        .chunks(3)
        .zip(built.chunks(3))
        .enumerate()
        .filter(|(_, (before, after))| before != after)
        .map(|(at, _)| at as u32 * 2);
    for pc in changed {
        if let Some(why) = voice_recipe::protected(pc) {
            return Err(VoiceError::Protected {
                patch: "the built image",
                pc,
                why,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
