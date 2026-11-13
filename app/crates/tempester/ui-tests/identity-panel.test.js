/* The identity answer shown in full (ui/identity-panel.js) and its short summary chip in the top bar, on a fake DOM,
   run by tools/test_app.sh: `node identity-panel.test.js <ui folder>`. Every field is visible text after an
   identity event, nothing sits in a popover, the chip opens the Machine page. The exact version the machine reports
   (shared state of os-verify.js) fills the Version row and resolves every feature from Rust's table; it is forgotten
   with the shared state. Then the same checks on copies of the scripts with one behaviour broken, each of which must
   fail. */
"use strict";
const fs = require("fs");
const os = require("os");
const path = require("path");
const assert = require("assert");
const { load, all, textOf, isShown } = require("./fake-dom.js");

const uiDir = process.argv[2];
const FILES = [
  "bridge.js",
  "icons.js",
  "kit.js",
  "conn-bar.js",
  "os-verify.js",
  "identity-panel.js",
];
const TEMPESTER = {
  event: "identity",
  device: 0x7f,
  version: 0x7f2,
  tempester: true,
};
const DSI = {
  event: "identity",
  device: 0x04,
  version: 0x1501,
  tempester: false,
};

/* What Rust sends: every feature of its table (id, label, since) and whether the OS has it. */
const TABLE = [
  ["dump_requests", "Sound and beat requests", "1.5.2.0"],
  ["nrpn_reception", "NRPN reception", "1.5.3.0"],
  ["remote_panel", "Remote panel", "1.6.0.0"],
  ["project_request", "Project request", "1.6.1.0"],
  ["beat_wide", "Wide beat positions", "1.6.2.0"],
  ["ribbons", "FX ribbons", "1.6.3.0"],
  ["pedal_pad", "Pedal as pad", "1.6.3.0"],
  ["probability", "Step probability", "1.6.3.0"],
  ["mixer_reports", "Mixer reports", "1.6.4.0"],
  ["long_notes", "Long notes", "1.6.5.0"],
  ["quantize_fine", "Fine quantize", "1.6.5.0"],
  ["pad_channels", "MIDI channel per pad", "1.6.5.0"],
];
const features = (count) =>
  Object.fromEntries(
    TABLE.map(([id, label, since], k) => [
      id,
      { label, on: k < count, since },
    ]),
  );
const running = (version, known = true) => ({
  version,
  label: "Tempester " + version,
  known,
});
const state = (event, count, run) => ({
  event,
  features: features(count),
  running: run,
});

/* The visible value shown next to a label, e.g. value(panel, "NRPN reception"). */
function value(panel, label) {
  const cells = all(panel).filter(
    (n) => n.tagName === "DT" || n.tagName === "DD",
  );
  const at = cells.findIndex((n) => n.tagName === "DT" && textOf(n) === label);
  assert.ok(at >= 0, "no row " + label);
  assert.ok(isShown(cells[at + 1]), label + " is hidden");
  return textOf(cells[at + 1]);
}

function setup(dir) {
  const env = load(dir, FILES);
  const panel = env.window.Tempester.identityPanel.build();
  const switched = [];
  env.kit.setPageSwitcher((id) => switched.push(id));
  env.receive({ event: "ports", ports: ["Tempest"] });
  env.kit.updateSession({ connected: true, port: "Tempest", fake: false });
  return { env, panel, switched, chip: env.byId.connbar.children[3] };
}

const popovers = (root) =>
  all(root).filter((n) =>
    /\b(popover|info)\b/.test((n.attrs && n.attrs.class) || ""),
  );

/* The rows and the guesses of an old OS or a machine that does not report its version. */
function runWord(env, panel, switched, chip) {
  assert.strictEqual(value(panel, "Status"), "Not checked");
  assert.strictEqual(value(panel, "Version word"), "-");
  assert.strictEqual(value(panel, "Version"), "-");
  assert.strictEqual(
    TABLE.filter(([, label]) => value(panel, label) === "-").length,
    TABLE.length,
    "every feature row exists from the first state and is empty",
  );

  env.receive(TEMPESTER);
  assert.strictEqual(value(panel, "Status"), "Answered");
  assert.strictEqual(value(panel, "OS"), "Tempester OS");
  assert.strictEqual(value(panel, "Device id"), "0x7F");
  assert.strictEqual(
    value(panel, "Manufacturer"),
    "Dave Smith Instruments (0x01)",
  );
  assert.strictEqual(value(panel, "Family"), "Tempest (0x28 0x01)");
  assert.strictEqual(value(panel, "Version word"), "0x7F2");
  assert.strictEqual(value(panel, "Version"), "-");
  assert.strictEqual(value(panel, "Came from"), "Tempest");
  assert.match(value(panel, "Checked at"), /^\d\d:\d\d:\d\d$/);
  assert.strictEqual(value(panel, "Sound and beat requests"), "Available");
  assert.match(value(panel, "NRPN reception"), /^From 1\.5\.3\.0/);
  assert.match(value(panel, "Remote panel"), /^From 1\.6\.0\.0/);
  assert.match(value(panel, "Project request"), /^From 1\.6\.1\.0/);
  assert.match(value(panel, "Fine quantize"), /^From 1\.6\.5\.0/);
  assert.match(
    textOf(panel),
    /System > Main OS Version/,
    "how to read the exact version",
  );
  assert.strictEqual(popovers(panel).length, 0, "nothing in a popover");
  assert.strictEqual(
    textOf(chip),
    "Tempester OS",
    "the chip is a short summary",
  );
  assert.strictEqual(
    popovers(env.byId.connbar).filter((n) => /popover/.test(n.attrs.class))
      .length,
    0,
  );
  chip.fire("click", {});
  assert.deepStrictEqual(
    switched,
    ["machine"],
    "the chip opens the Machine page",
  );
}

/* The exact version the machine reports resolves every feature and is forgotten with the shared state. */
function runReported(env, panel) {
  env.receive(state("installed", 12, running("1.6.5.0")));
  assert.strictEqual(
    value(panel, "Version"),
    "Tempester 1.6.5.0 (reported by the machine)",
  );
  for (const [, label] of TABLE)
    assert.strictEqual(value(panel, label), "Available", label);
  assert.doesNotMatch(
    textOf(panel),
    /System > Main OS Version/,
    "no look-on-the-machine hint when the version is known",
  );

  env.receive(state("installed", 5, running("1.6.2.0")));
  assert.strictEqual(value(panel, "Wide beat positions"), "Available");
  assert.strictEqual(value(panel, "Long notes"), "Not in this version");
  assert.strictEqual(value(panel, "Mixer reports"), "Not in this version");
  assert.doesNotMatch(textOf(panel), /From \d/, "no guess when resolved");

  env.receive(state("state", 5, running("1.6.2.0")));
  assert.strictEqual(
    value(panel, "Version"),
    "Tempester 1.6.2.0 (reported by the machine)",
  );

  env.receive(state("installed", 0, null));
  assert.strictEqual(value(panel, "Version"), "-", "forgotten by the state");
  assert.match(value(panel, "NRPN reception"), /^From 1\.5\.3\.0/);
  assert.match(textOf(panel), /System > Main OS Version/);

  env.receive(state("installed", 0, running("9.9.9.9", false)));
  assert.match(value(panel, "Version"), /not in this app's table/);
  assert.match(value(panel, "Long notes"), /^From 1\.6\.5\.0/);
  assert.match(textOf(panel), /does not know/);

  env.receive(state("installed", 12, running("1.6.5.0")));
  env.kit.updateSession({ connected: false });
  assert.strictEqual(value(panel, "Version"), "-", "disconnecting forgets it");
  env.kit.updateSession({ connected: true });
  assert.strictEqual(value(panel, "Version"), "-");
}

function runDsi(env, panel, chip) {
  env.kit.updateSession({ fake: true, port: "fake" });
  env.receive(state("installed", 0, null));
  env.receive(DSI);
  assert.strictEqual(value(panel, "OS"), "DSI OS");
  assert.strictEqual(value(panel, "Device id"), "0x04");
  assert.strictEqual(value(panel, "Version word"), "0x1501");
  assert.strictEqual(value(panel, "Came from"), "fake (fake machine)");
  for (const [, label] of TABLE)
    assert.strictEqual(value(panel, label), "Not in the DSI OS", label);
  assert.match(textOf(panel), /need a Tempester OS/);

  env.receive({ event: "error", during: "identity", code: "timeout" });
  assert.strictEqual(value(panel, "Status"), "No answer");
  assert.strictEqual(value(panel, "Version word"), "-");
  assert.match(
    textOf(panel),
    /MIDI: Sysex IN\+OUT Cable/,
    "the hint is shown, not hidden",
  );
  assert.strictEqual(textOf(chip), "No answer");

  env.receive(TEMPESTER);
  env.kit.updateSession({ connected: false });
  assert.strictEqual(
    value(panel, "Status"),
    "Not checked",
    "disconnecting clears it",
  );
  assert.strictEqual(value(panel, "Checked at"), "-");
}

function run(dir) {
  const { env, panel, switched, chip } = setup(dir);
  env.receive(state("state", 0, null));
  runWord(env, panel, switched, chip);
  runReported(env, panel);
  runDsi(env, panel, chip);
}

run(uiDir);
const read = (file) => fs.readFileSync(path.join(uiDir, file), "utf8");
const PANEL = "identity-panel.js";
const SHARED = "os-verify.js";
const broken = [
  [PANEL, "const PROVEN_FROM = { 0x7f2:", "const PROVEN_FROM = { 0x7f9:"],
  [
    PANEL,
    'if (m && !m.tempester) return [S.featuresNo, "off"];',
    "",
  ],
  [PANEL, "clock(checked)", "S.none"],
  [PANEL, "unchecked ? S.none : port,", "S.none,"],
  [
    PANEL,
    'answer({ m: null, status: "unchecked", checked: null, port: "" });',
    "void 0;",
  ],
  [PANEL, 'm ? "0x" + identity.hex(m.version) : S.none', "S.none"],
  [PANEL, "m ? S.family : S.none", "S.none"],
  [
    PANEL,
    'return status === "none" ? identity.hint : S.notChecked;',
    "return S.notChecked;",
  ],
  [
    PANEL,
    'answer({ m, status: "answered", checked: new Date(), port: portName() }),',
    "void m,",
  ],
  // The reported version: shown, resolves each feature, replaces the hint, is forgotten.
  [PANEL, "running ? S.reported(running) : S.none,", "S.none,"],
  [PANEL, "if (running && running.known)", "if (false)"],
  [
    PANEL,
    'feature.on ? [S.featuresYes, "good"] : [S.featuresNotHere, "off"]',
    '[S.featuresYes, "good"]',
  ],
  [
    PANEL,
    "if (running) return running.known ? S.reportedNote : S.unknownNote;",
    "",
  ],
  [
    PANEL,
    "running.known ? S.reportedNote : S.unknownNote",
    "S.reportedNote",
  ],
  [
    PANEL,
    "featureCells[id] = row(feature.label);",
    "featureCells[id] = row(id);",
  ],
  [
    SHARED,
    "features: m.features || view.features,",
    "features: view.features,",
  ],
  [
    SHARED,
    "if (!session.connected) setView({ running: null, verified: null });",
    "",
  ],
  [
    SHARED,
    "const running = m.running || null;",
    "const running = m.running || view.running;",
  ],
];
let refused = 0;
for (const [file, from, to] of broken) {
  const source = read(file);
  assert.ok(source.includes(from), "counter-proof anchor missing: " + from);
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "identity-panel-"));
  for (const f of FILES)
    fs.copyFileSync(path.join(uiDir, f), path.join(dir, f));
  fs.writeFileSync(path.join(dir, file), source.replace(from, to));
  assert.throws(() => run(dir), undefined, "broken copy accepted: " + from);
  refused++;
}
console.log(
  `ok: identity shown in full, no popover, chip opens Machine, reported version resolves features; ${refused} broken copies refused`,
);
