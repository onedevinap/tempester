/* Counter-proof of panel-machine.test.js: each deliberately broken copy of one of the Panel page's logic scripts must
   make the checks fail. `node counter-proof-view.js <ui folder> <scratch folder> <refs/panel_controls.tsv>`
   exits 1 when a variant is accepted or does not change its script. */
"use strict";
const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

const [uiDir, scratch, tsv] = process.argv.slice(2);
const FILES = [
  "panel-view.js",
  "panel-mode.js",
  "panel-link.js",
  "panel-machine.js",
];

const variants = {
  "mode decided at once, without waiting for the LEDs to settle": [
    "panel-mode.js",
    "if (touched && !announced) schedule();",
    "if (touched && !announced) settle();",
  ],
  "16 Tunings read as 16 Beats": [
    "panel-mode.js",
    "if (beats && steps && !sounds && !mutes) return 3;",
    "if (beats && steps && !sounds && !mutes) return 0;",
  ],
  "16 Mutes read as 16 Sounds": [
    "panel-mode.js",
    "if (mutes && !beats && !steps && !sounds) return 4;",
    "if (mutes && !beats && !steps && !sounds) return 2;",
  ],
  "announced mode ignored (the LEDs only)": [
    "panel-mode.js",
    "      announced = true;\n",
    "      return;\n",
  ],
  "announced mode waits for the settle delay": [
    "panel-mode.js",
    "      current = index;\n      onMode(current);\n    }",
    "      timer = setTimer(() => {\n        current = index;\n        onMode(current);\n      }, settleMs);\n    }",
  ],
  "LEDs override the announced mode": [
    "panel-mode.js",
    "if (touched && !announced) schedule();",
    "if (touched) schedule();",
  ],
  "announced mode of no Pad Function accepted": [
    "panel-mode.js",
    "index < 0 || index >= NAMES.length",
    "index < 0",
  ],
  "announcement kept after the panel is lost": [
    "panel-mode.js",
    "      announced = false;\n",
    "",
  ],
  "mode kept after the panel is lost": [
    "panel-mode.js",
    "if (current !== null) {",
    "if (false) {",
  ],
  "held keys not released when the view changes": [
    "panel-view.js",
    "releaseKeys();\n      view = name;",
    "view = name;",
  ],
  "restoring the saved view saves it again": [
    "panel-view.js",
    "zoomNow = clampZoom(savedZoom);",
    "zoomNow = clampZoom(savedZoom);\n      save(savedView, zoomNow);",
  ],
  "zoom not limited": [
    "panel-view.js",
    "return Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, stepped));",
    "return stepped;",
  ],
  "Cmd/Ctrl 0 ignored": [
    "panel-view.js",
    'if (e.key === "0") return "fit";',
    "",
  ],
  "pad sent as note 36 + pad (the bug of 09/11/2026)": [
    "panel-link.js",
    'bridge.send("pad", down ? "on" : "off", pad, down ? PAD_VELOCITY : 0);',
    'bridge.send("note", down ? "on" : "off", 36 + pad, down ? PAD_VELOCITY : 0);',
  ],
  "encoder steps throttled (steps lost)": [
    "panel-link.js",
    'bridge.send("encoder", index, step)',
    'throttled("encoder" + index, "encoder", index, step)',
  ],
  "absolute control sent to the next number": [
    "panel-link.js",
    'throttled("control" + index, "control", index, value)',
    'throttled("control" + index, "control", index + 1, value)',
  ],
  "Full view parameter value changed": [
    "panel-link.js",
    'throttled("nrpn" + index, "nrpn", index, value)',
    'throttled("nrpn" + index, "nrpn", index, value + 1)',
  ],
  "long encoder turn truncated to its first message": [
    "panel-machine.js",
    "link.encoder(e, chunk);\n        left -= chunk;",
    "link.encoder(e, chunk);\n        left = 0;",
  ],
  "encoder step every pixel": [
    "panel-machine.js",
    "ENC_PX_PER_STEP = 8",
    "ENC_PX_PER_STEP = 1",
  ],
  "absolute drag ten times too fast": [
    "panel-machine.js",
    "ABS_DRAG_PX = 150",
    "ABS_DRAG_PX = 15",
  ],
  "absolute value not limited to 255": [
    "panel-machine.js",
    "Math.max(0, Math.min(ABS_MAX, Math.round(n)))",
    "Math.max(0, Math.round(n))",
  ],
  "Lowpass Frequency mapped to Resonance": [
    "panel-machine.js",
    "5: 34 }",
    "5: 35 }",
  ],
  "machine report fights a knob being dragged": [
    "panel-machine.js",
    "guard.held(absKey(k)) || ",
    "",
  ],
  "encoder dot never moves": [
    "panel-machine.js",
    "ENDLESS_STEP_DEG = 15,",
    "ENDLESS_STEP_DEG = 0,",
  ],
  "encoder dot does not wrap after a turn": [
    "panel-machine.js",
    "(((detents * ENDLESS_STEP_DEG) % 360) + 360) % 360",
    "detents * ENDLESS_STEP_DEG",
  ],
  "ring segment always lit forward": [
    "panel-machine.js",
    "const sweep = (turned[e] - burst.flashStart) * ENDLESS_STEP_DEG;",
    "const sweep = Math.abs(turned[e] - burst.flashStart) * ENDLESS_STEP_DEG;",
  ],
  "ring segment never goes out": [
    "panel-machine.js",
    "age < FLASH_MS ? burst.flash : null",
    "burst.flash",
  ],
  "ring segment not restarted when the turn reverses": [
    "panel-machine.js",
    "Math.sign(detents) !== Math.sign(burst.flash.sweep)",
    "false",
  ],
  "ring segment not accumulated along a drag": [
    "panel-machine.js",
    "t - burst.at >= FLASH_MS ||",
    "true ||",
  ],
  "step count not accumulated": [
    "panel-machine.js",
    "burst.sum += detents;",
    "burst.sum = detents;",
  ],
  "step count never restarts": [
    "panel-machine.js",
    "if (t - burst.at > COUNT_WINDOW_MS) burst.sum = 0;",
    "",
  ],
  "step count never fades": [
    "panel-machine.js",
    'age < COUNT_WINDOW_MS ? formatCount(burst.sum) : ""',
    "formatCount(burst.sum)",
  ],
  "negative count printed with a plus": [
    "panel-machine.js",
    'sum < 0 ? "\\u2212" + -sum',
    'sum < 0 ? "+" + -sum',
  ],
  "NRPN attributed to an encoder without a recent turn": [
    "panel-machine.js",
    "now() - lastTurn.at <= ATTRIBUTION_MS &&",
    "",
  ],
  "attribution window ten times too long": [
    "panel-machine.js",
    "ATTRIBUTION_MS = 400",
    "ATTRIBUTION_MS = 4000",
  ],
  "attribution window closed at its last millisecond": [
    "panel-machine.js",
    "now() - lastTurn.at <= ATTRIBUTION_MS",
    "now() - lastTurn.at < ATTRIBUTION_MS",
  ],
  "caption kept when the control is turned again": [
    "panel-machine.js",
    "captions.delete(keyOf(kind, n));",
    "",
  ],
  "captions kept when the sound is synced": [
    "panel-machine.js",
    "clearCaptions: () => captions.clear(),",
    "clearCaptions: () => {},",
  ],
  "absolute knob loses the mapping by parameter number": [
    "panel-machine.js",
    "if (owner !== undefined) target",
    "if (false) target",
  ],
  "encoder takes every parameter the machine reports": [
    "panel-machine.js",
    "(lastTurn.index === null || lastTurn.index === index)",
    "true",
  ],
  "Level not greyed, Volume greyed instead": [
    "panel-machine.js",
    '["abs", 7, "Level"],',
    '["abs", 8, "Volume"],',
  ],
  "Delay Send greyed (it acts with Shift)": [
    "panel-machine.js",
    '["enc", 5, "Env Amount"],',
    '["enc", 5, "Env Amount"],\n    ["enc", 9, "Delay Send"],',
  ],
  "16 Mutes not seen as beat-wide": [
    "panel-machine.js",
    "BEAT_WIDE_MODES = [0, 4]",
    "BEAT_WIDE_MODES = [0]",
  ],
  "unknown mode greys the controls": [
    "panel-machine.js",
    "BEAT_WIDE_MODES.includes(mode) &&",
    "(BEAT_WIDE_MODES.includes(mode) || mode === null) &&",
  ],
  "grey kept after the mode changes": [
    "panel-machine.js",
    "setMode: (index) => (mode = index),",
    "setMode: (index) => (mode = mode === null ? index : mode),",
  ],
  "Distort knob given a number the firmware does not have": [
    "panel-machine.js",
    '["abs", 9, "Distort"',
    '["abs", 10, "Distort"',
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
  const folder = path.join(scratch, "variant");
  fs.rmSync(folder, { recursive: true, force: true });
  fs.mkdirSync(folder, { recursive: true });
  for (const f of FILES)
    fs.copyFileSync(path.join(uiDir, f), path.join(folder, f));
  fs.writeFileSync(path.join(folder, file), broken);
  const run = spawnSync(
    "node",
    [path.join(__dirname, "panel-machine.test.js"), folder, tsv],
    { encoding: "utf8" },
  );
  if (run.status === 0) {
    console.log("broken script accepted: " + name);
    failed += 1;
  }
}
console.log(Object.keys(variants).length + " variants checked");
process.exit(failed ? 1 : 0);
