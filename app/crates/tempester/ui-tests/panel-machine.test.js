/* Behaviour of the Panel page's logic modules on a fake window, no DOM (run by tools/test_app.sh with node):
   `node panel-machine.test.js <ui folder> <refs/panel_controls.tsv>`. The folder is a parameter so that
   counter-proof-view.js can run the same checks on a folder holding a deliberately broken copy: they must fail there.
   Covers: the Pad Function mode read from the LEDs (with the off/on sequence settling), the Machine-view controls and
   what they send, the view / zoom state, and that the Full view's commands do not depend on the view. */
"use strict";
const fs = require("fs");
const vm = require("vm");
const path = require("path");
const assert = require("assert");

const [uiDir, tsvPath] = process.argv.slice(2);
const FILES = [
  "panel-view.js",
  "panel-mode.js",
  "panel-link.js",
  "panel-machine.js",
];

function load() {
  const window = {};
  const context = vm.createContext({ window });
  for (const file of FILES)
    vm.runInContext(fs.readFileSync(path.join(uiDir, file), "utf8"), context, {
      filename: file,
    });
  return window.Tempester;
}

/* A bridge that records what the page would send to Rust; throttling is recorded, not timed. */
function fakeBridge() {
  const log = [];
  return {
    log,
    send: (...c) => log.push(["send", ...c]),
    sendThrottled: (key, ms, ...c) => log.push(["throttled", key, ...c]),
  };
}

function fakeTimers() {
  let next = 1;
  const pending = new Map();
  return {
    setTimer: (fn) => {
      pending.set(next, fn);
      return next++;
    },
    clearTimer: (id) => pending.delete(id),
    fire: () => {
      const all = [...pending.values()];
      pending.clear();
      all.forEach((fn) => fn());
    },
    count: () => pending.size,
  };
}

function modeRig(T) {
  const timers = fakeTimers();
  const seen = [];
  const mode = T.panelMode.create({
    onMode: (m) => seen.push(m),
    setTimer: timers.setTimer,
    clearTimer: timers.clearTimer,
  });
  return { mode, seen, timers };
}

function controlsRig(T) {
  const sent = [];
  const clock = { t: 1000 };
  const controls = T.panelMachine.createControls(
    {
      control: (k, v) => sent.push(["control", k, v]),
      encoder: (e, s) => sent.push(["encoder", e, s]),
    },
    () => clock.t,
  );
  return { controls, sent, clock };
}

const T = load();

/* Objects built by the scripts live in their own vm context: compare them as plain data. */
const same = (actual, expected, message) =>
  assert.deepStrictEqual(JSON.parse(JSON.stringify(actual)), expected, message);

function tsvRows() {
  return fs
    .readFileSync(tsvPath, "utf8")
    .trim()
    .split("\n")
    .slice(1)
    .map((line) => line.split("\t"))
    .map(([kind, number, nrpn, name]) => ({
      kind: kind === "encoder" ? "enc" : "abs",
      n: +number,
      nrpn: +nrpn,
      name,
    }));
}

const checks = {
  /* ----- Pad Function mode from the LEDs ----- */
  "mode: each single LED and each pair decodes to its Pad Function": () => {
    const lit = (...leds) => new Set(leds);
    const d = T.panelMode.decode;
    assert.strictEqual(d(lit(0x10)), 0);
    assert.strictEqual(d(lit(0x11)), 1);
    assert.strictEqual(d(lit(0x18)), 2);
    assert.strictEqual(d(lit(0x10, 0x11)), 3);
    assert.strictEqual(d(lit(0x19)), 4);
    assert.strictEqual(d(lit(0x18, 0x19)), 5);
    assert.deepStrictEqual(
      [0, 1, 2, 3, 4, 5].map((i) => T.panelMode.NAMES[i]),
      [
        "16 Beats",
        "16 Time Steps",
        "16 Sounds",
        "16 Tunings",
        "16 Mutes",
        "16 Levels",
      ],
    );
  },
  "mode: no LED, or a combination no mode produces, is unknown": () => {
    const d = T.panelMode.decode;
    assert.strictEqual(d(new Set()), null);
    assert.strictEqual(d(new Set([0x10, 0x18])), null);
    assert.strictEqual(d(new Set([0x10, 0x11, 0x18])), null);
    assert.strictEqual(d(new Set([0x10, 0x11, 0x18, 0x19])), null);
  },
  "mode: the off then on sequence is only decided once the LEDs settle": () => {
    const { mode, seen, timers } = modeRig(T);
    mode.feed([[0x10, 1]]);
    timers.fire();
    assert.deepStrictEqual(seen, [0]);
    /* the machine switches to 16 Sounds: all four off, then the new one on */
    mode.feed([
      [0x10, 0],
      [0x11, 0],
    ]);
    mode.feed([
      [0x18, 0],
      [0x19, 0],
    ]);
    assert.deepStrictEqual(seen, [0], "nothing decided mid-sequence");
    mode.feed([[0x18, 1]]);
    assert.deepStrictEqual(seen, [0], "still waiting for the LEDs to settle");
    assert.strictEqual(
      timers.count(),
      1,
      "one timer, restarted at each change",
    );
    timers.fire();
    assert.deepStrictEqual(seen, [0, 2]);
  },
  "mode: an unchanged mode is not announced twice": () => {
    const { mode, seen, timers } = modeRig(T);
    mode.feed([[0x19, 1]]);
    timers.fire();
    mode.feed([
      [0x19, 0],
      [0x19, 1],
    ]);
    timers.fire();
    assert.deepStrictEqual(seen, [4]);
  },
  "mode: LEDs of other keys do not touch the mode": () => {
    const { mode, seen, timers } = modeRig(T);
    mode.feed([
      [0x2c, 1],
      [0x24, 1],
    ]);
    assert.strictEqual(timers.count(), 0);
    assert.deepStrictEqual(seen, []);
  },
  "mode: losing the panel forgets the mode": () => {
    const { mode, seen, timers } = modeRig(T);
    mode.feed([[0x10, 1]]);
    timers.fire();
    mode.reset();
    assert.deepStrictEqual(seen, [0, null]);
    assert.strictEqual(mode.mode(), null);
  },

  /* ----- Machine view: what the controls send ----- */
  "machine: the knob table is exactly the firmware's 14 encoders and 10 absolute controls":
    () => {
      const rows = tsvRows();
      const table = T.panelMachine.CONTROLS;
      assert.strictEqual(table.length, rows.length);
      for (const row of rows) {
        const found = table.filter((c) => c.kind === row.kind && c.n === row.n);
        assert.strictEqual(found.length, 1, `${row.kind} ${row.n}`);
        const base = row.kind === "enc" ? 200 : 220;
        assert.strictEqual(row.nrpn, base + row.n, `NRPN of ${row.name}`);
      }
    },
  "machine: an absolute knob dragged up sends its control number and a 0-255 value":
    () => {
      const { controls, sent } = controlsRig(T);
      controls.dragAbs(
        5,
        null,
        30,
      ); /* position unknown: starts in the middle */
      assert.deepStrictEqual(sent, [["control", 5, 179]]);
      controls.dragAbs(5, 179, 500);
      assert.deepStrictEqual(sent.at(-1), ["control", 5, 255]);
      controls.dragAbs(5, 255, -9999);
      assert.deepStrictEqual(sent.at(-1), ["control", 5, 0]);
    },
  "machine: an absolute knob keeps its last position and nudges by one machine step (2 units)":
    () => {
      const { controls, sent } = controlsRig(T);
      assert.strictEqual(controls.abs(8), null);
      controls.setAbs(8, 100);
      controls.nudgeAbs(8, 1);
      controls.nudgeAbs(8, -2);
      assert.strictEqual(controls.abs(8), 98);
      assert.deepStrictEqual(
        sent.map((s) => s[2]),
        [100, 102, 98],
      );
    },
  "machine: an encoder sends relative steps, one per 8 pixels, none lost":
    () => {
      const { controls, sent } = controlsRig(T);
      controls.turnEncoder(1, 5);
      assert.deepStrictEqual(sent, [], "5 px is not a step yet");
      controls.turnEncoder(1, 5);
      assert.deepStrictEqual(sent, [["encoder", 1, 1]]);
      controls.turnEncoder(1, -20);
      assert.deepStrictEqual(sent.at(-1), ["encoder", 1, -2]);
      /* the remainders add up: -0.25, then -0.75 (no step yet), then -1.25 */
      controls.turnEncoder(1, -4);
      assert.strictEqual(sent.length, 2);
      controls.turnEncoder(1, -4);
      assert.deepStrictEqual(sent.at(-1), ["encoder", 1, -1]);
    },
  "machine: a long encoder turn leaves in messages of at most +15 / -16 detents":
    () => {
      const { controls, sent } = controlsRig(T);
      controls.stepEncoder(6, 40);
      controls.stepEncoder(6, -40);
      assert.deepStrictEqual(
        sent.map((s) => s[2]),
        [15, 15, 10, -16, -16, -8],
      );
    },
  "machine: an encoder has no position, only the turning is counted": () => {
    const { controls } = controlsRig(T);
    controls.stepEncoder(0, 3);
    controls.stepEncoder(0, -1);
    assert.strictEqual(controls.turned(0), 2);
    assert.strictEqual(
      controls.abs(0),
      null,
      "no absolute value for an encoder",
    );
  },
  "machine: a per-parameter NRPN moves the matching absolute knob, others not":
    () => {
      const { controls, sent } = controlsRig(T);
      assert.strictEqual(controls.paramToAbs(34, 82, 164), 5);
      assert.strictEqual(controls.abs(5), 128);
      assert.strictEqual(controls.paramToAbs(33, 127, 127), 0);
      assert.strictEqual(controls.abs(0), 255);
      assert.strictEqual(controls.paramToAbs(7, 50, 100), null);
      assert.deepStrictEqual(
        sent,
        [],
        "what the machine reports is never sent back",
      );
    },
  "machine: a knob being dragged ignores the machine's own reports": () => {
    const { controls } = controlsRig(T);
    controls.setAbs(5, 10);
    controls.hold(5);
    assert.strictEqual(controls.paramToAbs(34, 164, 164), null);
    assert.strictEqual(controls.abs(5), 10);
    controls.release(5);
    assert.strictEqual(controls.paramToAbs(34, 164, 164), 5);
  },
  "machine: the Machine view's knobs reach Rust as control and encoder commands":
    () => {
      const bridge = fakeBridge();
      const link = T.panelLink.create({
        bridge,
        connected: () => true,
        setTempo: () => {},
      });
      const controls = T.panelMachine.createControls(link);
      controls.setAbs(4, 90);
      controls.stepEncoder(3, 2);
      controls.stepEncoder(3, 1);
      assert.deepStrictEqual(bridge.log, [
        ["throttled", "control4", "control", 4, 90],
        ["send", "encoder", 3, 2],
        ["send", "encoder", 3, 1],
      ]);
    },
  /* ----- Machine view: encoder feedback (dot, ring segment, count) ----- */
  "encoder view: the dot moves by one fixed angle per detent, in both directions, and wraps":
    () => {
      const { controls } = controlsRig(T);
      const step = T.panelMachine.ENDLESS_STEP_DEG;
      assert.strictEqual(controls.encoderView(4).angle, 0);
      controls.stepEncoder(4, 1);
      assert.strictEqual(controls.encoderView(4).angle, step);
      controls.stepEncoder(4, 2);
      assert.strictEqual(controls.encoderView(4).angle, 3 * step);
      controls.stepEncoder(4, -4);
      assert.strictEqual(controls.encoderView(4).angle, 360 - step);
      controls.stepEncoder(4, 1 + 360 / step);
      assert.strictEqual(
        controls.encoderView(4).angle,
        0,
        "a whole turn is back at the start",
      );
      assert.strictEqual(controls.encoderView(5).angle, 0, "one dot per encoder");
    },
  "encoder view: the ring segment covers the last turn in its direction, then goes out":
    () => {
      const { controls, clock } = controlsRig(T);
      const step = T.panelMachine.ENDLESS_STEP_DEG;
      assert.strictEqual(controls.encoderView(1).flash, null);
      controls.stepEncoder(1, 2);
      same(controls.encoderView(1).flash, { from: 0, sweep: 2 * step });
      clock.t += 100;
      controls.stepEncoder(1, 1);
      same(
        controls.encoderView(1).flash,
        { from: 0, sweep: 3 * step },
        "a drag adds up while the light is on",
      );
      controls.stepEncoder(1, -1);
      same(
        controls.encoderView(1).flash,
        { from: 3 * step, sweep: -step },
        "a reversal restarts the segment, backwards",
      );
      controls.stepEncoder(1, 100);
      assert.ok(Math.abs(controls.encoderView(1).flash.sweep) < 360);
      clock.t += 449;
      assert.notStrictEqual(controls.encoderView(1).flash, null);
      clock.t += 2;
      assert.strictEqual(controls.encoderView(1).flash, null);
    },
  "encoder view: steps of one burst add up to a signed count that fades and restarts":
    () => {
      const { controls, clock } = controlsRig(T);
      assert.strictEqual(controls.encoderView(6).count, "");
      controls.stepEncoder(6, 1);
      clock.t += 300;
      controls.stepEncoder(6, 2);
      assert.strictEqual(controls.encoderView(6).count, "+3");
      clock.t += 300;
      controls.stepEncoder(6, -5);
      assert.strictEqual(controls.encoderView(6).count, "−2");
      assert.strictEqual(controls.encoderView(7).count, "", "per encoder");
      clock.t += 701;
      assert.strictEqual(controls.encoderView(6).count, "", "faded out");
      controls.stepEncoder(6, -1);
      assert.strictEqual(
        controls.encoderView(6).count,
        "−1",
        "a new burst starts from zero",
      );
    },

  /* ----- Machine view: the value the machine reports for the control just turned ----- */
  "value: the next parameter NRPN after an encoder step is that encoder's, with the parameter number and value":
    () => {
      const { controls } = controlsRig(T);
      controls.stepEncoder(6, 1);
      same(controls.attribute(68, 64), {
        kind: "enc",
        n: 6,
      });
      same(controls.caption("enc", 6), {
        index: 68,
        value: 64,
      });
      assert.strictEqual(controls.caption("enc", 7), null);
    },
  "value: an absolute knob that was moved owns the NRPN, and the others keep the mapping by number":
    () => {
      const { controls } = controlsRig(T);
      controls.setAbs(5, 100);
      same(controls.attribute(34, 82), { kind: "abs", n: 5 });
      same(controls.caption("abs", 5), {
        index: 34,
        value: 82,
      });
      controls.stepEncoder(3, 1);
      same(
        controls.attribute(35, 20),
        { kind: "abs", n: 4 },
        "Resonance belongs to its knob, not to the encoder turned just before",
      );
      assert.strictEqual(controls.caption("enc", 3), null);
    },
  "value: a parameter NRPN without a recent turn is attributed to no encoder": () => {
    const { controls, clock } = controlsRig(T);
    assert.strictEqual(controls.attribute(68, 64), null, "never turned");
    controls.stepEncoder(6, 1);
    clock.t += T.panelMachine.ATTRIBUTION_MS + 1;
    assert.strictEqual(controls.attribute(68, 64), null, "window over");
    assert.strictEqual(controls.caption("enc", 6), null);
  },
  "value: the window is 400 ms from the last step, and every step renews it": () => {
    const { controls, clock } = controlsRig(T);
    assert.strictEqual(T.panelMachine.ATTRIBUTION_MS, 400);
    controls.stepEncoder(2, 1);
    clock.t += 400;
    assert.notStrictEqual(controls.attribute(28, 10), null, "at 400 ms");
    clock.t += 300;
    controls.stepEncoder(2, 1);
    clock.t += 300;
    assert.notStrictEqual(controls.attribute(28, 11), null, "renewed");
  },
  "value: an encoder keeps the first parameter it reports, a later other one is not its caption":
    () => {
      const { controls } = controlsRig(T);
      controls.stepEncoder(1, 1);
      controls.attribute(0, 40);
      assert.strictEqual(controls.attribute(6, 40), null);
      assert.strictEqual(controls.attribute(0, 41).n, 1);
      same(controls.caption("enc", 1), {
        index: 0,
        value: 41,
      });
    },
  "value: the caption stays until the control is turned again, or the sound is synced": () => {
    const { controls, clock } = controlsRig(T);
    controls.stepEncoder(6, 1);
    controls.attribute(68, 64);
    clock.t += 60000;
    assert.notStrictEqual(controls.caption("enc", 6), null, "kept");
    controls.stepEncoder(6, 1);
    assert.strictEqual(controls.caption("enc", 6), null, "turned again");
    controls.attribute(68, 70);
    controls.setAbs(0, 10);
    controls.attribute(33, 5);
    controls.clearCaptions();
    assert.strictEqual(controls.caption("enc", 6), null, "synced");
    assert.strictEqual(controls.caption("abs", 0), null, "synced");
  },

  /* ----- Machine view: greying by Pad Function mode ----- */
  "greying: only the proven inert controls are inert, in 16 Beats and 16 Mutes": () => {
    const inert = (mode) =>
      T.panelMachine.CONTROLS.filter((c) =>
        T.panelMachine.isInert(c.kind, c.n, mode),
      ).map((c) => c.kind + c.n);
    const expected = ["enc0", "enc2", "enc3", "enc4", "enc5", "abs7"];
    same(inert(0), expected);
    same(inert(4), expected);
    for (const mode of [1, 2, 3, 5]) same(inert(mode), []);
    same(T.panelMachine.BEAT_WIDE_MODES, [0, 4]);
  },
  "greying: global controls, beat-wide ones and the soft knobs stay active": () => {
    for (const [kind, n] of [
      ["enc", 1],
      ["enc", 6],
      ["enc", 7],
      ["enc", 8],
      ["enc", 9],
      ["enc", 10],
      ["enc", 13],
      ["abs", 0],
      ["abs", 2],
      ["abs", 6],
      ["abs", 8],
      ["abs", 9],
    ])
      assert.ok(!T.panelMachine.isInert(kind, n, 0), kind + n);
  },
  "greying: an unknown mode greys nothing": () => {
    for (const mode of [null, undefined, 6, -1])
      assert.ok(!T.panelMachine.isInert("enc", 0, mode), String(mode));
    const { controls } = controlsRig(T);
    assert.ok(!controls.inert("abs", 7), "no mode read yet");
  },
  "greying: follows the mode chip once the LEDs settle, and back to active on a change": () => {
    const { controls } = controlsRig(T);
    const { mode, timers } = (() => {
      const timers = fakeTimers();
      const mode = T.panelMode.create({
        onMode: (m) => controls.setMode(m),
        setTimer: timers.setTimer,
        clearTimer: timers.clearTimer,
      });
      return { mode, timers };
    })();
    mode.feed([[0x10, 1]]);
    assert.ok(!controls.inert("abs", 7), "not before the LEDs settle");
    timers.fire();
    assert.ok(controls.inert("abs", 7), "16 Beats");
    mode.feed([
      [0x10, 0],
      [0x11, 0],
      [0x18, 0],
      [0x19, 0],
    ]);
    assert.ok(controls.inert("abs", 7), "mid-sequence: still the old mode");
    mode.feed([[0x18, 1]]);
    timers.fire();
    assert.ok(!controls.inert("abs", 7), "16 Sounds: active again");
    mode.feed([
      [0x18, 0],
      [0x19, 1],
    ]);
    timers.fire();
    assert.ok(controls.inert("enc", 5), "16 Mutes");
    mode.reset();
    assert.ok(!controls.inert("enc", 5), "panel lost: nothing greyed");
  },

  "link: nothing is sent while the machine is not connected": () => {
    const bridge = fakeBridge();
    const link = T.panelLink.create({
      bridge,
      connected: () => false,
      setTempo: () => {},
    });
    link.knob(34, 5);
    link.control(1, 5);
    link.encoder(1, 1);
    link.key(31, true);
    assert.deepStrictEqual(bridge.log, []);
  },

  /* ----- View and zoom ----- */
  "view: the zoom stays in 60-200 % by steps of 10 and a bad value is the fit":
    () => {
      const V = T.panelView;
      assert.strictEqual(V.clampZoom(100), 100);
      assert.strictEqual(V.clampZoom(10), 60);
      assert.strictEqual(V.clampZoom(900), 200);
      assert.strictEqual(V.clampZoom(134), 130);
      assert.strictEqual(V.clampZoom("x"), 100);
      assert.strictEqual(V.clampZoom(NaN), 100);
    },
  "view: Cmd/Ctrl plus, minus and zero are the zoom shortcuts, nothing else is":
    () => {
      const s = T.panelView.zoomShortcut;
      assert.strictEqual(s({ metaKey: true, key: "+" }), "in");
      assert.strictEqual(s({ ctrlKey: true, key: "=" }), "in");
      assert.strictEqual(s({ ctrlKey: true, key: "-" }), "out");
      assert.strictEqual(s({ metaKey: true, key: "0" }), "fit");
      assert.strictEqual(s({ key: "+" }), null, "no modifier");
      assert.strictEqual(s({ metaKey: true, key: "a" }), null);
      assert.strictEqual(s({ metaKey: true, altKey: true, key: "0" }), null);
    },
  "view: zoom steps are clamped, saved with the view, and a same value saves nothing":
    () => {
      const saved = [];
      const zooms = [];
      const v = T.panelView.create({
        releaseKeys: () => {},
        onView: () => {},
        onZoom: (z) => zooms.push(z),
        save: (view, zoom) => saved.push([view, zoom]),
      });
      v.zoomIn();
      v.zoomIn();
      v.zoomFit();
      v.zoomFit();
      for (let i = 0; i < 20; i++) v.zoomOut();
      assert.strictEqual(v.zoom(), 60);
      assert.deepStrictEqual(zooms.slice(0, 3), [110, 120, 100]);
      assert.deepStrictEqual(saved[0], ["full", 110]);
      assert.strictEqual(saved.filter((s) => s[1] === 100).length, 1);
      v.restore("machine", 140);
      assert.strictEqual(v.zoom(), 140);
      assert.strictEqual(saved.at(-1)[1], 60, "restore saves nothing");
    },
  "view: switching releases the held keys before the other drawing shows":
    () => {
      const order = [];
      const v = T.panelView.create({
        releaseKeys: () => order.push("release"),
        onView: (name) => order.push("show " + name),
        onZoom: () => {},
        save: (name) => order.push("save " + name),
      });
      v.setView("machine");
      v.setView("machine");
      v.setView("nonsense");
      v.setView("full");
      assert.deepStrictEqual(order, [
        "release",
        "show machine",
        "save machine",
        "release",
        "show full",
        "save full",
      ]);
    },
  /* A pad goes out by number (A1-B16): Rust plays the machine's own note of that pad, never 36 + pad. */
  "link: a pad is sent by its number, press and release": () => {
    const bridge = fakeBridge();
    const link = T.panelLink.create({
      bridge,
      connected: () => true,
      setTempo: () => {},
    });
    link.pad(19, true);
    link.pad(19, false);
    assert.deepStrictEqual(bridge.log, [
      ["send", "pad", "on", 19, 100],
      ["send", "pad", "off", 19, 0],
    ]);
  },
  "view: restoring an unknown view opens the Full view": () => {
    const shown = [];
    const v = T.panelView.create({
      releaseKeys: () => {},
      onView: (n) => shown.push(n),
      onZoom: () => {},
      save: () => {},
    });
    v.restore("garbage", 100);
    assert.deepStrictEqual(shown, ["full"]);
  },
  "view: the Full view's commands are the same before and after a trip through the Machine view":
    () => {
      const bridge = fakeBridge();
      const link = T.panelLink.create({
        bridge,
        connected: () => true,
        setTempo: () => {},
      });
      const v = T.panelView.create({
        releaseKeys: () => {},
        onView: () => {},
        onZoom: () => {},
        save: () => {},
      });
      const full = () => {
        link.knob(34, 96);
        link.pad(3, true);
        link.key(31, true);
        link.local("distort", 20);
      };
      full();
      const before = bridge.log.splice(0);
      v.setView("machine");
      v.setView("full");
      assert.deepStrictEqual(bridge.log, [], "switching sends nothing itself");
      full();
      assert.deepStrictEqual(bridge.log, before);
      assert.deepStrictEqual(before[0], [
        "throttled",
        "nrpn34",
        "nrpn",
        34,
        96,
      ]);
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
