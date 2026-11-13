#!/bin/bash
# Builds two packages in dist/:
#   tempester_<version>_<arch>[_<distro>].deb          the app: release binary, desktop entry, icons
#   tempester-plugins_<version>_<arch>[_<distro>].deb  the DAW plugin "Tempester Control": CLAP in /usr/lib/clap, VST3
#                                                      in /usr/lib/vst3 (the folders every Linux DAW scans)
#
# usage: [TEMPESTER_DISTRO=ubuntu24.04] tools/package_linux.sh   (needs dpkg-deb, and dpkg-shlibdeps on Linux)
# Build libraries (Debian/Ubuntu): libwebkit2gtk-4.1-dev libgtk-3-dev libasound2-dev dpkg-dev (see
# .github/workflows/release.yml).
# TEMPESTER_DISTRO=<id><release> (ubuntu24.04, ubuntu26.04) ends both file names with `_<distro>`, so the debs built on
# two releases can sit side by side; on Linux it must be the build system's own ID and VERSION_ID (/etc/os-release).
# The package name and version inside the debs do not change. Unset (the default), the names have no suffix.
# TEMPESTER_BIN=<file> packages an existing binary instead of running cargo (layout tests off Linux).
#
# The Depends of both packages are computed by dpkg-shlibdeps from the libraries the app binary and the plugin
# libraries link on the build system, so each release gets its own package names (libasound2t64, libgtk-3-0t64 after
# the 64-bit time_t transition, the WebKitGTK 4.1 runtime) and minimum versions (libc6): apt refuses a system older
# than the build system. Off Linux (layout tests only, never published) fixed lists stand in.
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
DISTRO="${TEMPESTER_DISTRO:-}"
SUFFIX=""
if [ -n "$DISTRO" ]; then
  [[ "$DISTRO" =~ ^[a-z]+[0-9][0-9.]*$ ]] || { echo "TEMPESTER_DISTRO must look like ubuntu24.04: $DISTRO"; exit 1; }
  if [ "$(uname -s)" = Linux ]; then
    # shellcheck disable=SC1091
    OS_RELEASE=$(. /etc/os-release && echo "$ID$VERSION_ID")
    [ "$OS_RELEASE" = "$DISTRO" ] || { echo "TEMPESTER_DISTRO=$DISTRO but this system is $OS_RELEASE"; exit 1; }
  fi
  SUFFIX="_$DISTRO"
fi
DEB="$DIST/tempester_${VERSION}_$ARCH$SUFFIX.deb"
PLUGINS_DEB="$DIST/tempester-plugins_${VERSION}_$ARCH$SUFFIX.deb"
ICONS="$P/app/crates/tempester/icons"
STAGE=$(mktemp -d)
PLUGIN_STAGE=$(mktemp -d)
CHECK=$(mktemp -d)
SHLIBS=$(mktemp -d)
trap 'rm -rf "$STAGE" "$PLUGIN_STAGE" "$CHECK" "$SHLIBS"' EXIT

# shlib_depends <package> <fallback> <ELF file>...: the Depends line of a package, computed by dpkg-shlibdeps from the
# libraries the files link on the build system. It carries minimum versions (`libc6 (>= 2.39)`), so apt refuses a
# system older than the build system instead of installing a binary that fails with « GLIBC_x not found ».
# dpkg-shlibdeps needs a debian/control naming the package; it prints `shlibs:Depends=...`. Off Linux (layout tests
# only, never published) it prints <fallback>.
shlib_depends() {
  local package="$1" fallback="$2" depends
  shift 2
  if [ "$(uname -s)" != Linux ]; then
    echo "$fallback"
    return
  fi
  command -v dpkg-shlibdeps > /dev/null || { echo "dpkg-shlibdeps not found: install dpkg-dev" >&2; exit 1; }
  install -d "$SHLIBS/debian"
  printf 'Source: tempester\n\nPackage: %s\nArchitecture: any\n' "$package" > "$SHLIBS/debian/control"
  depends=$(cd "$SHLIBS" && dpkg-shlibdeps -O "$@" | sed -n 's/^shlibs:Depends=//p')
  [[ "$depends" == *libc6* ]] || { echo "no libc6 in the computed Depends of $package: $depends" >&2; exit 1; }
  echo "$depends"
}

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

APP_DEPENDS=$(shlib_depends tempester "libc6, libwebkit2gtk-4.1-0, libgtk-3-0t64 | libgtk-3-0, libasound2t64 | libasound2" \
  "$STAGE/usr/bin/tempester")
echo "tempester Depends: $APP_DEPENDS"
# Without WebKitGTK 4.1 and ALSA in the list, the binary was not the real app.
[[ "$APP_DEPENDS" == *libwebkit2gtk-4.1* ]] || { echo "no WebKitGTK 4.1 in the app Depends: $APP_DEPENDS"; exit 1; }
[[ "$APP_DEPENDS" == *libasound2* ]] || { echo "no ALSA in the app Depends: $APP_DEPENDS"; exit 1; }
write_control "$STAGE" tempester "$APP_DEPENDS" tempester-plugins \
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
# The CLAP file and the VST3 .so are the plugin's two ELF libraries.
PLUGIN_LIBS=()
while IFS= read -r -d '' lib; do
  PLUGIN_LIBS+=("$lib")
done < <(find "$PLUGIN_STAGE/usr/lib" -type f \( -name '*.clap' -o -name '*.so' \) -print0)
[ "${#PLUGIN_LIBS[@]}" -eq 2 ] || { echo "expected the CLAP and VST3 libraries, found ${#PLUGIN_LIBS[@]}"; exit 1; }
PLUGIN_DEPENDS=$(shlib_depends tempester-plugins "libc6, libgcc-s1" "${PLUGIN_LIBS[@]}")
echo "tempester-plugins Depends: $PLUGIN_DEPENDS"
write_control "$PLUGIN_STAGE" tempester-plugins "$PLUGIN_DEPENDS" tempester \
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
