/* Counter-proof of panel-perform.test.js: each deliberately broken copy of one of the scripts it covers must make the
   checks fail. `node panel-perform-counter-proof.js <ui folder> <scratch folder>` exits 1 when a variant is accepted
   or does not change its script. */
"use strict";
const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

const [uiDir, scratch] = process.argv.slice(2);
const FILES = ["panel-link.js", "panel-machine.js", "panel-perform.js"];

const variants = {
  "a DSI identity answer ignored": [
    "panel-perform.js",
    "if (tempester === false)",
    "if (tempester === 0)",
  ],
  "an OS without the feature gets it": [
    "panel-perform.js",
    'if (f.on) return { on: true, why: "" };',
    'if (f.since) return { on: true, why: "" };',
  ],
  "the fake machine treated as an old OS": [
    "panel-perform.js",
    'if (fake) return { on: true, why: "" };',
    "",
  ],
  "the tooltip does not say which version brings it": [
    "panel-perform.js",
    'LABELS[name] + " Tempester " + (since || "1.6.3.0") + " or later"',
    'LABELS[name] + " need a newer OS"',
  ],
  "a ribbon resends the same position on every move": [
    "panel-perform.js",
    "if (touched && next === position && nextPressure === pressure)",
    "if (false)",
  ],
  "a ribbon released twice": [
    "panel-perform.js",
    "if (!touched) return false;",
    "if (false) return false;",
  ],
  "the machine's ribbon drawn over the user's finger": [
    "panel-perform.js",
    "if (touched) return false;",
    "if (false) return false;",
  ],
  "a probability sent again when unchanged": [
    "panel-perform.js",
    "values[pad] === null || values[pad] === sent[pad]",
    "values[pad] === null",
  ],
  "a probability not bounded to 0-100": [
    "panel-perform.js",
    "const next = clampInt(v, PROB_MAX);",
    "const next = Math.round(v);",
  ],
  "a beat dump read from the wrong pad": [
    "panel-perform.js",
    "const v = Array.isArray(list) ? list[k] : undefined;",
    "const v = Array.isArray(list) ? list[k + 1] : undefined;",
  ],
  "a ribbon release on its own throttle (a pending touch could overtake it)": [
    "panel-link.js",
    'throttled("ribbon" + r, "ribbon", r, "release")',
    'throttled("release" + r, "ribbon", r, "release")',
  ],
  "the probability throttled (a gesture's value may be dropped)": [
    "panel-link.js",
    'bridge.send("pad_probability", pad, percent)',
    'throttled("prob" + pad, "pad_probability", pad, percent)',
  ],
  "beat-wide knobs in every Pad Function": [
    "panel-machine.js",
    "if (!bwAvailable || !BEAT_WIDE_MODES.includes(mode)) return null;",
    "if (!bwAvailable) return null;",
  ],
  "beat-wide knobs without n°29": [
    "panel-machine.js",
    "if (!bwAvailable || !BEAT_WIDE_MODES.includes(mode)) return null;",
    "if (!BEAT_WIDE_MODES.includes(mode)) return null;",
  ],
  "a reported position moves the knob under the user's drag": [
    "panel-machine.js",
    "bwHeld.has(i)",
    "false",
  ],
  "every report of the machine puts up a caption (8 at once hide the labels)": [
    "panel-machine.js",
    "bwCaption(i, true);",
    "bwCaption(i);",
  ],
  "Attack and Decay swapped": [
    "panel-machine.js",
    '["enc", 7, 6, "Attack"],',
    '["enc", 7, 7, "Attack"],',
  ],
  "a beat-wide position above 254": [
    "panel-machine.js",
    "const next = Math.max(0, Math.min(BW_MAX, Math.round(position)));",
    "const next = Math.max(0, Math.round(position));",
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
  const folder = path.join(scratch, "perform-variant");
  fs.rmSync(folder, { recursive: true, force: true });
  fs.mkdirSync(folder, { recursive: true });
  for (const f of FILES)
    fs.copyFileSync(path.join(uiDir, f), path.join(folder, f));
  fs.writeFileSync(path.join(folder, file), broken);
  const run = spawnSync(
    "node",
    [path.join(__dirname, "panel-perform.test.js"), folder],
    { encoding: "utf8" },
  );
  if (run.status === 0) {
    console.log("broken script accepted: " + name);
    failed += 1;
  }
}
console.log(Object.keys(variants).length + " variants refused");
process.exit(failed ? 1 : 0);
