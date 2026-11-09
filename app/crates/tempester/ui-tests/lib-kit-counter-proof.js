/* Counter-proof of lib-kit.test.js: copies of the page scripts with one behaviour broken must make the checks fail.
   `node lib-kit-counter-proof.js <ui folder> <scratch folder>` exits 1 when a variant is accepted or does not change
   the file. */
"use strict";
const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

const [uiDir, scratch] = process.argv.slice(2);
const FILES = [
  "bridge.js",
  "icons.js",
  "kit.js",
  "project-ui.js",
  "lib-model.js",
  "lib-dialogs.js",
  "lib-flow.js",
  "lib-kit.js",
  "lib-browser.js",
  "page-library.js",
];

const variants = {
  "a pad out of the 16 in use is assigned": [
    "lib-kit.js",
    "if (pad >= 0 && pad < kitState.count)",
    "if (pad >= 0)",
  ],
  "dropped pads 17-32 are kept when going back to 16": [
    "lib-kit.js",
    "for (let pad = count; pad < PADS; pad++) kitState.slots[pad] = null;",
    "",
  ],
  "an untouched pad is not kept": [
    "lib-kit.js",
    'const KEEP = "_";',
    'const KEEP = "";',
  ],
  "fill ignores the number of pads in use": [
    "lib-kit.js",
    "pad < kitState.count && pad < items.length",
    "pad < items.length",
  ],
  "drop does nothing": [
    "lib-kit.js",
    "if (dropped) assign(state, pad, dropped);",
    "",
  ],
  "Cancel still sends the check": [
    "lib-kit.js",
    'if (answer === "write" && filled(state) > 0) flow.kitCheck',
    "if (filled(state) > 0) flow.kitCheck",
  ],
  "the apply is sent without the confirmation": [
    "lib-flow.js",
    'if (await dialogs.sendPlan(plan))\n      bridge.send("lib_kit_apply"',
    'await dialogs.sendPlan(plan);\n    if (true)\n      bridge.send("lib_kit_apply"',
  ],
  "the apply drops the token": [
    "lib-flow.js",
    'bridge.send("lib_kit_apply", kitIds, plan.token)',
    'bridge.send("lib_kit_apply", kitIds, "none")',
  ],
  "an auto backup slot can be picked": [
    "lib-kit.js",
    ".filter((item) => !item.slot)",
    "",
  ],
  "export of the fetched beat removed": [
    "page-library.js",
    'bridge.send("lib_export_midi", "")',
    'bridge.send("x", "")',
  ],
  "export button enabled before a beat is fetched": [
    "page-library.js",
    "if (els.export) els.export.disabled = !fetched;",
    "",
  ],
  "kit builder usable without a machine": [
    "lib-browser.js",
    "els.kitButton.disabled = !session.connected;",
    "",
  ],
};

let failed = 0;
for (const [name, [file, from, to]] of Object.entries(variants)) {
  const dir = fs.mkdtempSync(path.join(scratch, "libkit-variant-"));
  for (const f of FILES)
    fs.copyFileSync(path.join(uiDir, f), path.join(dir, f));
  const source = fs.readFileSync(path.join(dir, file), "utf8");
  const broken = source.replace(from, to);
  if (broken === source) {
    console.log("variant did not change the file: " + name);
    failed += 1;
    continue;
  }
  fs.writeFileSync(path.join(dir, file), broken);
  const run = spawnSync(
    "node",
    [path.join(__dirname, "lib-kit.test.js"), dir],
    { encoding: "utf8" },
  );
  if (run.status === 0) {
    console.log("broken page accepted: " + name);
    failed += 1;
  }
}
console.log(Object.keys(variants).length + " variants checked");
process.exit(failed ? 1 : 0);
