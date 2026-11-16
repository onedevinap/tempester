#!/bin/bash
# Builds the macOS app bundle Tempester.app (Dock icon, name) around the release binary, no dependency.
#
# usage: tools/app_mac.sh [<output folder>]     (default: app/target/release)
# Contents/MacOS/tempester, Contents/Resources/tempest.icns (app/crates/tempester/icons), Contents/Info.plist with the
# workspace version of app/Cargo.toml. Ad-hoc signed (no Developer ID): the first launch needs Open Anyway in
# System Settings > Privacy & Security (macOS 15 and later).
set -euo pipefail
P="$(cd "$(dirname "$0")/.." && pwd)"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export PATH="$CARGO_HOME/bin:$PATH"
OUT="${1:-$P/app/target/release}"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' "$P/app/Cargo.toml" | head -1)
[ -n "$VERSION" ] || { echo "no version in app/Cargo.toml"; exit 1; }
(cd "$P/app" && cargo build --release -p tempester > /dev/null)
A="$OUT/Tempester.app/Contents"
rm -rf "$OUT/Tempester.app"
mkdir -p "$A/MacOS" "$A/Resources"
cp "$P/app/target/release/tempester" "$A/MacOS/tempester"
cp "$P/app/crates/tempester/icons/tempest.icns" "$A/Resources/tempest.icns"
cat > "$A/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Tempester</string>
  <key>CFBundleDisplayName</key><string>Tempester</string>
  <key>CFBundleExecutable</key><string>tempester</string>
  <key>CFBundleIdentifier</key><string>io.github.onedevinap.tempester</string>
  <key>CFBundleIconFile</key><string>tempest</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
plutil -lint "$A/Info.plist" > /dev/null
# Ad-hoc signature of the whole bundle: the linker signs only the binary, so Info.plist and Resources were not sealed
# and `codesign --verify` failed; a downloaded (quarantined) bundle with an invalid signature is reported « damaged »
# by Gatekeeper, with no Open Anyway. Ad-hoc is not a Developer ID: the first launch still needs Open Anyway.
codesign --force --deep --sign - "$OUT/Tempester.app" > /dev/null 2>&1
codesign --verify --deep --strict "$OUT/Tempester.app"
echo "$OUT/Tempester.app ($VERSION)"
