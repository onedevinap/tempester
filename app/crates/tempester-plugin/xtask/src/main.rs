//! nih-plug bundler, isolated-workspace variant.
//!
//! `nih_plug_xtask::main` climbs to the topmost ancestor `Cargo.toml`, which here is the app workspace and does not
//! contain this plugin ("package did not match any packages"). So the bundler moves to the plugin root (the parent
//! of this folder) itself, then calls `build` and `bundle`.
//! Then it gives the macOS bundles the plugin's own identifier (nih-plug writes `com.nih-plug.<package>` and has no
//! setting for it) and signs them again.
//! Usage: `cargo xtask bundle tempester-plugin --release` -> `target/bundled/`.

use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

/// The macOS bundle identifier nih-plug's bundler writes for this package.
const NIH_PLUG_BUNDLE_ID: &str = "com.nih-plug.tempester-plugin";
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
    set_bundle_identifiers(&target_dir.join("bundled"))?;
    Ok(())
}

/// Replaces nih-plug's bundle identifier in every macOS bundle (`Contents/Info.plist`; Linux and Windows bundles have
/// none) of `bundled`, then signs the bundle again (ad hoc, as nih-plug does): the signature seals the Info.plist, and
/// macOS refuses to load a bundle whose seal is broken.
fn set_bundle_identifiers(bundled: &Path) -> io::Result<()> {
    let old = format!("<string>{NIH_PLUG_BUNDLE_ID}</string>");
    let new = format!("<string>{BUNDLE_ID}</string>");
    for entry in fs::read_dir(bundled)? {
        let bundle = entry?.path();
        let plist = bundle.join("Contents").join("Info.plist");
        if !plist.is_file() {
            continue;
        }
        let text = fs::read_to_string(&plist)?;
        if text.contains(&new) {
            continue;
        }
        if text.matches(&old).count() != 1 {
            return Err(io::Error::other(format!(
                "{}: expected one bundle identifier {NIH_PLUG_BUNDLE_ID}",
                plist.display()
            )));
        }
        fs::write(&plist, text.replace(&old, &new))?;
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
