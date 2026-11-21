/* The full machine backup section of the Backup page (full-backup.js, Tempester 1.6.6.0) on a fake DOM (run by
   tools/test_app.sh): `node full-backup.test.js <ui folder>`. What it holds: the section follows the feature table
   and the connection, Save is possible only when all eight messages passed, a refused or missing chip is shown with
   its reason, the page says the files are never sent back and has no control that does. full-backup-counter-proof.js
   runs the same checks on copies of the script with one behaviour broken: they must fail there. */
"use strict";
const assert = require("assert");
const { all, textOf, load, control, press, isShown } = require("./fake-dom.js");

const FILES = [
  "bridge.js",
  "icons.js",
  "kit.js",
  "panel-perform.js",
  "full-backup.js",
  "page-backup.js",
];
const [uiDir] = process.argv.slice(2);
const env = load(uiDir, FILES);
const root = env.roots.backup;
const section = () =>
  all(root).find((n) => n.attrs["aria-label"] === "Full machine backup");
const REQUEST = "Request the full backup from the machine";
const SAVE = "Save the full backup in a dated folder";
const CANCEL = "Cancel the full backup";
const feature = (on) => ({
  event: "state",
  os_known: true,
  features: {
    full_backup: { on, since: "1.6.6.0", label: "Full machine backup" },
  },
});
const CHIPS = [0, 1, 2, 3, 4, 5].map((k) => "Voice " + k).concat([
  "Main boot flash",
  "System settings",
]);
const row = (chip, verdict, detail, size) => ({
  chip,
  id: 0x78,
  size: size || 74951,
  verdict,
  detail: detail || "",
});
const rows = (verdict, detail) => CHIPS.map((c) => row(c, verdict, detail));
const sentSince = (n) => env.sent.slice(n).map((line) => line.trim());
let count = 0;
function check(name, fn) {
  fn();
  count++;
  void name;
}

check("the section is on the Backup page, with the PICkit notice", () => {
  assert.ok(section(), "a Full machine backup section");
  const text = textOf(section());
  assert.ok(text.includes("backups for a programmer (PICkit) restore"));
  assert.ok(text.includes("never sends them back to the machine"));
});

check("the request needs a connection and an OS that has the feature", () => {
  assert.ok(control(root, REQUEST).disabled, "not connected");
  env.receive(feature(true));
  assert.ok(control(root, REQUEST).disabled, "not connected, even with the feature");
  env.kit.updateSession({ connected: true, fake: false });
  env.receive(feature(false));
  assert.ok(control(root, REQUEST).disabled, "OS without the feature");
  assert.ok(
    textOf(section()).includes(
      "The full machine backup needs Tempester 1.6.6.0 or later",
    ),
    "says which version brings it",
  );
  env.receive({ event: "identity", device: 0x7f, version: 0x1501, tempester: false });
  env.receive(feature(true));
  assert.ok(control(root, REQUEST).disabled, "the DSI OS has none, whatever the table says");
  env.receive({ event: "identity", device: 0x7f, version: 0x7f2, tempester: true });
  assert.ok(!control(root, REQUEST).disabled, "1.6.6.0 or later, connected");
});

check("the request sends exactly one command, and locks itself while it runs", () => {
  const before = env.sent.length;
  press(root, REQUEST);
  assert.deepStrictEqual(sentSince(before), ["full_backup_start"]);
  env.receive({ event: "full_backup", state: "waiting", rows: rows("waiting"), extras: [] });
  assert.ok(control(root, REQUEST).disabled, "no second request while one runs");
  assert.ok(!control(root, CANCEL).disabled);
  assert.ok(control(root, SAVE).disabled);
  assert.ok(textOf(section()).includes("Waiting for the machine"));
});

check("a partial answer cannot be saved", () => {
  const partial = rows("ok", "ok").slice(0, 3).concat(rows("waiting").slice(3));
  env.receive({ event: "full_backup", state: "receiving", rows: partial, extras: [] });
  assert.ok(control(root, SAVE).disabled);
});

check("a refused or missing chip is shown with its reason and blocks the save", () => {
  const list = rows("ok", "aligned");
  list[2] = row("Voice 2", "refused", "misaligned dump: power the machine off and on, then ask again");
  list[3] = row("Voice 3", "missing", "Voice 3 did not come", 0);
  env.receive({ event: "full_backup", state: "refused", rows: list, extras: [] });
  const text = textOf(section());
  assert.ok(text.includes("misaligned dump: power the machine off and on"));
  assert.ok(text.includes("Voice 3 did not come"));
  const line = (chip) =>
    textOf(all(section()).filter((n) => n.tagName === "TR").find((n) => textOf(n).startsWith(chip)));
  assert.ok(line("Voice 2").includes("refused") && !line("Voice 2").includes("OK"));
  assert.ok(line("Voice 3").includes("missing"));
  assert.ok(line("Voice 0").includes("OK"));
  assert.ok(text.includes("Nothing is written until all eight pass"));
  assert.ok(control(root, SAVE).disabled, "refused: no save");
  assert.ok(!control(root, REQUEST).disabled, "a new request is possible after a refusal");
});

check("a clean answer can be saved, and the Debug-menu dumps are listed with it", () => {
  const extras = [{ chip: "Main", id: 0x71, size: 1, verdict: "ok", detail: "512 blocks, checksum 0x1a2b OK" }];
  env.receive({ event: "full_backup", state: "ready", rows: rows("ok", "aligned"), extras });
  assert.ok(!control(root, SAVE).disabled);
  const text = textOf(section());
  assert.ok(text.includes("All eight messages passed"));
  assert.ok(text.includes("Main (0x71): 512 blocks"));
  const before = env.sent.length;
  press(root, SAVE);
  assert.deepStrictEqual(sentSince(before), ["full_backup_save"]);
  env.receive({ event: "full_backup_saved", dir: "/d/backups/Full backup 1", files: ["/d/backups/Full backup 1/Voice 0.syx"] });
  assert.ok(textOf(section()).includes("Saved in /d/backups/Full backup 1"));
  env.receive({ event: "full_backup", state: "idle", rows: [], extras: [] });
  assert.ok(control(root, SAVE).disabled);
});

const CALIBRATION = {
  a: ["8340", "8240", "8600", "8a00", "9200", "a200", "8b00", "a100", "c700", "f300"],
  b: ["8210", "8410", "8810", "9010", "a010", "c010", "8b10", "a110", "c710", "f310"],
  c: ["d300", "c6c0", "ba80", "ae40", "a200", "95c0", "8980", "7d40", "7100", "64c0", "5880", "4c40"],
  scalars: ["0344", "7ea3", "7b90"],
};
const withWords = (r, words) => Object.assign({}, r, { calibration: words || CALIBRATION });
const lineOf = (chip) =>
  all(section()).filter((n) => n.tagName === "TR").find((n) => textOf(n).startsWith(chip));
const wordsOf = (chip) =>
  all(section())
    .filter((n) => n.tagName === "TR")
    .find((n) => textOf(n).startsWith("Calibration of " + chip + ":"));
const badgeOf = (chip) => all(lineOf(chip)).find((n) => n.tagName === "SPAN" && /\bbadge\b/.test(n.attrs.class || ""));

check("a chip with a calibration warning can be saved, its row and the status say so", () => {
  const list = rows("ok", "aligned").map((r) => withWords(r));
  list[2] = withWords(
    row("Voice 2", "warning", "aligned; WARNING: table A does not increase: an unusual calibration shape, saved exactly as read"),
  );
  list[3] = withWords(row("Voice 3", "warning", "aligned; WARNING: table A does not increase"));
  env.receive({ event: "full_backup", state: "ready", rows: list, extras: [] });
  assert.ok(!control(root, SAVE).disabled, "a warning passes: the set can be saved");
  assert.ok(textOf(lineOf("Voice 2")).includes("warning"), "the row says warning");
  assert.ok(!textOf(lineOf("Voice 2")).includes("OK"), "a warned row is not shown as OK");
  assert.ok(textOf(lineOf("Voice 2")).includes("table A does not increase"), "the reason is shown");
  assert.ok(/\bconstant\b/.test(badgeOf("Voice 2").attrs.class), "the warning colour, not the refusal's");
  assert.ok(!/\binvalid\b/.test(badgeOf("Voice 2").attrs.class));
  assert.ok(/\bok\b/.test(badgeOf("Voice 0").attrs.class));
  const status = all(section()).find((n) => n.attrs.role === "status" && textOf(n).startsWith("You can save"));
  assert.ok(status, "the status says the set can be saved");
  assert.ok(textOf(status).includes("2 calibration page(s) with a warning"), textOf(status));
  assert.ok(/\bwarn\b/.test(status.attrs.class), "in the warning colour");
});

check("every Voice row shows its 35 calibration words, a refused one included, with what may be posted", () => {
  const refusedWords = Object.assign({}, CALIBRATION, { a: ["ffff"].concat(CALIBRATION.a.slice(1)) });
  const list = rows("ok", "aligned").map((r, k) => (k < 6 ? withWords(r) : r));
  list[4] = withWords(
    row("Voice 4", "refused", "calibration page: word 0 reads as erased (0xFFFF)"),
    refusedWords,
  );
  list[5] = row("Voice 5", "refused", "refused: checksum");
  env.receive({ event: "full_backup", state: "refused", rows: list, extras: [] });
  const words = textOf(wordsOf("Voice 0"));
  assert.ok(words.includes("A " + CALIBRATION.a.join(" ")), words);
  assert.ok(words.includes("B " + CALIBRATION.b.join(" ")), words);
  assert.ok(words.includes("C " + CALIBRATION.c.join(" ")), words);
  assert.ok(words.includes("scalars 0344 7ea3 7b90"), words);
  assert.ok(wordsOf("Voice 4"), "the refused chip's words are shown");
  assert.ok(textOf(wordsOf("Voice 4")).includes("A ffff 8240"), textOf(wordsOf("Voice 4")));
  assert.ok(!wordsOf("Voice 5"), "no words when the message did not decode");
  assert.ok(!wordsOf("Main boot flash"));
  const text = textOf(section());
  assert.ok(text.includes("you may copy them into a bug report"), "the words may be posted");
  assert.ok(text.includes("Never post the Voice .syx files: they hold DSI's Voice OS."), "the image never");
  env.receive({ event: "full_backup", state: "idle", rows: [], extras: [] });
  const note = all(section()).find((n) => textOf(n).startsWith("Below each Voice row"));
  assert.ok(!isShown(note), "no words, no note");
});

check("cancel sends its command", () => {
  env.receive({ event: "full_backup", state: "receiving", rows: rows("waiting"), extras: [] });
  const before = env.sent.length;
  press(root, CANCEL);
  assert.deepStrictEqual(sentSince(before), ["full_backup_cancel"]);
  env.receive({ event: "full_backup", state: "idle", rows: [], extras: [] });
  assert.ok(control(root, CANCEL).disabled, "nothing to cancel when idle");
});

check("no control of the section can send a backup to the machine", () => {
  const labels = all(section())
    .filter((n) => n.tagName === "BUTTON")
    .map((n) => n.attrs["aria-label"] || textOf(n));
  assert.ok(labels.length >= 3, "the three buttons are there: " + labels);
  for (const label of labels)
    assert.ok(!/\bsend\b|restore|write to the machine/i.test(label), "a button that could send: " + label);
  const before = env.sent.length;
  for (const label of [REQUEST, SAVE, CANCEL]) {
    const button = control(root, label);
    button.disabled = false;
    button.click();
  }
  for (const line of sentSince(before))
    assert.ok(
      ["full_backup_start", "full_backup_save", "full_backup_cancel"].includes(line),
      "unexpected command: " + line,
    );
});

check("a refused full-backup command is reported to the user", () => {
  env.byId.toast = env.document.getElementById("toast");
  env.receive({ event: "error", during: "full_backup_start", code: "precondition", message: "The full backup needs Tempester 1.6.6.0 or later" });
  assert.ok(textOf(env.byId.toast).includes("needs Tempester 1.6.6.0"), "a toast shows the reason");
});

check("before 1.6.8.0 the page says one backup per power-on and offers the power-cycle confirmation", () => {
  const CYCLED = "I powered the machine off and on: allow the next full backup";
  const withRepeat = (repeat) => ({
    event: "state",
    os_known: true,
    features: {
      full_backup: { on: true, since: "1.6.6.0", label: "Full machine backup" },
      full_backup_repeat: { on: repeat, since: "1.6.8.0", label: "Repeated full backups" },
    },
  });
  const limit = () =>
    all(section()).find((n) => textOf(n).startsWith("One full backup per power-on on this OS"));
  env.kit.updateSession({ connected: true, fake: false });
  env.receive(withRepeat(false));
  assert.ok(limit() && isShown(limit()), "the rule is shown before any request");
  assert.ok(control(root, CYCLED), "the confirmation is offered");
  const before = env.sent.length;
  press(root, CYCLED);
  assert.deepStrictEqual(sentSince(before), ["full_backup_power_cycled"]);
  env.receive(withRepeat(true));
  assert.ok(!isShown(limit()), "1.6.8.0: no limit said");
  assert.throws(() => control(root, CYCLED), /no visible button/, "1.6.8.0: no confirmation to give");
});

console.log(count + " checks");
