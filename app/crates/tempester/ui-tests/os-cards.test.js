/* The OS cards on a fake DOM (run by tools/test_app.sh): `node os-cards.test.js <ui folder>`. A folded card keeps its
   Build button in the header row, visible, labelled, reachable by keyboard and working. A Voice card has the shape of
   a Main release card (short title, date chip, Build and fold in the header; summary, change lists, Details in the
   body); the Voice note is said once, for the group; what the chips report (`voice_installed`, counted by Rust) is one
   line for the group, and a card says « Installed on k/6 chips » only when no other version answers the same word.
   os-cards-counter-proof.js runs the same checks on copies with one behaviour broken. */
"use strict";
const assert = require("assert");
const {
  all,
  textOf,
  isShown,
  load,
  tick,
  control,
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
  version: "1.6.8.0",
  name: "Tempester 1.6.8.0",
  kind: "release",
  date: "2026-11-14",
  summary: "Newest Main.",
  changes: { fixes: ["A fix."] },
  description: "Technical text.",
};
/* Kind "release" on purpose: the shape of an older Voice version that is still offered ("Previous"). The real 1.5.1.0
   is "superseded" and gets no card (os-groups.test.js); the two numbers are kept for the word they share. */
const VOICE = {
  version: "1.5.1.0",
  name: "Tempester Voice 1.5.1.0",
  kind: "release",
  date: "2026-11-13",
  summary: "First Voice.",
  changes: { fixes: ["A Voice fix."] },
  description: "Technical text of Voice.",
};
const OTHER_VOICE = {
  ...VOICE,
  version: "1.5.1.1",
  name: "Tempester Voice 1.5.1.1",
  date: "2026-11-14",
  summary: "Later Voice.",
  changes: { fixes: ["A later Voice fix."] },
};
const LATEST_VOICE = "Latest (1.5.1.1)";
const PREVIOUS_VOICE = "Previous (1.5.1.0)";
const NOTE = "One update goes to all six chips at once.";

const byClass = (root, name) =>
  all(root).filter((n) => (n.attrs.class || "").split(/\s+/).includes(name));
const card = (root, title) =>
  all(root).find((n) => n.tagName === "SECTION" && n.attrs["aria-label"] === title);
const group = (root, id) =>
  all(root).find((n) => n.tagName === "SECTION" && n.attrs["data-os"] === id);
const head = (section) => byClass(section, "vcard-head")[0];
const chips = (section) => byClass(head(section), "chip").map(textOf);
const countIn = (text, piece) => text.split(piece).length - 1;

function setup() {
  const env = load(process.argv[2], FILES);
  env.kit.updateSession({ connected: true });
  env.kit.pages.find((p) => p.id === "os").onShow();
  env.receive({
    event: "os_versions",
    versions: [MAIN],
    voice_versions: [OTHER_VOICE, VOICE],
  });
  return env;
}

async function checkFoldedBuild(env) {
  const root = env.roots.os;
  await pick(root, 0, "Tempest_Main_1.5.0.2.syx", [1, 2]);
  await pick(root, 2, "Tempest_Voice_1.5.syx", [3, 4]);
  const cases = [
    ["Latest (1.6.8.0)", "Build Tempester 1.6.8.0", "os_build\t1.6.8.0\t0102"],
    ["Back to DSI 1.5.0.2", "Back to DSI 1.5.0.2", "os_original\t0102"],
    [PREVIOUS_VOICE, "Build Tempester Voice 1.5.1.0", "os_build_voice\t1.5.1.0\t0304"],
  ];
  for (const [title, label, sent] of cases) {
    control(root, "Show or hide " + title).click();
    const folded = card(root, title);
    assert.ok(
      !isShown(byClass(folded, "vcard-body")[0]),
      title + ": the body is folded",
    );
    const build = control(folded, label); // visible: controls() only finds shown buttons
    assert.strictEqual(build.tagName, "BUTTON", "a real button, so the keyboard reaches it");
    assert.ok(
      head(folded).children.includes(build),
      title + ": the Build button is in the header row",
    );
    assert.notStrictEqual(build.attrs.tabindex, "-1", "not taken out of the tab order");
    assert.ok(!build.disabled, "enabled by the DSI file even when folded");
    assert.strictEqual(build.attrs["aria-label"], label, "labelled");
    const before = env.sent.length;
    press(folded, label);
    assert.deepStrictEqual(Array.from(env.sent).slice(before), [sent]);
    control(root, "Show or hide " + title).click();
    assert.ok(isShown(byClass(card(root, title), "vcard-body")[0]));
  }
}

/* What a card is made of, without its words: the header row's parts and the body's blocks, in order. */
function shape(section) {
  const part = (n) => n.tagName.toLowerCase() + (n.attrs.class ? "." + n.attrs.class : "");
  const body = byClass(section, "vcard-body")[0];
  return {
    head: head(section).children.map(part),
    body: body.children.map(part),
    groups: byClass(body, "vgroup").map((g) => textOf(g.children[0])),
  };
}

function checkVoiceShape(env) {
  const root = env.roots.os;
  const main = card(root, "Latest (1.6.8.0)");
  for (const [title, v] of [
    [LATEST_VOICE, OTHER_VOICE],
    [PREVIOUS_VOICE, VOICE],
  ]) {
    const voice = card(root, title);
    assert.ok(voice, title + ": titled like the Main cards");
    assert.ok(all(group(root, "voice")).includes(voice), "in the Voice group");
    assert.deepStrictEqual(shape(voice), shape(main), title + ": the shape of a Main release card");
    assert.strictEqual(textOf(head(voice).children[0]), title, "a short title");
    assert.deepStrictEqual(chips(voice), [v.date], title + ": the date chip, nothing else");
    const body = textOf(byClass(voice, "vcard-body")[0]);
    assert.ok(body.includes(v.summary), "the summary");
    assert.ok(body.includes(v.changes.fixes[0]), "the change list");
    assert.strictEqual(countIn(body, NOTE), 0, title + ": the Voice note is not repeated in the card");
  }
  const lead = byClass(group(root, "voice"), "vlead");
  assert.strictEqual(lead.length, 1, "one lead for the group");
  assert.ok(isShown(lead[0]));
  assert.strictEqual(countIn(textOf(group(root, "voice")), NOTE), 1, "the note once for the group");
  assert.strictEqual(byClass(group(root, "main"), "vlead").length, 0, "the Main group has none");
}

const status = (root) => textOf(byClass(group(root, "voice"), "vstatus")[0]);
const installedCards = (root) =>
  byClass(group(root, "voice"), "vcard").filter((c) => chips(c).some((t) => t.includes("Installed")));

function checkInstalled(env) {
  const root = env.roots.os;
  const voiceText = () => textOf(group(root, "voice"));
  assert.ok(status(root).startsWith("Installed: not reported yet"), "nothing reported yet");
  assert.strictEqual(installedCards(root).length, 0);

  const shared = ["1.5.1.0", "1.5.1.1"];
  const both = [
    { version: "1.5.1.0", count: 6, of: 6, same_word: shared },
    { version: "1.5.1.1", count: 6, of: 6, same_word: shared },
  ];
  env.receive({ event: "installed", voice_installed: both });
  assert.strictEqual(
    status(root),
    "Installed on 6/6 chips: Tempester Voice 1.5.1.0 or 1.5.1.1. They answer the same version word, so the chips cannot say which of them runs.",
    "a shared word names both versions",
  );
  assert.strictEqual(countIn(voiceText(), "Installed"), 1, "said once, for the group");
  assert.strictEqual(installedCards(root).length, 0, "no card claims a shared word");
  assert.deepStrictEqual(chips(card(root, LATEST_VOICE)), [OTHER_VOICE.date]);

  env.receive({
    event: "installed",
    voice_installed: [{ version: "1.5.1.0", count: 4, of: 6 }],
  });
  assert.deepStrictEqual(
    chips(card(root, PREVIOUS_VOICE)),
    [VOICE.date, "Installed on 4/6 chips"],
    "a word only one version answers: its card says so",
  );
  assert.deepStrictEqual(chips(card(root, LATEST_VOICE)), [OTHER_VOICE.date], "another version is not marked");
  assert.strictEqual(status(root), "Installed on 4/6 chips: Tempester Voice 1.5.1.0.");

  env.receive({
    event: "installed",
    voice_installed: [{ version: "1.5.1.0", count: 0, of: 6 }],
  });
  assert.strictEqual(installedCards(root).length, 0, "a count of zero is not installed");
  assert.strictEqual(status(root), "Installed: no chip runs Tempester Voice.");

  env.receive({ event: "installed", voice_installed: [] });
  assert.strictEqual(status(root), "Installed: no chip runs Tempester Voice.", "reported, none runs it");

  env.receive({ event: "state", voice_installed: both });
  assert.ok(status(root).startsWith("Installed on 6/6 chips"), "the state event too");

  env.receive({ event: "installed", voice_installed: null });
  assert.ok(status(root).startsWith("Installed: not reported yet"), "not reported");

  env.receive({ event: "installed", voice_installed: both });
  env.receive({
    event: "os_versions",
    versions: [MAIN],
    voice_versions: [OTHER_VOICE, VOICE],
  });
  assert.ok(status(root).startsWith("Installed on 6/6 chips"), "kept after a new list");
}

(async () => {
  const env = setup();
  await checkFoldedBuild(env);
  checkVoiceShape(env);
  checkInstalled(env);
  await tick();
  console.log(
    "ok, Build stays in a folded card's header, Voice cards shaped like Main ones, one note and one installed line for the group",
  );
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
