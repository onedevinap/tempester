/* Counter-proof of layout-stability.js: on deliberately broken copies of the page, the test must fail.
     1. a label that follows its value ("99 BPM") loses its fixed width,
     2. an encoder's count is put back in the flow of a box with no fixed size (the count changes the box),
     3. the readouts of the bar lose their fixed slots,
     4. the notice goes back above the face; a touched FX ribbon or the probability mode of the pads changes a box.
     5. a row of keys no longer on one line, small knobs off the knobs' centre line, a Machine key off its row (alignment).
     6. the transport not as on the machine (« and » as wide as Rec, a LED of REC / PLAY missing or on its title) or its
        LEDs not lit by the LED events 0x23 / 0x24 (transportRow, transportLeds).
   A variant that does not turn the test red means the test could not see that defect.

   usage: node layout-counter-proof.js <ui dir> <work dir> <repo root> */
"use strict";
const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

const [uiDir, workDir, repo] = process.argv.slice(2, 5);

const VARIANTS = [
  {
    name: "BPM label as wide as its text, no fixed slot",
    file: "panel.css",
    from: ".panel-root .pot .l1.dyn { display: inline-block; min-width: 7.5ch;",
    to: ".panel-root .pot .l1.dyn { display: inline-block; font-size: 28px; min-width: 0;",
  },
  {
    name: "encoder count in the flow of an unsized box",
    file: "panel-face.css",
    from: ".mv .mvpot .cnt { position: absolute;",
    to: ".mv .mvpot .cnt { position: static; height: auto;",
    also: {
      file: "panel-machine.js",
      from: "at(d, cx - F.KNOB.box / 2, cy - F.KNOB.box / 2, F.KNOB.box, F.KNOB.box);",
      to: "at(d, cx - F.KNOB.box / 2, cy - F.KNOB.box / 2);",
    },
  },
  {
    name: "bar readouts without fixed slots",
    file: "panel.css",
    from: ".panelbar .slot-status { flex: 0 0 100px; width: 100px; }",
    to: ".panelbar .slot-status { flex: 0 0 auto; width: auto; }",
  },
  {
    name: "toolbar back to a wide row (a 600 px status slot)",
    file: "panel.css",
    from: ".panelbar .slot-status { flex: 0 0 100px; width: 100px; }",
    to: ".panelbar .slot-status { flex: 0 0 600px; width: 600px; }",
  },
  {
    name: "Rec key narrower than Stop and Play",
    file: "panel-face.js",
    from: "      TRANSPORT_KEY.w,\n      TRANSPORT_KEY.h,\n      kind,",
    to: "      TRANSPORT_KEY.w - (kind === \"rec\" ? 12 : 0),\n      TRANSPORT_KEY.h,\n      kind,",
  },
  {
    name: "Play key lower than Rec and Stop",
    file: "panel-face.js",
    from: "      TRANSPORT_KEY.x + i * TRANSPORT_KEY.pitch,\n      TKEY,",
    to: "      TRANSPORT_KEY.x + i * TRANSPORT_KEY.pitch,\n      TKEY + (kind === \"play\" ? 6 : 0),",
  },
  /* The transport as the author saw it on the machine (2026-11-09): the earlier "five keys one size" put back, a LED
     of REC / PLAY missing, on its title, or not driven by its LED event. */
  {
    name: "the five transport keys one size again (« and » as wide as Rec)",
    file: "panel-face.js",
    from: "const TRANSPORT_SMALL = { w: 24, h: 47,",
    to: "const TRANSPORT_SMALL = { w: 62, h: 45,",
  },
  {
    name: "the « and » keys of the Full view as wide as Rec",
    file: "panel.css",
    from: ".panel-root .tb.small button { width: 20px; height: 40px; }",
    to: ".panel-root .tb.small button { width: 58px; height: 36px; }",
  },
  {
    name: "no LED right of the PLAY title",
    file: "panel-face.js",
    from: '    [0x24, 1822, TLAB - LED_SIZE.h / 2, "play"],\n',
    to: "",
  },
  {
    name: "the REC LED on its title",
    file: "panel-face.js",
    from: '[0x23, 1598, TLAB',
    to: '[0x23, 1584, TLAB',
  },
  {
    name: "the LEDs of REC and PLAY not driven by the LED events (Machine view)",
    file: "panel-machine.js",
    from: 'const i = el("i", "tled " + colour);\n      i.dataset.led = led;',
    to: 'const i = el("i", "tled " + colour);\n      i.dataset.lamp = led;',
  },
  {
    name: "the REC LED of the Full view lit by 0x14 instead of 0x23",
    file: "panel.js",
    from: 'Rec: [0x23, "rec"]',
    to: 'Rec: [0x14, "rec"]',
  },
  {
    name: "the REC LED blue when lit, not red",
    file: "panel.css",
    from: '.panel-root .tled.rec[aria-pressed="true"] { background: #ff6a5f;',
    to: '.panel-root .tled.rec[aria-pressed="true"] { background: #8fd0ff;',
  },
  {
    name: "Pad Options keys 20 px lower than the keys of the other function-key groups",
    file: "panel.css",
    from: ".panel-root .fonctions .grp > .etendu.haut.vide,",
    to: ".panel-root .fonctions .grp:nth-child(2) > .cles { margin-top: 20px; }\n.panel-root .fonctions .grp > .etendu.haut.vide,",
  },
  {
    name: "knobs of different sizes out of the common 56 px slot",
    file: "panel.css",
    from: ".panel-root .pot svg { width: 46px; height: 46px; margin: 5px 0;",
    to: ".panel-root .pot svg { width: 46px; height: 46px; margin: 0;",
  },
  {
    name: "one key of the Machine view 5 px off its row",
    file: "panel-face.js",
    from: '["16 Beats", 466, K1,',
    to: '["16 Beats", 466, K1 + 5,',
  },
  /* The three alignment defects the author saw on the previous Machine view, as named cases. */
  {
    name: "Lowpass: the Audio Mod knob sits higher than Frequency and Resonance (its label wrapped)",
    file: "panel-face.js",
    from: '["abs", 3, 1036, KNOB_B1]',
    to: '["abs", 3, 1036, KNOB_B1 - 14]',
  },
  {
    name: "LFO 1&2: the Mod Paths key lower than the LFO2 key (its label wrapped)",
    file: "panel-face.js",
    from: '["Mod Paths", 872.5, KEY2,',
    to: '["Mod Paths", 872.5, KEY2 + 10,',
  },
  {
    name: "Envelopes: the Amount knob off the row of the Attack and Decay knobs",
    file: "panel-face.js",
    from: '["enc", 5, 618, KNOB_B2]',
    to: '["enc", 5, 618, KNOB_B2 - 11]',
  },
  {
    name: "Envelopes: the AD Mode key not on the column of the Pitch key below it",
    file: "panel-face.js",
    from: '["AD Mode", 183, B2 + 52,',
    to: '["AD Mode", 198, B2 + 52,',
  },
  {
    name: "a touched FX ribbon gets wider",
    file: "panel.css",
    from: ".panel-root .ruban:not(.touche) .leds::after { opacity: .35; }",
    to: ".panel-root .ruban:not(.touche) .leds::after { opacity: .35; }\n.panel-root .ruban.touche { width: 60px; }",
  },
  {
    name: "the probability mode puts the value in the flow of the pads' zone",
    file: "panel.css",
    from: ".panel-root.probmode .pad .prob { display: block; position: absolute;",
    to: ".panel-root.probmode .sec.pads { padding-top: 40px; }\n.panel-root.probmode .pad .prob { display: block; position: absolute;",
  },
  {
    name: "notice above the face",
    file: "page-panel.js",
    from: "      fullHost,\n      machineHost,\n      notice,\n      info,",
    to: "      notice,\n      fullHost,\n      machineHost,\n      info,",
  },
];

function copyDir(from, to) {
  fs.rmSync(to, { recursive: true, force: true });
  fs.mkdirSync(to, { recursive: true });
  for (const name of fs.readdirSync(from))
    fs.copyFileSync(path.join(from, name), path.join(to, name));
}

function replaceOnce(dir, file, from, to) {
  const target = path.join(dir, file);
  const text = fs.readFileSync(target, "utf8");
  if (!text.includes(from))
    throw new Error(`${file}: the text to break is not there: ${from}`);
  fs.writeFileSync(target, text.replace(from, to));
}

let failed = 0;
VARIANTS.forEach((variant, i) => {
  const broken = path.join(workDir, "broken-ui-" + i);
  copyDir(uiDir, broken);
  replaceOnce(broken, variant.file, variant.from, variant.to);
  if (variant.also)
    replaceOnce(broken, variant.also.file, variant.also.from, variant.also.to);
  const run = spawnSync(
    "node",
    [
      path.join(__dirname, "layout-stability.js"),
      broken,
      path.join(workDir, "broken-run-" + i),
      repo,
      "--quick",
    ],
    { encoding: "utf8" },
  );
  if (run.status === 1)
    console.log(`  broken (${variant.name}): refused, as it must be`);
  else {
    failed++;
    console.log(
      `  NOT SEEN (${variant.name}): exit ${run.status}\n${run.stdout}${run.stderr}`,
    );
  }
});
if (failed) process.exit(1);
console.log("every broken variant turns the layout test red");
