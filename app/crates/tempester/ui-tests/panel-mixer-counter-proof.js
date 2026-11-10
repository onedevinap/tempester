/* Counter-proof of panel-mixer.test.js: each deliberately broken copy of one of the scripts it covers must make the
   checks fail. `node panel-mixer-counter-proof.js <ui folder> <scratch folder>` exits 1 when a variant is accepted or
   does not change its script. */
"use strict";
const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

const [uiDir, scratch] = process.argv.slice(2);
const FILES = ["panel-link.js", "panel-machine.js"];

const variants = {
  "Level reported at the 7-bit value, not doubled": [
    "panel-machine.js",
    "reportKnob(target.abs, halfByte(target.abs, Math.min(HALF_MAX, value)))",
    "reportKnob(target.abs, Math.min(HALF_MAX, value))",
  ],
  "Distort halved like Level": [
    "panel-machine.js",
    "return reportKnob(target.abs, clampAbs(value));",
    "return reportKnob(target.abs, clampAbs(value * 2));",
  ],
  "Distort above the knob's maximum": [
    "panel-machine.js",
    "return reportKnob(target.abs, clampAbs(value));",
    "return reportKnob(target.abs, Math.round(value));",
  ],
  "Level on the Pan knob": [
    "panel-machine.js",
    "level: { abs: 7 },",
    "level: { abs: 6 },",
  ],
  "Compress on the Distort knob": [
    "panel-machine.js",
    "compress: { abs: 2 },",
    "compress: { abs: 9 },",
  ],
  "the knob's own byte not kept (1-unit jitter)": [
    "panel-machine.js",
    "(abs[k] !== null && abs[k] >> 1 === v ? abs[k] : v * 2)",
    "v * 2",
  ],
  "Delay Repeats ignores Shift (a full state ends on Repeats)": [
    "panel-machine.js",
    "const v = shift ? pair.repeats : pair.pan;",
    "const v = pair.repeats !== null ? pair.repeats : pair.pan;",
  ],
  "Delay Repeats scale wrong": [
    "panel-machine.js",
    "Math.round((v * HALF_MAX) / REPEATS_MAX) * 2",
    "v * 2",
  ],
  "Shift change does not move the knob": [
    "panel-machine.js",
    "shift = !!on;\n      return showPanKnob();",
    "shift = !!on;\n      return false;",
  ],
  "tempo shown without its decimal": [
    "panel-machine.js",
    "(value / 10).toFixed(1)",
    "String(Math.round(value / 10))",
  ],
  "a tempo of 0 shown": [
    "panel-machine.js",
    'if (item === "tempo" && value === 0) return false;',
    "",
  ],
  "a report older than the turn replaces the count": [
    "panel-machine.js",
    "said.at >= burst.at",
    "true",
  ],
  "the bubble never fades": [
    "panel-machine.js",
    "now() - said.at < REPORT_SHOW_MS",
    "true",
  ],
  "a report moves the knob under the held pointer": [
    "panel-machine.js",
    "if (guard.busy(absKey(k)) || abs[k] === byte) return false;",
    "if (abs[k] === byte) return false;",
  ],
  "no quiet time after a move (held only)": [
    "panel-machine.js",
    "(touched.has(key) && now() - touched.get(key) <= REPORT_QUIET_MS)",
    "false",
  ],
  "a quiet time that never ends": [
    "panel-machine.js",
    "now() - touched.get(key) <= REPORT_QUIET_MS",
    "true",
  ],
  "a release does not start the quiet time": [
    "panel-machine.js",
    "held.delete(key);\n        touched.set(key, now());",
    "held.delete(key);",
  ],
  "a move of the knob by the user does not start the quiet time": [
    "panel-machine.js",
    "guard.touch(absKey(k));\n      if (k === 6)",
    "if (k === 6)",
  ],
  "the quiet time is shared by every knob": [
    "panel-machine.js",
    'const absKey = (k) => "abs" + k;',
    'const absKey = (k) => "abs";',
  ],
  "the Full view follows the level too": [
    "panel-machine.js",
    'const FULL_VIEW_REPORTS = ["volume", "distort", "compress"];',
    'const FULL_VIEW_REPORTS = ["volume", "distort", "compress", "level", "tempo"];',
  ],
  "the Full view ignores its guard": [
    "panel-machine.js",
    "if (!FULL_VIEW_REPORTS.includes(item) || guard.busy(item)) return false;",
    "if (!FULL_VIEW_REPORTS.includes(item)) return false;",
  ],
  "a report is sent back to the machine": [
    "panel-machine.js",
    "return reportKnob(target.abs, clampAbs(value));",
    "link.control(target.abs, value);\n          return reportKnob(target.abs, clampAbs(value));",
  ],
  "Distort sent as CC 12 with the remote panel": [
    "panel-link.js",
    "if (remote()) {",
    "if (false) {",
  ],
  "Distort sent as controls without the remote panel": [
    "panel-link.js",
    "if (remote()) {",
    "if (true) {",
  ],
  "CC above 127": ["panel-link.js", "Math.min(value, CC_MAX)", "value"],
  "Compress on the wrong control": [
    "panel-link.js",
    "compress: 2 };",
    "compress: 3 };",
  ],
};

let failed = 0;
for (const [name, [file, from, to]] of Object.entries(variants)) {
  const source = fs.readFileSync(path.join(uiDir, file), "utf8");
  const broken = source.replace(from, to);
  if (broken === source) {
    console.log("variant did not change the script: " + name);
    failed += 1;
    continue;
  }
  const folder = path.join(scratch, "mixer-variant");
  fs.rmSync(folder, { recursive: true, force: true });
  fs.mkdirSync(folder, { recursive: true });
  for (const f of FILES)
    fs.copyFileSync(path.join(uiDir, f), path.join(folder, f));
  fs.writeFileSync(path.join(folder, file), broken);
  const run = spawnSync(
    "node",
    [path.join(__dirname, "panel-mixer.test.js"), folder],
    { encoding: "utf8" },
  );
  if (run.status === 0) {
    console.log("broken script accepted: " + name);
    failed += 1;
  }
}
console.log(Object.keys(variants).length + " variants refused");
process.exit(failed ? 1 : 0);
