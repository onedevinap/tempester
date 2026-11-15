#!/bin/bash
# Full check of the repository, to run before every commit. Red at the first failure (non-zero exit code).
#
# usage: tools/verify.sh <folder of the DSI .syx files> [--long | --scope <base ref>] [--no-cache]
#   --long: adds the counter-proof of patch n°8 (loop_sim_patch8.py, about 5 min) and the simulation of patch n°15
#   --scope <base ref>: for an agent's commit in its own copy of the repository: when nothing changed since <base ref>
#      can reach the app step (tools/verify_scope.py: documentation and tools the app never runs), step 5 is skipped
#      and the last line says so instead of "all green". The merged tree always gets the full run before a push.
#      When every change is a page file of the app (ui/, ui-tests/) or documentation, only what a page file reaches
#      runs (tools/verify_ui_scope.sh: tools compile, the Python tests that read the page, the page tests and the Rust
#      tests of src/ui, through the registry: only the page tests that read a changed file run), and the last line is
#      "all green".
#   --no-cache (anywhere): no result of an earlier run is reused (VERIFY_CACHE=cold): for a public release or before an
#      OS goes to the machine. Otherwise (the run before a push included) a simulation, a node page test, a browser layout
#      test or a Rust test binary whose every traced input is byte-identical to a green run is not run again, marked
#      "(cached, key …)": tools/verify_cache.sh for the simulations, tools/verify_cache_app.py for the app step.
# Checks:
#   1. round trip dsi_unpack -> dsi_pack identical to the byte on the four stock OSes;
#   2. every version of versions/*.json and versions/archive/*.json (test builds, kept out of the app) builds with all its checks (build_version.py), including its
#      recorded hashes sha256_image / sha256_image_written / sha256_syx (stale or edited: red; new version: build_version.py --record);
#   3. counter-proofs of build_version.py: two patches on the same address, flasher page touched, hash
#      altered or missing -> refused (the message names the manifest and the field);
#   3b. every patch is a canonical recipe (classify_patches.py --check) and nothing publishable holds DSI firmware
#      content (dsi_content_guard.py on the default tree), with the guard's counter-proofs;
#   3c. Voice recipes: a test recipe written and built as a Voice version, the forbidden writes refused
#      (voice_recipe_counter_proofs.py); patch/voice/ canonical and versions/voice/ built, when they exist; the
#      Voice sources rebuilt to their recipes by the hand encoder (pic24_asm.py, self-test on DSI's image), the
#      release table regenerated, and every Voice version's fixes proved with counter-proofs (sim_voice_tempester.py;
#      --long: whole releases of every index of the five envelopes, about 4 min);
#   4. the Python tools compile, tempest_midi.swift compiles (nothing is sent to the machine), tests/test_*.py pass;
#   5. Rust workspace app/ (tools/test_app.sh): generated tables up to date, cargo fmt, clippy -D warnings, tests, and the
#      #[ignore] tests on the real DSI files in <folder> (TEMPESTER_DSI_SYX), with a disposable HOME, built in this copy's
#      own app/target; the node page tests, the browser layout tests and each Rust test binary go through the registry of
#      proven results (tools/verify_cache_app.py: a CSS change re-runs the tests that read it, a change in one crate the
#      test binaries that link it);
#   6.-13. option --long: the firmware simulations and their counter-proofs (tools/verify_simulations.sh, private:
#      they run the image's code at DSI's routine addresses; the public tree has no simulator and says so).
# Works in a temporary folder; writes nothing in the repository outside work/ (decoded images, not versioned).
set -euo pipefail
P="$(cd "$(dirname "$0")/.." && pwd)"
ARGS=()
for a in "$@"; do if [ "$a" = --no-cache ]; then export VERIFY_CACHE=cold; else ARGS+=("$a"); fi; done
# Set before the scope dispatch: the UI scope (tools/verify_ui_scope.sh, which ends the run) reaches test_app.sh, whose
# app tests go through the registry only when VERIFY_CACHE is on or cold.
export VERIFY_CACHE=${VERIFY_CACHE:-on}
set -- ${ARGS[@]+"${ARGS[@]}"}
SYX=${1:?"folder of the DSI .syx files expected"}
SCOPE=full
if [ "${2:-}" = "--scope" ]; then
  python3 -I "$P/tools/verify_scope.py" "$P" "${3:?"base ref expected after --scope"}" > "${TMPDIR:-/tmp}/verify_scope.$$" \
    || { cat "${TMPDIR:-/tmp}/verify_scope.$$"; rm -f "${TMPDIR:-/tmp}/verify_scope.$$"; exit 1; }
  SCOPE=$(head -1 "${TMPDIR:-/tmp}/verify_scope.$$")
  echo "scope since $3: $SCOPE"; tail -n +2 "${TMPDIR:-/tmp}/verify_scope.$$"
  rm -f "${TMPDIR:-/tmp}/verify_scope.$$"
fi
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
ok() { printf '  ✔ %s\n' "$1"; }
# shellcheck source=tools/verify_ui_scope.sh
if [ "$SCOPE" = ui ]; then . "$P/tools/verify_ui_scope.sh"; fi
# The firmware simulations (tools/verify_simulations.sh) run the code of the user's own image at the addresses of
# DSI's routines: they stay in the private repository. The public export writes SIMULATIONS=withheld on the next line
# (then the verifier says they are not in the tree); with SIMULATIONS=required a missing driver is red.
SIMULATIONS=withheld
if [ "$SIMULATIONS" = required ]; then
  # shellcheck source=tools/verify_simulations.sh
  . "$P/tools/verify_simulations.sh"
  # shellcheck source=tools/verify_cache.sh
  . "$P/tools/verify_cache.sh"
fi
simulations() {
  if [ "$SIMULATIONS" = required ]; then
    "sims_$1"
  elif [ "$1" = quick ]; then
    echo "  - firmware simulations: not in this tree (kept private, see docs/testing.md)"
  fi
}

echo "1. round trip of the stock OSes"
for f in Main_1.5.0.2:71 Voice_1.5:72 Panel_1.3:73 Sam_1.1:74; do
  name=${f%%:*}; id=${f##*:}
  python3 -I "$P/tools/dsi_unpack.py" "$SYX/Tempest_$name.syx" "$T/$name.bin" > /dev/null
  python3 -I "$P/tools/dsi_pack.py" "$T/$name.bin" "$id" "$T/$name.syx" > /dev/null
  cmp -s "$T/$name.syx" "$SYX/Tempest_$name.syx" || { echo "  ✘ $name: the rebuilt .syx differs"; exit 1; }
  ok "$name"
done
mkdir -p "$P/work"; cp "$T/Main_1.5.0.2.bin" "$P/work/Main_1.5.0.2.bin"

echo "2. versions"
# Each build also compares its image and .syx SHA-256 with the ones recorded in the manifest (stale or edited -> red,
# the message names the manifest, the field and the command that records the fresh value).
for v in "$P"/versions/*.json "$P"/versions/archive/*.json; do
  python3 -I "$P/tools/build_version.py" "$v" "$T/Main_1.5.0.2.bin" "$T/versions" > "$T/build.txt" \
    || { echo "  ✘ $(basename "$v")"; exit 1; }
  head -1 "$T/build.txt" | sed 's/^/  ✔ /'
done
# A combined release is exactly the union of its test builds (each made on the previous release) plus its version
# digits: no word changed by two test builds, every test build's word in the release, nothing else changed.
V=$T/versions
U165=("$V/Main_1.6.4.0.bin" "$V/Main_1.6.5.0.bin" "$P/patch/version_1.6.5.0.json")
python3 -I "$P/tools/version_union.py" "${U165[@]}" "$V"/Main_1.6.5.0-[f-j].bin > "$T/union.txt" || { cat "$T/union.txt"; exit 1; }
ok "1.6.5.0 = 1.6.4.0 + $(tail -1 "$T/union.txt")"
U163=("$V/Main_1.6.2.0.bin" "$V/Main_1.6.3.0.bin" "$P/patch/version_1.6.3.0.json")
python3 -I "$P/tools/version_union.py" "${U163[@]}" "$V/Main_1.6.3.0-a.bin" "$V/Main_1.6.3.0-b.bin" "$V/Main_1.6.3.0-c.bin" \
  > "$T/union.txt" || { cat "$T/union.txt"; exit 1; }
ok "1.6.3.0 = 1.6.2.0 + $(tail -1 "$T/union.txt")"
U162=("$V/Main_1.6.1.0.bin" "$V/Main_1.6.2.0.bin" "$P/patch/version_1.6.2.0.json")
python3 -I "$P/tools/version_union.py" "${U162[@]}" "$V"/Main_1.6.1.[1-5].bin > "$T/union.txt" || { cat "$T/union.txt"; exit 1; }
ok "1.6.2.0 = 1.6.1.0 + $(tail -1 "$T/union.txt")"
# Counter-proofs: a test build left out, a test build given twice, the base as the release, one word of the release
# altered, each refused.
python3 -I - "$V/Main_1.6.3.0.bin" "$T/altered.bin" <<'EOF'
import sys
b = bytearray(open(sys.argv[1], 'rb').read())
b[0x7E000] ^= 1                              # erased flash that no patch writes
open(sys.argv[2], 'wb').write(b)
EOF
refused() { if python3 -I "$P/tools/version_union.py" "$@" > /dev/null 2>&1; then echo "  ✘ union: accepted: $*"; exit 1; fi; }
refused "${U163[@]}" "$V/Main_1.6.3.0-a.bin" "$V/Main_1.6.3.0-b.bin"
refused "${U163[@]}" "$V/Main_1.6.3.0-a.bin" "$V/Main_1.6.3.0-b.bin" "$V/Main_1.6.3.0-c.bin" "$V/Main_1.6.3.0-c.bin"
refused "$V/Main_1.6.2.0.bin" "$V/Main_1.6.2.0.bin" "$P/patch/version_1.6.3.0.json" "$V/Main_1.6.3.0-a.bin"
refused "$V/Main_1.6.2.0.bin" "$T/altered.bin" "$P/patch/version_1.6.3.0.json" "$V/Main_1.6.3.0-a.bin" \
  "$V/Main_1.6.3.0-b.bin" "$V/Main_1.6.3.0-c.bin"
ok "version_union.py: a test build left out, given twice, the base as the release, one altered word: each refused"
# 1.6.6.0: its test builds are stacked (1.6.6.0-b = 1.6.6.0-a + n°63): each owns what it changes from the previous one.
U166=("$V/Main_1.6.5.0.bin" "$V/Main_1.6.6.0.bin" "$P/patch/version_1.6.6.0.json")
python3 -I "$P/tools/version_union.py" --stacked "${U166[@]}" "$V/Main_1.6.6.0-a.bin" "$V/Main_1.6.6.0-b.bin" \
  > "$T/union.txt" || { cat "$T/union.txt"; exit 1; }
ok "1.6.6.0 = 1.6.5.0 + $(tail -1 "$T/union.txt") (stacked test builds)"
refused --stacked "${U166[@]}" "$V/Main_1.6.6.0-a.bin"
refused --stacked "${U166[@]}" "$V/Main_1.6.6.0-b.bin" "$V/Main_1.6.6.0-a.bin"
refused "${U166[@]}" "$V/Main_1.6.6.0-a.bin" "$V/Main_1.6.6.0-b.bin"
ok "version_union.py --stacked: 1.6.6.0-b left out, the builds in the wrong order, the stacked builds read as side by side: each refused"
# 1.6.7.0: one test build, 1.6.7.0-a = 1.6.6.0 + n°63 in its 1.6.7.0 form + n°64.
U167=("$V/Main_1.6.6.0.bin" "$V/Main_1.6.7.0.bin" "$P/patch/version_1.6.7.0.json")
python3 -I "$P/tools/version_union.py" "${U167[@]}" "$V/Main_1.6.7.0-a.bin" > "$T/union.txt" || { cat "$T/union.txt"; exit 1; }
ok "1.6.7.0 = 1.6.6.0 + $(tail -1 "$T/union.txt")"
refused "${U167[@]}" "$V/Main_1.6.6.0-b.bin"
refused "$V/Main_1.6.6.0.bin" "$V/Main_1.6.6.0.bin" "$P/patch/version_1.6.7.0.json" "$V/Main_1.6.7.0-a.bin"
ok "version_union.py: 1.6.7.0 with 1.6.6.0-b in place of 1.6.7.0-a, the base as the release: each refused"
# 1.6.8.0: one test build, 1.6.8.0-a = 1.6.7.0 with n°62 in its 1.6.8.0 form (patch/full_backup_1680.json).
U168=("$V/Main_1.6.7.0.bin" "$V/Main_1.6.8.0.bin" "$P/patch/version_1.6.8.0.json")
python3 -I "$P/tools/version_union.py" "${U168[@]}" "$V/Main_1.6.8.0-a.bin" > "$T/union.txt" || { cat "$T/union.txt"; exit 1; }
ok "1.6.8.0 = 1.6.7.0 + $(tail -1 "$T/union.txt")"
refused "${U168[@]}" "$V/Main_1.6.7.0-a.bin"
refused "$V/Main_1.6.7.0.bin" "$V/Main_1.6.7.0.bin" "$P/patch/version_1.6.8.0.json" "$V/Main_1.6.8.0-a.bin"
ok "version_union.py: 1.6.8.0 with 1.6.7.0-a in place of 1.6.8.0-a, the base as the release: each refused"
# 1.6.9.0: one test build, 1.6.9.0-a = 1.6.8.0 with n°63 and n°62 in their 1.6.9.0 forms (n°65); the words it changes
# from 1.6.8.0 are exactly VOICE_CHECK's (0x9d07be04-0x9d07be3c) and ALIGN's range test (0x9d07ace4-0x9d07acf4).
U169=("$V/Main_1.6.8.0.bin" "$V/Main_1.6.9.0.bin" "$P/patch/version_1.6.9.0.json")
python3 -I "$P/tools/version_union.py" "${U169[@]}" "$V/Main_1.6.9.0-a.bin" > "$T/union.txt" || { cat "$T/union.txt"; exit 1; }
ok "1.6.9.0 = 1.6.8.0 + $(tail -1 "$T/union.txt")"
refused "${U169[@]}" "$V/Main_1.6.8.0-a.bin"
refused "$V/Main_1.6.8.0.bin" "$V/Main_1.6.8.0.bin" "$P/patch/version_1.6.9.0.json" "$V/Main_1.6.9.0-a.bin"
ok "version_union.py: 1.6.9.0 with 1.6.8.0-a in place of 1.6.9.0-a, the base as the release: each refused"
n65_words() {
  python3 -I -c 'import sys
a, b = (open(f, "rb").read() for f in sys.argv[1:3])
got = [0x9D000000 + i for i in range(0, len(a), 4) if a[i:i + 4] != b[i:i + 4]]
want = list(range(0x9D07ACE4, 0x9D07ACF8, 4)) + list(range(0x9D07BE04, 0x9D07BE40, 4))
sys.exit(0 if got == want else "words " + ", ".join(map(hex, got[:8])))' "$1" "$2"
}
n65_words "$V/Main_1.6.8.0.bin" "$V/Main_1.6.9.0-a.bin" || exit 1
if n65_words "$V/Main_1.6.7.0.bin" "$V/Main_1.6.9.0-a.bin" 2> /dev/null; then echo "  ✘ n°65 words: 1.6.7.0 accepted"; exit 1; fi
ok "n°65: 1.6.9.0-a differs from 1.6.8.0 by VOICE_CHECK and ALIGN's range test only (20 words); 1.6.7.0 as the base refused"

echo "3. counter-proofs of build_version.py"
mkdir -p "$T/ce/versions"
python3 -I - "$P" "$T/ce" <<'EOF'
import json, sys
p, ce = sys.argv[1], sys.argv[2]
base = json.load(open(f'{p}/patch/beat_frame.json'))
flasher = dict(base, edits=[{'at': '0x9d07f000', 'word': '01020304', 'why': 'counter-proof'}])
json.dump(flasher, open(f'{ce}/flasher.json', 'w'))
json.dump(base, open(f'{ce}/double_a.json', 'w')); json.dump(base, open(f'{ce}/double_b.json', 'w'))
for name, patches in (('flasher', ['flasher.json']), ('overlap', ['double_a.json', 'double_b.json'])):
    json.dump({'version': '0.0.0.0', 'name': name, 'description': '', 'patches': patches}, open(f'{ce}/versions/{name}.json', 'w'))
EOF
for case_name in flasher overlap; do
  if python3 -I "$P/tools/build_version.py" "$T/ce/versions/$case_name.json" "$T/Main_1.5.0.2.bin" "$T/ce/x" > /dev/null 2>&1; then
    echo "  ✘ $case_name: wrongly accepted"; exit 1
  fi
  ok "$case_name refused"
done

# Counter-proof of the recorded hashes: a manifest with one hash altered, or missing, must be refused, and the message
# must name the manifest and the field. The patches are reached through a link so that the copy builds for real.
mkdir -p "$T/ce_hash/versions"
ln -s "$P/patch" "$T/ce_hash/patch"
for case_name in sha256_image sha256_image_written sha256_syx missing missing_written; do
  python3 -I - "$P/versions/1.5.1.0.json" "$T/ce_hash/versions/1.5.1.0.json" "$case_name" <<'EOF'
import json, sys
m = json.load(open(sys.argv[1]))
if sys.argv[3] == 'missing':
    del m['sha256_syx']
elif sys.argv[3] == 'missing_written':
    del m['sha256_image_written']
else:
    m[sys.argv[3]] = ('0' if m[sys.argv[3]][0] != '0' else '1') + m[sys.argv[3]][1:]
json.dump(m, open(sys.argv[2], 'w'))
EOF
  if python3 -I "$P/tools/build_version.py" "$T/ce_hash/versions/1.5.1.0.json" "$T/Main_1.5.0.2.bin" "$T/ce_hash/x" \
    > /dev/null 2> "$T/ce_hash/err.txt"; then
    echo "  ✘ altered hash ($case_name) accepted"; exit 1
  fi
  field=$case_name; [ "$case_name" = missing ] && field=sha256_syx; [ "$case_name" = missing_written ] && field=sha256_image_written
  grep -q "1.5.1.0.json : $field " "$T/ce_hash/err.txt" || { cat "$T/ce_hash/err.txt"; echo "  ✘ message lacks manifest and field"; exit 1; }
  ok "recorded hash: $case_name case refused, message names the manifest and the field"
done

echo "3b. recipes and publishable content (no DSI byte)"
# Every patch is a canonical recipe (tools/classify_patches.py: each word classified, a DSI-derived word stored as an
# operation on the image, never as bytes), and nothing that would be published holds DSI firmware content.
python3 -I "$P/tools/classify_patches.py" "$T/Main_1.5.0.2.bin" --check "$P"/patch/*.json > "$T/classes.txt" \
  || { tail -3 "$T/classes.txt"; echo "  ✘ patches are not canonical recipes"; exit 1; }
ok "$(ls "$P"/patch/*.json | wc -l | tr -d ' ') canonical recipes (classify_patches.py --check)"
python3 -I "$P/tools/dsi_content_guard.py" "$SYX" --quiet > "$T/guard.txt" || { cat "$T/guard.txt"; exit 1; }
ok "$(tail -1 "$T/guard.txt")"
python3 -I "$P/tools/dsi_content_guard_counter_proofs.py" "$SYX" "$T/guard_cp" > "$T/guard_cp.txt" \
  || { cat "$T/guard_cp.txt"; exit 1; }
ok "guard counter-proofs: a planted DSI fragment refused in 6 forms and a Voice pair in 2 (broken copies accepted), a .syx refused, the patches in the former format and their Rust table refused"

echo "3c. Voice recipes and builds (dsPIC33F)"
# tools/sim_voice_tempester.py --counter-proofs: 11 checks on DSI's image + 22 planted defects (1.5.1.0: the reset past the
# end of the flash); a version with patch/voice/dump_wrap.json (1.5.1.1 on) is run with --wrap: 11 + 23 (the wrap
# removed, the wrap one instruction early). A version with patch/voice/version_reply_1512.json (1.5.1.2) is run with
# --reply 0x17 (its counter-proof: the image answering 0x16).
VOICE_TEMPESTER_COUNTER_PROOFS=33
VOICE_TEMPESTER_COUNTER_PROOFS_WRAP=34
# The Voice recipe rules (reset goto, update entry, nothing at or above PC 0xA400) and the Voice build path (.syx id 72):
# a test recipe written and built, each forbidden write refused (tools/voice_recipe_counter_proofs.py); then the Voice
# recipes and versions of the repository, if any (patch/voice/, versions/voice/: out of the app's tables). The
# counter-proof tool names DSI routine addresses: private like the simulations (SIMULATIONS above).
# The static rules do not protect the update path (the SPI1 ISR, its vector, the SPI1 set-up): every built Voice
# version must also pass the simulated update (sim_voice_os.py --only update: 0xF6A5, 0xF700, pages 0-40 written to
# the image sent, nothing at or above PC 0xA400, reset), private like the simulators.
if [ "$SIMULATIONS" = required ]; then
  "$SIM_PY" -I "$P/tools/voice_recipe_counter_proofs.py" "$T/Voice_1.5.bin" "$T/Main_1.5.0.2.bin" "$T/voice_recipes" \
    > "$T/vr.txt" || { cat "$T/vr.txt"; exit 1; }
  ok "Voice recipe: written, built (id 72 round trip, hashes, simulated update); writes at PC 0-2, in the update entry, at or above 0xA400 and $(grep -c '  ok  refused\|  ok  built-image\|  ok  update path' "$T/vr.txt") wrong cases refused"
else
  echo "  - Voice recipe counter-proofs and simulated updates: not in this tree (kept private, see docs/testing.md)"
fi
if ls "$P"/patch/voice/*.s > /dev/null 2>&1; then
  # The Voice recipes come from their sources through the hand encoder (no xc16-as): the encoder gives DSI's own
  # words on every DSI instruction it encodes and refuses a word that does not read back; each recipe equals its
  # source rebuilt; the release table of release_curve.s is the generator's (0-100 = DSI, 101-127 to 60 s, ≤ 1 %).
  python3 -I "$P/tools/pic24_asm.py" --self-test "$T/Voice_1.5.bin" > "$T/va.txt" || { cat "$T/va.txt"; exit 1; }
  cat "$T/va.txt"
  python3 -I "$P/tools/voice_recipe_build.py" "$T/Voice_1.5.bin" "$T/voice_sources" --check > "$T/vs.txt" \
    || { cat "$T/vs.txt"; exit 1; }
  cat "$T/vs.txt"
  python3 -I "$P/tools/voice_release_table.py" "$T/Voice_1.5.bin" --check > "$T/vt.txt" || { cat "$T/vt.txt"; exit 1; }
  cat "$T/vt.txt"
fi
for r in "$P"/patch/voice/*.json; do
  [ -e "$r" ] || continue
  python3 -I "$P/tools/classify_patches.py" "$T/Voice_1.5.bin" --voice --check "$r" > "$T/vc.txt" \
    || { cat "$T/vc.txt"; echo "  ✘ $(basename "$r") is not a canonical Voice recipe"; exit 1; }
  ok "$(basename "$r"): canonical Voice recipe"
done
for v in "$P"/versions/voice/*.json; do
  [ -e "$v" ] || continue
  python3 -I "$P/tools/build_version.py" "$v" "$T/Voice_1.5.bin" "$T/versions_voice" > "$T/build.txt" \
    || { cat "$T/build.txt"; echo "  ✘ $(basename "$v")"; exit 1; }
  head -1 "$T/build.txt" | sed 's/^/  ✔ /'
  if [ "$SIMULATIONS" = required ]; then
    built="$T/versions_voice/Voice_$(python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))["version"])' "$v").bin"
    "$SIM_PY" -I "$P/tools/sim_voice_os.py" "$built" --only update > "$T/vu.txt" \
      || { cat "$T/vu.txt"; echo "  ✘ $(basename "$v"): the simulated update fails"; exit 1; }
    ok "$(basename "$v"): simulated update through the built image's own boot path"
    # The fixes of the Tempester Voice OS on the simulator (tools/sim_voice_tempester.py), each with DSI's image and
    # planted defects as counter-proofs; the real calibration pages of the author's backup when present (local only).
    VT=("$built" "$T/Voice_1.5.bin")
    WRAP=() expected=$VOICE_TEMPESTER_COUNTER_PROOFS
    if grep -q '"patch/voice/dump_wrap.json"' "$v"; then WRAP=(--wrap) expected=$VOICE_TEMPESTER_COUNTER_PROOFS_WRAP; fi
    if grep -q '"patch/voice/version_reply_1512.json"' "$v"; then WRAP+=(--reply 0x17); fi
    VT+=(${WRAP[@]+"${WRAP[@]}"})
    if [ -d "$P/work/backup-2026-11-13/decoded" ]; then
      VT+=(--backup "$P/work/backup-2026-11-13/decoded")
    else
      echo "  - the six real calibration pages: skipped, work/backup-2026-11-13/decoded is not in this copy (private, local)"
    fi
    [ "${2:-}" = "--long" ] && VT+=(--long)
    "$SIM_PY" -I "$P/tools/sim_voice_tempester.py" "${VT[@]}" > "$T/vte.txt" \
      || { cat "$T/vte.txt"; echo "  ✘ $(basename "$v"): a Tempester Voice proof fails"; exit 1; }
    sed 's/: .*//' "$T/vte.txt" | tr -d '\n' | sed 's/  ✔ / /g; s/^/  ✔ Tempester Voice proofs:/'; echo
    "$SIM_PY" -I "$P/tools/sim_voice_tempester.py" "$built" "$T/Voice_1.5.bin" ${WRAP[@]+"${WRAP[@]}"} --counter-proofs > "$T/vtc.txt" \
      || { cat "$T/vtc.txt"; echo "  ✘ $(basename "$v"): a counter-proof passed"; exit 1; }
    # The number of counter-proofs is fixed: one dropped silently from the tool turns the step red.
    n=$(grep -c '✔' "$T/vtc.txt")
    [ "$n" -eq "$expected" ] || { cat "$T/vtc.txt"; echo "  ✘ $n counter-proofs, $expected expected"; exit 1; }
    ok "$(basename "$v"): $n counter-proofs refused by their own check (DSI's image, planted defects)"
  fi
done
# Tempester Voice 1.5.1.2 = 1.5.1.1 with the version word alone (n°66): the two built images differ at PC 0x669C only
# (the 0xFE00 reply; boot page and calibration are build_version.py's rule above). Counter-proof: 1.5.1.0 against
# 1.5.1.2 (the dump wrap besides the word) refused.
if [ -f "$T/versions_voice/Voice_1.5.1.2.bin" ]; then
  voice_diff() {
    python3 -I -c 'import sys
a, b = (open(f, "rb").read() for f in sys.argv[1:3])
pcs = sorted({i // 3 * 2 for i in range(len(a)) if a[i] != b[i]})
sys.exit(0 if len(a) == len(b) and pcs == [0x669C] else "PCs " + ", ".join(map(hex, pcs[:6])))' "$1" "$2"
  }
  voice_diff "$T/versions_voice/Voice_1.5.1.1.bin" "$T/versions_voice/Voice_1.5.1.2.bin" || exit 1
  if voice_diff "$T/versions_voice/Voice_1.5.1.0.bin" "$T/versions_voice/Voice_1.5.1.2.bin" 2> /dev/null; then
    echo "  ✘ Voice diff: 1.5.1.0 against 1.5.1.2 accepted"; exit 1
  fi
  ok "Voice 1.5.1.2 = 1.5.1.1 but PC 0x669C (the 0xFE00 reply 0x17); 1.5.1.0 against it refused (counter-proof)"
fi

echo "4. tools"
for f in "$P"/tools/*.py; do python3 -I -m py_compile "$f"; done
rm -rf "$P/tools/__pycache__"
ok "Python"
python3 -I "$P/tools/verify_scope.py" "$P" --self-test || exit 1
ok "verify_scope.py: the files the app step reads found, a change reaching them refused, the UI scope only for page files and documentation (28 checks)"
swiftc -O -o "$T/tempest_midi" "$P/tools/tempest_midi.swift" 2> "$T/swift.log" || { cat "$T/swift.log"; exit 1; }
ok "tempest_midi.swift"
for t in "$P"/tests/test_*.py; do
  python3 -I "$t" > "$T/test.log" 2>&1 || { cat "$T/test.log"; echo "  ✘ $(basename "$t")"; exit 1; }
  ok "$(basename "$t")"
done

simulations quick

APP_START=$SECONDS
if [ "$SCOPE" = docs ]; then
  echo "5. Tempester app (Rust): skipped, nothing since $3 reaches it (tools/verify_scope.py)"
else
  echo "5. Tempester app (Rust)"
  [ "${2:-}" = "--long" ] && simulations exports
  [ "${2:-}" = "--long" ] && export TEMPESTER_LONG=1
  "$P/tools/test_app.sh" "$SYX"
fi
echo "  - time: $SECONDS s in all, of which $((SECONDS - APP_START)) s for the app step and what follows"

if [ "${2:-}" = "--long" ]; then
  simulations long
fi
if [ "$SIMULATIONS" = required ] && [ "$VERIFY_CACHE" != off ]; then verify_cache_summary; fi
# The exact last line on success: the workflow and the published docs (public-src/) check it word for word. A run that
# skipped the app step must not print it.
if [ "$SCOPE" = docs ]; then
  echo "green, app step skipped (docs scope since $3): run the full verifier on the merged tree"
else
  echo "all green"
fi
