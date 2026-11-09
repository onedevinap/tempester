/* The version chooser of the OS update page on a fake DOM (run by tools/test_app.sh):
   `node os-versions.test.js <ui folder>`. The three recommended cards (latest release, corrections only, back to
   DSI), their summaries and change lists, the Details toggle, the History of every release newest first with its
   expandable changelog, and the install of an older release (same command as before, only with the DSI file chosen).
   os-versions-counter-proof.js runs the same checks on copies with one behaviour broken. */
"use strict";
const assert = require("assert");
const {
  all,
  textOf,
  isShown,
  load,
  tick,
  control,
  controls,
  press,
  pick,
} = require("./fake-dom.js");

const FILES = [
  "bridge.js",
  "icons.js",
  "kit.js",
  "pad-notes.js",
  "conn-bar.js",
  "monitor-ui.js",
  "identity-panel.js",
  "pad-notes-panel.js",
  "os-hash.js",
  "os-versions.js",
  "os-verify.js",
  "page-machine.js",
  "page-backup.js",
  "page-os.js",
  "page-play.js",
];

const LIST = [
  {
    version: "1.6.2.0",
    name: "Tempester 1.6.2.0 — MIDI transfers",
    kind: "release",
    date: "2026-11-08",
    summary: "Five improvements to MIDI transfers.",
    changes: {
      fixes: ["A project sent back is read correctly.", "A SysEx timeout."],
      additions: ["Beat-wide positions over MIDI."],
      changes: ["The project request draws nothing."],
    },
    description: "Technical text of 1.6.2.0 with n°26-n°30.",
  },
  {
    version: "1.6.1.0",
    name: "Tempester 1.6.1.0 — project request",
    kind: "release",
    date: "2026-11-08",
    summary: "A computer can ask for the whole project.",
    changes: { additions: ["The project request."] },
    description: "Technical text of 1.6.1.0.",
  },
  {
    version: "1.5.6.0",
    name: "Tempester 1.5.6.0 — fixes and small additions",
    kind: "corrections",
    date: "2026-11-07",
    summary: "Fixes and small additions only; no remote panel.",
    changes: { fixes: ["Voices no longer fall silent."] },
    description: "Technical text of 1.5.6.0.",
  },
  {
    version: "1.5.1.0",
    name: "Tempester 1.5.1.0 — bug fixes",
    kind: "release",
    date: "2026-11-07",
    summary: "DSI 1.5.0.2 with its bug fixes only.",
    changes: { fixes: ["The Loop screen no longer freezes."] },
    description: "Technical text of 1.5.1.0.",
  },
];

const byClass = (root, name) =>
  all(root).filter((n) => (n.attrs.class || "").split(/\s+/).includes(name));
const card = (root, title) =>
  all(root).find((n) => n.attrs["aria-label"] === title && n.tagName === "SECTION");
const shownText = (node) =>
  isShown(node) ? node.children.map((c) => (c.text !== undefined ? c.text : shownText(c))).join(" ") : "";

function setup() {
  const env = load(process.argv[2], FILES);
  env.kit.updateSession({ connected: true });
  env.kit.pages.find((p) => p.id === "os").onShow();
  env.receive({ event: "os_versions", versions: LIST });
  return env;
}

function checkCards(root) {
  const latest = card(root, "Latest (1.6.2.0)");
  const corrections = card(root, "Corrections only (1.5.6.0)");
  const original = card(root, "Back to DSI 1.5.0.2");
  assert.ok(latest && corrections && original, "the three cards are there");
  const cards = byClass(root, "vcard");
  assert.deepStrictEqual(
    cards.map((c) => c.attrs["aria-label"]),
    ["Latest (1.6.2.0)", "Corrections only (1.5.6.0)", "Back to DSI 1.5.0.2"],
    "cards in order",
  );
  const text = shownText(latest);
  for (const piece of [
    "Five improvements to MIDI transfers.",
    "A project sent back is read correctly.",
    "A SysEx timeout.",
    "Beat-wide positions over MIDI.",
    "The project request draws nothing.",
    "Fixes",
    "Additions",
    "Changes",
    "2026-11-08",
  ])
    assert.ok(text.includes(piece), "latest card says: " + piece);
  const small = shownText(corrections);
  assert.ok(small.includes("Fixes and small additions only; no remote panel."));
  assert.ok(small.includes("Voices no longer fall silent."));
  assert.ok(!small.includes("Additions"), "an empty group is not drawn");
  assert.ok(!shownText(latest).includes("Technical text"), "details hidden");
  assert.ok(shownText(original).includes("original DSI software"));
}

function checkDetails(root) {
  const latest = card(root, "Latest (1.6.2.0)");
  const toggle = control(latest, "Details of Latest (1.6.2.0)");
  assert.strictEqual(toggle.attrs["aria-expanded"], "false");
  toggle.click();
  assert.ok(shownText(latest).includes("Technical text of 1.6.2.0"));
  assert.strictEqual(toggle.attrs["aria-expanded"], "true");
  toggle.click();
  assert.ok(!shownText(latest).includes("Technical text of 1.6.2.0"));
  assert.ok(
    !shownText(card(root, "Corrections only (1.5.6.0)")).includes(
      "Technical text of 1.5.6.0",
    ),
    "one card's details do not open another's",
  );
}

function checkHistory(root) {
  const rows = byClass(root, "vrow");
  assert.deepStrictEqual(
    rows.map((r) => textOf(byClass(r, "vtoggle")[0]).slice(0, 5)),
    ["1.6.2.0", "1.6.1.0", "1.5.6.0", "1.5.1.0"].map((v) => v.slice(0, 5)),
    "every release, newest first",
  );
  assert.ok(textOf(rows[2]).includes("Corrections only"), "the corrections chip");
  assert.ok(!textOf(rows[0]).includes("Corrections only"));
  const old = rows[3];
  assert.ok(!shownText(old).includes("The Loop screen no longer freezes."));
  const open = control(old, "Changes of 1.5.1.0");
  open.click();
  assert.ok(shownText(old).includes("The Loop screen no longer freezes."));
  assert.ok(shownText(old).includes("DSI 1.5.0.2 with its bug fixes only."));
  assert.strictEqual(open.attrs["aria-expanded"], "true");
  assert.ok(!shownText(old).includes("Technical text of 1.5.1.0"));
  control(old, "Details of 1.5.1.0").click();
  assert.ok(shownText(old).includes("Technical text of 1.5.1.0"));
  assert.ok(
    !shownText(rows[1]).includes("The project request."),
    "other rows stay closed",
  );
  open.click();
  assert.ok(!shownText(old).includes("The Loop screen no longer freezes."));
}

async function checkInstall(env) {
  const root = env.roots.os;
  const label = "Build Tempester 1.5.1.0";
  assert.ok(control(root, label).disabled, "no build without the DSI file");
  assert.ok(control(root, "Back to DSI 1.5.0.2").disabled);
  assert.ok(controls(root, "Build Tempester 1.6.2.0")[0].disabled);
  const before = env.sent.length;
  await pick(root, 0, "Tempest_Main_1.5.0.2.syx", [1, 2]);
  assert.ok(!control(root, label).disabled, "the DSI file enables the builds");
  press(root, label);
  const twice = controls(root, "Build Tempester 1.6.2.0");
  assert.strictEqual(twice.length, 2, "latest: its card and its history row");
  twice[0].click();
  twice[1].click();
  press(root, "Build Tempester 1.5.6.0");
  press(root, "Back to DSI 1.5.0.2");
  assert.deepStrictEqual(Array.from(env.sent).slice(before), [
    "os_build\t1.5.1.0\t0102",
    "os_build\t1.6.2.0\t0102",
    "os_build\t1.6.2.0\t0102",
    "os_build\t1.5.6.0\t0102",
    "os_original\t0102",
  ]);
  env.receive({ event: "os_versions", versions: LIST.slice(1) });
  assert.ok(
    !control(root, "Build Tempester 1.6.1.0").disabled,
    "a new list keeps the builds enabled",
  );
}

function checkEmpty() {
  const env = setup();
  env.receive({ event: "os_versions", versions: [] });
  const root = env.roots.os;
  assert.ok(textOf(root).includes("No version listed."));
  assert.deepStrictEqual(
    byClass(root, "vcard").map((c) => c.attrs["aria-label"]),
    ["Back to DSI 1.5.0.2"],
    "only the original card without versions",
  );
}

(async () => {
  const env = setup();
  const root = env.roots.os;
  checkCards(root);
  checkDetails(root);
  checkHistory(root);
  await checkInstall(env);
  checkEmpty();
  await tick();
  console.log("ok, 3 cards, details, history of 4 releases, install of an older one");
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
