/* Behaviour of the Tempester 1.6.2.0 / 1.6.3.0 performance controls of the Panel page on a fake window, no DOM (run by
   tools/test_app.sh with node): `node panel-perform.test.js <ui folder>`. The folder is a parameter so that
   panel-perform-counter-proof.js can run the same checks on deliberately broken copies: they must fail there.
   Covers: which OS gets which control (and the tooltip naming the version), the FX ribbon gestures (touch, move,
   release, mirror of the machine's ribbon), the pads' play probability (one message per gesture, read from a beat),
   what panel-link.js sends for them, and the Machine view's beat-wide knobs (n°29). */
"use strict";
const fs = require("fs");
const vm = require("vm");
const path = require("path");
const assert = require("assert");

const [uiDir] = process.argv.slice(2);
const FILES = ["panel-link.js", "panel-machine.js", "panel-perform.js"];

function load() {
  const window = {};
  const context = vm.createContext({ window });
  for (const file of FILES)
    vm.runInContext(fs.readFileSync(path.join(uiDir, file), "utf8"), context, {
      filename: file,
    });
  return window.Tempester;
}

const T = load();
/* Objects made inside the vm context have its prototypes: compare their JSON. */
const plain = (v) => JSON.parse(JSON.stringify(v));
const P = T.panelPerform;
let count = 0;
function check(name, fn) {
  fn();
  count++;
  void name;
}

/* ---------- Features ---------- */
const ON_163 = {
  beat_wide: { on: true, since: "1.6.2.0" },
  ribbons: { on: true, since: "1.6.3.0" },
  probability: { on: true, since: "1.6.3.0" },
};
const ON_162 = {
  beat_wide: { on: true, since: "1.6.2.0" },
  ribbons: { on: false, since: "1.6.3.0" },
  probability: { on: false, since: "1.6.3.0" },
};

check("1.6.3.0 has every control", () => {
  const f = P.createFeatures();
  f.set(ON_163);
  for (const name of ["beat_wide", "ribbons", "probability"])
    assert.deepStrictEqual(plain(f.status(name)), { on: true, why: "" });
});
check(
  "1.6.2.0 has the beat-wide positions only, the others name 1.6.3.0",
  () => {
    const f = P.createFeatures();
    f.set(ON_162);
    assert.strictEqual(f.status("beat_wide").on, true);
    for (const name of ["ribbons", "probability"]) {
      const s = f.status(name);
      assert.strictEqual(s.on, false);
      assert.ok(s.why.includes("Tempester 1.6.3.0"), s.why);
    }
  },
);
check("no record: nothing, and the tooltip says the OS is unknown", () => {
  const f = P.createFeatures();
  const s = f.status("ribbons");
  assert.strictEqual(s.on, false);
  assert.ok(s.why.includes("not known"), s.why);
});
check(
  "a DSI identity answer turns everything off whatever the record says",
  () => {
    const f = P.createFeatures();
    f.set(ON_163);
    f.setIdentity(false);
    const s = f.status("probability");
    assert.strictEqual(s.on, false);
    assert.ok(s.why.includes("DSI OS") && s.why.includes("1.6.3.0"), s.why);
    f.setIdentity(true);
    assert.strictEqual(f.status("probability").on, true);
  },
);
check("the fake machine plays 1.6.3.0", () => {
  const f = P.createFeatures();
  f.setFake(true);
  assert.strictEqual(f.status("ribbons").on, true);
});

/* ---------- Ribbons ---------- */
function ribbonRig(r = 1) {
  const sent = [];
  const ribbon = P.createRibbon(r, (...m) => sent.push(m));
  return { ribbon, sent };
}
check("a drag touches, moves, then releases once", () => {
  const { ribbon, sent } = ribbonRig(1);
  ribbon.touch(0.5);
  ribbon.touch(0.5);
  ribbon.touch(1.4);
  ribbon.touch(-1);
  assert.strictEqual(ribbon.release(), true);
  assert.strictEqual(ribbon.release(), false);
  assert.deepStrictEqual(sent, [
    [1, 64, undefined],
    [1, 127, undefined],
    [1, 0, undefined],
    [1, null],
  ]);
});
check("a pen's pressure goes with the position", () => {
  const { ribbon, sent } = ribbonRig(0);
  ribbon.touch(0, 0.5);
  ribbon.touch(0, 1);
  assert.deepStrictEqual(sent, [
    [0, 0, 64],
    [0, 0, 127],
  ]);
});
check("the machine's ribbon is shown, but not over the user's finger", () => {
  const { ribbon } = ribbonRig(0);
  assert.strictEqual(ribbon.report(90), true);
  assert.deepStrictEqual(plain(ribbon.state()), { position: 90, on: true });
  ribbon.report(null);
  assert.deepStrictEqual(plain(ribbon.state()), { position: 90, on: false });
  ribbon.touch(0.25);
  assert.strictEqual(ribbon.report(10), false);
  assert.strictEqual(ribbon.state().position, 32);
  ribbon.release();
  assert.strictEqual(ribbon.state().on, false);
});

/* ---------- Probability ---------- */
function probRig() {
  const sent = [];
  const prob = P.createProbability((...m) => sent.push(m));
  return { prob, sent };
}
check("a drag sends one value when it ends, only if it changed", () => {
  const { prob, sent } = probRig();
  prob.fromBeat([100, 50, ...new Array(30).fill(100)]);
  prob.begin(1);
  prob.drag(1, 30);
  prob.drag(1, -45);
  assert.deepStrictEqual(sent, []);
  assert.strictEqual(prob.commit(1), true);
  assert.deepStrictEqual(sent, [[1, 20]]);
  prob.begin(1);
  prob.drag(1, 0);
  assert.strictEqual(prob.commit(1), false);
  assert.strictEqual(sent.length, 1);
});
check("values stay in 0-100 and an unknown pad starts at 100", () => {
  const { prob, sent } = probRig();
  prob.nudge(3, 1);
  assert.strictEqual(prob.value(3), 100);
  prob.nudge(3, -250);
  prob.commit(3);
  assert.deepStrictEqual(sent, [[3, 0]]);
  prob.begin(4);
  prob.drag(4, 1000);
  assert.strictEqual(prob.value(4), 100);
});
check("a beat dump sets the first 16 pads; a report or reset follows", () => {
  const { prob } = probRig();
  const list = Array.from({ length: 32 }, (_, k) => 100 - k);
  prob.fromBeat(list);
  assert.strictEqual(prob.value(0), 100);
  assert.strictEqual(prob.value(15), 85);
  prob.report(15, 7);
  assert.strictEqual(prob.value(15), 7);
  prob.fromBeat(undefined);
  assert.strictEqual(prob.value(0), null);
});

/* ---------- panel-link.js ---------- */
function linkRig(connected = true) {
  const log = [];
  const link = T.panelLink.create({
    bridge: {
      send: (...c) => log.push(["send", ...c]),
      sendThrottled: (key, ms, ...c) => log.push(["throttled", key, ...c]),
    },
    connected: () => connected,
    setTempo: () => {},
  });
  return { link, log };
}
check(
  "ribbon touch and release share one throttle key, the probability is sent at once",
  () => {
    const { link, log } = linkRig();
    link.ribbon(1, 40);
    link.ribbon(1, 41, 99);
    link.ribbon(1, null);
    link.beatWide(2, 131);
    link.probability(4, 75);
    assert.deepStrictEqual(log, [
      ["throttled", "ribbon1", "ribbon", 1, "touch", 40],
      ["throttled", "ribbon1", "ribbon", 1, "touch", 41, 99],
      ["throttled", "ribbon1", "ribbon", 1, "release"],
      ["throttled", "beat_wide2", "beat_wide", 2, 131],
      ["send", "pad_probability", 4, 75],
    ]);
  },
);
check("nothing goes out when not connected", () => {
  const { link, log } = linkRig(false);
  link.ribbon(0, 1);
  link.beatWide(0, 1);
  link.probability(0, 1);
  assert.deepStrictEqual(log, []);
});

/* ---------- Beat-wide knobs of the Machine view ---------- */
function bwRig() {
  const sent = [];
  const controls = T.panelMachine.createControls(
    {
      control: (k, v) => sent.push(["control", k, v]),
      encoder: (e, s) => sent.push(["encoder", e, s]),
      beatWide: (i, p) => sent.push(["beat_wide", i, p]),
    },
    () => 1000,
  );
  return { controls, sent };
}
check(
  "the 8 controls are beat-wide only in 16 Beats / 16 Mutes with n°29",
  () => {
    const { controls } = bwRig();
    controls.setMode(0);
    assert.strictEqual(controls.beatWideOf("abs", 5), null);
    controls.setBeatWideAvailable(true);
    assert.strictEqual(controls.beatWideOf("abs", 5), 2);
    assert.strictEqual(controls.beatWideOf("enc", 1), 0);
    assert.strictEqual(controls.beatWideOf("enc", 6), 7);
    assert.strictEqual(controls.beatWideOf("abs", 7), null);
    controls.setMode(4);
    assert.strictEqual(controls.beatWideOf("abs", 1), 5);
    controls.setMode(2);
    assert.strictEqual(controls.beatWideOf("abs", 1), null);
    const mapped = T.panelMachine.BEAT_WIDE.map((row) => row[2]).sort();
    assert.deepStrictEqual(plain(mapped), [0, 1, 2, 3, 4, 5, 6, 7]);
  },
);
check(
  "turning one sends its position, from neutral when unknown, clamped to 0-254",
  () => {
    const { controls, sent } = bwRig();
    controls.nudgeBeatWide(2, 1);
    controls.dragBeatWide(2, 129, 600);
    controls.dragBeatWide(2, 129, -600);
    controls.dragBeatWide(2, 129, -600);
    assert.deepStrictEqual(sent, [
      ["beat_wide", 2, 129],
      ["beat_wide", 2, 254],
      ["beat_wide", 2, 0],
    ]);
    assert.deepStrictEqual(plain(controls.caption("abs", 5)), {
      text: { name: "LP Freq", value: "−127" },
    });
  },
);
check("a reported position moves the knob, not while the user holds it", () => {
  const { controls, sent } = bwRig();
  assert.strictEqual(controls.reportBeatWide(7, 140), true);
  assert.strictEqual(controls.beatWide(7), 140);
  assert.strictEqual(controls.caption("enc", 6), null);
  controls.nudgeBeatWide(7, 1);
  sent.length = 0;
  controls.reportBeatWide(7, 140);
  assert.deepStrictEqual(plain(controls.caption("enc", 6)), {
    text: { name: "Decay", value: "+13" },
  });
  controls.holdBeatWide(7);
  assert.strictEqual(controls.reportBeatWide(7, 3), false);
  controls.releaseBeatWide(7);
  assert.strictEqual(controls.reportBeatWide(7, 127), true);
  assert.strictEqual(controls.caption("enc", 6).text.value, "0");
  assert.deepStrictEqual(sent, []);
});

console.log(count + " checks");
