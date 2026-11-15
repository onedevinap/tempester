use super::*;
use crate::voice_recipe::{put_word, JumpKind, VoiceOp};

/// A synthetic 64 KB Voice image (never a DSI byte): patterned instructions, `goto 0x15F0` at PC 0.
fn original() -> Vec<u8> {
    let mut image: Vec<u8> = (0..64 * 1024usize)
        .map(|i| (i.wrapping_mul(97) ^ (i >> 4)) as u8)
        .collect();
    put_word(&mut image, 0, 0x04_15F0);
    put_word(&mut image, 2, 0);
    image
}

fn leak<T>(value: T) -> &'static T {
    Box::leak(Box::new(value))
}

fn patch(name: &'static str, edits: Vec<VoiceEdit>) -> &'static VoicePatch {
    let input_sha256: &'static str = Box::leak(sha256_hex(&original()).into_boxed_str());
    leak(VoicePatch {
        name,
        file: name,
        input_sha256,
        edits: Box::leak(edits.into_boxed_slice()),
    })
}

fn word(pc: u32, value: u32) -> VoiceEdit {
    VoiceEdit {
        address: pc,
        count: 1,
        op: VoiceOp::Word(value),
    }
}

fn goto(pc: u32, target: u32) -> VoiceEdit {
    VoiceEdit {
        address: pc,
        count: 1,
        op: VoiceOp::Jump {
            kind: JumpKind::Goto,
            target,
        },
    }
}

const NEW: u32 = 0x12_3456;

fn version_of(built: &VoiceBuilt) -> VoiceVersion {
    let hash = |bytes: &[u8]| -> &'static str { Box::leak(sha256_hex(bytes).into_boxed_str()) };
    VoiceVersion {
        version: "9.9.9.9",
        name: "synthetic",
        kind: VoiceKind::Test,
        date: "2026-11-14",
        summary: "",
        changes: Changes::default(),
        description: "",
        sha256_image: hash(&built.image),
        sha256_image_written: hash(sent_range(&built.image)),
        sha256_syx: hash(&built.syx),
        patches: &[],
    }
}

#[test]
fn test_build_when_recipe_valid_returns_the_image_with_only_its_words_changed_and_a_decodable_syx()
{
    let p = patch("p", vec![word(0x1000, NEW), goto(0x2000, 0x3000)]);

    let built = build_voice_from_image(&[p], &original()).expect("builds");

    assert_eq!(voice_recipe::word_at(&built.image, 0x1000), Some(NEW));
    assert_eq!(voice_recipe::word_at(&built.image, 0x2000), Some(0x04_3000));
    let os = os_image::decode(&built.syx).expect("decodes");
    assert_eq!((os.id, &os.image), (OsId::Voice, &built.image));
    let changed = original()
        .chunks(3)
        .zip(built.image.chunks(3))
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(changed, 3, "two words and the second word of the goto");
}

#[test]
fn test_build_when_recipe_writes_a_protected_pc_returns_protected() {
    // The counter-proof of the protected zones: the reset goto, the update entry, the boot page, the calibration.
    for pc in [2, 0x650E, 0x651C, 0xA400, 0xFC00] {
        let p = patch("bad", vec![word(pc, NEW)]);

        let result = build_voice_from_image(&[p], &original());

        assert!(
            matches!(result, Err(VoiceError::Protected { pc: found, .. }) if found == pc),
            "{pc:#x}: {result:?}"
        );
    }
}

#[test]
fn test_build_when_a_goto_reaches_into_a_protected_pc_returns_protected() {
    let p = patch("bad", vec![goto(0x650C, 0x3000)]);

    assert!(matches!(
        build_voice_from_image(&[p], &original()),
        Err(VoiceError::Protected { pc: 0x650E, .. })
    ));
}

#[test]
fn test_check_built_image_when_a_protected_pc_or_the_reset_changed_returns_the_error() {
    let before = original();
    let mut entry = before.clone();
    put_word(&mut entry, 0x6510, NEW);
    let mut reset = before.clone();
    put_word(&mut reset, 0, NEW);

    assert!(matches!(
        check_built_image(&before, &entry),
        Err(VoiceError::Protected { pc: 0x6510, .. })
    ));
    assert_eq!(
        check_built_image(&before, &reset),
        Err(VoiceError::ResetChanged)
    );
    assert_eq!(check_built_image(&before, &before), Ok(()));
}

#[test]
fn test_build_when_two_recipes_own_the_same_pc_returns_overlap() {
    let a = patch("a", vec![word(0x1000, NEW)]);
    let b = patch("b", vec![word(0x1000, 7)]);
    // The second word of a goto is owned even when it does not change.
    let c = patch("c", vec![goto(0x2000, 0x3000)]);
    let d = patch("d", vec![word(0x2002, 7)]);

    assert!(matches!(
        build_voice_from_image(&[a, b], &original()),
        Err(VoiceError::Overlap { pc: 0x1000, .. })
    ));
    assert!(matches!(
        build_voice_from_image(&[c, d], &original()),
        Err(VoiceError::Overlap { pc: 0x2002, .. })
    ));
}

#[test]
fn test_build_when_a_word_changes_nothing_returns_no_effect() {
    let same = voice_recipe::word_at(&original(), 0x1000).expect("in the image");
    let p = patch("p", vec![word(0x1000, same)]);

    assert_eq!(
        build_voice_from_image(&[p], &original()),
        Err(VoiceError::NoEffect {
            patch: "p",
            pc: 0x1000
        })
    );
}

#[test]
fn test_build_when_the_image_is_not_the_origin_returns_wrong_origin() {
    let p = patch("p", vec![word(0x1000, NEW)]);
    let mut other = original();
    put_word(&mut other, 0x8000, 1);

    assert!(matches!(
        build_voice_from_image(&[p], &other),
        Err(VoiceError::WrongOrigin { .. })
    ));
}

#[test]
fn test_build_voice_when_the_file_is_for_another_chip_returns_not_voice() {
    let main = os_image::encode(OsId::Main, &vec![0u8; 512 * 1024]).expect("encodes");
    let version = leak(version_of(&VoiceBuilt {
        image: original(),
        syx: Vec::new(),
    }));

    assert_eq!(
        build_voice(version, &main),
        Err(VoiceError::NotVoice(OsId::Main))
    );
}

#[test]
fn test_check_voice_reference_when_any_of_the_three_hashes_differs_returns_which() {
    let p = patch("p", vec![word(0x1000, NEW)]);
    let built = build_voice_from_image(&[p], &original()).expect("builds");
    let good = version_of(&built);
    assert_eq!(check_voice_reference(&good, &built), Ok(()));

    let wrong: &'static str = Box::leak("0".repeat(64).into_boxed_str());
    let cases = [
        (
            "image",
            VoiceVersion {
                sha256_image: wrong,
                ..good
            },
        ),
        (
            "part of the image an update sends",
            VoiceVersion {
                sha256_image_written: wrong,
                ..good
            },
        ),
        (
            ".syx file",
            VoiceVersion {
                sha256_syx: wrong,
                ..good
            },
        ),
    ];
    for (what, bad) in cases {
        let found = check_voice_reference(&bad, &built);
        assert!(
            matches!(&found, Err(VoiceError::ReferenceMismatch { what: w, .. }) if *w == what),
            "{what}: {found:?}"
        );
    }
}

#[test]
fn test_sent_range_when_a_whole_image_returns_the_41_pages() {
    let image = original();

    assert_eq!(sent_range(&image).len(), 41 * 512 * 3);
    assert_eq!(sent_range(&image[..100]).len(), 100);
}

#[test]
fn test_table_when_read_returns_voice_1_5_1_0_with_recorded_hashes_and_voice_recipes() {
    let version = find_voice_version("1.5.1.0").expect("1.5.1.0 is in versions/voice/");

    for hash in [
        version.sha256_image,
        version.sha256_image_written,
        version.sha256_syx,
    ] {
        assert_eq!(hash.len(), 64);
        assert!(hash.bytes().all(|b| b.is_ascii_hexdigit()));
    }
    assert!(!version.patches.is_empty());
    for patch in version.patches {
        assert!(patch.file.starts_with("patch/voice/"), "{}", patch.file);
        assert_eq!(patch.input_sha256, VOICE_ORIGIN_SHA256);
        assert!(patch.edits.iter().all(|e| e.count > 0));
    }
    assert_eq!(
        find_voice_by_syx_sha256(version.sha256_syx).map(|v| v.version),
        Some("1.5.1.0")
    );
    assert_eq!(
        find_voice_by_written_sha256(version.sha256_image_written).map(|v| v.version),
        Some("1.5.1.0")
    );
    assert!(find_voice_by_written_sha256(&"0".repeat(64)).is_none());
}

#[test]
fn test_table_when_read_returns_1_5_1_0_superseded_and_the_newest_offered() {
    // The author's decision of 15/11/2026: Tempester Voice 1.5.1.1 holds the same fixes, so 1.5.1.0 is no longer
    // offered; it stays in the table to name a chip that holds it and for its history row.
    let old = find_voice_version("1.5.1.0").expect("1.5.1.0 is in versions/voice/");
    let newest = voice_versions()
        .iter()
        .max_by_key(|v| {
            v.version
                .split('.')
                .map(|p| p.parse::<u32>().unwrap_or(0))
                .collect::<Vec<_>>()
        })
        .expect("a Voice version");

    assert_eq!(old.kind, VoiceKind::Superseded);
    assert!(!old.kind.is_offered());
    assert_eq!(newest.version, "1.5.1.2");
    assert!(
        newest.kind.is_offered(),
        "the newest Voice version stays offered"
    );
    assert!(VoiceKind::Release.is_offered() && VoiceKind::Test.is_offered());
}

#[test]
fn test_table_when_read_has_no_recipe_inside_a_protected_zone() {
    // The recipes of the table, checked as the build checks them (before any image is read).
    for version in voice_versions() {
        assert_eq!(
            check_ownership(version.patches),
            Ok(()),
            "{}",
            version.version
        );
    }
}
