#!/bin/bash
# Shared by tools/package_mac.sh and tools/package_linux.sh (sourced, not run): version, output folder, output check,
# plugin bundles.
#
# Sets P (repository root), VERSION (workspace version of app/Cargo.toml) and DIST (output folder, `dist/`, never
# versioned). `check_package <file> [<minimum size in bytes>]` fails unless the file exists, is big enough and carries the
# version in its name. `plugin_bundles` prints the folder holding "Tempester Control.clap" and ".vst3" (see below).
# `licence_page <file>` writes the licence text the installers show: the notice (GPL version 3 or any later version,
# tools/licence-notice.txt) followed by LICENSE, the GPL-3.0 text, kept verbatim.
# Nothing DSI ever goes into a package: only the release binaries, the icons, the licence and the desktop entry.

P="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' "$P/app/Cargo.toml" | head -1)
[ -n "$VERSION" ] || { echo "no version in app/Cargo.toml"; exit 1; }
DIST="${TEMPESTER_DIST:-$P/dist}"
mkdir -p "$DIST"

check_package() {
  local file="$1" minimum="${2:-1000000}" size
  [ -f "$file" ] || { echo "package missing: $file"; exit 1; }
  size=$(wc -c < "$file" | tr -d ' ')
  [ "$size" -ge "$minimum" ] || { echo "package too small ($size bytes < $minimum): $file"; exit 1; }
  case "$(basename "$file")" in
    *"$VERSION"*) ;;
    *) echo "version $VERSION not in the file name: $file"; exit 1 ;;
  esac
  echo "$file ($size bytes, version $VERSION)"
}

PLUGIN_NAME="Tempester Control"

licence_page() {
  cat "$P/tools/licence-notice.txt" "$P/LICENSE" > "$1"
}

# Prints the folder holding the plugin bundles (`Tempester Control.clap` and `.vst3`). The plugin is an isolated crate
# (own workspace, lock file and target/): it is built here with `cargo xtask bundle`, `--locked` so a package never
# resolves a dependency the lock file does not name. TEMPESTER_PLUGIN_BUNDLES=<folder> uses bundles already built
# (CI builds them once for the packages of an OS; layout tests). Fails unless both bundles are there.
plugin_bundles() {
  local dir="${TEMPESTER_PLUGIN_BUNDLES:-}"
  if [ -z "$dir" ]; then
    export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
    export PATH="$CARGO_HOME/bin:$PATH"
    (cd "$P/app/crates/tempester-plugin" && cargo xtask bundle tempester-plugin --release --locked >&2)
    dir="$P/app/crates/tempester-plugin/target/bundled"
  fi
  [ -e "$dir/$PLUGIN_NAME.clap" ] || { echo "no CLAP bundle in $dir" >&2; exit 1; }
  [ -e "$dir/$PLUGIN_NAME.vst3" ] || { echo "no VST3 bundle in $dir" >&2; exit 1; }
  echo "$dir"
}
