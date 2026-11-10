/* Behaviour of the n°38 reports of the Panel page (Tempester 1.6.4.0: the machine reports its mixer, delay and main
   values) on a fake window, no DOM (run by tools/test_app.sh with node): `node panel-mixer.test.js <ui folder>`. The
   folder is a parameter so that panel-mixer-counter-proof.js can run the same checks on deliberately broken copies:
   they must fail there.
   Covers: each item's place on the Machine view (knob scale, encoder bubble), the Pan / Delay Repeats knob and the
   machine's Shift, the rule that a report never moves a knob under the user's hand (one guard, both views), the Full
   view following Volume, Distort and Compress only, and what panel-link.js sends for Distort and Compress. */
"use strict";
const fs = require("fs");
const vm = require("vm");
const path = require("path");
const assert = require("assert");

const [uiDir] = process.argv.slice(2);
const FILES = ["panel-link.js", "panel-machine.js"];

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
const plain = (v) => JSON.parse(JSON.stringify(v));

function rig() {
  const sent = [];
  const clock = { t: 10000 };
  const controls = T.panelMachine.createControls(
    {
      control: (k, v) => sent.push(["control", k, v]),
      encoder: (e, s) => sent.push(["encoder", e, s]),
    },
    () => clock.t,
  );
  return { controls, sent, clock };
}

const checks = {
  "Level, Pan and Volume sit on knobs 7, 6 and 8 at twice the 7-bit value":
    () => {
      const { controls } = rig();
      assert.strictEqual(controls.report("level", 72), true);
      assert.strictEqual(controls.report("pan", 10), true);
      assert.strictEqual(controls.report("volume", 127), true);
      assert.strictEqual(controls.abs(7), 144);
      assert.strictEqual(controls.abs(6), 20);
      assert.strictEqual(controls.abs(8), 254);
    },
  "Distort and Compress are the raw 0-255 value, on knobs 9 and 2": () => {
    const { controls } = rig();
    controls.report("distort", 200);
    controls.report("compress", 255);
    assert.strictEqual(controls.abs(9), 200);
    assert.strictEqual(controls.abs(2), 255);
    controls.report("distort", 999);
    assert.strictEqual(
      controls.abs(9),
      255,
      "never above the knob's own maximum",
    );
  },
  "a report never sends anything to the machine": () => {
    const { controls, sent } = rig();
    for (const item of Object.keys(T.panelMachine.MIXER_REPORTS))
      controls.report(item, 3);
    assert.deepStrictEqual(sent, []);
  },
  "a knob that already stands for the reported 7-bit value keeps its own byte":
    () => {
      const { controls, clock } = rig();
      controls.setAbs(7, 145);
      clock.t += 5000;
      assert.strictEqual(controls.report("level", 72), false);
      assert.strictEqual(controls.abs(7), 145);
      assert.strictEqual(controls.report("level", 73), true);
      assert.strictEqual(controls.abs(7), 146);
    },
  "Delay Repeats is on the Pan knob while Shift is lit, Pan otherwise": () => {
    const { controls } = rig();
    controls.report("pan", 10);
    controls.report("delay_repeats", 126);
    assert.strictEqual(controls.abs(6), 20, "Shift off: the knob is Pan's");
    controls.setShift(true);
    assert.strictEqual(controls.abs(6), 254, "Shift on: 126 repeats");
    controls.report("delay_repeats", 63);
    assert.strictEqual(controls.abs(6), 128);
    controls.report("pan", 64);
    assert.strictEqual(
      controls.abs(6),
      128,
      "a Pan report does not move the knob under Shift",
    );
    controls.setShift(false);
    assert.strictEqual(controls.abs(6), 128, "back to Pan 64");
    controls.report("pan", 65);
    assert.strictEqual(controls.abs(6), 130);
  },
  "a full state (Pan then Delay Repeats) ends on the knob Shift selects":
    () => {
      const { controls } = rig();
      controls.report("pan", 40);
      controls.report("delay_repeats", 100);
      assert.strictEqual(controls.abs(6), 80);
    },
  "encoders have no position: the bubble shows the number, then fades": () => {
    const { controls, clock } = rig();
    controls.report("tempo", 1215);
    assert.strictEqual(controls.encoderView(8).count, "121.5");
    controls.report("swing", 7);
    assert.strictEqual(controls.encoderView(8).count, "7");
    controls.report("delay_send", 64);
    assert.strictEqual(controls.encoderView(9).count, "64");
    controls.report("delay_time", 5);
    assert.strictEqual(controls.encoderView(9).count, "5");
    clock.t += 2000;
    assert.strictEqual(controls.encoderView(9).count, "");
    assert.strictEqual(
      controls.abs(9),
      null,
      "no knob moved by an encoder's report",
    );
  },
  "a tempo of 0 (unknown) shows nothing": () => {
    const { controls } = rig();
    assert.strictEqual(controls.report("tempo", 0), false);
    assert.strictEqual(controls.encoderView(8).count, "");
  },
  "a turn of the encoder is followed by the machine's own number": () => {
    const { controls, clock } = rig();
    controls.stepEncoder(8, 3);
    assert.strictEqual(controls.encoderView(8).count, "+3");
    clock.t += 50;
    controls.report("tempo", 1005);
    assert.strictEqual(controls.encoderView(8).count, "100.5");
    clock.t += 50;
    controls.stepEncoder(8, 1);
    assert.strictEqual(
      controls.encoderView(8).count,
      "+4",
      "a newer turn wins again",
    );
  },
  "an unknown item or a value that is not a number moves nothing": () => {
    const { controls } = rig();
    assert.strictEqual(controls.report("nonsense", 5), false);
    assert.strictEqual(controls.report("level", NaN), false);
    assert.strictEqual(controls.abs(7), null);
  },
  "a knob held by the pointer ignores reports, however long": () => {
    const { controls, clock } = rig();
    controls.report("level", 50);
    controls.hold(7);
    clock.t += 60000;
    assert.strictEqual(controls.report("level", 90), false);
    assert.strictEqual(controls.abs(7), 100);
  },
  "a knob moved in the last 400 ms ignores reports, then follows again": () => {
    const { controls, clock } = rig();
    controls.setAbs(8, 200);
    clock.t += T.panelMachine.REPORT_QUIET_MS - 1;
    assert.strictEqual(controls.report("volume", 10), false);
    assert.strictEqual(controls.abs(8), 200);
    clock.t += 2;
    assert.strictEqual(controls.report("volume", 10), true);
    assert.strictEqual(controls.abs(8), 20);
  },
  "letting go starts the quiet time again": () => {
    const { controls, clock } = rig();
    controls.hold(9);
    clock.t += 5000;
    controls.release(9);
    clock.t += 100;
    assert.strictEqual(controls.report("distort", 10), false);
    clock.t += 400;
    assert.strictEqual(controls.report("distort", 10), true);
  },
  "holding an encoder or a knob does not quiet another knob": () => {
    const { controls } = rig();
    controls.hold(7);
    assert.strictEqual(controls.report("volume", 10), true);
    assert.strictEqual(controls.report("compress", 10), true);
  },
  "the guard is one rule: held or touched, per key": () => {
    let t = 0;
    const g = T.panelMachine.createReportGuard(() => t);
    assert.strictEqual(g.busy("x"), false);
    g.touch("x");
    assert.strictEqual(g.busy("x"), true);
    assert.strictEqual(g.busy("y"), false);
    t = 401;
    assert.strictEqual(g.busy("x"), false);
    g.hold("x");
    t = 99999;
    assert.strictEqual(g.busy("x"), true);
  },
  "the Full view follows Volume, Distort and Compress only": () => {
    let t = 0;
    const g = T.panelMachine.createReportGuard(() => t);
    const set = [];
    const follow = (item, v) =>
      T.panelMachine.followInFullView(g, (n, x) => set.push([n, x]), item, v);
    for (const item of ["volume", "distort", "compress"])
      assert.strictEqual(follow(item, 9), true);
    for (const item of [
      "level",
      "pan",
      "delay_send",
      "delay_time",
      "delay_repeats",
      "tempo",
      "swing",
    ])
      assert.strictEqual(follow(item, 9), false, item);
    assert.deepStrictEqual(set, [
      ["volume", 9],
      ["distort", 9],
      ["compress", 9],
    ]);
  },
  "the Full view ignores a report for a knob in the user's hand": () => {
    let t = 0;
    const g = T.panelMachine.createReportGuard(() => t);
    const set = [];
    const follow = (item, v) =>
      T.panelMachine.followInFullView(g, (n, x) => set.push([n, x]), item, v);
    g.hold("distort");
    t = 10000;
    assert.strictEqual(follow("distort", 5), false);
    g.release("distort");
    assert.strictEqual(follow("distort", 5), false);
    t += 401;
    assert.strictEqual(follow("distort", 5), true);
    assert.strictEqual(follow("compress", 5), true, "another knob is free");
  },
  "Distort and Compress go out as controls 9 and 2 (0-255) with the remote panel":
    () => {
      const log = [];
      const bridge = {
        send: () => {},
        sendThrottled: (key, ms, ...c) => log.push(c),
      };
      const link = T.panelLink.create({
        bridge,
        connected: () => true,
        setTempo: () => {},
        remote: () => true,
      });
      link.local("distort", 200);
      link.local("compress", 255);
      link.local("volume", 100);
      assert.deepStrictEqual(plain(log), [
        ["control", 9, 200],
        ["control", 2, 255],
        ["control", 8, 200],
      ]);
    },
  "without the remote panel Distort and Compress are CC 12 / 13 cut at 127":
    () => {
      const log = [];
      const bridge = {
        send: () => {},
        sendThrottled: (key, ms, ...c) => log.push(c),
      };
      const link = T.panelLink.create({
        bridge,
        connected: () => true,
        setTempo: () => {},
        remote: () => false,
      });
      link.local("distort", 200);
      link.local("compress", 90);
      assert.deepStrictEqual(plain(log), [
        ["cc", 12, 127],
        ["cc", 13, 90],
      ]);
    },
  "the page never sends a report number to the machine": () => {
    const sent = [];
    const bridge = {
      send: (...c) => sent.push(c),
      sendThrottled: (key, ms, ...c) => sent.push(c),
    };
    const link = T.panelLink.create({
      bridge,
      connected: () => true,
      setTempo: () => {},
      remote: () => true,
    });
    for (let i = 0; i < 10; i++) link.control(i, 5);
    for (const name of ["distort", "compress", "volume"]) link.local(name, 5);
    link.knob(34, 5);
    const numbers = sent.filter((c) => c[0] === "nrpn").map((c) => c[1]);
    assert.ok(
      numbers.every((n) => n < 200),
      "sound parameters only",
    );
    assert.ok(sent.every((c) => c[0] !== "control" || c[1] <= 9));
  },
};

let failed = 0;
for (const [name, check] of Object.entries(checks)) {
  try {
    check();
  } catch (error) {
    failed += 1;
    console.log("FAIL " + name + ": " + error.message.split("\n")[0]);
  }
}
console.log(Object.keys(checks).length + " checks, " + failed + " failed");
process.exit(failed ? 1 : 0);
