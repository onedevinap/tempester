/* An OS send in one piece on the OS update page, on a fake DOM with a fake clock (run by tools/test_app.sh):
   `node os-one-piece.test.js <ui folder>`. Windows and Linux hand the file to the MIDI driver in one call, which
   reports nothing until it ends (about 190 s for a Main at the cable rate, maybe longer): the page counts the elapsed
   time itself against the least the send takes, says when it runs longer, never calls it failed on its own, and stops
   counting at the end; a close of the window then asks whether to quit now (interrupting the update, with the Main or
   the Voice warning) or to keep sending, and only "Quit now" sends `quit`. A send in pieces keeps its toast.
   os-one-piece-counter-proof.js runs the same checks on copies with one behaviour broken. */
"use strict";
const assert = require("assert");
const { all, textOf, isShown, load, tick, control } = require("./fake-dom.js");

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

/* The page's timers and `Date.now`, moved by `advance` only. */
function fakeClock() {
  const timers = new Map();
  let now = 1_700_000_000_000;
  let next = 1;
  class FakeDate extends Date {
    static now() {
      return now;
    }
  }
  return {
    globals: {
      Date: FakeDate,
      setTimeout: (fn, ms) => {
        timers.set(next, { at: now + (ms || 0), fn });
        return next++;
      },
      clearTimeout: (id) => timers.delete(id),
    },
    advance(ms) {
      const end = now + ms;
      for (;;) {
        const due = [...timers].filter(([, t]) => t.at <= end).sort((a, b) => a[1].at - b[1].at)[0];
        if (!due) break;
        timers.delete(due[0]);
        now = due[1].at;
        due[1].fn();
      }
      now = end;
    },
  };
}

const MAIN = { label: "Tempester 1.6.9.1", chip: "Main", total: 599559, seconds: 192 };
const VOICE = { label: "Tempester Voice 1.5.1.2", chip: "Voice", total: 74951, seconds: 24 };

const shownText = (node) =>
  isShown(node)
    ? node.children.map((c) => (c.text !== undefined ? c.text : shownText(c))).join(" ")
    : "";
const statusOf = (root) =>
  all(root).filter((n) => n.attrs && n.attrs.role === "status").map(shownText);
const sendingLine = (root) => statusOf(root).find((t) => t.includes("Tempester")) || "";
const sentSince = (env, from) => Array.from(env.sent).slice(from);
const dialogText = (env) =>
  textOf(env.byId["confirm-title"]) + "\n" + textOf(env.byId["confirm-body"]);

function setup() {
  const clock = fakeClock();
  const env = load(process.argv[2], FILES, clock.globals);
  env.kit.updateSession({ connected: true });
  env.kit.pages.find((p) => p.id === "os").onShow();
  return { env, clock, root: env.roots.os };
}

function start(env, send, inOnePiece) {
  env.receive({ event: "os_started", ...send, in_one_piece: inOnePiece });
}

/* The time counted once a second against the least the send takes; past it, "still being sent", never a failure. */
function checkElapsed({ env, clock, root }) {
  start(env, MAIN, true);
  let line = sendingLine(root);
  assert.ok(line.includes("being sent in one piece"), line);
  assert.ok(line.includes("no progress until the end and no cancel"), line);
  assert.ok(line.includes("0 s elapsed; it takes at least 3 min 12 s"), line);
  assert.ok(control(root, "Cancel the update").disabled, "no cancel in one piece");
  clock.advance(65_000);
  line = sendingLine(root);
  assert.ok(line.includes("1 min 05 s elapsed; it takes at least 3 min 12 s"), line);
  clock.advance(127_000);
  assert.ok(sendingLine(root).includes("3 min 12 s elapsed"), "the least time itself is not late");
  assert.ok(!sendingLine(root).includes("longer than"), sendingLine(root));
  const before = env.sent.length;
  clock.advance(1_728_000);
  line = sendingLine(root);
  assert.ok(line.includes("still being sent in one piece, 32 min 00 s elapsed"), line);
  assert.ok(line.includes("longer than the 3 min 12 s the cable rate needs"), line);
  const page = shownText(root);
  assert.ok(!page.includes("failed") && !page.includes("incomplete"), "no failure before Rust says so\n" + page);
  assert.ok(control(root, "Send to the machine...").disabled, "still sending");
  assert.deepStrictEqual(sentSince(env, before), [], "the page asks nothing of Rust on its own");
}

/* A close during the send: "Keep sending" keeps the app; "Quit now" sends `quit`; the texts say what it costs. */
async function checkQuit({ env, root }) {
  const before = env.sent.length;
  env.answers.dialog = "cancel";
  env.receive({ event: "close_blocked" });
  await tick();
  assert.deepStrictEqual(sentSince(env, before), [], "kept sending: nothing sent");
  const dialog = dialogText(env);
  assert.ok(dialog.includes("Quit while the update is being sent?"), dialog);
  assert.ok(dialog.includes("cannot be cancelled"), dialog);
  assert.ok(dialog.includes("power it off and on, and send the file again"), dialog);
  assert.ok(!dialog.includes("PICkit"), "the Main text is its own\n" + dialog);
  assert.strictEqual(textOf(env.byId["confirm-ok"]), "Quit now");
  assert.strictEqual(textOf(env.byId["confirm-cancel"]), "Keep sending");
  assert.ok(control(root, "Send to the machine...").disabled, "still sending after Keep sending");
  env.answers.dialog = "ok";
  env.receive({ event: "close_blocked" });
  await tick();
  assert.deepStrictEqual(sentSince(env, before), ["quit"]);
}

/* The end: the counted time stops, the line says how long it took, the result is Rust's. */
function checkEnd({ env, clock, root }) {
  env.receive({ event: "os_progress", sent: MAIN.total, total: MAIN.total });
  assert.ok(sendingLine(root).includes("32 min 00 s elapsed"), "the only progress, at the end, keeps the counted line");
  clock.advance(5_000);
  env.receive({ event: "os_finished", label: MAIN.label, state: "done", sent: MAIN.total, total: MAIN.total });
  const ended = sendingLine(root);
  assert.ok(ended.includes("the send in one piece ended after 32 min 05 s"), ended);
  assert.ok(shownText(root).includes("All bytes were sent."), shownText(root));
  clock.advance(10_000);
  assert.strictEqual(sendingLine(root), ended, "nothing counts after the end");
  assert.strictEqual(control(root, "Cancel the update").disabled, true);
}

async function checkVoiceAndFailure({ env, clock, root }) {
  start(env, VOICE, true);
  env.answers.dialog = "cancel";
  env.receive({ event: "close_blocked" });
  await tick();
  const dialog = dialogText(env);
  assert.ok(dialog.includes("six Voice chips"), dialog);
  assert.ok(dialog.includes("PICkit"), dialog);
  clock.advance(3_000);
  env.receive({ event: "os_finished", label: VOICE.label, state: "failed", message: "sending sysex message failed", total: VOICE.total });
  assert.ok(sendingLine(root).includes("ended after 3 s"), sendingLine(root));
  assert.ok(shownText(root).includes("The send failed: sending sysex message failed"), shownText(root));
}

/* A send in pieces (macOS): its own progress, a cancel, and a close only toasts. */
async function checkPieces({ env, root }) {
  start(env, MAIN, false);
  assert.ok(!sendingLine(root).includes("in one piece"), sendingLine(root));
  assert.ok(!control(root, "Cancel the update").disabled);
  const before = env.sent.length;
  env.answers.dialog = "ok";
  env.receive({ event: "close_blocked" });
  await tick();
  assert.deepStrictEqual(sentSince(env, before), [], "no quit offered for a send in pieces");
  assert.ok(textOf(env.byId["toast"]).includes("cancel it first"), textOf(env.byId["toast"]));
  env.receive({ event: "os_finished", label: MAIN.label, state: "done", sent: MAIN.total, total: MAIN.total });
}

(async () => {
  const rig = setup();
  checkElapsed(rig);
  await checkQuit(rig);
  checkEnd(rig);
  await checkVoiceAndFailure(rig);
  await checkPieces(rig);
  console.log("ok, elapsed time, longer than the cable time without failure, quit or keep sending, end, Voice, pieces");
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
