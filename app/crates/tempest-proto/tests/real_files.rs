//! Checks on the user's real DSI files, which are proprietary and not in the repository.
//! Run with `TEMPESTER_DSI_SYX=<folder> cargo test -- --ignored` (done by `tools/test_app.sh <folder>`).
//! Files: Tempest_Main_1.5.0.2.syx, Tempest_Voice_1.5.syx, Tempest_Panel_1.3.syx, Tempest_Sam_1.1.syx.

#![allow(clippy::expect_used)] // integration tests: a panic with a message is the failure report

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempest_proto::os_image::{self, OsId};
use tempest_proto::sha256::sha256_hex;
use tempest_proto::versions::{self, VersionError, MAIN_ORIGIN_SHA256};

const FILES: [(&str, OsId); 4] = [
    ("Tempest_Main_1.5.0.2.syx", OsId::Main),
    ("Tempest_Voice_1.5.syx", OsId::Voice),
    ("Tempest_Panel_1.3.syx", OsId::Panel),
    ("Tempest_Sam_1.1.syx", OsId::Sam),
];

fn folder() -> PathBuf {
    let value = std::env::var_os("TEMPESTER_DSI_SYX")
        .expect("set TEMPESTER_DSI_SYX to the folder of the DSI .syx files");
    PathBuf::from(value)
}

fn read(name: &str) -> Vec<u8> {
    fs::read(folder().join(name)).expect("DSI file readable")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

#[test]
#[ignore = "needs the real DSI files (TEMPESTER_DSI_SYX)"]
fn test_round_trip_when_real_os_files_returns_byte_identical_syx() {
    for (name, id) in FILES {
        let original = read(name);

        let decoded = os_image::decode(&original).expect("real file decodes");
        let rebuilt = os_image::encode(id, &decoded.image).expect("image encodes");

        assert_eq!(decoded.id, id, "{name}");
        assert_eq!(rebuilt, original, "{name}");
    }
}

#[test]
#[ignore = "needs the real DSI files (TEMPESTER_DSI_SYX)"]
fn test_decode_when_real_main_returns_known_sha256() {
    let image = os_image::decode(&read(FILES[0].0))
        .expect("real file decodes")
        .image;

    assert_eq!(sha256_hex(&image), MAIN_ORIGIN_SHA256);
    assert_eq!(
        sha256_hex(&image),
        "86aaf7ca41be60140f11dd4fed84bffed2590746dcb267d4e9947128974336be"
    );
}

#[test]
#[ignore = "needs the real DSI files (TEMPESTER_DSI_SYX)"]
fn test_decode_when_voice_file_given_as_main_returns_not_main() {
    let voice = read(FILES[1].0);
    let version = versions::find_version("1.5.5.0").expect("1.5.5.0 exists");

    assert_eq!(
        versions::build(version, &voice),
        Err(VersionError::NotMain(OsId::Voice))
    );
}

#[test]
#[ignore = "needs the real DSI files (TEMPESTER_DSI_SYX)"]
fn test_build_when_real_main_returns_every_version_with_intact_flasher_page() {
    let dsi = read(FILES[0].0);
    let original = os_image::decode(&dsi).expect("real file decodes").image;
    let flasher = (versions::FLASHER_PAGE - versions::BASE) as usize;

    for version in versions::versions() {
        let built = versions::build(version, &dsi).expect("version builds");

        assert_eq!(
            built.image[flasher..],
            original[flasher..],
            "{}",
            version.version
        );
        assert_ne!(built.image, original, "{}", version.version);
    }
}

#[test]
#[ignore = "needs the real DSI files and python3"]
fn test_build_when_1_5_5_0_returns_exactly_the_python_tool_output() {
    let dsi = read(FILES[0].0);
    let work = std::env::temp_dir().join(format!("tempester-proto-{}", std::process::id()));
    fs::create_dir_all(&work).expect("work folder");
    let main_bin = work.join("Main_1.5.0.2.bin");
    fs::write(&main_bin, os_image::decode(&dsi).expect("decodes").image).expect("write");

    let status = Command::new("python3")
        .arg("-I")
        .arg(repo_root().join("tools/build_version.py"))
        .arg(repo_root().join("versions/1.5.5.0.json"))
        .arg(&main_bin)
        .arg(&work)
        .output()
        .expect("python3 runs");
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );

    let built =
        versions::build(versions::find_version("1.5.5.0").expect("exists"), &dsi).expect("builds");
    let python_image = fs::read(work.join("Main_1.5.5.0.bin")).expect("python image");
    let python_syx = fs::read(work.join("Tempest_Main_1.5.5.0_Tempester.syx")).expect("python syx");
    fs::remove_dir_all(&work).expect("cleanup");

    assert_eq!(built.image, python_image);
    assert_eq!(built.syx, python_syx);
    // Counter-proof: another version must not equal the tool's 1.5.5.0.
    let other =
        versions::build(versions::find_version("1.5.3.0").expect("exists"), &dsi).expect("builds");
    assert_ne!(other.image, python_image);
}

/// A real beat dump (`F0 01 28 5F`, fetched from the machine): `TEMPESTER_BEAT_SYX=<file>` with `--ignored`.
#[test]
#[ignore = "needs a real beat dump (TEMPESTER_BEAT_SYX)"]
fn test_summarize_when_real_beat_dump_returns_a_printable_name_and_events() {
    // A beat dump is the author's own data, not part of the DSI folder: without it the check is skipped.
    let Some(path) = std::env::var_os("TEMPESTER_BEAT_SYX") else {
        println!("TEMPESTER_BEAT_SYX not set: skipped");
        return;
    };
    let dump = fs::read(path).expect("beat dump readable");

    let summary = tempest_proto::beat::summarize(&dump).expect("a well-formed beat dump");

    println!("beat name: {:?}, events: {}", summary.name, summary.events);
    assert!(!summary.name.is_empty() && !summary.name.contains('?'));
}
