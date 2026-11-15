#!/bin/bash
# The checks a change of the app's page files can reach (tools/verify_scope.py says `ui`: every changed path is under
# app/crates/tempester/ui/ or ui-tests/, or documentation): sourced by tools/verify.sh right after the scope is known,
# never run alone (it uses P, T, ok() and $3 from there). It runs the tools compile step, the Python tests that read
# the page (verify_scope.py --ui-tests), then the page part of tools/test_app.sh (TEMPESTER_UI_ONLY=1: the Rust tests
# of the module that embeds the page, the node page tests and their counter-proofs, the browser layout tests), and
# ends the verifier with "all green". Nothing of the firmware runs (round trips, versions, recipes, Voice builds,
# simulations), nor the rest of the Rust workspace: a page file reaches none of them.
echo "UI scope since $3: the page files, the tools compile step and the tests that read the page"
echo "4. tools"
for f in "$P"/tools/*.py; do python3 -I -m py_compile "$f"; done
rm -rf "$P/tools/__pycache__"
ok "Python"
python3 -I "$P/tools/verify_scope.py" "$P" --self-test || exit 1
ok "verify_scope.py: the files the app step reads found, a change reaching them refused, the UI scope only for page files"
swiftc -O -o "$T/tempest_midi" "$P/tools/tempest_midi.swift" 2> "$T/swift.log" || { cat "$T/swift.log"; exit 1; }
ok "tempest_midi.swift"
UI_TESTS=$(python3 -I "$P/tools/verify_scope.py" "$P" --ui-tests) || exit 1
[ -n "$UI_TESTS" ] || { echo "  ✘ no Python test reads the page: verify_scope.py --ui-tests found none"; exit 1; }
for t in $UI_TESTS; do
  python3 -I "$P/$t" > "$T/test.log" 2>&1 || { cat "$T/test.log"; echo "  ✘ $(basename "$t")"; exit 1; }
  ok "$(basename "$t")"
done
echo "5. Tempester app, page files only"
TEMPESTER_UI_ONLY=1 "$P/tools/test_app.sh"
echo "  - time: $SECONDS s in all"
echo "all green"
exit 0
