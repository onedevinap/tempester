#!/bin/bash
# Full check of the repository, to run before every commit. Red at the first failure (non-zero exit code).
#
# usage: tools/verify.sh <folder of the DSI .syx files> [--long]
#   --long: adds the counter-proof of patch n°8 (loop_sim_patch8.py, about 5 min) and the simulation of patch n°15
# Checks:
#   1. round trip dsi_unpack -> dsi_pack identical to the byte on the four stock OSes;
#   2. every version of versions/*.json and versions/archive/*.json (test builds, kept out of the app) builds with all its checks (build_version.py), including its
#      recorded hashes sha256_image / sha256_image_written / sha256_syx (stale or edited: red; new version: build_version.py --record);
#   3. counter-proofs of build_version.py: two patches on the same address, flasher page touched, hash
#      altered or missing -> refused (the message names the manifest and the field);
#   3b. every patch is a canonical recipe (classify_patches.py --check) and nothing publishable holds DSI firmware
#      content (dsi_content_guard.py on the default tree), with the guard's counter-proofs;
#   4. the Python tools compile, tempest_midi.swift compiles (nothing is sent to the machine), tests/test_*.py pass;
#   5. Rust workspace app/ (tools/test_app.sh): generated tables up to date, cargo fmt, clippy -D warnings, tests, and the
#      #[ignore] tests on the real DSI files in <folder> (TEMPESTER_DSI_SYX), with a disposable HOME;
#   6.-9. option --long: the firmware simulations and their counter-proofs (tools/verify_simulations.sh, private:
#      they run the image's code at DSI's routine addresses; the public tree has no simulator and says so).
# Works in a temporary folder; writes nothing in the repository outside work/ (decoded images, not versioned).
set -euo pipefail
P="$(cd "$(dirname "$0")/.." && pwd)"
SYX=${1:?"folder of the DSI .syx files expected"}
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
ok() { printf '  ✔ %s\n' "$1"; }
# The firmware simulations (tools/verify_simulations.sh) run the code of the user's own image at the addresses of
# DSI's routines: they stay in the private repository. The public export writes SIMULATIONS=withheld on the next line
# (then the verifier says they are not in the tree); with SIMULATIONS=required a missing driver is red.
SIMULATIONS=withheld
if [ "$SIMULATIONS" = required ]; then
  # shellcheck source=tools/verify_simulations.sh
  . "$P/tools/verify_simulations.sh"
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
ok "guard counter-proofs: a planted DSI fragment refused in 6 forms (broken copies accepted), a .syx refused, the patches in the former format and their Rust table refused"

echo "4. tools"
for f in "$P"/tools/*.py; do python3 -I -m py_compile "$f"; done
rm -rf "$P/tools/__pycache__"
ok "Python"
swiftc -O -o "$T/tempest_midi" "$P/tools/tempest_midi.swift" 2> "$T/swift.log" || { cat "$T/swift.log"; exit 1; }
ok "tempest_midi.swift"
for t in "$P"/tests/test_*.py; do
  python3 -I "$t" > "$T/test.log" 2>&1 || { cat "$T/test.log"; echo "  ✘ $(basename "$t")"; exit 1; }
  ok "$(basename "$t")"
done

simulations quick

echo "5. Tempester app (Rust)"
[ "${2:-}" = "--long" ] && simulations exports
[ "${2:-}" = "--long" ] && export TEMPESTER_LONG=1
"$P/tools/test_app.sh" "$SYX"

if [ "${2:-}" = "--long" ]; then
  simulations long
fi
# The exact last line on success: the workflow and the published docs (public-src/) check it word for word.
echo "all green"
