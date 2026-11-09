/* Counter-proof of panel-remote.test.js: each deliberately broken copy of ui/panel-remote.js must make the checks
   fail, one broken release path at a time (a key whose release is lost stays down on the machine).
   `node counter-proof.js <ui folder> <scratch folder>` exits 1 when a variant is accepted or does not change. */
"use strict";
const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

const [uiDir, scratch] = process.argv.slice(2);
const source = fs.readFileSync(path.join(uiDir, "panel-remote.js"), "utf8");
const ENDS =
  'for (const end of ["pointerup", "pointerleave", "pointercancel"])';

const variants = {
  "no release on pointerup": [
    ENDS,
    'for (const end of ["pointerleave", "pointercancel"])',
  ],
  "no release on pointerleave": [
    ENDS,
    'for (const end of ["pointerup", "pointercancel"])',
  ],
  "no release on pointercancel": [
    ENDS,
    'for (const end of ["pointerup", "pointerleave"])',
  ],
  "no release on window blur": [
    'window.addEventListener("blur", releaseAll);',
    "",
  ],
  "no release when the page is hidden": [
    "if (document.hidden) releaseAll();",
    "",
  ],
  "no release when the panel is lost": [
    "if (!on) {\n      releaseAll();",
    "if (!on) {",
  ],
  "no release when the key loses focus": [
    'b.addEventListener("blur", () => release(key));',
    "",
  ],
  "press accepted while greyed": [
    "if (!available || held.has(key.code)) return;",
    "if (held.has(key.code)) return;",
  ],
  "press repeated while held": [
    "if (!available || held.has(key.code)) return;",
    "if (!available) return;",
  ],
};

let failed = 0;
for (const [name, [from, to]] of Object.entries(variants)) {
  const broken = source.replace(from, to);
  if (broken === source) {
    console.log("variant did not change the script: " + name);
    failed += 1;
    continue;
  }
  const file = path.join(scratch, "broken-remote.js");
  fs.writeFileSync(file, broken);
  const run = spawnSync(
    "node",
    [
      path.join(__dirname, "panel-remote.test.js"),
      file,
      path.join(uiDir, "panel-keys.js"),
    ],
    { encoding: "utf8" },
  );
  if (run.status === 0) {
    console.log("broken script accepted: " + name);
    failed += 1;
  }
}
console.log(Object.keys(variants).length + " variants checked");
process.exit(failed ? 1 : 0);
