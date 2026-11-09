#!/bin/bash
# Builds two packages in dist/:
#   Tempester-<version>-macos-<arch>.dmg   Tempester.app and an Applications shortcut (no administrator rights)
#   Tempester-<version>-macos-<arch>.pkg   installer with three selectable components: the app, the CLAP plugin and
#                                          the VST3 plugin "Tempester Control" (licence page, readme)
#
# usage: tools/package_mac.sh        (macOS only; hdiutil, pkgbuild, productbuild: no dependency)
# TEMPESTER_PLUGIN_BUNDLES=<folder> uses plugin bundles already built (tools/package_common.sh).
# Unsigned (ADR 001): the first launch needs right-click > Open, the .pkg opens the same way (tools/macos/readme.txt
# says so). Both are checked: file, size, version in the name; dmg: `hdiutil verify`, the mounted app carries the same
# version in its Info.plist and answers `--version`; pkg: payload paths, install locations, no relocation.
#
# Why the dmg stays: it is the only package that needs no password and no choice (drag, drop), and removing a working
# deliverable was not asked. Why the plugins are not in the dmg: a dmg cannot install into /Library.
# Why /Library and not ~/Library: that is where DAWs of every account look; copies installed by
# hand under ~/Library are left alone (relocation is switched off so that the installer never "updates" a copy it
# finds elsewhere).
set -euo pipefail
# shellcheck source=tools/package_common.sh
. "$(dirname "$0")/package_common.sh"
[ "$(uname -s)" = Darwin ] || { echo "package_mac.sh runs on macOS"; exit 1; }

ARCH=$(uname -m)
DMG="$DIST/Tempester-$VERSION-macos-$ARCH.dmg"
PKG="$DIST/Tempester-$VERSION-macos-$ARCH.pkg"
ID=io.github.onedevinap.tempester.pkg
PLUGIN_BUNDLE_ID=io.github.onedevinap.tempester-control
STAGE=$(mktemp -d)
MOUNT=$(mktemp -d)
WORK=$(mktemp -d)
trap 'hdiutil detach "$MOUNT" -quiet 2> /dev/null || true; rm -rf "$STAGE" "$MOUNT" "$WORK"' EXIT

BUNDLES=$(plugin_bundles)
# Both plugin bundles carry the plugin's own identifier (set by its xtask), never nih-plug's default one.
for format in clap vst3; do
  id=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$BUNDLES/$PLUGIN_NAME.$format/Contents/Info.plist")
  [ "$id" = "$PLUGIN_BUNDLE_ID" ] || { echo "$PLUGIN_NAME.$format: bundle identifier $id, $PLUGIN_BUNDLE_ID expected"; exit 1; }
done
"$P/tools/app_mac.sh" "$STAGE" > /dev/null

# ---- dmg: the app only
ln -s /Applications "$STAGE/Applications"
rm -f "$DMG"
hdiutil create -quiet -volname "Tempester $VERSION" -srcfolder "$STAGE" -ov -format UDZO "$DMG"

hdiutil verify -quiet "$DMG"
hdiutil attach -quiet -readonly -nobrowse -noverify -mountpoint "$MOUNT" "$DMG"
APP="$MOUNT/Tempester.app/Contents"
INSIDE=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Info.plist")
[ "$INSIDE" = "$VERSION" ] || { echo "version in the app ($INSIDE) differs from $VERSION"; exit 1; }
"$APP/MacOS/tempester" --version | grep -qx "tempester $VERSION" \
  || { echo "the app does not report version $VERSION"; exit 1; }
check_package "$DMG" 1000000

# ---- pkg: one component package per choice, then the product archive
# component <name> <install location> <bundle>: the payload holds the bundle only, installed under the location.
component() {
  local name="$1" location="$2" source="$3" root="$WORK/root-$1" plist="$WORK/$1.plist" i=0
  mkdir -p "$root"
  ditto "$source" "$root/$(basename "$source")"
  pkgbuild --analyze --root "$root" "$plist" > /dev/null
  # Relocation off and version check off, whatever the analysis wrote (a key may be absent, hence Add after Set).
  # Relocatable would update a copy found anywhere (a plugin installed by hand under ~/Library) instead of installing
  # to the location; version-checked would keep an older build when the plugin crate version did not change between
  # two releases.
  while /usr/libexec/PlistBuddy -c "Print :$i" "$plist" > /dev/null 2>&1; do
    for key in BundleIsRelocatable BundleIsVersionChecked; do
      /usr/libexec/PlistBuddy -c "Set :$i:$key false" "$plist" 2> /dev/null \
        || /usr/libexec/PlistBuddy -c "Add :$i:$key bool false" "$plist"
    done
    i=$((i + 1))
  done
  [ "$i" -gt 0 ] || { echo "no bundle found in the $name payload"; exit 1; }
  pkgbuild --root "$root" --component-plist "$plist" --identifier "$ID.$name" --version "$VERSION" \
    --install-location "$location" --ownership recommended "$WORK/$name.pkg" > /dev/null
}
component app /Applications "$STAGE/Tempester.app"
component clap /Library/Audio/Plug-Ins/CLAP "$BUNDLES/$PLUGIN_NAME.clap"
component vst3 /Library/Audio/Plug-Ins/VST3 "$BUNDLES/$PLUGIN_NAME.vst3"

mkdir -p "$WORK/resources"
licence_page "$WORK/resources/license.txt"
cp "$P/tools/macos/readme.txt" "$WORK/resources/readme.txt"
# customize="always": the choices page is always shown. localSystem only: every path is under /Applications or /Library.
cat > "$WORK/distribution.xml" <<DISTRIBUTION
<?xml version="1.0" encoding="utf-8"?>
<installer-gui-script minSpecVersion="2">
  <title>Tempester $VERSION</title>
  <license file="license.txt" mime-type="text/plain"/>
  <readme file="readme.txt" mime-type="text/plain"/>
  <options customize="always" require-scripts="false" hostArchitectures="$ARCH"/>
  <domains enable_anywhere="false" enable_currentUserHome="false" enable_localSystem="true"/>
  <choices-outline>
    <line choice="app"/>
    <line choice="clap"/>
    <line choice="vst3"/>
  </choices-outline>
  <choice id="app" title="Tempester (app)" description="The controller, librarian and updater, in /Applications.">
    <pkg-ref id="$ID.app"/>
  </choice>
  <choice id="clap" title="Tempester Control, CLAP plugin" description="DAW plugin, in /Library/Audio/Plug-Ins/CLAP.">
    <pkg-ref id="$ID.clap"/>
  </choice>
  <choice id="vst3" title="Tempester Control, VST3 plugin" description="DAW plugin, in /Library/Audio/Plug-Ins/VST3.">
    <pkg-ref id="$ID.vst3"/>
  </choice>
  <pkg-ref id="$ID.app" version="$VERSION">app.pkg</pkg-ref>
  <pkg-ref id="$ID.clap" version="$VERSION">clap.pkg</pkg-ref>
  <pkg-ref id="$ID.vst3" version="$VERSION">vst3.pkg</pkg-ref>
</installer-gui-script>
DISTRIBUTION
rm -f "$PKG"
productbuild --distribution "$WORK/distribution.xml" --package-path "$WORK" --resources "$WORK/resources" \
  "$PKG" > /dev/null

# `pkgutil --payload-files` lists only the first component of a product archive: expand it, read each component's bill of materials.
pkgutil --expand "$PKG" "$WORK/expanded"
# name | install location | bundle | the binary inside it
for entry in "app|/Applications|Tempester.app|tempester" \
  "clap|/Library/Audio/Plug-Ins/CLAP|$PLUGIN_NAME.clap|$PLUGIN_NAME" \
  "vst3|/Library/Audio/Plug-Ins/VST3|$PLUGIN_NAME.vst3|$PLUGIN_NAME"; do
  IFS='|' read -r name location bundle binary <<< "$entry"
  info="$WORK/expanded/$name.pkg/PackageInfo"
  grep -qF "install-location=\"$location\"" "$info" || { echo "wrong install location for $name"; exit 1; }
  grep -q 'relocatable="false"' "$info" || { echo "$name is relocatable"; exit 1; }
  payload=$(lsbom -s "$WORK/expanded/$name.pkg/Bom")
  for path in "./$bundle/Contents/Info.plist" "./$bundle/Contents/MacOS/$binary"; do
    grep -qxF "$path" <<< "$payload" || { echo "missing in the $name payload: $path"; exit 1; }
  done
done
check_package "$PKG" 1000000
