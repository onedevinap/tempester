/* The Tempester Voice OS on the OS update page, on a fake DOM (run by tools/test_app.sh):
   `node os-voice.test.js <ui folder>`. The Voice card (built from the user's DSI Voice file, which is another file than
   the Main one; in its own Voice group and history, never in the Main one), the build command, and the send of a Voice file: a confirmation that says it goes
   to all six chips at once and what a power cut means, before any byte; the same wording for an unknown Voice file;
   the Main flow keeping its own text; the end message that asks for a full backup; the cancel warning.
   os-voice-counter-proof.js runs the same checks on copies with one behaviour broken. */
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

const MAIN = {
  version: "1.6.7.0",
  name: "Tempester 1.6.7.0",
  kind: "release",
  date: "2026-11-14",
  summary: "Release up to 127 with Tempester Voice.",
  changes: { additions: ["Voice versions reported."] },
  description: "Technical text of 1.6.7.0.",
};
const CORRECTIONS = {
  version: "1.5.6.0",
  name: "Tempester 1.5.6.0",
  kind: "corrections",
  date: "2026-11-07",
  summary: "Fixes only.",
  changes: { fixes: ["Voices no longer fall silent."] },
  description: "Technical text of 1.5.6.0.",
};
const VOICE = {
  version: "1.5.1.1",
  name: "Tempester Voice 1.5.1.1",
  kind: "test",
  date: "2026-11-13",
  summary: "DSI Voice 1.5 with a hardened parser and longer release times.",
  description: "Technical text of Voice 1.5.1.1.",
};
const IMAGE = "ab".repeat(32);
const SYX = "cd".repeat(32);

const voiceReady = (extra) => ({
  event: "os_ready",
  label: "Tempester Voice 1.5.1.1",
  source: "built",
  chip: "Voice",
  id: 0x72,
  all_chips: true,
  blocks: 64,
  checksum: 0x1234,
  size: 74951,
  seconds: 25,
  known: true,
  version: "1.5.1.1",
  sha256_image: IMAGE,
  sha256_syx: SYX,
  ...extra,
});
const mainReady = (extra) =>
  voiceReady({
    label: "Tempester 1.6.7.0",
    chip: "Main",
    id: 0x71,
    all_chips: false,
    blocks: 512,
    size: 600000,
    seconds: 192,
    version: "1.6.7.0",
    ...extra,
  });

const byClass = (root, name) =>
  all(root).filter((n) => (n.attrs.class || "").split(/\s+/).includes(name));
const shownText = (node) =>
  isShown(node)
    ? node.children.map((c) => (c.text !== undefined ? c.text : shownText(c))).join(" ")
    : "";
const sentSince = (env, from) => Array.from(env.sent).slice(from);
const dialogText = (env) =>
  textOf(env.byId["confirm-title"]) + "\n" + textOf(env.byId["confirm-body"]);

function setup() {
  const env = load(process.argv[2], FILES);
  env.kit.updateSession({ connected: true });
  env.kit.pages.find((p) => p.id === "os").onShow();
  env.receive({
    event: "os_versions",
    versions: [MAIN, CORRECTIONS],
    voice_versions: [VOICE],
  });
  return env;
}

async function checkCard(env) {
  const root = env.roots.os;
  const cards = byClass(root, "vcard").map((c) => c.attrs["aria-label"]);
  assert.deepStrictEqual(
    cards,
    [
      "Latest (1.6.7.0)",
      "Corrections only (1.5.6.0)",
      "Back to DSI 1.5.0.2",
      "Latest (1.5.1.1)",
    ],
    "the Voice card comes after the Main ones",
  );
  const card = byClass(root, "vcard")[3];
  const text = shownText(card);
  assert.ok(text.includes("hardened parser"), "its summary");
  // What is true of every Voice version is said once, for the group (os-cards.test.js checks it is not per card).
  const voiceGroup = all(root).find((n) => n.tagName === "SECTION" && n.attrs["data-os"] === "voice");
  const lead = shownText(byClass(voiceGroup, "vlead")[0]);
  assert.ok(lead.includes("all six chips at once"), "it says where it goes");
  assert.ok(lead.includes("full backup before and after"));
  assert.ok(lead.includes("power the machine off and on between the two"), "one backup per power-on before 1.6.8.0");
  assert.ok(lead.includes("Installed: not reported yet"), "nothing reported yet");
  assert.ok(!text.includes("Technical text"), "details hidden");
  control(card, "Details of Latest (1.5.1.1)").click();
  assert.ok(shownText(card).includes("Technical text of Voice 1.5.1.1"));
  assert.strictEqual(
    controls(root, "Build Tempester Voice 1.5.1.1").length,
    1,
    "one build button",
  );
  const groupRows = (id) =>
    byClass(
      all(root).find((n) => n.tagName === "SECTION" && n.attrs["data-os"] === id),
      "vrow",
    ).map((r) => textOf(byClass(r, "vtoggle")[0]));
  assert.ok(
    groupRows("main").every((r) => !r.includes("Voice")),
    "the Voice OS is not in the Main history",
  );
  const voiceRows = groupRows("voice");
  assert.strictEqual(voiceRows.length, 1, "it has its own history, labelled Voice");
  assert.ok(voiceRows[0].includes("1.5.1.1") && voiceRows[0].includes("Voice"));
}

async function checkBuild(env) {
  const root = env.roots.os;
  const label = "Build Tempester Voice 1.5.1.1";
  assert.ok(control(root, label).disabled, "no build without the Voice file");
  await pick(root, 0, "Tempest_Main_1.5.0.2.syx", [1, 2]);
  assert.ok(control(root, label).disabled, "the Main file does not enable the Voice build");
  assert.ok(!control(root, "Build Tempester 1.6.7.0").disabled, "but it enables the Main ones");
  const before = env.sent.length;
  await pick(root, 2, "Tempest_Voice_1.5.syx", [3, 4]);
  assert.ok(!control(root, label).disabled, "the Voice file enables the Voice build");
  press(root, label);
  press(root, "Build Tempester 1.6.7.0");
  assert.deepStrictEqual(sentSince(env, before), [
    "os_build_voice\t1.5.1.1\t0304",
    "os_build\t1.6.7.0\t0102",
  ]);
  env.receive({
    event: "os_versions",
    versions: [MAIN, CORRECTIONS],
    voice_versions: [VOICE],
  });
  assert.ok(!control(root, label).disabled, "a new list keeps the Voice build enabled");
  env.receive({ event: "os_versions", versions: [MAIN, CORRECTIONS] });
  assert.strictEqual(controls(root, label).length, 0, "no Voice version: no Voice card");
  env.receive({ event: "os_versions", versions: [MAIN, CORRECTIONS], voice_versions: [VOICE] });
}

async function checkVoiceConfirm(env) {
  const root = env.roots.os;
  env.receive(voiceReady());
  assert.ok(textOf(root).includes("Tempester Voice 1.5.1.1: Voice OS (id 0x72), 64 blocks"));
  const before = env.sent.length;
  env.answers.dialog = "cancel";
  press(root, "Send to the machine...");
  await tick();
  assert.deepStrictEqual(sentSince(env, before), [], "refused: nothing sent");
  const dialog = dialogText(env);
  for (const piece of [
    "Send Tempester Voice 1.5.1.1 to ALL SIX Voice chips?",
    "all six Voice chips at once",
    "no trying one chip first",
    "first 29 of the 41 pages",
    "PICkit",
    "ICSP",
    "restores it only partly",
    "full machine backup",
    "right before and right after",
    "power the machine off and on between the two",
    "Stop the sequencer",
  ])
    assert.ok(dialog.includes(piece), "the confirmation says: " + piece + "\n" + dialog);
  assert.ok(!dialog.includes("Warning:"), "no Main warning when Rust gives none\n" + dialog);
  env.answers.dialog = "ok";
  press(root, "Send to the machine...");
  await tick();
  assert.deepStrictEqual(sentSince(env, before), ["os_send"]);
}

/* Voice 1.5.1.2 on a Main older than 1.6.9.0 (or unknown): Rust's `main_warning` comes first in the confirmation,
   and the update can still be sent (a warning, not a refusal). */
async function checkVoiceMainWarning(env) {
  const root = env.roots.os;
  const warning =
    "Install Main Tempester 1.6.9.0 or later first: the machine announced Tempester 1.6.8.0.";
  env.receive(
    voiceReady({ label: "Tempester Voice 1.5.1.2", version: "1.5.1.2", main_warning: warning }),
  );
  const before = env.sent.length;
  env.answers.dialog = "cancel";
  press(root, "Send to the machine...");
  await tick();
  const dialog = dialogText(env);
  assert.ok(dialog.includes("Warning: " + warning), "the Main warning is said\n" + dialog);
  assert.ok(
    dialog.indexOf("Warning:") < dialog.indexOf("This replaces the software"),
    "before the rest\n" + dialog,
  );
  env.answers.dialog = "ok";
  press(root, "Send to the machine...");
  await tick();
  assert.deepStrictEqual(sentSince(env, before), ["os_send"], "a warning, not a refusal");
}

async function checkMainKeepsItsText(env) {
  const root = env.roots.os;
  env.receive(mainReady());
  env.answers.dialog = "cancel";
  press(root, "Send to the machine...");
  await tick();
  const dialog = dialogText(env);
  assert.ok(dialog.includes("Send Tempester 1.6.7.0 to the Tempest?"), dialog);
  assert.ok(!dialog.includes("ALL SIX"), "the Main confirmation is its own");
  assert.ok(dialog.includes("flasher page is never touched"));
}

async function checkUnknownVoice(env) {
  const root = env.roots.os;
  env.receive(voiceReady({ label: "mine.syx", known: false, version: null, source: "file" }));
  const before = env.sent.length;
  env.answers.dialog = "ok";
  press(root, "Send to the machine...");
  await tick(); // two dialogs one after the other: the unknown build's, then the Voice one
  await tick();
  assert.deepStrictEqual(
    sentSince(env, before),
    ["os_send_unknown"],
    "an unknown Voice file needs both confirmations",
  );
  assert.ok(dialogText(env).includes("ALL SIX Voice chips"), "and the second one is the Voice one");
}

async function checkProgressAndEnd(env) {
  const root = env.roots.os;
  env.receive({ event: "os_started", label: "Tempester Voice 1.5.1.1", chip: "Voice", total: 74951 });
  env.answers.dialog = "cancel";
  press(root, "Cancel the update");
  await tick();
  const cancel = dialogText(env);
  assert.ok(cancel.includes("six Voice chips received only part"), cancel);
  assert.ok(cancel.includes("PICkit"), cancel);
  env.receive({ event: "os_finished", state: "done", sent: 74951, total: 74951 });
  const text = textOf(root);
  assert.ok(text.includes("sent to the six Voice chips"), "the end message names the six chips");
  assert.ok(text.includes("make a full machine backup"), "and suggests the backup");
  assert.ok(text.includes("compares pages 0-40 of every chip"), "and says what it will check");

  env.receive({ event: "os_started", label: "Tempester 1.6.7.0", chip: "Main", total: 600000 });
  env.receive({ event: "os_finished", state: "done", sent: 600000, total: 600000 });
  assert.ok(!textOf(root).includes("six Voice chips"), "a Main update says nothing of the Voice chips");
  env.receive({ event: "os_started", label: "Tempester 1.6.7.0", chip: "Main", total: 600000 });
  env.answers.dialog = "cancel";
  press(root, "Cancel the update");
  await tick();
  assert.ok(!dialogText(env).includes("Voice chips"), "the Main cancel text is its own");
}

(async () => {
  const env = setup();
  await checkCard(env);
  await checkBuild(env);
  await checkVoiceConfirm(env);
  await checkVoiceMainWarning(env);
  await checkMainKeepsItsText(env);
  await checkUnknownVoice(env);
  await checkProgressAndEnd(env);
  console.log("ok, Voice card, build, all-six confirmation, end message and cancel warning");
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
