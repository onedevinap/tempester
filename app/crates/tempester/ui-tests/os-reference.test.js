/* The reference-hash flows of the OS update page and the installed-OS check on a fake DOM (run by
   tools/test_app.sh): `node os-reference.test.js <ui folder>`.
   Built image against its reference, a file recognised or called unknown (an extra confirmation before sending it),
   a refused build leaving nothing to send, the read-back of the machine (match, mismatch, constant, incomplete),
   the installed-OS line on the OS page and the Machine page, the copy of the full hashes, the fixed slots of the
   evidence row. os-reference-counter-proof.js runs the same checks on copies with one behaviour broken. */
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
  "page-backup.js",
  "page-os.js",
  "page-play.js",
];

const IMAGE = "4fa3c1b2d5e6" + "0".repeat(52);
const SYX = "9b2e77aa10cd" + "1".repeat(52);
const OTHER = "e0e0e0e0e0e0" + "2".repeat(52);
const TICK = "✓";
const sentSince = (env, from) => Array.from(env.sent).slice(from);

const ready = (extra) => ({
  event: "os_ready",
  label: "Tempester 1.6.1.1",
  source: "built",
  chip: "Main",
  id: 0x71,
  blocks: 512,
  checksum: 0x1a2b,
  size: 600000,
  seconds: 192,
  known: true,
  version: "1.6.1.1",
  sha256_image: IMAGE,
  sha256_syx: SYX,
  ...extra,
});

const record = (extra) => ({
  label: "Tempester 1.6.1.1",
  version: "1.6.1.1",
  at: "2026-11-08_120000Z",
  sha256_image: IMAGE,
  verified_at: null,
  ...extra,
});

async function checkBuilt(env) {
  const root = env.roots.os;
  env.kit.updateSession({ connected: true });
  env.receive(ready());
  const shown = textOf(root);
  assert.ok(
    shown.includes("Built image matches the reference " + TICK),
    "a built image says it matches the reference",
  );
  assert.ok(shown.includes("SHA-256 4fa3c1b2d5e6…"), "the hash is shortened");
  assert.ok(shown.includes(IMAGE), "the full image hash is in the page");
  assert.ok(shown.includes(SYX), "the full .syx hash is in the page");
  const row = all(root).find((n) => n.attrs.class === "evidence");
  assert.ok(
    all(row).some((n) => n.attrs.class === "evidence-hashes mono"),
    "hashes are monospace",
  );
  let copied = null;
  env.navigator.clipboard = { writeText: async (text) => (copied = text) };
  press(root, "Copy the full SHA-256 values");
  await tick();
  assert.strictEqual(
    copied,
    "Image SHA-256 " + IMAGE + "\n.syx file SHA-256 " + SYX,
    "the copy button gives both full hashes",
  );
  const before = env.sent.length;
  env.answers.dialog = "ok";
  press(root, "Send to the machine...");
  await tick();
  assert.deepStrictEqual(
    sentSince(env, before),
    ["os_send"],
    "a known build is sent without the extra confirmation",
  );
}

async function checkKnownFile(env) {
  const root = env.roots.os;
  env.receive(ready({ source: "file", label: "Tempester 1.6.1.0" }));
  assert.ok(textOf(root).includes("This file is Tempester 1.6.1.0 " + TICK));
}

async function checkUnknown(env) {
  const root = env.roots.os;
  env.receive(
    ready({
      source: "file",
      label: "mine.syx",
      known: false,
      version: null,
      sha256_syx: OTHER,
    }),
  );
  assert.ok(textOf(root).includes("Unknown build"), "an unknown file says so");
  const before = env.sent.length;
  env.answers.dialog = "cancel";
  press(root, "Send to the machine...");
  await tick();
  assert.deepStrictEqual(sentSince(env, before), [], "refused: nothing sent");
  const body = textOf(env.byId["confirm-body"]);
  assert.ok(body.includes("matches no Tempester version known"), body);
  assert.ok(body.includes("needs a programmer"), "the risk is spelled out");
  assert.ok(
    textOf(env.byId["confirm-title"]).includes("Send an unknown build?"),
    "the extra confirmation comes first",
  );
  env.answers.dialog = "ok";
  press(root, "Send to the machine...");
  await tick(); // the extra confirmation closes, then the usual one opens
  await tick();
  assert.deepStrictEqual(
    sentSince(env, before),
    ["os_send_unknown"],
    "confirmed twice: the unknown send goes out, not the plain one",
  );
  assert.ok(
    textOf(env.byId["confirm-title"]).includes("Send mine.syx to the Tempest?"),
    "the usual confirmation follows",
  );
}

function checkRefused(env) {
  const root = env.roots.os;
  env.receive(ready());
  env.receive({
    event: "error",
    during: "os_build",
    code: "invalid",
    message:
      "the built image does not match the reference of Tempester 1.6.1.1",
  });
  assert.ok(
    textOf(root).includes("Refused: the built image does not match"),
    "the refusal is shown in plain words",
  );
  assert.ok(
    !textOf(root).includes("Built image matches"),
    "no reference match left on screen",
  );
  assert.ok(
    control(root, "Send to the machine...").disabled,
    "nothing to send after a refusal",
  );
}

function checkInstalledLines(env) {
  const os = env.roots.os;
  const machine = env.roots.machine;
  assert.ok(
    textOf(machine).includes("Installed: nothing sent from this app yet"),
  );
  env.receive({ event: "state", channel: 1, installed: record() });
  for (const root of [os, machine])
    assert.ok(
      textOf(root).includes("Installed: Tempester 1.6.1.1, not verified"),
      "installed, not verified",
    );
  env.receive({
    event: "installed",
    installed: record({ verified_at: "2026-11-08_130000Z" }),
  });
  for (const root of [os, machine])
    assert.ok(
      textOf(root).includes("Installed: Tempester 1.6.1.1, verified " + TICK),
      "installed, verified",
    );
  env.receive({ event: "installed", installed: record() });
}

function checkVerify(env) {
  const root = env.roots.os;
  env.kit.updateSession({ connected: true });
  const before = env.sent.length;
  press(root, "Verify the installed OS");
  assert.deepStrictEqual(sentSince(env, before), ["os_verify_start"]);
  const base = {
    event: "os_verify",
    label: "Tempester 1.6.1.1",
    version: "1.6.1.1",
    expected: IMAGE,
  };
  env.receive({ ...base, state: "listening" });
  assert.ok(textOf(root).includes("hold SHIFT + SYSTEM"), "guides the user");
  assert.ok(
    control(root, "Stop listening") &&
      !control(root, "Stop listening").disabled,
  );
  env.receive({ ...base, state: "incomplete", detail: "refused: checksum" });
  assert.ok(textOf(root).includes("Incomplete or damaged dump"));
  assert.ok(!control(root, "Stop listening").disabled, "still listening");
  env.receive({ ...base, state: "constant", byte: 255, found: OTHER });
  assert.ok(textOf(root).includes("Constant dump: every byte is 0xFF"));
  env.receive({
    ...base,
    state: "mismatch",
    found: OTHER,
    dir: "/d/backups/x",
  });
  const mismatch = textOf(root);
  assert.ok(
    mismatch.includes("Mismatch: the machine does not run Tempester 1.6.1.1"),
  );
  assert.ok(
    mismatch.includes("Sent 4fa3c1b2d5e6…") &&
      mismatch.includes("Machine e0e0e0e0e0e0…"),
    "both hashes shown",
  );
  assert.ok(mismatch.includes(OTHER) && mismatch.includes("/d/backups/x"));
  const FLASHER = "f1".repeat(32);
  env.receive({
    ...base,
    state: "match",
    found: IMAGE,
    flasher: FLASHER,
    dir: "/d/backups/y",
  });
  const matched = textOf(root);
  assert.ok(
    matched.includes(
      "Update routine (flasher page): the machine's own, never rewritten by an update",
    ) && matched.includes(FLASHER),
    "the flasher page is reported apart, with its hash",
  );
  assert.ok(
    !matched.includes("Mismatch"),
    "a different flasher page is never a mismatch",
  );
  assert.ok(
    textOf(root).includes("The machine runs exactly Tempester 1.6.1.1 " + TICK),
    "match",
  );
  assert.ok(
    control(root, "Stop listening").disabled,
    "a result ends the listening",
  );
  press(root, "Verify the installed OS");
  env.receive({ ...base, state: "listening" });
  press(root, "Stop listening");
  assert.deepStrictEqual(sentSince(env, before).slice(-2), [
    "os_verify_start",
    "os_verify_stop",
  ]);
  env.receive({ ...base, state: "idle" });
}

function checkVerifyWithoutRecord(env) {
  const root = env.roots.os;
  env.receive({ event: "installed", installed: null });
  assert.ok(
    control(root, "Verify the installed OS").disabled,
    "nothing installed from the app: nothing to verify",
  );
  env.receive({ event: "installed", installed: record() });
  assert.ok(!control(root, "Verify the installed OS").disabled);
}

const RUNNING = {
  version: "1.6.5.0",
  label: "Tempester 1.6.5.0",
  known: true,
};
const REPORTED_LINE =
  "The machine runs Tempester 1.6.5.0 (it reports it), not verified";

/* What the machine reports (n°39) is what it runs, even when this app sent another OS or none. */
function checkReportedVersion(env) {
  const os = env.roots.os;
  const machine = env.roots.machine;
  env.kit.updateSession({ connected: true });
  env.receive({ event: "installed", installed: record(), running: RUNNING });
  for (const root of [os, machine]) {
    const shown = textOf(root);
    assert.ok(shown.includes(REPORTED_LINE), "the reported version is shown");
    assert.ok(
      !shown.includes("Installed: Tempester 1.6.1.1"),
      "the record no longer stands for what runs",
    );
  }
  const note = all(os).find(
    (n) =>
      n.attrs.class === "statusbox warn" &&
      textOf(n).includes("this app last sent"),
  );
  assert.ok(note && isShown(note), "the difference is said in a note");
  assert.strictEqual(
    textOf(note),
    "The machine reports Tempester 1.6.5.0; this app last sent Tempester 1.6.1.1.",
  );
  assert.ok(
    textOf(env.byId.connbar).includes("OS: Tempester 1.6.5.0"),
    "the pill names the OS",
  );

  env.receive({
    event: "installed",
    installed: record({ label: "Tempester 1.6.5.0", version: "1.6.5.0" }),
    running: RUNNING,
  });
  assert.ok(!isShown(note), "no note when the record agrees");

  env.receive({ event: "installed", installed: null, running: RUNNING });
  assert.ok(textOf(os).includes(REPORTED_LINE), "no record: still shown");
  assert.ok(!isShown(note), "no record: nothing to differ from");
  assert.ok(
    !control(os, "Verify the installed OS").disabled,
    "a known reported version can be verified without any record",
  );
  env.receive({
    event: "installed",
    installed: null,
    running: { ...RUNNING, version: "9.9.9.9", known: false },
  });
  assert.ok(
    control(os, "Verify the installed OS").disabled,
    "an unknown reported version and no record: nothing to compare with",
  );
  env.receive({ event: "installed", installed: record(), running: null });
  assert.ok(
    textOf(os).includes("Installed: Tempester 1.6.1.1, not verified"),
    "an OS that cannot answer keeps the record line",
  );
  assert.ok(
    !textOf(env.byId.connbar).includes("1.6.5.0"),
    "a version no longer reported leaves the pill",
  );
}

function checkReportedVerify(env) {
  const os = env.roots.os;
  const machine = env.roots.machine;
  env.receive({ event: "installed", installed: record(), running: RUNNING });
  press(os, "Verify the installed OS");
  const base = {
    event: "os_verify",
    label: "Tempester 1.6.5.0",
    version: "1.6.5.0",
    source: "reported",
    expected: IMAGE,
  };
  env.receive({ ...base, state: "match", found: IMAGE, dir: "/d/b" });
  assert.ok(
    textOf(os).includes(
      "The machine runs exactly Tempester 1.6.5.0 " +
        TICK +
        " (compared with the image of the version the machine reports)",
    ),
    "the match says what it was compared with",
  );
  assert.ok(textOf(os).includes("Reported 4fa3c1b2d5e6"), "reported hash");
  for (const root of [os, machine])
    assert.ok(
      textOf(root).includes(
        "The machine runs Tempester 1.6.5.0 (it reports it), verified " + TICK,
      ),
      "verified is shown for the reported version",
    );
  env.receive({
    event: "installed",
    installed: record(),
    running: { ...RUNNING, version: "1.6.4.0", label: "Tempester 1.6.4.0" },
  });
  assert.ok(
    !textOf(os).includes("Tempester 1.6.4.0 (it reports it), verified"),
    "a proof of one version never covers another",
  );
  const other = { ...RUNNING, version: "1.6.4.0", label: "Tempester 1.6.4.0" };
  assert.strictEqual(
    env.window.Tempester.osVerify.installedLine(null, other, "1.6.5.0").tone,
    "",
    "the line itself refuses a proof made for another version",
  );
  assert.strictEqual(
    env.window.Tempester.osVerify.installedLine(null, RUNNING, "1.6.5.0").tone,
    "good",
  );
  env.receive({ event: "installed", installed: record(), running: RUNNING });
  assert.ok(
    textOf(os).includes(REPORTED_LINE),
    "the proof is forgotten once the version changed",
  );
  env.receive({ ...base, state: "mismatch", found: OTHER, dir: "/d/b" });
  assert.ok(
    textOf(os).includes(
      "Mismatch: the machine does not run Tempester 1.6.5.0 (the version the machine reports)",
    ),
  );
  assert.ok(textOf(os).includes(REPORTED_LINE), "a mismatch proves nothing");
  env.receive({
    ...base,
    label: "Tempester 1.6.4.0",
    version: "1.6.4.0",
    source: "sent",
    also: "Tempester 1.6.5.0",
    state: "mismatch",
    found: OTHER,
    dir: "/d/b",
  });
  assert.ok(
    textOf(os).includes(
      "Mismatch: the machine does not run Tempester 1.6.4.0 (sent by this app) nor Tempester 1.6.5.0 (the version the machine reports)",
    ),
    "a mismatch against two candidates names both",
  );
  env.receive({
    ...base,
    source: "sent",
    also: null,
    state: "match",
    found: IMAGE,
    dir: "/d/b",
  });
  assert.ok(
    textOf(os).includes("The machine runs exactly Tempester 1.6.5.0 " + TICK),
    "a match with what was sent says so, whatever the machine announces",
  );
  assert.ok(
    !textOf(os).includes("compared with the image of the version"),
    "a match with the sent OS is not said to be a comparison with the report",
  );
  env.kit.updateSession({ connected: false });
  assert.ok(
    textOf(os).includes("Installed: Tempester 1.6.1.1, not verified"),
    "disconnected: the reported version is forgotten",
  );
}

function checkSlots(env) {
  const row = all(env.roots.os).find((n) => n.attrs.class === "evidence");
  assert.strictEqual(
    row.children.length,
    5,
    "the evidence row keeps its five slots",
  );
}

(async () => {
  const env = load(process.argv[2], FILES);
  await checkBuilt(env);
  await checkKnownFile(env);
  await checkUnknown(env);
  checkRefused(env);
  checkInstalledLines(env);
  checkVerify(env);
  checkVerifyWithoutRecord(env);
  checkReportedVersion(env);
  checkReportedVerify(env);
  checkSlots(env);
  console.log(
    "ok, built, known, unknown, refused, installed line, read-back, reported version, slots",
  );
})().catch((e) => {
  console.error(e.stack || e);
  process.exit(1);
});
