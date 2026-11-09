/* Sending a whole project back to the machine, on a fake DOM (run by tools/test_app.sh with node):
   `node project-send.test.js <ui folder>`. The send action on a project row and on the versions of an auto backup,
   the confirmation (what is replaced, which beats stay, the backup), the version guard (warning in full, only
   « Send anyway » and « Send without Play Lists » offered, never a plain send), the progress with cancel, the offer
   to fetch the project back and compare, and the answer of the comparison. project-send-counter-proof.js runs the
   same checks on copies of the scripts with one behaviour broken: they must fail there. */
"use strict";
const fs = require("fs");
const path = require("path");
const assert = require("assert");
const { all, textOf, load, tick, controls, press } = require("./fake-dom.js");

const FILES = [
  "bridge.js",
  "icons.js",
  "kit.js",
  "project-ui.js",
  "lib-model.js",
  "lib-dialogs.js",
  "lib-flow.js",
  "lib-browser.js",
  "page-library.js",
];

const TOKEN = "c".repeat(64);
const SEND_ROW = "Send the whole project to the machine...";
const WARNING =
  "The machine's OS may misread Play Lists: lists after a used one can come back wrong; beats and sounds are not affected.";

function project(id, name, extra) {
  return Object.assign(
    {
      id,
      kind: "projects",
      name,
      display: name,
      folder: "",
      size: 100,
      hash: "h" + id,
      favourite: false,
      tags: [],
      note: "",
      source: "machine",
      created: 1e9,
      modified: 1e9,
      fetched: null,
      history: 0,
      capture: null,
      beats: 16,
      sound_type: "",
      osc: [],
      slot: false,
    },
    extra,
  );
}

const withClass = (root, cls) =>
  all(root).filter((n) =>
    (n.attrs && n.attrs.class ? n.attrs.class.split(" ") : []).includes(cls),
  );

function setup(uiDir) {
  const env = load(uiDir, FILES);
  const T = env.window.Tempester;
  const root = env.roots.library;
  env.kit.updateSession({ connected: true, fake: true });
  const slot = project("machine-projects", "Machine: current project", {
    slot: true,
    history: 2,
    path: "",
  });
  for (const kind of ["sounds", "beats"])
    env.receive({ event: "library", kind, files: [], trash: 0 });
  env.receive({
    event: "library",
    kind: "projects",
    files: [project("p1", "Project A.syx"), slot],
    trash: 0,
  });
  const seg = all(root).find(
    (n) => n.tagName === "BUTTON" && textOf(n).startsWith("Projects"),
  );
  seg.click();
  const asked = [];
  const choice = { value: "cancel" };
  T.dialogs.ask = async (spec) => (asked.push(spec), choice.value);
  env.sent.length = 0;
  return { env, T, root, asked, choice };
}

const plan = (extra) =>
  Object.assign(
    {
      event: "lib_project_plan",
      kind: "projects",
      id: "p1",
      version: null,
      status: "unknown",
      changed: false,
      item: "Project A.syx",
      from: "projects/Project A.syx",
      token: TOKEN,
      beats: 16,
      total: 16,
      lists_used: [true, false, false, false],
      guard: "fixed",
      installed: "Tempester 1.6.2.0",
      lines: [
        "Sending replaces the machine's whole project in RAM: its beats, their sounds and its 4 Play Lists. It is not saved to the machine's flash until you save it there.",
        "The machine's current project is NOT in your library. It is first saved as \"auto backup <date>\".",
      ],
      warning: [],
      without_lists: ["Send without Play Lists: the machine's own 4 Play Lists become empty."],
    },
    extra,
  );

const bodyText = (spec) =>
  textOf({ children: spec.body.filter(Boolean), text: undefined });
const values = (spec) => Array.from(spec.buttons, (b) => b.value);

/* ---- the send action ------------------------------------------------------------------------------------------ */

function checkActions(ctx) {
  const { env, root } = ctx;
  const sends = controls(root, SEND_ROW);
  assert.ok(sends.length, "a project row has a send action");
  assert.ok(!sends[0].disabled, "the send action of a project is enabled");
  sends[0].click();
  assert.deepStrictEqual(
    Array.from(env.sent),
    ["lib_project_check\tp1\t-"],
    "the check goes out first, for the project file",
  );
  const slotSend = controls(root, "Send (open a version from its history)");
  assert.ok(slotSend.length && slotSend[0].disabled, "an auto backup is sent from its history");
  const opens = controls(root, "Open (not available for projects)");
  assert.ok(opens.length && opens[0].disabled, "a project is not opened in a card");
}

async function checkHistory(ctx) {
  const { env, root } = ctx;
  env.sent.length = 0;
  const slotRow = withClass(root, "lib-row").find((n) =>
    textOf(n).includes("Machine: current project"),
  );
  slotRow.fire("click", {});
  env.receive({
    event: "lib_history",
    id: "machine-projects",
    kind: "projects",
    name: "Machine: current project",
    slot: true,
    modified: 1e9,
    versions: [
      { index: 0, date: 1e9, label: "auto backup 2026-11-08", same_as_current: false },
      { index: 1, date: 1e9 + 60, label: "auto backup 2026-11-09", same_as_current: false },
    ],
  });
  await tick();
  const label = "Send this version to the machine as a whole project...";
  const buttons = controls(root, label);
  assert.strictEqual(buttons.length, 2, "each version of an auto backup can be sent");
  env.sent.length = 0;
  buttons[1].click();
  assert.deepStrictEqual(
    Array.from(env.sent),
    ["lib_project_check\tmachine-projects\t1"],
    "the version number goes with the check",
  );
}

/* ---- the confirmations ---------------------------------------------------------------------------------------- */

async function checkConfirmations(ctx) {
  const { env, asked, choice } = ctx;
  const answer = async (p, value) => {
    asked.length = 0;
    env.sent.length = 0;
    choice.value = value;
    env.receive(p);
    await tick();
    return asked.at(-1);
  };

  let spec = await answer(plan(), "normal");
  assert.deepStrictEqual(values(spec), ["cancel", "normal"]);
  const text = bodyText(spec);
  assert.ok(text.includes("whole project in RAM"), "says what is replaced");
  assert.ok(text.includes("not saved to the machine's flash"), "says it is not in flash");
  assert.ok(text.includes("auto backup"), "says where the machine's project goes");
  assert.ok(text.includes("includes the Play List fix"), "says why there is no warning");
  assert.deepStrictEqual(Array.from(env.sent), ["lib_project_send\tp1\t-\t" + TOKEN + "\tnormal"]);

  await answer(plan(), "cancel");
  assert.deepStrictEqual(Array.from(env.sent), [], "declined: nothing is sent");

  spec = await answer(
    plan({ beats: 4, lines: ["The file holds 4 of 16 beats: beat(s) 5, 6 are not sent and keep their current content on the machine."] }),
    "normal",
  );
  assert.ok(bodyText(spec).includes("keep their current content"), "says which beats stay");

  spec = await answer(plan({ version: 3, id: "machine-projects" }), "normal");
  assert.deepStrictEqual(Array.from(env.sent), ["lib_project_send\tmachine-projects\t3\t" + TOKEN + "\tnormal"]);

  spec = await answer(plan({ guard: "no_risk", lists_used: [false, false, false, false] }), "normal");
  assert.ok(bodyText(spec).includes("No Play List of this project can be misread"), "no list used: says so, no warning");
  assert.deepStrictEqual(values(spec), ["cancel", "normal"]);

  spec = await answer(plan({ status: "identical", lines: ["The machine already holds exactly this project: nothing to send."] }), "ok");
  assert.ok(spec.title.includes("Nothing to send"));
  assert.deepStrictEqual(Array.from(env.sent), [], "identical: nothing is sent");

  spec = await answer(plan({ status: "no_answer", token: "none", lines: ["The machine did not answer, so its current project cannot be backed up."] }), "normal");
  assert.ok(spec.buttons.some((b) => b.label === "Send without a backup"), "a silent machine needs the explicit consent");
  assert.deepStrictEqual(Array.from(env.sent), ["lib_project_send\tp1\t-\tnone\tnormal"]);
}

async function checkGuard(ctx) {
  const { env, asked, choice } = ctx;
  const risky = plan({
    guard: "at_risk",
    installed: null,
    lists_used: [true, true, false, false],
    warning: [
      WARNING,
      "The app has no record of the OS installed on the machine.",
      "This project uses Play List(s) 1, 2.",
    ],
  });
  asked.length = 0;
  env.sent.length = 0;
  choice.value = "cancel";
  env.receive(risky);
  await tick();
  const spec = asked.at(-1);
  assert.ok(bodyText(spec).includes(WARNING), "the warning is shown in full");
  assert.ok(bodyText(spec).includes("Play List(s) 1, 2"), "says which lists the project uses");
  assert.ok(bodyText(spec).includes("become empty"), "says what sending without Play Lists does");
  assert.deepStrictEqual(values(spec), ["cancel", "anyway", "no_lists"], "no plain send is offered");
  assert.ok(spec.buttons.find((b) => b.value === "anyway").label.startsWith("Send anyway"));
  assert.ok(spec.buttons.find((b) => b.value === "no_lists").label.startsWith("Send without Play Lists"));
  assert.deepStrictEqual(Array.from(env.sent), [], "dismissed: nothing is sent");

  for (const mode of ["anyway", "no_lists"]) {
    env.sent.length = 0;
    choice.value = mode;
    env.receive(risky);
    await tick();
    assert.deepStrictEqual(Array.from(env.sent), ["lib_project_send\tp1\t-\t" + TOKEN + "\t" + mode]);
  }

  choice.value = "anyway";
  env.sent.length = 0;
  env.receive(Object.assign({}, risky, { status: "no_answer", token: "none" }));
  await tick();
  const silent = asked.at(-1);
  assert.ok(
    silent.buttons.every((b) => b.value === "cancel" || b.label.includes("(no backup)")),
    "a silent machine and a risky OS: both consents in the labels",
  );
}

/* ---- progress, cancel, verify ---------------------------------------------------------------------------------- */

async function checkCard(ctx) {
  const { env, root, asked, choice } = ctx;
  const status = () => textOf(withClass(root, "lib-card-status").at(-1));
  const cancel = () => controls(root, "Cancel");
  const verifyLabel = "Fetch the project back from the machine and compare it with what was sent";
  const verify = () => controls(root, verifyLabel)[0];
  assert.ok(verify().disabled, "nothing to verify before a send");

  env.sent.length = 0;
  env.receive({ event: "project_send", state: "started", message: 0, beats: 16, sent: 0, total: 1000 });
  env.receive({ event: "project_send", state: "sending", message: 3, beats: 16, sent: 250, total: 1000 });
  assert.ok(status().includes("beat 3 of 16"), "progress names the beat: " + status());
  assert.ok(cancel().length, "a send can be cancelled");
  cancel()[0].click();
  assert.deepStrictEqual(Array.from(env.sent), ["lib_project_cancel"], "cancel stops the project send, not a receive");
  assert.ok(verify().disabled, "nothing to verify while sending");

  env.receive({ event: "project_send", state: "cancelled", beats: 16, sent: 300, total: 1000 });
  assert.ok(status().includes("cancelled") && status().includes("mix"), "a cancel says what the machine holds");
  assert.ok(verify().disabled, "a cancelled send leaves nothing to verify");
  assert.strictEqual(cancel().length, 0, "the cancel button goes away");

  asked.length = 0;
  env.sent.length = 0;
  choice.value = "ok";
  env.receive({ event: "project_send", state: "done", beats: 16, sent: 1000, total: 1000 });
  await tick();
  assert.ok(status().includes("Project sent"), "the result is clear: " + status());
  assert.ok(asked.at(-1).title.includes("Project sent"), "the fetch-back is offered");
  assert.deepStrictEqual(Array.from(env.sent), ["lib_project_verify"], "accepting fetches it back");
  assert.ok(!verify().disabled, "the compare button stays available after a send");
  env.sent.length = 0;
  verify().click();
  assert.deepStrictEqual(Array.from(env.sent), ["lib_project_verify"]);

  choice.value = "cancel";
  env.sent.length = 0;
  env.receive({ event: "project_send", state: "started", message: 0, beats: 16, sent: 0, total: 10 });
  env.receive({ event: "project_send", state: "done", beats: 16, sent: 10, total: 10 });
  await tick();
  assert.deepStrictEqual(Array.from(env.sent), [], "« Not now »: nothing is fetched");

  env.sent.length = 0;
  env.receive({ event: "project_send", state: "failed", message: "port closed", code: "midi", beats: 16, sent: 5, total: 10 });
  assert.ok(status().includes("port closed"), "a failure says why");
}

async function checkVerify(ctx) {
  const { env, asked } = ctx;
  asked.length = 0;
  env.receive({
    event: "project_verify",
    state: "match",
    differences: [],
    notes: ["Beat(s) 5, 6 were not in the sent project."],
    beats_compared: 16,
    lines: ["The machine holds exactly what was sent."],
  });
  await tick();
  let text = bodyText(asked.at(-1));
  assert.ok(asked.at(-1).title.includes("holds what was sent"));
  assert.ok(text.includes("exactly what was sent") && text.includes("not in the sent project"));

  env.receive({
    event: "project_verify",
    state: "differ",
    differences: ["Play List 2 differs: its End word", "beat 7: name \"A\" sent, \"B\" on the machine"],
    notes: [],
    beats_compared: 16,
    lines: ["The machine does NOT hold exactly what was sent: 2 difference(s)."],
  });
  await tick();
  text = bodyText(asked.at(-1));
  assert.ok(asked.at(-1).title.includes("does NOT hold"));
  assert.ok(text.includes("Play List 2 differs") && text.includes("beat 7: name"), "each difference is reported");

  const many = Array.from({ length: 45 }, (_, i) => "difference " + i);
  env.receive({ event: "project_verify", state: "differ", differences: many, notes: [], beats_compared: 16, lines: ["x"] });
  await tick();
  assert.ok(bodyText(asked.at(-1)).includes("and 15 more"), "a long list is cut and says so");
}

function checkFake(ctx) {
  const { env, root } = ctx;
  env.sent.length = 0;
  const off = "The fake machine reads Play Lists correctly. Click to make it read them like the stock OS (n°28 defect).";
  press(root, off);
  assert.deepStrictEqual(Array.from(env.sent), ["fake_project_defect\t1"]);
  env.receive({ event: "fake_project_defect", on: true });
  env.sent.length = 0;
  press(root, "The fake machine reads Play Lists like the stock OS (n°28 defect). Click to make it read them correctly.");
  assert.deepStrictEqual(Array.from(env.sent), ["fake_project_defect\t0"]);
}

function checkSources(uiDir) {
  const read = (f) => fs.readFileSync(path.join(uiDir, f), "utf8");
  for (const file of ["project-ui.js", "lib-browser.js"])
    assert.ok(
      !/waits for a verified fix|not available yet|receiver is not verified/.test(read(file)),
      file + " still says the project send waits for the receiver fix",
    );
  const css = read("library.css");
  assert.ok(/\.lib-warning \{[^}]*border/.test(css), "the warning has its own box");
}

async function main() {
  const uiDir = process.argv[2];
  const ctx = setup(uiDir);
  checkActions(ctx);
  await checkHistory(ctx);
  await checkConfirmations(ctx);
  await checkGuard(ctx);
  await checkCard(ctx);
  await checkVerify(ctx);
  checkFake(ctx);
  checkSources(uiDir);
  console.log("ok: send action, confirmations, version guard, progress and cancel, compare, fake");
}

main().catch((e) => {
  console.error(e.stack);
  process.exit(1);
});
