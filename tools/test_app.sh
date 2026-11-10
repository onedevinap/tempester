#!/bin/bash
# Checks the Rust workspace of the Tempester app (app/) and the plugin crate beside it: generated tables up to date,
# formatting, clippy, tests.
#
# usage: tools/test_app.sh [<folder of the DSI .syx files>]
# Tests run with a disposable HOME (testing rules: never the user's real home). With a folder, the `#[ignore]` tests
# on the real DSI files run too (TEMPESTER_DSI_SYX): round trips, SHA-256, build of 1.5.5.0 compared with
# tools/build_version.py, and the 16 factory projects cut like a Send Project (TEMPESTER_FACTORY_PROJECTS, taken
# from the folder next to the OS folder when it exists), and the exports of 1.6.1.0 / 1.6.1.2 simulated by
# the firmware simulations (not in the public tree) when TEMPESTER_FIXED_EXPORT and TEMPESTER_STOCK_EXPORT are set
# (tools/verify.sh --long).
# Nothing is ever sent over MIDI by these tests (they use the in-memory fake).
# CARGO_NET_OFFLINE=true makes cargo use only its local cache.
# Runs on macOS, Linux and Windows (Git Bash, as on the GitHub runner). Windows has no HOME for the app: it reads
# %APPDATA%, so that one is pointed at the disposable folder too (data_dir.rs refuses anything outside the temp folder).
set -euo pipefail
P="$(cd "$(dirname "$0")/.." && pwd)"
# Pin the toolchain folders before HOME is replaced (rustup looks for them under HOME). A CI runner already sets them
# and has cargo on the PATH; a Git Bash PATH must not receive a Windows path.
[ -n "${RUSTUP_HOME:-}" ] || { [ -d "$HOME/.rustup" ] && export RUSTUP_HOME="$HOME/.rustup"; } || true
[ -n "${CARGO_HOME:-}" ] || { [ -d "$HOME/.cargo" ] && export CARGO_HOME="$HOME/.cargo"; } || true
if ! command -v cargo > /dev/null; then
  export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
fi
PY=python3
command -v python3 > /dev/null || PY=python
REAL_HOME="$HOME" # only to find the browsers of the layout test below: nothing of it is read or written
HOME=$(mktemp -d)
export HOME
unset XDG_DATA_HOME TEMPESTER_DATA_DIR
if command -v cygpath > /dev/null; then
  APPDATA=$(cygpath -w "$HOME")
  export APPDATA
fi
T=$(mktemp -d)
trap 'rm -rf "$HOME" "$T"' EXIT
ok() { printf '  ✔ %s\n' "$1"; }
cd "$P/app"

# Not "check && ok": under set -e a failing command on the left of && does not stop the script.
$PY -I "$P/tools/generate_rust_tables.py" --check || { echo "  ✘ generated tables out of date"; exit 1; }
ok "generated tables up to date"
# Counter-proof: a hand-edited generated file must be refused.
$PY -I "$P/tools/generate_rust_tables.py" --out "$T/gen" > /dev/null
echo "// hand edit" >> "$T/gen/params.rs"
if $PY -I "$P/tools/generate_rust_tables.py" --check --out "$T/gen" > /dev/null 2>&1; then
  echo "  ✘ edited generated table accepted"; exit 1
fi
ok "edited generated table refused"
cargo fmt --check || { echo "  ✘ cargo fmt: the code is not formatted (run cargo fmt)"; exit 1; }
ok "cargo fmt"
cargo clippy --all-targets -- -D warnings 2> "$T/clippy.log" || { cat "$T/clippy.log"; exit 1; }
ok "cargo clippy -D warnings"
cargo test 2> "$T/test.log" > "$T/test.out" || { cat "$T/test.out" "$T/test.log"; exit 1; }
ok "cargo test ($(grep -h '^test result' "$T/test.out" | sed 's/test result: ok. //; s/;.*//' | paste -sd+ -) )"

# Tempester Control (CLAP / VST3 plugin): its own workspace (nih-plug git dependency, own lock file and target/), so the
# same three checks run there. The bundles and the headless CLAP host are built by the `plugin` job of the CI and by
# hand (app/crates/tempester-plugin/README or Docs/plans/plugin.md): a release build of nih-plug takes minutes.
(cd "$P/app/crates/tempester-plugin" && cargo fmt --check) || { echo "  ✘ plugin: cargo fmt (run cargo fmt there)"; exit 1; }
(cd "$P/app/crates/tempester-plugin" && cargo clippy --all-targets -- -D warnings 2> "$T/clippy-plugin.log") \
  || { cat "$T/clippy-plugin.log"; exit 1; }
ok "plugin: cargo fmt, clippy -D warnings"
(cd "$P/app/crates/tempester-plugin" && cargo test 2> "$T/test-plugin.log" > "$T/test-plugin.out") \
  || { cat "$T/test-plugin.out" "$T/test-plugin.log"; exit 1; }
ok "plugin: cargo test ($(grep -h '^test result' "$T/test-plugin.out" | sed 's/test result: ok. //; s/;.*//' | paste -sd+ -) )"

if command -v node > /dev/null; then
  # The page scripts are plain classic scripts: a syntax error would only show when the window opens.
  node -e '
    const fs = require("fs"), vm = require("vm"), dir = process.argv[1];
    const bad = fs.readdirSync(dir).filter((f) => f.endsWith(".js")).filter((f) => {
      try { new vm.Script(fs.readFileSync(dir + "/" + f, "utf8"), { filename: f }); return false; }
      catch (e) { console.log(f + ": " + e.message); return true; }
    });
    process.exit(bad.length ? 1 : 0);' "$P/app/crates/tempester/ui" && ok "page scripts parse (node)"
  UI="$P/app/crates/tempester/ui"
  # The shared icon module and its icon buttons (a button without a label is refused).
  node "$P/app/crates/tempester/ui-tests/icons.test.js" "$UI" > "$T/icons.log" \
    || { cat "$T/icons.log"; exit 1; }
  ok "shared icons ($(cat "$T/icons.log"))"
  # The remote-panel script on a fake DOM, then its counter-proof: every released-key path removed in turn must make
  # the same checks fail.
  node "$P/app/crates/tempester/ui-tests/panel-remote.test.js" "$UI/panel-remote.js" "$UI/panel-keys.js" \
    || { echo "  ✘ remote panel script"; exit 1; }
  ok "remote panel script (node)"
  node "$P/app/crates/tempester/ui-tests/counter-proof.js" "$UI" "$T" > "$T/counter.log" \
    || { cat "$T/counter.log"; exit 1; }
  ok "broken remote panel scripts refused ($(cat "$T/counter.log"))"
  # The Library manager on a fake DOM: the logic (search, filters, sort, selection, name rules), the page with 320
  # sounds (search as you type, filters, keyboard, multi-select and its commands), the review of a batch, the look at
  # the machine before a send, the card commands and the fixed geometry; then the same checks on copies with one
  # behaviour broken, which must fail.
  node "$P/app/crates/tempester/ui-tests/library-browser.test.js" "$UI" > "$T/library.log" \
    || { cat "$T/library.log"; exit 1; }
  ok "library manager ($(cat "$T/library.log"))"
  node "$P/app/crates/tempester/ui-tests/library-counter-proof.js" "$UI" "$T" > "$T/library-counter.log" \
    || { cat "$T/library-counter.log"; exit 1; }
  ok "library manager with one behaviour broken refused ($(cat "$T/library-counter.log"))"
  # Sending a whole project back: the send action, the confirmations, the version guard (the warning in full, no plain
  # send when the OS may misread Play Lists), the progress and cancel, the fetch-back and its answer; then 22 broken
  # copies that must fail.
  node "$P/app/crates/tempester/ui-tests/project-send.test.js" "$UI" > "$T/projsend.log" \
    || { cat "$T/projsend.log"; exit 1; }
  ok "project send ($(cat "$T/projsend.log"))"
  node "$P/app/crates/tempester/ui-tests/project-send-counter-proof.js" "$UI" "$T" > "$T/projsend-counter.log" \
    || { cat "$T/projsend-counter.log"; exit 1; }
  ok "project send with one behaviour broken refused ($(cat "$T/projsend-counter.log"))"
  # The kit builder (library sounds on the pads of the machine's beat: the choices, the dialog, the check then the
  # apply with the token) and the MIDI export buttons; then 12 broken copies that must fail.
  node "$P/app/crates/tempester/ui-tests/lib-kit.test.js" "$UI" > "$T/libkit.log" \
    || { cat "$T/libkit.log"; exit 1; }
  ok "kit builder and MIDI export ($(cat "$T/libkit.log"))"
  node "$P/app/crates/tempester/ui-tests/lib-kit-counter-proof.js" "$UI" "$T" > "$T/libkit-counter.log" \
    || { cat "$T/libkit-counter.log"; exit 1; }
  ok "kit builder with one behaviour broken refused ($(cat "$T/libkit-counter.log"))"
  node "$P/app/crates/tempester/ui-tests/conn-bar.test.js" "$UI" > "$T/connbar.log" \
    || { cat "$T/connbar.log"; exit 1; }
  ok "top-bar connection controls ($(cat "$T/connbar.log"))"
  # Tooltip and popover placement (above with a gap, flip, clamp, focus, Escape), the tab order (Panel first, tools
  # apart, Panel by default) and the identity answer shown in full: each followed by its broken copies, which must fail.
  node "$P/app/crates/tempester/ui-tests/tooltip.test.js" "$UI" > "$T/tooltip.log" \
    || { cat "$T/tooltip.log"; exit 1; }
  ok "tooltip and popover placement ($(cat "$T/tooltip.log"))"
  node "$P/app/crates/tempester/ui-tests/nav-order.test.js" "$UI" > "$T/nav.log" \
    || { cat "$T/nav.log"; exit 1; }
  ok "tab order ($(cat "$T/nav.log"))"
  node "$P/app/crates/tempester/ui-tests/identity-panel.test.js" "$UI" > "$T/identity.log" \
    || { cat "$T/identity.log"; exit 1; }
  ok "identity panel ($(cat "$T/identity.log"))"
  # The compact Machine, Backup, OS update and Play pages: every command line of the old pages still goes out and the
  # risky confirmations keep their text; then the same checks on copies with one command or confirmation removed.
  node "$P/app/crates/tempester/ui-tests/compact-pages.test.js" "$UI" > "$T/compact.log" 2>&1 \
    || { cat "$T/compact.log"; exit 1; }
  ok "compact pages ($(cat "$T/compact.log"))"
  node "$P/app/crates/tempester/ui-tests/compact-counter-proof.js" "$UI" "$T" > "$T/compact-counter.log" 2>&1 \
    || { cat "$T/compact-counter.log"; exit 1; }
  ok "compact pages without a command or a confirmation refused ($(tail -n 1 "$T/compact-counter.log"))"
  # The reference-hash flows of the OS page (built image against its reference, known file, unknown build and its extra
  # confirmation, refused build, read-back of the installed OS, installed line), then 20 broken copies that must fail.
  node "$P/app/crates/tempester/ui-tests/os-reference.test.js" "$UI" > "$T/osref.log" 2>&1 \
    || { cat "$T/osref.log"; exit 1; }
  ok "OS reference hashes ($(cat "$T/osref.log"))"
  node "$P/app/crates/tempester/ui-tests/os-reference-counter-proof.js" "$UI" "$T" > "$T/osref-counter.log" 2>&1 \
    || { cat "$T/osref-counter.log"; exit 1; }
  ok "OS reference flows with one behaviour broken refused ($(tail -n 1 "$T/osref-counter.log"))"
  # The version chooser of the OS page (three cards, details, history of every release, install of an older one), then
  # 17 broken copies that must fail.
  node "$P/app/crates/tempester/ui-tests/os-versions.test.js" "$UI" > "$T/osver.log" 2>&1 \
    || { cat "$T/osver.log"; exit 1; }
  ok "OS version chooser ($(cat "$T/osver.log"))"
  node "$P/app/crates/tempester/ui-tests/os-versions-counter-proof.js" "$UI" "$T" > "$T/osver-counter.log" 2>&1 \
    || { cat "$T/osver-counter.log"; exit 1; }
  ok "OS version chooser with one behaviour broken refused ($(tail -n 1 "$T/osver-counter.log"))"
  # The Panel page's logic (Pad Function mode from the LEDs, Machine-view controls, view and zoom), then its counter-proof.
  node "$P/app/crates/tempester/ui-tests/panel-machine.test.js" "$UI" "$P/refs/panel_controls.tsv" \
    || { echo "  ✘ panel page logic"; exit 1; }
  ok "panel page logic: mode, Machine view, view and zoom (node)"
  node "$P/app/crates/tempester/ui-tests/counter-proof-view.js" "$UI" "$T" "$P/refs/panel_controls.tsv" > "$T/counter-view.log" \
    || { cat "$T/counter-view.log"; exit 1; }
  ok "broken panel page scripts refused ($(cat "$T/counter-view.log"))"
  # The editing tools of the Panel page (undo / redo, A/B, variations, MIDI learn): what each button sends, the values
  # redrawn, a learned controller moving its control through the knob path, learn mode picking a control without
  # changing it; then broken copies that must fail.
  node "$P/app/crates/tempester/ui-tests/edit-tools.test.js" "$UI" > "$T/edit-tools.log" 2>&1 \
    || { cat "$T/edit-tools.log"; exit 1; }
  ok "panel editing tools ($(tail -n 1 "$T/edit-tools.log"))"
  # The 1.6.2.0 / 1.6.3.0 performance controls (beat-wide knobs, FX ribbons, pad probability, which OS gets them),
  # then broken copies that must fail.
  node "$P/app/crates/tempester/ui-tests/panel-perform.test.js" "$UI" > "$T/perform.log" 2>&1 \
    || { cat "$T/perform.log"; exit 1; }
  ok "performance controls ($(cat "$T/perform.log"))"
  node "$P/app/crates/tempester/ui-tests/panel-perform-counter-proof.js" "$UI" "$T" > "$T/perform-counter.log" 2>&1 \
    || { cat "$T/perform-counter.log"; exit 1; }
  ok "performance controls with one behaviour broken refused ($(tail -n 1 "$T/perform-counter.log"))"
  # The n°38 reports (Tempester 1.6.4.0: the machine's mixer, delay and main values moving the Machine view's knobs
  # and encoder bubbles and the Full view's Volume, Distort and Compress, never under the user's hand; what Distort
  # and Compress send), then broken copies that must fail.
  node "$P/app/crates/tempester/ui-tests/panel-mixer.test.js" "$UI" > "$T/mixer.log" 2>&1 \
    || { cat "$T/mixer.log"; exit 1; }
  ok "mixer reports ($(cat "$T/mixer.log"))"
  node "$P/app/crates/tempester/ui-tests/panel-mixer-counter-proof.js" "$UI" "$T" > "$T/mixer-counter.log" 2>&1 \
    || { cat "$T/mixer-counter.log"; exit 1; }
  ok "mixer reports with one behaviour broken refused ($(tail -n 1 "$T/mixer-counter.log"))"
  # No control of either view may move or change size, whatever the page shows (Chrome and WebKit, the standalone
  # preview, nothing is sent); then the same test on broken copies, which must fail. Needs playwright-core (not a
  # dependency of the project): PLAYWRIGHT_CORE, or the author's npx cache.
  PW="${PLAYWRIGHT_CORE:-$REAL_HOME/.npm/_npx/9833c18b2d85bc59/node_modules/playwright-core}"
  if [ -d "$PW" ]; then
    export PLAYWRIGHT_CORE="$PW" PLAYWRIGHT_BROWSERS_PATH="${PLAYWRIGHT_BROWSERS_PATH:-$REAL_HOME/Library/Caches/ms-playwright}"
    node "$P/app/crates/tempester/ui-tests/layout-stability.js" "$UI" "$T/layout" "$P" > "$T/layout.log" 2>&1 \
      || { cat "$T/layout.log"; echo "  ✘ layout stability"; exit 1; }
    ok "layout stability, Full and Machine views, Chrome and WebKit ($(tail -1 "$T/layout.log"))"
    # The browser counter-proofs re-render many broken copies (about 15 min): run by `verify.sh --long`
    # (TEMPESTER_LONG=1), and by hand before changing the Panel files.
    if [ "${TEMPESTER_LONG:-0}" = "1" ]; then
      node "$P/app/crates/tempester/ui-tests/layout-counter-proof.js" "$UI" "$T/layout-broken" "$P" > "$T/layout-counter.log" 2>&1 \
        || { cat "$T/layout-counter.log"; echo "  ✘ layout counter-proof"; exit 1; }
      ok "layout shifts caught ($(tail -1 "$T/layout-counter.log"))"
    fi
    # No text on the face may overlap another text, a control (knob and ring, key and LED, pad, screen) or a printed
    # line: both views, 1100 / 1320 / 1920 px, Chrome and WebKit; then the named defects put back, which must fail.
    node "$P/app/crates/tempester/ui-tests/layout-overlap.js" "$UI" "$T/overlap" "$P" > "$T/overlap.log" 2>&1 \
      || { cat "$T/overlap.log"; echo "  ✘ text overlaps"; exit 1; }
    ok "no overlapping text, both views, 3 widths, Chrome and WebKit ($(tail -1 "$T/overlap.log"))"
    if [ "${TEMPESTER_LONG:-0}" = "1" ]; then
      node "$P/app/crates/tempester/ui-tests/layout-overlap-counter-proof.js" "$UI" "$T/overlap-broken" "$P" > "$T/overlap-counter.log" 2>&1 \
        || { cat "$T/overlap-counter.log"; echo "  ✘ overlap counter-proof"; exit 1; }
      ok "text overlaps caught ($(tail -1 "$T/overlap-counter.log"))"
    else
      echo "  - layout and overlap counter-proofs: run by verify.sh --long (TEMPESTER_LONG=1)"
    fi
  else
    echo "  - playwright-core not found (set PLAYWRIGHT_CORE): the layout stability test is skipped"
  fi
else
  echo "  - node not found: page scripts not syntax-checked"
fi

if [ -n "${1:-}" ]; then
  # The factory project files (tests/real_projects.rs) sit next to the OS folder on the author's disk; without them
  # that one test is skipped, said out loud.
  FACTORY="$1/../Tempest_Factory_Projects&Sounds_1.4/Tempest_Factory_Projects_1.4"
  SKIP=(--skip factory_projects)
  if [ -d "$FACTORY" ]; then
    TEMPESTER_FACTORY_PROJECTS="$(cd "$FACTORY" && pwd)"
    export TEMPESTER_FACTORY_PROJECTS
    SKIP=()
  else
    echo "  - factory project files not found next to the OS folder: their test is skipped"
  fi
  # The project captured from the machine (tests/real_capture.rs): the author's data, never in the repository.
  CAPTURE="${TEMPESTER_PROJECT_CAPTURE:-$P/work/captures/projet_machine_2026-11-08.syx}"
  if [ -f "$CAPTURE" ]; then
    export TEMPESTER_PROJECT_CAPTURE="$CAPTURE"
  else
    echo "  - captured machine project not found ($CAPTURE): its tests are skipped"
    SKIP+=(--skip real_capture --skip real_project_capture)
  fi
  # The exports of 1.6.1.0 and 1.6.1.2 simulated by the firmware simulations (tests/fixed_export.rs): built from the
  # DSI image by tools/verify.sh --long where the simulators are present, which sets both variables; otherwise their
  # tests are skipped.
  if [ -f "${TEMPESTER_FIXED_EXPORT:-}" ] && [ -f "${TEMPESTER_STOCK_EXPORT:-}" ]; then
    export TEMPESTER_FIXED_EXPORT TEMPESTER_STOCK_EXPORT
  else
    echo "  - simulated exports not given (TEMPESTER_FIXED_EXPORT, TEMPESTER_STOCK_EXPORT): their tests are skipped"
    SKIP+=(--skip simulated_export)
  fi
  TEMPESTER_DSI_SYX="$(cd "$1" && pwd)" cargo test -- --ignored ${SKIP[@]+"${SKIP[@]}"} > "$T/real.out" 2>&1 || { cat "$T/real.out"; exit 1; }
  ok "real DSI files ($(grep -h '^test result' "$T/real.out" | sed 's/test result: ok. //; s/;.*//' | paste -sd+ -) )"
  # Name each real-data test that passed: a skipped one must not pass for a run one.
  grep -h '^test .* ok$' "$T/real.out" | sed 's/^test /      /; s/ \.\.\. ok$//'
fi
