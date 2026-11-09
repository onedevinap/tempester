#!/bin/bash
# Builds two packages in dist/:
#   tempester_<version>_<arch>.deb          the app: release binary, desktop entry, icons
#   tempester-plugins_<version>_<arch>.deb  the DAW plugin "Tempester Control": CLAP in /usr/lib/clap, VST3 in
#                                           /usr/lib/vst3 (the folders every Linux DAW scans)
#
# usage: tools/package_linux.sh      (needs dpkg-deb; on the GitHub ubuntu runner it is there)
# Build libraries (Debian/Ubuntu): libwebkit2gtk-4.1-dev libgtk-3-dev libasound2-dev (see .github/workflows/ci.yml).
# TEMPESTER_BIN=<file> packages an existing binary instead of running cargo (layout tests off Linux).
#
# Why two packages: the plugin links no GUI toolkit, so a machine that only runs a DAW should not have to pull WebKitGTK
# and GTK; `sudo apt install ./tempester*.deb` installs both. The app only suggests the plugins.
# TEMPESTER_PLUGIN_BUNDLES=<folder> uses plugin bundles already built (tools/package_common.sh).
#
# AppImage is not built: it would have to bundle WebKitGTK, which is fragile, and needs a downloaded tool. The .deb
# declares the shared libraries instead; any distribution can also run the binary after installing them.
#
# The results are checked: the file, its size, the version in the name, the control fields, the file list, and on Linux
# the unpacked app binary must answer `--version`.
set -euo pipefail
# shellcheck source=tools/package_common.sh
. "$(dirname "$0")/package_common.sh"
command -v dpkg-deb > /dev/null || { echo "dpkg-deb not found: build the .deb on Debian or Ubuntu"; exit 1; }

ARCH=$(dpkg --print-architecture)
DEB="$DIST/tempester_${VERSION}_$ARCH.deb"
PLUGINS_DEB="$DIST/tempester-plugins_${VERSION}_$ARCH.deb"
ICONS="$P/app/crates/tempester/icons"
STAGE=$(mktemp -d)
PLUGIN_STAGE=$(mktemp -d)
CHECK=$(mktemp -d)
trap 'rm -rf "$STAGE" "$PLUGIN_STAGE" "$CHECK"' EXIT

# write_control <stage> <package> <depends> <suggests or empty> <summary> <long description>: DEBIAN/control, the licence
# notice and text as the Debian copyright file (the GPL-3 text is also at /usr/share/common-licenses/GPL-3 on Debian).
write_control() {
  local stage="$1" package="$2" depends="$3" suggests="$4" summary="$5" long="$6"
  install -d "$stage/usr/share/doc/$package"
  licence_page "$stage/usr/share/doc/$package/copyright"
  chmod 0644 "$stage/usr/share/doc/$package/copyright"
  install -d "$stage/DEBIAN"
  {
    echo "Package: $package"
    echo "Version: $VERSION"
    echo "Architecture: $ARCH"
    echo "Maintainer: Tempester <noreply@users.noreply.github.com>"
    echo "Section: sound"
    echo "Priority: optional"
    echo "Installed-Size: $(du -sk "$stage" | cut -f1)"
    echo "Depends: $depends"
    [ -z "$suggests" ] || echo "Suggests: $suggests"
    echo "Description: $summary"
    echo "$long"
  } > "$stage/DEBIAN/control"
}

BIN="${TEMPESTER_BIN:-}"
if [ -z "$BIN" ]; then
  (cd "$P/app" && cargo build --release --locked -p tempester > /dev/null)
  BIN="$P/app/target/release/tempester"
fi

install -D -m 0755 "$BIN" "$STAGE/usr/bin/tempester"
for size in 16 32 48 64 128 256 512; do
  install -D -m 0644 "$ICONS/png/tempest_$size.png" "$STAGE/usr/share/icons/hicolor/${size}x${size}/apps/tempester.png"
done
install -D -m 0644 "$ICONS/tempest.svg" "$STAGE/usr/share/icons/hicolor/scalable/apps/tempester.svg"
install -d "$STAGE/usr/share/applications"
cat > "$STAGE/usr/share/applications/tempester.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Tempester
Comment=Controller, librarian and updater for the DSI Tempest
Exec=tempester
Icon=tempester
Terminal=false
Categories=AudioVideo;Audio;Midi;
DESKTOP

# The WebKitGTK and ALSA package names changed with the 64-bit time_t transition (libasound2t64): accept both.
write_control "$STAGE" tempester "libwebkit2gtk-4.1-0, libgtk-3-0, libasound2t64 | libasound2" tempester-plugins \
  "Controller, librarian and updater for the DSI Tempest" \
  " MIDI only, no audio. Backs up and restores sounds, beats and projects, updates the
 operating system, and plays the machine from the computer. Nothing is sent to the
 machine without a confirmation in the app."

BUNDLES=$(plugin_bundles)
install -d -m 0755 "$PLUGIN_STAGE/usr/lib/clap" "$PLUGIN_STAGE/usr/lib/vst3"
cp -R "$BUNDLES/$PLUGIN_NAME.clap" "$PLUGIN_STAGE/usr/lib/clap/"
cp -R "$BUNDLES/$PLUGIN_NAME.vst3" "$PLUGIN_STAGE/usr/lib/vst3/"
find "$PLUGIN_STAGE/usr/lib" -type d -exec chmod 0755 {} +
find "$PLUGIN_STAGE/usr/lib" -type f -exec chmod 0644 {} +
find "$PLUGIN_STAGE/usr/lib" -type f \( -name '*.clap' -o -name '*.so' \) -exec chmod 0755 {} +
write_control "$PLUGIN_STAGE" tempester-plugins "libc6, libgcc-s1" tempester \
  "Tempester Control, DAW plugin (CLAP and VST3) for the DSI Tempest" \
  " A MIDI effect whose parameters are the Tempest sound parameters: automation or a
 learned controller goes out on the plugin MIDI output, for the DAW to route to the
 machine. It opens no MIDI port. Needs a Tempester Main OS that applies NRPN (1.5.3.0
 or later)."

rm -f "$DEB"
dpkg-deb --root-owner-group --build "$STAGE" "$DEB" > /dev/null
rm -f "$PLUGINS_DEB"
dpkg-deb --root-owner-group --build "$PLUGIN_STAGE" "$PLUGINS_DEB" > /dev/null

[ "$(dpkg-deb --field "$DEB" Version)" = "$VERSION" ] || { echo "control version differs from $VERSION"; exit 1; }
[ "$(dpkg-deb --field "$DEB" Package)" = tempester ] || { echo "control package name wrong"; exit 1; }
CONTENTS=$(dpkg-deb --contents "$DEB")
for path in ./usr/bin/tempester ./usr/share/applications/tempester.desktop \
  ./usr/share/icons/hicolor/256x256/apps/tempester.png; do
  grep -q " $path\$" <<< "$CONTENTS" || { echo "missing in the package: $path"; exit 1; }
done
PLUGIN_CONTENTS=$(dpkg-deb --contents "$PLUGINS_DEB")
[ "$(dpkg-deb --field "$PLUGINS_DEB" Version)" = "$VERSION" ] || { echo "plugins control version differs from $VERSION"; exit 1; }
[ "$(dpkg-deb --field "$PLUGINS_DEB" Package)" = tempester-plugins ] || { echo "plugins control package name wrong"; exit 1; }
for pattern in " \./usr/lib/clap/$PLUGIN_NAME\.clap\$" " \./usr/lib/vst3/$PLUGIN_NAME\.vst3/Contents/[^/]+/$PLUGIN_NAME\.so\$" \
  " \./usr/share/doc/tempester-plugins/copyright\$"; do
  grep -Eq "$pattern" <<< "$PLUGIN_CONTENTS" || { echo "missing in the plugins package: $pattern"; exit 1; }
done
if [ "$(uname -s)" = Linux ] && [ "$(uname -m)" = "$(dpkg --print-architecture | sed 's/amd64/x86_64/; s/arm64/aarch64/')" ]; then
  dpkg-deb --extract "$DEB" "$CHECK"
  "$CHECK/usr/bin/tempester" --version | grep -qx "tempester $VERSION" \
    || { echo "the packaged binary does not report version $VERSION"; exit 1; }
fi
check_package "$DEB" 500000
check_package "$PLUGINS_DEB" 100000
