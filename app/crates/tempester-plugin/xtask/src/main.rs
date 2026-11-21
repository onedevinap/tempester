//! nih-plug bundler, isolated-workspace variant.
//!
//! `nih_plug_xtask::main` climbs to the topmost ancestor `Cargo.toml`, which here is the app workspace and does not
//! contain this plugin ("package did not match any packages"). So the bundler moves to the plugin root (the parent
//! of this folder) itself, then calls `build` and `bundle`.
//! Then it gives the macOS bundles the plugin's own identifier (nih-plug writes `com.nih-plug.<package>` and has no
//! setting for it) and the app's version (nih-plug writes 1.0.0), and signs them again.
//! Usage: `cargo xtask bundle tempester-plugin --release` -> `target/bundled/`.

use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

/// The plugin's own reverse-DNS identifier, the same as its CLAP id (`src/lib.rs`).
const BUNDLE_ID: &str = "io.github.onedevinap.tempester-control";

fn main() -> nih_plug_xtask::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| io::Error::other("the xtask folder has no parent"))?;
    std::env::set_current_dir(&root)?;

    // Arguments after the binary: `["bundle", "tempester-plugin", "--release", ...]`; the command word is ignored
    // because this bundler only bundles.
    let mut packages: Vec<String> = Vec::new();
    let mut other: Vec<String> = Vec::new();
    let mut args = std::env::args().skip(2);
    while let Some(arg) = args.next() {
        if arg == "-p" || arg == "--package" {
            packages.extend(args.next());
        } else if let Some(name) = arg.strip_prefix("--package=") {
            packages.push(name.to_string());
        } else if arg.starts_with('-') || !packages.is_empty() {
            other.push(arg);
        } else {
            packages.push(arg);
        }
    }
    if packages.is_empty() {
        packages.push("tempester-plugin".to_string());
    }

    let target_dir = root.join("target");
    nih_plug_xtask::build(&packages, &other)?;
    for package in &packages {
        nih_plug_xtask::bundle(&target_dir, package, &other, false)?;
    }
    set_bundle_metadata(&target_dir.join("bundled"))?;
    Ok(())
}

/// Writes the plugin's identifier and the app's version (nih-plug's are `com.nih-plug.<package>` and 1.0.0) in every
/// macOS bundle (`Contents/Info.plist`; Linux and Windows bundles have none) of `bundled`, then signs the bundle again (ad hoc, as nih-plug does): the signature seals the Info.plist, and
/// macOS refuses to load a bundle whose seal is broken.
fn set_bundle_metadata(bundled: &Path) -> io::Result<()> {
    for entry in fs::read_dir(bundled)? {
        let bundle = entry?.path();
        let plist = bundle.join("Contents").join("Info.plist");
        if !plist.is_file() {
            continue;
        }
        let text = fs::read_to_string(&plist)?;
        let in_plist = |error: io::Error| io::Error::other(format!("{}: {error}", plist.display()));
        let mut updated =
            set_plist_string(&text, "CFBundleIdentifier", BUNDLE_ID).map_err(in_plist)?;
        for key in ["CFBundleShortVersionString", "CFBundleVersion"] {
            updated =
                set_plist_string(&updated, key, tempest_proto::APP_VERSION).map_err(in_plist)?;
        }
        if updated == text {
            continue;
        }
        fs::write(&plist, updated)?;
        let signed = Command::new("codesign")
            .args(["-f", "-s", "-"])
            .arg(&bundle)
            .status()?;
        if !signed.success() {
            return Err(io::Error::other(format!(
                "codesign failed for {}",
                bundle.display()
            )));
        }
    }
    Ok(())
}

/// `text` with the `<string>` value right after `<key>{key}</key>` replaced by `value`. Refuses a key that is absent,
/// repeated or not followed by a string, so a change in nih-plug's Info.plist template stops the bundling instead of
/// shipping nih-plug's value.
fn set_plist_string(text: &str, key: &str, value: &str) -> io::Result<String> {
    const OPEN: &str = "<string>";
    const CLOSE: &str = "</string>";
    let key_tag = format!("<key>{key}</key>");
    let fail = || io::Error::other(format!("expected one {key_tag} followed by a {OPEN}"));
    if text.matches(&key_tag).count() != 1 {
        return Err(fail());
    }
    let after_key = text.find(&key_tag).ok_or_else(fail)? + key_tag.len();
    let rest = &text[after_key..];
    if !rest.trim_start().starts_with(OPEN) {
        return Err(fail());
    }
    let start = after_key + rest.find(OPEN).ok_or_else(fail)? + OPEN.len();
    let end = start + text[start..].find(CLOSE).ok_or_else(fail)?;
    Ok(format!("{}{value}{}", &text[..start], &text[end..]))
}
