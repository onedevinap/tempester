/* The version chooser grouped by OS, on a fake DOM (run by tools/test_app.sh): `node os-groups.test.js <ui folder>`.
   Every OS (Main, Voice; Panel and SAM later) is its own group with its choices and its read-only history, the
   Voice releases included and labelled; a superseded Voice version (1.5.1.0, kind "superseded") keeps its history
   row, marked, and gets no card nor build button; each group, each card and each history folds (a real button with a
   label and aria-expanded, remembered when Rust sends a new list); a filter shows every OS or one.
   os-groups-counter-proof.js runs the same checks on copies with one behaviour broken. */
"use strict";
const assert = require("assert");
const {
  all,
  textOf,
  isShown,
  load,
  control,
  controls,
} = require("./fake-dom.js");

const FILES = [
  "bridge.js",
  "icons.js",
  "kit.js",
  "pad-notes.js",
  "conn-bar.js",
  "monitor-ui.js",
  "identity-panel.js",
  "panel-perform.js",
  "pad-channels.js",
  "pad-channels-panel.js",
  "pad-notes-panel.js",
  "os-hash.js",
  "os-versions.js",
  "os-verify.js",
  "page-machine.js",
  "full-backup.js",
  "page-backup.js",
  "page-os.js",
  "page-play.js",
];

const main = (version, kind, summary) => ({
  version,
  name: "Tempester " + version,
  kind,
  date: "2026-11-14",
  summary,
  changes: { fixes: ["A fix of " + version + "."] },
  description: "Technical text of " + version + ".",
});
const MAIN_LIST = [
  main("1.6.8.0", "release", "Newest Main."),
  main("1.5.6.0", "corrections", "Corrections Main."),
  main("1.5.1.0", "release", "Oldest Main."),
];
const voice = (version, kind, summary) => ({
  version,
  name: "Tempester Voice " + version,
  kind,
  date: "2026-11-14",
  summary,
  changes: { fixes: ["A fix of Voice " + version + "."] },
  description: "Technical text of Voice " + version + ".",
});
const VOICE_LIST = [
  voice("1.5.1.1", "release", "Newest Voice."),
  voice("1.5.1.0", "superseded", "First Voice."),
];

const byClass = (root, name) =>
  all(root).filter((n) => (n.attrs.class || "").split(/\s+/).includes(name));
const group = (root, id) =>
  all(root).find((n) => n.tagName === "SECTION" && n.attrs["data-os"] === id);
const card = (root, title) =>
  all(root).find(
    (n) => n.tagName === "SECTION" && n.attrs["aria-label"] === title,
  );
const toggleText = (row) => textOf(byClass(row, "vtoggle")[0]);

function setup(voices = VOICE_LIST) {
  const env = load(process.argv[2], FILES);
  env.kit.updateSession({ connected: true });
  env.kit.pages.find((p) => p.id === "os").onShow();
  env.receive({
    event: "os_versions",
    versions: MAIN_LIST,
    voice_versions: voices,
  });
  return env;
}

const send = (env, voices = VOICE_LIST) =>
  env.receive({
    event: "os_versions",
    versions: MAIN_LIST,
    voice_versions: voices,
  });

function checkGroups(root) {
  const sections = byClass(root, "vos");
  assert.deepStrictEqual(
    sections.map((s) => s.attrs["data-os"]),
    ["main", "voice"],
    "one group per OS, Main first",
  );
  const titles = (id) =>
    byClass(group(root, id), "vcard").map((c) => c.attrs["aria-label"]);
  assert.deepStrictEqual(titles("main"), [
    "Latest (1.6.8.0)",
    "Corrections only (1.5.6.0)",
    "Back to DSI 1.5.0.2",
  ]);
  assert.deepStrictEqual(
    titles("voice"),
    ["Latest (1.5.1.1)"],
    "the Voice choices are in the Voice group, titled like the Main ones; the superseded 1.5.1.0 is not offered",
  );
  assert.ok(
    textOf(group(root, "voice")).includes("1 choice, 2 releases"),
    "the header counts the choice offered and both releases",
  );
  const mainRows = byClass(group(root, "main"), "vrow").map(toggleText);
  const voiceRows = byClass(group(root, "voice"), "vrow").map(toggleText);
  assert.strictEqual(mainRows.length, 3);
  assert.ok(mainRows.every((r) => r.includes("Main") && !r.includes("Voice")));
  assert.strictEqual(voiceRows.length, 2, "the Voice releases are in the history");
  assert.ok(voiceRows.every((r) => r.includes("Voice")), "labelled Voice");
  assert.ok(voiceRows[0].indexOf("1.5.1.1") >= 0 && voiceRows[1].indexOf("1.5.1.0") >= 0);
  assert.ok(voiceRows[1].includes("Superseded"), "the superseded row says so");
  assert.ok(!voiceRows[0].includes("Superseded"), "the offered one does not");
  // The history is read only for Voice too: one build button per Voice card, none in the rows; none for 1.5.1.0.
  assert.strictEqual(controls(root, "Build Tempester Voice 1.5.1.1").length, 1);
  assert.strictEqual(controls(root, "Build Tempester Voice 1.5.1.0").length, 0, "1.5.1.0 is no longer offered");
  assert.ok(!textOf(root).includes("Previous (1.5.1.0)"), "no card for 1.5.1.0");
  // Its rows open like the Main ones, with a name of their own (a Main 1.5.1.0 exists too).
  const open = control(root, "Changes of Voice 1.5.1.0");
  assert.strictEqual(control(root, "Changes of 1.5.1.0").attrs["aria-expanded"], "false");
  open.click();
  assert.strictEqual(open.attrs["aria-expanded"], "true");
  const row = byClass(group(root, "voice"), "vrow")[1];
  assert.ok(isShown(byClass(row, "vbody")[0]));
  assert.ok(textOf(row).includes("First Voice."));
  assert.ok(textOf(row).includes("A fix of Voice 1.5.1.0."), "its changes, as a Main row shows them");
  open.click();
}

function checkFolds(env) {
  const root = env.roots.os;
  const groupFold = (os) => control(root, os + ": show or hide");
  for (const button of [groupFold("Main OS"), groupFold("Voice OS")]) {
    assert.strictEqual(button.tagName, "BUTTON", "a real button, so the keyboard works");
    assert.strictEqual(button.attrs["aria-expanded"], "true", "open at first");
  }
  const latest = card(root, "Latest (1.6.8.0)");
  const voiceCard = card(root, "Latest (1.5.1.1)");
  assert.ok(isShown(latest) && isShown(voiceCard));

  groupFold("Main OS").click();
  assert.strictEqual(groupFold("Main OS").attrs["aria-expanded"], "false");
  assert.ok(!isShown(card(root, "Latest (1.6.8.0)")), "a folded group hides its cards");
  assert.ok(isShown(group(root, "main")), "its header stays");
  assert.ok(isShown(voiceCard), "the other group is not folded");
  send(env);
  assert.strictEqual(groupFold("Main OS").attrs["aria-expanded"], "false", "kept after a new list");
  assert.ok(!isShown(card(root, "Latest (1.6.8.0)")));
  groupFold("Main OS").click();
  assert.ok(isShown(card(root, "Latest (1.6.8.0)")));

  const cardFold = (title) => control(root, "Show or hide " + title);
  assert.strictEqual(cardFold("Latest (1.6.8.0)").attrs["aria-expanded"], "true");
  cardFold("Latest (1.6.8.0)").click();
  const folded = card(root, "Latest (1.6.8.0)");
  assert.ok(!isShown(byClass(folded, "vcard-body")[0]), "a folded card hides its body");
  assert.ok(isShown(byClass(folded, "vcard-head")[0]), "the title stays");
  assert.ok(isShown(byClass(card(root, "Corrections only (1.5.6.0)"), "vcard-body")[0]), "another card is not folded");
  send(env);
  assert.ok(
    !isShown(byClass(card(root, "Latest (1.6.8.0)"), "vcard-body")[0]),
    "a folded card stays folded after a new list",
  );
  assert.ok(isShown(byClass(card(root, "Corrections only (1.5.6.0)"), "vcard-body")[0]));
  assert.ok(isShown(byClass(card(root, "Back to DSI 1.5.0.2"), "vcard-body")[0]));
  assert.ok(
    isShown(byClass(card(root, "Latest (1.5.1.1)"), "vcard-body")[0]),
    "the Voice latest card keeps its own fold",
  );
  cardFold("Latest (1.6.8.0)").click();

  const historyFold = (os) => control(root, "History of the " + os + ": show or hide");
  assert.ok(isShown(byClass(group(root, "voice"), "vhistory")[0]));
  historyFold("Voice OS").click();
  assert.ok(!isShown(byClass(group(root, "voice"), "vrow")[0]), "a folded history hides its rows");
  assert.ok(isShown(byClass(group(root, "main"), "vrow")[0]), "the other history stays");
  assert.ok(isShown(card(root, "Latest (1.5.1.1)")), "its cards stay");
  assert.strictEqual(historyFold("Voice OS").attrs["aria-expanded"], "false");
  send(env);
  assert.ok(!isShown(byClass(group(root, "voice"), "vrow")[0]), "kept after a new list");
  historyFold("Voice OS").click();
  assert.ok(isShown(byClass(group(root, "voice"), "vrow")[0]));
}

function checkFilter(env) {
  const root = env.roots.os;
  const filter = (label) => control(root, label);
  const pressed = () =>
    byClass(root, "seg")[0].children.map((b) => b.attrs["aria-pressed"]);
  assert.deepStrictEqual(
    byClass(root, "seg")[0].children.map((b) => textOf(b)),
    ["All", "Main", "Voice"],
  );
  assert.deepStrictEqual(pressed(), ["true", "false", "false"]);
  assert.ok(isShown(group(root, "main")) && isShown(group(root, "voice")));

  filter("Show the Voice only").click();
  assert.deepStrictEqual(pressed(), ["false", "false", "true"]);
  assert.ok(!isShown(group(root, "main")), "Main is filtered out");
  assert.ok(isShown(group(root, "voice")));
  assert.ok(!isShown(byClass(group(root, "main"), "vrow")[0]), "its history too");
  send(env);
  assert.ok(!isShown(group(root, "main")), "the filter is kept after a new list");
  assert.deepStrictEqual(pressed(), ["false", "false", "true"]);

  filter("Show the Main only").click();
  assert.ok(isShown(group(root, "main")) && !isShown(group(root, "voice")));
  assert.deepStrictEqual(pressed(), ["false", "true", "false"]);

  filter("Show every OS").click();
  assert.ok(isShown(group(root, "main")) && isShown(group(root, "voice")));
  assert.deepStrictEqual(pressed(), ["true", "false", "false"]);

  filter("Show the Voice only").click();
  send(env, []);
  assert.ok(
    isShown(group(root, "main")),
    "a filter on an OS that is no longer offered falls back to all",
  );
  assert.strictEqual(group(root, "voice"), undefined);
  assert.deepStrictEqual(
    byClass(root, "seg")[0].children.map((b) => textOf(b)),
    ["All", "Main"],
    "no filter for an OS without versions",
  );
}

(async () => {
  const env = setup();
  const root = env.roots.os;
  checkGroups(root);
  checkFolds(env);
  checkFilter(env);
  console.log(
    "ok, one group per OS, Voice in the history, folds of groups, cards and histories kept, filter kept",
  );
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
