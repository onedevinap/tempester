/* Counter-proof of layout-overlap.js: on deliberately broken copies of the page, the test must fail AND name the text
   that the author saw hidden. Each variant puts one defect of the earlier Machine view back; `expect` lists the lines
   (or pieces of lines) the report must contain.
   A variant that does not turn the test red, or does not name its texts, means the test could not see that defect.

   usage: node layout-overlap-counter-proof.js <ui dir> <work dir> <repo root> */
"use strict";
const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

const [uiDir, workDir, repo] = process.argv.slice(2, 5);

const VARIANTS = [
  {
    name: "the REC LED on its title",
    file: "panel-face.js",
    from: "[0x23, 1598, TLAB",
    to: "[0x23, 1584, TLAB",
    expect: ['text "REC" overlaps control tled rec'],
  },
  {
    name: "Oscillators, filters, amp: the labels back on the knobs",
    file: "panel-face.js",
    from: "const R1 = B1 + 92;",
    to: "const R1 = B1 + 60;",
    expect: [
      'text "Shape" overlaps control',
      'text "1/2 Mix" overlaps control',
      'text "Pitch" overlaps control',
      'text "Frequency" overlaps control',
      'text "Feedback" overlaps control',
    ],
  },
  {
    name: "Envelopes, LFO, mixer: the labels back on the knobs",
    file: "panel-face.js",
    from: "const E1 = B2 + 92;",
    to: "const E1 = B2 + 60;",
    expect: [
      'text "Attack" overlaps control',
      'text "Decay" overlaps control',
      'text "Amount" overlaps control',
      'text "Rate" overlaps control',
      'text "Level" overlaps control',
      'text "Pan" overlaps control',
      'text "Delay Send" overlaps control',
    ],
  },
  {
    name: "shifted labels on top of the main ones (Attack / Sustain Level and the like)",
    file: "panel-face.js",
    from: "const E2 = B2 + 113;",
    to: "const E2 = B2 + 98;",
    expect: [
      'text "Attack" overlaps text "Sustain Level"',
      'text "Decay" overlaps text "Release"',
      'text "Amount" overlaps text "Velocity Amt"',
      'text "Rate" overlaps text "Shape"',
      'text "Amount" overlaps text "Destination"',
      'text "Delay Send" overlaps text "Delay Time"',
      'text "Pan" overlaps text "Delay Repeats"',
    ],
  },
  {
    name: "the Envelopes line through the shifted labels",
    file: "panel-face.js",
    from: "const LINE2 = B2 + 127;",
    to: "const LINE2 = B2 + 113;",
    expect: [
      'text "Sustain Level" is crossed by a printed line',
      'text "Release" is crossed by a printed line',
    ],
  },
  {
    name: "16 Tunings over 16 Beats / 16 Time Steps",
    file: "panel-face.js",
    from: "const G0 = B3 + 14;",
    to: "const G0 = B3 + 30;",
    expect: [
      'text "16 Tunings" overlaps text "16 Beats"',
      'text "16 Tunings" overlaps text "16 Time Steps"',
    ],
  },
  {
    name: "Rec/Edit over Undo Snd",
    file: "panel-face.js",
    from: '["Undo Snd", 1212, SUB1, "s"]',
    to: '["Undo Snd", 1140, GT, "s"]',
    expect: ['text "Undo Snd" overlaps text "Rec/Edit"'],
  },
  {
    name: "Sound Bank B crowded (Sound over Bank B)",
    file: "panel-face.js",
    from: '["Sound", 696, G0, "m"]',
    to: '["Sound", 696, L1 - 10, "m"]',
    expect: ['text "Sound" overlaps text "Bank B"'],
  },
  {
    name: "a knob drawn bigger than its box (ring over the labels below)",
    file: "panel-face.css",
    from: ".panel-root .mv .mvpot svg { display: block; width: 64px; height: 64px;",
    to: ".panel-root .mv .mvpot svg { display: block; width: 110px; height: 110px;",
    expect: ["overlaps control"],
  },
  {
    name: "a key LED taller than its key (over the label above)",
    file: "panel-face.js",
    from: "const LED_SIZE = { w: 6, h: 14, gap: 6 };",
    to: "const LED_SIZE = { w: 6, h: 60, gap: 6 };",
    expect: ["overlaps LED of"],
  },
  {
    name: "pad numbers of the second row on the pads of the first",
    file: "panel-face.css",
    from: ".panel-root .mv .pad .n { top: -30px;",
    to: ".panel-root .mv .pad .n { top: -50px;",
    expect: [
      'text "9" overlaps control Pad 1',
      'text "16" overlaps control Pad 8',
    ],
  },
  {
    name: "Full view: the shifted label hung on the key's own label (Undo Rec / Undo Snd)",
    file: "panel.css",
    from: "top: calc(100% + 3px); margin: 0; white-space: nowrap; }",
    to: "top: calc(100% - 6px); margin: 0; white-space: nowrap; }",
    expect: ['full/1320px: text "Undo Rec" overlaps text "Undo Snd"'],
  },
];

function copyDir(from, to) {
  fs.rmSync(to, { recursive: true, force: true });
  fs.mkdirSync(to, { recursive: true });
  for (const name of fs.readdirSync(from))
    fs.copyFileSync(path.join(from, name), path.join(to, name));
}

let failed = 0;
VARIANTS.forEach((variant, i) => {
  const broken = path.join(workDir, "broken-ui-" + i);
  copyDir(uiDir, broken);
  const target = path.join(broken, variant.file);
  const text = fs.readFileSync(target, "utf8");
  if (!text.includes(variant.from))
    throw new Error(
      `${variant.file}: the text to break is not there: ${variant.from}`,
    );
  fs.writeFileSync(target, text.replace(variant.from, variant.to));
  const run = spawnSync(
    "node",
    [
      path.join(__dirname, "layout-overlap.js"),
      broken,
      path.join(workDir, "broken-run-" + i),
      repo,
      "--quick",
    ],
    { encoding: "utf8" },
  );
  const missing = variant.expect.filter((piece) => !run.stdout.includes(piece));
  if (run.status === 1 && missing.length === 0)
    console.log(
      `  broken (${variant.name}): refused, naming ${variant.expect.length} texts, as it must be`,
    );
  else {
    failed++;
    console.log(
      `  NOT SEEN (${variant.name}): exit ${run.status}, not named: ${missing.join(" | ")}\n${run.stderr}`,
    );
  }
});
if (failed) process.exit(1);
console.log(
  "every broken variant turns the overlap test red and names its texts",
);
