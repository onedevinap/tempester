/* The kit builder (lib-kit.js) and the export buttons of the Library, on the fake DOM (run by tools/test_app.sh):
   `node lib-kit.test.js <ui folder>`. The decisions (what goes on which pad, the 32 entries Rust takes), the dialog
   (select then click, drag and drop, 16 or 32 pads, Cancel sends nothing) and the two-step flow with Rust (check,
   then apply with the token the check returned, never when the plan is declined). lib-kit-counter-proof.js runs these
   checks on copies with one behaviour broken: they must fail there. */
"use strict";
const assert = require("assert");
const { all, textOf, load, tick, press } = require("./fake-dom.js");

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

const sound = (n, type) => ({
  id: "s" + n,
  display: "Sound " + n,
  sound_type: type || "Kicks",
  folder: "",
  slot: false,
});
const SOUNDS = [sound(1), sound(2, "Snares"), sound(3, "HiHats"), sound(4)];
const KEEP32 = (entries) =>
  Array.from({ length: 32 }, (_, i) => entries[i] || "_").join(",");

function checkModel(libkit) {
  const k = libkit.createKit(16);
  libkit.assign(k, 0, SOUNDS[0]);
  libkit.assign(k, 20, SOUNDS[1]);
  assert.strictEqual(libkit.ids(k), KEEP32({ 0: "s1" }), "pad 21 is out of 16");
  libkit.setCount(k, 32);
  libkit.assign(k, 31, SOUNDS[1]);
  assert.strictEqual(libkit.ids(k), KEEP32({ 0: "s1", 31: "s2" }));
  assert.strictEqual(libkit.filled(k), 2);
  libkit.setCount(k, 16);
  assert.strictEqual(libkit.ids(k), KEEP32({ 0: "s1" }), "pads 17-32 dropped");
  libkit.setCount(k, 32);
  assert.strictEqual(libkit.filled(k), 1, "pad 32 is empty again after 16 -> 32");
  libkit.setCount(k, 16);
  const many = Array.from({ length: 20 }, (_, i) => sound(100 + i));
  libkit.fill(k, many);
  libkit.setCount(k, 32);
  assert.strictEqual(libkit.filled(k), 16, "fill stops at the 16 pads in use");
  libkit.setCount(k, 16);
  libkit.clearAll(k);
  libkit.fill(k, SOUNDS);
  assert.strictEqual(
    libkit.ids(k),
    KEEP32({ 0: "s1", 1: "s2", 2: "s3", 3: "s4" }),
    "fill in order",
  );
  libkit.clear(k, 1);
  assert.strictEqual(libkit.ids(k).split(",")[1], "_");
  libkit.clearAll(k);
  assert.strictEqual(libkit.filled(k), 0);
  assert.deepStrictEqual(
    libkit.matching(SOUNDS, "snares").map((s) => s.id),
    ["s2"],
    "search by type",
  );
  assert.deepStrictEqual(
    libkit.matching([Object.assign({}, SOUNDS[0], { slot: true })], ""),
    [],
    "an auto backup slot is not a sound to pick",
  );
}

const byClass = (root, name) =>
  all(root).filter((n) => (n.attrs.class || "").split(/\s+/).includes(name));

/* Plays the user in the dialog: `script(body, buttons)` acts, and the answer is the button value it returns. */
function playUser(env, script) {
  const dialogs = env.window.Tempester.dialogs;
  dialogs.ask = async (spec) => {
    const root = { children: spec.body, attrs: {}, tagName: "DIV" };
    return script(root, spec);
  };
}

async function checkDialog(env) {
  const libkit = env.window.Tempester.libkit;
  const dialogs = env.window.Tempester.dialogs;
  const start = env.sent.length;
  let seen = null;
  playUser(env, (root, spec) => {
    const rows = byClass(root, "kit-sound");
    assert.strictEqual(rows.length, 4, "the four sounds are listed");
    rows[1].fire("click"); // Sound 2
    let tiles = byClass(root, "kit-pad");
    assert.strictEqual(tiles.length, 16, "16 pads at first");
    tiles[0].fire("click"); // pad 1 gets Sound 2
    byClass(root, "kit-sound")[2].fire("dragstart", {
      dataTransfer: { setData() {} },
    });
    byClass(root, "kit-pad")[9].fire("drop", {
      dataTransfer: { getData: () => "s3" },
      preventDefault() {},
    });
    const seg = byClass(root, "seg")[0].children;
    seg[1].fire("click"); // 32 pads
    assert.strictEqual(byClass(root, "kit-pad").length, 32);
    tiles = byClass(root, "kit-pad");
    tiles[31].fire("click"); // Sound 3 is still selected (drag selected it)
    assert.ok(
      textOf(tiles[0]).includes("Sound 2") || textOf(byClass(root, "kit-pad")[0]).includes("Sound 2"),
      "the pad shows the sound's name",
    );
    assert.ok(
      textOf(byClass(root, "kit-pad")[0]).includes("Snares"),
      "and its type",
    );
    seen = textOf(byClass(root, "kit-count")[0]);
    return "write";
  });
  await libkit.open(SOUNDS);
  assert.ok(seen.includes("3 of 32"), "count shown: " + seen);
  assert.deepStrictEqual(Array.from(env.sent).slice(start), [
    "lib_kit_check\t" + KEEP32({ 0: "s2", 9: "s3", 31: "s3" }),
  ]);

  const before = env.sent.length;
  playUser(env, (root) => {
    byClass(root, "kit-sound")[0].fire("click");
    byClass(root, "kit-pad")[0].fire("click");
    return "cancel";
  });
  await libkit.open(SOUNDS);
  assert.strictEqual(env.sent.length, before, "Cancel sends nothing");

  const plan = {
    event: "lib_kit_plan",
    kind: "beats",
    status: "unknown",
    changed: false,
    item: "Kit of 3 sound(s)",
    from: "the library",
    machine: "Beat",
    token: "a".repeat(64),
    lines: ["x"],
  };
  const confirmed = [];
  dialogs.sendPlan = async (p) => (confirmed.push(p), true);
  env.receive(plan);
  await tick();
  assert.strictEqual(confirmed.length, 1);
  assert.strictEqual(
    env.sent.at(-1),
    "lib_kit_apply\t" + KEEP32({ 0: "s2", 9: "s3", 31: "s3" }) + "\t" + "a".repeat(64),
    "the apply carries the kit asked for and the token the check saw",
  );
  const sentCount = env.sent.length;
  dialogs.sendPlan = async () => false;
  env.receive(plan);
  await tick();
  assert.strictEqual(env.sent.length, sentCount, "declined: nothing is applied");
}

async function checkExport(env) {
  const root = env.roots.library;
  const exportButton = "Export the fetched beat as a MIDI file (one track per pad)";
  assert.ok(
    all(root).some((n) => n.attrs["aria-label"] === exportButton && n.disabled),
    "the export button of the beat card starts disabled",
  );
  env.receive({
    event: "beat",
    name: "Groove",
    events: 4,
    assigned: 1,
    pads: 32,
    size: 100,
    voice_assign: new Array(32).fill(1),
  });
  const before = env.sent.length;
  press(root, exportButton);
  assert.deepStrictEqual(Array.from(env.sent).slice(before), ["lib_export_midi\t"]);
  const kitButton = all(root).find((n) =>
    (n.attrs["aria-label"] || "").startsWith("Kit builder"),
  );
  assert.ok(kitButton && kitButton.disabled, "the kit builder needs a connection");
  env.kit.updateSession({ connected: true });
  assert.ok(!kitButton.disabled, "and is enabled once connected");
}

(async () => {
  const env = load(process.argv[2], FILES);
  checkModel(env.window.Tempester.libkit);
  await checkDialog(env);
  await checkExport(env);
  console.log("ok: kit model, dialog, two-step flow, export buttons");
})().catch((e) => {
  console.error(e.stack || e);
  process.exit(1);
});
