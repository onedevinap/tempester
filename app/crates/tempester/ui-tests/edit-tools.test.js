/* The editing tools of the Panel page (ui/edit-tools.js, ui/edit-learn.js) on the minimal fake DOM, run by
   tools/test_app.sh: `node edit-tools.test.js <ui folder>`. What each button sends, what the page does with what
   Rust says (values redrawn, learned controller moving its control through the knob path, learn mode picking a
   control without changing it), then the same checks on broken copies of the two scripts, which must fail. */
"use strict";
const fs = require("fs");
const os = require("os");
const path = require("path");
const assert = require("assert");
const { load, press, control, all } = require("./fake-dom.js");

const uiDir = process.argv[2];
const FILES = [
  "bridge.js",
  "icons.js",
  "kit.js",
  "edit-learn.js",
  "edit-tools.js",
];
const NAMES = { 25: "Glide Mode", 34: "LP Freq", 35: "LP Resonance" };

/* A piece of the face as panel.js draws it: a knob of parameter 34, an OLED menu of parameter 25, the Distort knob,
   a switch of parameter 40 and a key of the remote panel. */
function face(env) {
  const make = (tag, attrs, ...children) => {
    const node = env.document.createElement(tag);
    for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, v);
    node.append(...children);
    return node;
  };
  const ring = make("path", {});
  const knob = make(
    "div",
    { class: "pot", "data-param": "34" },
    make("svg", { role: "slider", "aria-label": "LP Freq" }, ring),
  );
  const fen = make("button", { class: "fen", id: "fen-25" });
  const distortSvg = make("svg", { role: "slider", "aria-label": "Distort" });
  const distort = make("div", { class: "pot" }, distortSvg);
  const switchButton = make("button", { "aria-label": "4 Pole" });
  const sw = make("div", { class: "bt", "data-param": "40" }, switchButton);
  const keyButton = make("button", { "aria-label": "Shift" });
  const key = make("div", { class: "bt" }, keyButton);
  const host = make(
    "div",
    { class: "panel-root" },
    knob,
    fen,
    distort,
    sw,
    key,
  );
  return { host, ring, fen, distortSvg, switchButton, keyButton };
}

function event(target) {
  const e = { target, prevented: false, stopped: false };
  e.preventDefault = () => (e.prevented = true);
  e.stopPropagation = () => (e.stopped = true);
  return e;
}

function run(dir) {
  const env = load(dir, FILES);
  const parts = face(env);
  const calls = [];
  const spy =
    (name) =>
    (...args) =>
      calls.push([name, ...args]);
  const panel = {
    setValue: spy("panel.setValue"),
    setLocal: spy("panel.setLocal"),
  };
  const machine = { setParam: spy("machine.setParam") };
  const link = { knob: spy("link.knob"), local: spy("link.local") };
  const learn = env.window.Tempester.editLearn;
  const drawn = [];
  const tools = env.window.Tempester.editTools.create({
    bridge: env.window.Tempester.bridge,
    names: NAMES,
    learnHost: parts.host,
    shown: () => true,
    onValues: (values) => drawn.push(values),
    drive: (id, value) => learn.drive(id, value, { panel, machine, link }),
  });
  const sent = env.sent;
  const slot = tools.slot;
  const pop = tools.popover;
  const last = () => sent[sent.length - 1];

  assert.deepStrictEqual(
    Array.from(sent),
    ["edit_state"],
    "asks for the state at start",
  );
  press(slot, "Sound tools: redo, A/B, variations, MIDI learn");
  assert.strictEqual(
    last(),
    "learn_ports",
    "opening the tools looks for inputs",
  );
  assert.ok(!pop.hidden);

  /* Nothing fetched: A/B and variations wait for a Sync; nothing to undo. */
  env.receive({
    event: "edit",
    synced: false,
    undo: null,
    redo: null,
    depth: 0,
    a: false,
    b: false,
    active: null,
  });
  tools.setConnected(true);
  assert.ok(
    control(pop, "Keep the current sound as A").disabled,
    "store waits for a Sync",
  );
  assert.ok(
    control(pop, "Vary the current sound with this seed and send it (undoable)")
      .disabled,
  );
  assert.ok(control(slot, "Undo (Cmd/Ctrl Z)").disabled);

  /* Synced: store A and B, then the toggle sends the other one. */
  env.receive({
    event: "edit",
    synced: true,
    undo: null,
    redo: null,
    depth: 0,
    a: false,
    b: false,
    active: null,
  });
  press(pop, "Keep the current sound as A");
  assert.strictEqual(last(), "edit_store\ta");
  press(pop, "Keep the current sound as B");
  assert.strictEqual(last(), "edit_store\tb");
  env.receive({
    event: "edit",
    synced: true,
    undo: null,
    redo: null,
    depth: 0,
    a: true,
    b: true,
    active: "B",
  });
  press(pop, "Compare: send the other one of A and B");
  assert.strictEqual(last(), "edit_recall\ta");
  env.receive({
    event: "edit",
    synced: true,
    undo: "recall A",
    redo: null,
    depth: 1,
    a: true,
    b: true,
    active: "A",
    values: [1, 2, 3],
  });
  assert.deepStrictEqual(drawn.pop(), [1, 2, 3], "the recalled sound is drawn");
  press(pop, "Compare: send the other one of A and B");
  assert.strictEqual(last(), "edit_recall\tb");

  /* Undo and redo: buttons, names of the step, keyboard. */
  env.receive({
    event: "edit",
    synced: true,
    undo: "n34",
    redo: "variation 4242",
    depth: 2,
    a: true,
    b: true,
    active: "A",
  });
  press(slot, "Undo LP Freq (Cmd/Ctrl Z)");
  assert.strictEqual(last(), "edit_undo");
  press(pop, "Redo variation (seed 4242) (Shift Cmd/Ctrl Z)");
  assert.strictEqual(last(), "edit_redo");
  env.document.fire("keydown", {
    key: "z",
    metaKey: true,
    preventDefault() {},
  });
  assert.strictEqual(last(), "edit_undo", "Cmd Z undoes");
  env.document.fire("keydown", {
    key: "Z",
    ctrlKey: true,
    shiftKey: true,
    preventDefault() {},
  });
  assert.strictEqual(last(), "edit_redo", "Shift Ctrl Z redoes");
  tools.setConnected(false);
  assert.ok(
    control(slot, "Undo LP Freq (Cmd/Ctrl Z)").disabled,
    "nothing is sent without a machine",
  );
  tools.setConnected(true);

  /* Variation: group, amount and seed as chosen. */
  const [group, amount] = all(pop).filter((n) => n.tagName === "SELECT");
  const seed = all(pop).find((n) => n.attrs.type === "number");
  group.value = "mod";
  amount.value = "50";
  seed.value = "1234";
  press(pop, "Vary the current sound with this seed and send it (undoable)");
  assert.strictEqual(last(), "edit_vary\tmod\t50\t1234");
  seed.value = "999999";
  press(pop, "Vary the current sound with this seed and send it (undoable)");
  assert.strictEqual(
    last(),
    "edit_vary\tmod\t50\t99999",
    "the seed is kept in range",
  );

  /* MIDI learn: no port, no learn mode. */
  const learnLabel =
    "MIDI learn: click a control, then move a knob of the controller";
  assert.ok(
    control(pop, "MIDI learn: choose the controller's port first (Sound tools)",
    ).disabled,
  );
  env.receive({
    event: "learn",
    port: "Knobs",
    armed: null,
    map: [[21, "n34"]],
    fake: false,
  });
  press(pop, learnLabel);
  assert.ok(pop.hidden, "learn mode closes the popover (it would hide the knobs)");
  const on = control(
    slot,
    "MIDI learn on: click a control of the Full view, then move a knob of the controller (Escape: stop)",
  );
  assert.ok(on.classList.contains("learning"), "the tools button shows learn mode");

  /* A click on a control in learn mode picks it and never reaches it. */
  const picks = [
    [parts.ring, "n34"],
    [parts.fen, "n25"],
    [parts.distortSvg, "distort"],
    [parts.switchButton, "n40"],
  ];
  for (const [target, id] of picks) {
    const e = event(target);
    parts.host.fire("pointerdown", e);
    assert.strictEqual(last(), "learn_arm\t" + id, "picked " + id);
    assert.ok(
      e.stopped && e.prevented,
      "the control does not see the click: " + id,
    );
  }
  const count = sent.length;
  parts.host.fire("pointerdown", event(parts.keyButton));
  assert.strictEqual(
    sent.length,
    count,
    "a remote-panel key is not a learnable control",
  );
  const click = event(parts.ring);
  parts.host.fire("click", click);
  assert.ok(click.stopped, "a click in learn mode changes nothing");
  assert.strictEqual(learn.targetOf(parts.keyButton, parts.host), null);

  /* Leaving learn mode: Escape; the controls get their clicks back. */
  env.receive({
    event: "learn",
    port: "Knobs",
    armed: "n35",
    map: [[21, "n34"]],
    fake: false,
  });
  env.document.fire("keydown", { key: "Escape", preventDefault() {} });
  assert.strictEqual(last(), "learn_cancel");
  const free = event(parts.ring);
  parts.host.fire("pointerdown", free);
  assert.ok(!free.stopped, "out of learn mode the knob works");

  /* A learned controller moves its control exactly like a hand: the same link as the knob. */
  env.receive({ event: "learned", controller: 21, target: "n34", value: 83 });
  assert.deepStrictEqual(calls.splice(0), [
    ["panel.setValue", 34, 83],
    ["machine.setParam", 34, 83],
    ["link.knob", 34, 83],
  ]);
  env.receive({
    event: "learned",
    controller: 22,
    target: "volume",
    value: 90,
  });
  assert.deepStrictEqual(calls.splice(0), [
    ["panel.setLocal", "volume", 90],
    ["link.local", "volume", 90],
  ]);

  /* The table: clear one, clear all, the port (Escape closed the popover: open it again). */
  assert.ok(pop.hidden, "Escape closes the popover");
  press(slot, "Sound tools: redo, A/B, variations, MIDI learn");
  press(pop, "Forget CC 21 (LP Freq)");
  assert.strictEqual(last(), "learn_clear\tn34");
  press(pop, "Forget every learned controller");
  assert.strictEqual(last(), "learn_clear_all");
  const port = all(pop).find((n) => n.attrs.class === "ep-port");
  port.value = "";
  port.fire("change");
  assert.strictEqual(last(), "learn_open\t", "No controller closes the port");
  return "ok";
}

/* Broken copies: each one must make a check above fail. */
const BROKEN = [
  ["edit-learn.js", "      link.knob(index, value);\n", ""],
  ["edit-learn.js", "    link.local(id, value);\n", ""],
  ["edit-learn.js", "      e.stopPropagation();\n", ""],
  [
    "edit-learn.js",
    'if (kind === "fen" && id.startsWith("fen-"))',
    "if (false)",
  ],
  ["edit-learn.js", "if (id) onPick(id);", ""],
  ["edit-tools.js", 'state.active === "A" ? "b" : "a"', '"a"'],
  [
    "edit-tools.js",
    "store[slotName].disabled = !state.synced;",
    "store[slotName].disabled = false;",
  ],
  ["edit-tools.js", 'return e.shiftKey ? "redo" : "undo";', 'return "undo";'],
  [
    "edit-tools.js",
    'bridge.send("edit_vary", groupSelect.value, amountSelect.value, seed);',
    'bridge.send("edit_vary", "all", amountSelect.value, seed);',
  ],
  [
    "edit-tools.js",
    "Math.min(SEED_MAX, Math.floor(Number(seedInput.value) || 0)),",
    "Math.floor(Number(seedInput.value) || 0),",
  ],
  ["edit-tools.js", "      if (m.values) onValues(m.values);\n", ""],
  [
    "edit-tools.js",
    "undo.disabled = !live || !m.undo;",
    "undo.disabled = !m.undo;",
  ],
  [
    "edit-tools.js",
    'bridge.on("learned", (m) => drive(m.target, m.value));',
    "",
  ],
  ["edit-tools.js", 'if (!on && state.armed) bridge.send("learn_cancel");', ""],
  ["edit-tools.js", "      if (learner.isOn()) closePopover();\n", ""],
];

run(uiDir);
let refused = 0;
for (const [file, from, to] of BROKEN) {
  const source = fs.readFileSync(path.join(uiDir, file), "utf8");
  assert.ok(
    source.includes(from),
    "counter-proof anchor missing in " + file + ": " + from,
  );
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "edit-tools-"));
  for (const f of FILES)
    fs.copyFileSync(path.join(uiDir, f), path.join(dir, f));
  fs.writeFileSync(path.join(dir, file), source.replace(from, to));
  let accepted = false;
  try {
    run(dir);
    accepted = true;
  } catch (e) {
    refused++;
  }
  fs.rmSync(dir, { recursive: true, force: true });
  if (accepted) {
    console.error("broken copy accepted: " + file + ": " + from);
    process.exit(1);
  }
}
console.log(`ok, ${refused} broken copies refused`);
process.exit(0);
