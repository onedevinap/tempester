/* Counter-proof of pad-channels.test.js: each deliberately broken copy must make the checks fail.
   `node pad-channels-counter-proof.js <ui folder> <scratch folder>` exits 1 when a variant is accepted or does not
   change its script. */
"use strict";
const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

const [uiDir, scratch] = process.argv.slice(2);
const FILES = ["pad-channels.js", "panel-perform.js"];
const variants = {
  "a value above 32 sent": [
    "pad-channels.js",
    "const isValue = (v) => Number.isInteger(v) && v >= 0 && v <= MAX;",
    "const isValue = (v) => Number.isInteger(v) && v >= 0;",
  ],
  "pad 32 accepted": [
    "pad-channels.js",
    "pad >= 0 && pad < PADS;",
    "pad >= 0 && pad <= PADS;",
  ],
  "a fixed channel labelled with its raw value": [
    "pad-channels.js",
    'return v - 16 + " Fix";',
    'return v + " Fix";',
  ],
  "a report stored on the wrong pad": [
    "pad-channels.js",
    "values[m.pad] = m.value;",
    "values[(m.pad + 1) % PADS] = m.value;",
  ],
  "a save allowed while a read runs": [
    "pad-channels.js",
    "  function save() {\n    if (reading) return false;",
    "  function save() {\n    if (false) return false;",
  ],
  "a read that never ends": [
    "pad-channels.js",
    "if (reading && ++answered >= PADS) setReading(false);",
    "if (reading) ++answered;",
  ],
  "a timeout that leaves the read running": [
    "pad-channels.js",
    "    setReading(false);\n    for (const fn of timeoutListeners)",
    "    for (const fn of timeoutListeners)",
  ],
  "a timeout never told": [
    "pad-channels.js",
    "for (const fn of timeoutListeners) fn(m.pad);",
    "",
  ],
  "the gate without a version name": [
    "panel-perform.js",
    'LABELS[name] + " Tempester " + (since || "1.6.3.0") + " or later"',
    'LABELS[name] + " a newer OS"',
  ],
};

fs.mkdirSync(scratch, { recursive: true });
let failed = 0;
for (const [name, [file, from, to]] of Object.entries(variants)) {
  const dir = fs.mkdtempSync(path.join(scratch, "padchan-"));
  for (const f of FILES)
    fs.copyFileSync(path.join(uiDir, f), path.join(dir, f));
  const source = fs.readFileSync(path.join(dir, file), "utf8");
  if (!source.includes(from)) {
    console.error("variant does not apply: " + name);
    failed++;
    continue;
  }
  fs.writeFileSync(path.join(dir, file), source.replace(from, to));
  const run = spawnSync("node", [
    path.join(__dirname, "pad-channels.test.js"),
    dir,
  ]);
  if (run.status === 0) {
    console.error("variant accepted: " + name);
    failed++;
  }
}
if (failed) process.exit(1);
console.log(Object.keys(variants).length + " broken copies refused");
