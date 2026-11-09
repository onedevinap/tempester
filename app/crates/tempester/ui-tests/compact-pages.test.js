/* The compact Machine, Backup, OS update and Play pages and the top-bar connection controls on a fake DOM
   (run by tools/test_app.sh): `node compact-pages.test.js <ui folder>`. The pages were rebuilt with icon buttons
   and info badges; every command line the old pages sent must still go out, from the new controls, and the
   confirmations of the risky actions must keep their full text. compact-counter-proof.js runs these checks on
   copies of the scripts with one command or one confirmation removed: they must fail there. */
"use strict";
const assert = require("assert");
const {
  all,
  textOf,
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

/* Command lines the old pages sent, each from the new control that now sends it. */
const OLD_COMMANDS = {
  machine: [
    "ports",
    "connect\tTempest",
    "disconnect",
    "identity",
    "set_channel\t5",
    "reveal\tdata",
    "monitor\ton",
    "monitor\toff",
    "monitor_clock\ton",
    "monitor_clock\toff",
  ],
  backup: [
    "backup_start",
    "backup_stop",
    "backup_save",
    "reveal\tbackups",
    "fake_debug_send",
  ],
  os: [
    "os_versions",
    "os_build\t1.6.1.0\t0102",
    "os_original\t0102",
    "os_validate\tother.syx\t0102",
    "os_send",
    "os_cancel",
    "identity",
  ],
  /* The pads go out by number (Rust plays the machine's note of the pad), no more as "note 36 + k". */
  play: [
    "pad\ton\t0\t100",
    "pad\toff\t0\t0",
    "clock_start\t120",
    "clock_stop",
    "clock_bpm\t130",
  ],
};

/* A pad note table far from 36 + k and from the white keys: pad k is note 7k + 20. */
const SCRAMBLED = Array.from({ length: 32 }, (_, k) => (k * 7 + 20) % 128);

const sentSince = (env, from) => Array.from(env.sent).slice(from);

function topBar(env) {
  return env.byId.connbar;
}

function checkTopBar(env) {
  const bar = topBar(env);
  const [select] = bar.children;
  env.receive({ event: "ports", ports: ["IAC", "Tempest"] });
  assert.strictEqual(select.value, "Tempest", "the Tempest port is preferred");
  const before = env.sent.length;
  press(bar, "Refresh the MIDI ports");
  press(bar, "Connect");
  env.kit.updateSession({ connected: true, port: "Tempest" });
  press(bar, "Disconnect");
  env.kit.updateSession({ connected: false });
  assert.deepStrictEqual(sentSince(env, before), [
    "ports",
    "connect\tTempest",
    "disconnect",
  ]);
  env.kit.updateSession({ connected: true });
  env.receive({
    event: "identity",
    device: 0x7f,
    version: 0x1501,
    tempester: true,
  });
  assert.ok(
    textOf(bar).includes("Tempester OS"),
    "the identity answer is shown in the top bar",
  );
  env.receive({ event: "error", during: "identity", code: "timeout" });
  assert.ok(textOf(bar).includes("No answer"), "no answer is shown there too");
}

function checkMachine(env) {
  const root = env.roots.machine;
  env.kit.updateSession({ connected: true, channel: 1 });
  const before = env.sent.length;
  const channel = all(root).find((n) => n.attrs.id === "channel");
  channel.value = "5";
  channel.fire("change");
  press(root, "Check the machine identity");
  press(root, "Show the data folder");
  press(root, "Monitor off (click to start)");
  env.receive({ event: "monitor", on: true, clock: false });
  press(root, "Clock and active sensing hidden (click to show)");
  env.receive({ event: "monitor", on: true, clock: true });
  press(root, "Clock and active sensing shown (click to hide)");
  press(root, "Monitor on (click to stop)");
  assert.deepStrictEqual(sentSince(env, before), [
    "set_channel\t5",
    "identity",
    "reveal\tdata",
    "monitor\ton",
    "monitor_clock\ton",
    "monitor_clock\toff",
    "monitor\toff",
  ]);
  env.receive({
    event: "identity",
    device: 0x7f,
    version: 0x1501,
    tempester: true,
  });
  assert.ok(textOf(root).includes("version word 0x1501"), "identity shown");
  for (const label of [
    "Pause the list",
    "Clear the list",
    "Copy the selected lines, or every line shown",
    "Jump to the latest line",
  ])
    control(root, label);
  const line = {
    t: 1,
    dir: "in",
    kind: "cc",
    text: "Note on 36",
    raw: "90 24 64",
  };
  env.receive({ event: "monitor", on: true, clock: false });
  env.receive({
    event: "midi_log",
    lines: [line],
    dropped: 0,
    clock_in: 0,
    clock_out: 0,
  });
  assert.ok(textOf(root).includes("Note on 36"), "a monitor line is listed");
  press(root, "Clear the list");
  assert.ok(!textOf(root).includes("Note on 36"), "Clear empties the list");
  assert.ok(
    controls(root, "Connect").length === 0,
    "no connect button left on the Machine page",
  );
}

function checkBackup(env) {
  const root = env.roots.backup;
  env.kit.updateSession({ connected: true, fake: true });
  const before = env.sent.length;
  press(root, "Start listening");
  env.receive({ event: "backup", state: "listening", pieces: [] });
  press(root, "Stop listening");
  env.receive({
    event: "backup",
    state: "idle",
    pieces: [
      { chip: "Main", id: 0x71, size: 1000, verdict: "ok", detail: "ok" },
    ],
  });
  press(root, "Save backup");
  press(root, "Show the backups folder");
  press(root, "Play the Debug-menu sends (fake machine)");
  assert.deepStrictEqual(sentSince(env, before), [
    "backup_start",
    "backup_stop",
    "backup_save",
    "reveal\tbackups",
    "fake_debug_send",
  ]);
  const all_text = textOf(root);
  assert.ok(all_text.includes("Main (0x71)"), "received images are listed");
  assert.ok(
    all_text.includes("hold both SHIFT and SYSTEM"),
    "the steps are still in the page (info badge)",
  );
  assert.ok(all_text.includes("every byte 0xFF"), "the constant warning too");
  env.receive({
    event: "backup_saved",
    dir: "/d/backups",
    files: ["Main.syx"],
    flagged: true,
  });
  const shown = textOf(root);
  for (const text of ["Saved in /d/backups", "Main.syx", "not a real backup"])
    assert.ok(shown.includes(text), "saved result shows: " + text);
  env.kit.updateSession({ fake: false });
  assert.ok(
    controls(root, "Play the Debug-menu sends (fake machine)").length === 0,
    "the fake helper is hidden on a real machine",
  );
}

const CONFIRM_TEXT = [
  "This replaces the Main software of the machine with Tempester 1.6.1.0",
  "Do not power off or unplug anything while it is sent.",
  "When the FX1 LEDs stop at the top, power the Tempest off and on.",
  "The flasher page is never touched, so a new update is always possible.",
];

async function checkOs(env) {
  const root = env.roots.os;
  env.kit.updateSession({ connected: true });
  const before = env.sent.length;
  env.kit.pages.find((p) => p.id === "os").onShow();
  env.receive({
    event: "os_versions",
    versions: [
      {
        version: "1.6.1.0",
        name: "Additions",
        kind: "release",
        date: "2026-11-08",
        summary: "Newest.",
        changes: { additions: ["One addition."] },
        description: "Newest, in detail.",
      },
    ],
  });
  await pick(root, 0, "Tempest_Main_1.5.0.2.syx", [1, 2]);
  press(root, "Build Tempester 1.6.1.0");
  press(root, "Back to DSI 1.5.0.2");
  await pick(root, 1, "other.syx", [1, 2]);
  env.receive({
    event: "os_ready",
    label: "Tempester 1.6.1.0",
    chip: "Main",
    id: 0x71,
    blocks: 512,
    checksum: 0x1a2b,
    size: 1024,
    seconds: 300,
  });
  assert.ok(textOf(root).includes("512 blocks, checksum 0x1a2b OK"));
  // A refused confirmation sends nothing; the full text is in the dialog.
  env.answers.dialog = "cancel";
  press(root, "Send to the machine...");
  await tick();
  assert.ok(
    !sentSince(env, before).includes("os_send"),
    "Send is not sent when the confirmation is refused",
  );
  const dialog = textOf(env.byId["confirm-body"]);
  for (const text of CONFIRM_TEXT)
    assert.ok(dialog.includes(text), "confirmation says: " + text);
  assert.ok(
    textOf(env.byId["confirm-title"]).includes(
      "Send Tempester 1.6.1.0 to the Tempest?",
    ),
  );
  env.answers.dialog = "ok";
  press(root, "Send to the machine...");
  await tick();
  env.receive({ event: "os_started", label: "Tempester 1.6.1.0", total: 1024 });
  env.answers.dialog = "cancel";
  press(root, "Cancel the update");
  await tick();
  assert.ok(
    !sentSince(env, before).includes("os_cancel"),
    "Cancel is not sent when the confirmation is refused",
  );
  assert.ok(
    textOf(env.byId["confirm-body"]).includes("the update will be incomplete"),
  );
  env.answers.dialog = "ok";
  press(root, "Cancel the update");
  await tick();
  env.receive({
    event: "os_finished",
    state: "cancelled",
    sent: 512,
    total: 1024,
  });
  assert.ok(textOf(root).includes("Cancelled after"), "result is shown");
  press(root, "Check the machine identity");
  assert.deepStrictEqual(sentSince(env, before), [
    "os_versions",
    "os_build\t1.6.1.0\t0102",
    "os_original\t0102",
    "os_validate\tother.syx\t0102",
    "os_send",
    "os_cancel",
    "identity",
  ]);
}

/* The pad note table of the Machine page: opened over the page, shows Rust's table, sends an edited note, goes back
   to the table when Rust refuses it, learns, resets. */
function checkPadNotes(env) {
  assert.ok(
    control(env.roots.play, "Pad 1"),
    "no note shown before Rust sends the table",
  );
  const root = env.roots.machine;
  const panel = all(root).find((n) => n.attrs.class === "padnotes");
  assert.ok(panel && panel.hidden, "the table is closed at first");
  const before = env.sent.length;
  press(root, "Pad notes: the MIDI note of each pad");
  assert.ok(!panel.hidden, "opened");
  env.receive({ event: "pad_notes", notes: SCRAMBLED });
  const input = (name) =>
    all(root).find((n) => n.attrs["aria-label"] === "Note of pad " + name);
  assert.strictEqual(input("A9").value, "76");
  assert.strictEqual(input("B16").value, String(SCRAMBLED[31]));
  assert.ok(textOf(panel).includes("E5"), "note names shown (76 = E5)");
  input("A9").value = "50";
  input("A9").fire("change");
  env.receive({
    event: "error",
    during: "pad_note",
    code: "invalid",
    message: "note D3 is already the note of pad A5",
  });
  assert.strictEqual(input("A9").value, "76", "a refused note goes back");
  assert.ok(textOf(panel).includes("already the note of pad A5"));
  press(root, "Learn: tap the pads on the machine");
  assert.ok(textOf(panel).includes("16 Sounds"), "learning says what to do");
  env.receive({ event: "pad_learned", pad: 3, note: 41 });
  const cell = all(panel).find(
    (n) => n.attrs.class === "padnote" && textOf(n).startsWith("A4"),
  );
  assert.ok(cell.classList.contains("learned"), "the learned pad is marked");
  assert.ok(textOf(panel).includes("Pad A4 = note 41 F2."));
  press(root, "Back to the default notes");
  press(panel, "Close");
  assert.ok(panel.hidden, "closed");
  assert.deepStrictEqual(sentSince(env, before), [
    "pad_notes",
    "pad_note\t8\t50",
    "pad_notes_reset",
  ]);
}

function checkPlay(env) {
  const root = env.roots.play;
  env.kit.updateSession({ connected: true, channel: 3 });
  const before = env.sent.length;
  env.receive({ event: "pad_notes", notes: SCRAMBLED });
  const pad = control(root, "Pad 1, note 20 G#0");
  pad.fire("pointerdown", { pointerId: 1 });
  pad.fire("pointerup");
  const fifteen = control(root, "Pad 15, note 118 A#8");
  fifteen.fire("pointerdown", { pointerId: 1 });
  fifteen.fire("pointerup");
  const bpm = all(root).find((n) => n.attrs.id === "bpm");
  bpm.value = "120";
  press(root, "Start the clock");
  env.receive({ event: "clock", running: true, bpm: 120 });
  bpm.value = "130";
  bpm.fire("input");
  press(root, "Stop the clock");
  assert.deepStrictEqual(sentSince(env, before), [
    "pad\ton\t0\t100",
    "pad\toff\t0\t0",
    "pad\ton\t14\t100",
    "pad\toff\t14\t0",
    "clock_start\t120",
    "clock_bpm\t130",
    "clock_stop",
  ]);
  assert.ok(textOf(root).includes("Ch 3"), "the channel is shown");
  assert.ok(textOf(root).includes("Clock running at 120 BPM."));
  assert.strictEqual(
    all(root).filter((n) => n.attrs.class === "playpad").length,
    16,
    "16 pads",
  );
  checkPlayGrid(root);
  checkSequencer(env, root);
}

/* The pads sit in the grid in the machine's order: the CSS lays 8 per row, so pads 1-8 are the top row. */
function checkPlayGrid(root) {
  const grid = all(root).find((n) => n.attrs.class === "playpads");
  const cells = grid.children;
  assert.strictEqual(cells.length, 16, "16 cells");
  cells.forEach((cell, k) =>
    assert.strictEqual(
      cell.children[0].attrs["aria-label"].split(",")[0],
      "Pad " + (k + 1),
      "cell " + (k + 1) + " holds pad " + (k + 1),
    ),
  );
}

function checkSequencer(env, root) {
  const before = env.sent.length;
  const field = (pad, what) =>
    all(root).find((n) => n.attrs["aria-label"] === "Pad " + pad + ", " + what);
  field(1, "steps").value = "8";
  field(1, "steps").fire("change");
  field(1, "pulses").value = "3";
  field(1, "pulses").fire("change");
  field(2, "steps").value = "8";
  field(2, "pulses").value = "12";
  field(2, "rotation").value = "20";
  field(2, "probability %").value = "250";
  field(2, "pulses").fire("change");
  field(3, "steps").value = "";
  field(3, "steps").fire("change");
  const toggle = control(
    root,
    "Euclidean sequencer off: click to play the patterns on the clock",
  );
  assert.strictEqual(toggle.attrs["aria-pressed"], "false");
  toggle.fire("click");
  env.receive({ event: "seq_run", enabled: true, velocity: 100 });
  assert.strictEqual(toggle.attrs["aria-pressed"], "true", "shown as on");
  toggle.fire("click");
  assert.deepStrictEqual(sentSince(env, before), [
    "seq_pad\t0\t8\t0\t0\t100",
    "seq_pad\t0\t8\t3\t0\t100",
    "seq_pad\t1\t8\t8\t7\t100",
    "seq_run\t1\t100",
    "seq_run\t0\t100",
  ]);
  env.receive({
    event: "seq_pad",
    pad: 4,
    steps: 8,
    pulses: 3,
    rotation: 1,
    probability: 80,
    pattern: ".x..x..x",
  });
  assert.strictEqual(field(5, "steps").value, "8", "Rust's answer is shown");
  assert.strictEqual(field(5, "probability %").value, "80");
  const dots = all(root).filter((n) => n.attrs.class === "seqdots")[4];
  assert.deepStrictEqual(
    Array.from(dots.children, (d) => d.attrs.class === "on"),
    [false, true, false, false, true, false, false, true],
    "the pattern is drawn from the answer",
  );
}

function checkIcons(env) {
  for (const id of ["machine", "backup", "os", "play"]) {
    const page = env.kit.pages.find((p) => p.id === id);
    assert.ok(page.fit, id + " is a one-screen page");
    for (const b of all(env.roots[id]).filter(
      (n) =>
        n.tagName === "BUTTON" &&
        n.attrs.class &&
        n.attrs.class.includes("ibtn"),
    ))
      assert.ok(
        b.attrs["aria-label"] && b.attrs["data-tip"],
        id + ": icon button without label",
      );
  }
}

(async () => {
  const uiDir = process.argv[2];
  const env = load(uiDir, FILES);
  assert.deepStrictEqual(
    Array.from(env.kit.pages, (p) => p.id),
    ["machine", "backup", "os", "play"],
  );
  checkTopBar(env);
  checkMachine(env);
  checkPadNotes(env);
  checkBackup(env);
  await checkOs(env);
  checkPlay(env);
  checkIcons(env);
  const sentAll = new Set(env.sent);
  const old = Object.values(OLD_COMMANDS).flat();
  for (const line of old)
    assert.ok(sentAll.has(line), "old command no longer sent: " + line);
  const count = old.length;
  console.log(`ok, ${count} old commands and 2 confirmations kept`);
})().catch((e) => {
  console.error(e.stack || e);
  process.exit(1);
});
