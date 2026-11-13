/* What the Panel page and the pad-channel panel (Machine page) do with the feature table Rust sends (`features` and
   `os_known` of the `state` and `installed` events), on a fake DOM, run by tools/test_app.sh:
   `node feature-gates.test.js <ui folder> <scratch folder>`.
   - before the first state, no control names a version it has been told nothing about;
   - an OS the app cannot name is called unknown; a named OS that lacks a feature is not;
   - a machine that reports 1.6.5.0 (shared state) opens every control, the same page on 1.6.2.0 greys the later ones
     and names the version that brings each.
   The Panel page is built for real around stubs of the drawn machine (panel.js, panel-machine.js, edit-tools.js,
   panel-perform's drawings), which need a real DOM; the rule under test lives in page-panel.js and panel-perform.js.
   Then the same checks on copies with one behaviour broken, each of which must fail. */
"use strict";
const fs = require("fs");
const os = require("os");
const path = require("path");
const assert = require("assert");
const { load, textOf } = require("./fake-dom.js");

const [uiDir, scratchArg] = process.argv.slice(2);
const scratch = scratchArg || os.tmpdir();

const TABLE = [
  ["beat_wide", "1.6.2.0"],
  ["ribbons", "1.6.3.0"],
  ["pedal_pad", "1.6.3.0"],
  ["probability", "1.6.3.0"],
  ["mixer_reports", "1.6.4.0"],
  ["long_notes", "1.6.5.0"],
  ["quantize_fine", "1.6.5.0"],
  ["pad_channels", "1.6.5.0"],
];
const features = (count) =>
  Object.fromEntries(
    TABLE.map(([id, since], k) => [id, { on: k < count, since, label: id }]),
  );
const reported = (version) => ({
  version,
  label: "Tempester " + version,
  known: true,
});
const event = (name, count, running, known) => ({
  event: name,
  features: features(count),
  running,
  os_known: known,
});

/* ---------- The Panel page, around stubs of the drawn machine ---------- */
const PANEL_FILES = [
  "bridge.js",
  "icons.js",
  "kit.js",
  "pad-notes.js",
  "panel-labels.js",
  "panel-keys.js",
  "panel-remote.js",
  "panel-view.js",
  "panel-mode.js",
  "panel-link.js",
  "panel-perform.js",
  "panel-stubs.js",
  "page-panel.js",
];
/* Stand-ins for the files that draw the machine. `auto` answers any call or member, so the page can use them freely. */
const STUBS = `
"use strict";
const auto = (own = {}) =>
  new Proxy(function () {}, {
    get: (t, key) => (key in own ? own[key] : auto()),
    apply: () => auto(),
  });
const T = window.Tempester;
T.panel = auto({ parts: {} });
T.panelMachine = auto();
T.editTools = {
  create: () => ({
    slot: document.createElement("span"),
    popover: document.createElement("div"),
    setView() {},
    setConnected() {},
  }),
};
T.editLearn = auto();
window.TEMPEST_PARAMS = [];
const calls = (window.gates = { ribbons: [], pads: [] });
T.panelPerform.attach = () =>
  auto({
    setRibbonsUsable: (on, why) => calls.ribbons.push({ on, why }),
    setPadsEditable: (on, why) => calls.pads.push({ on, why }),
    probMode: () => false,
  });
`;

function panelFolder(from, replace) {
  const dir = fs.mkdtempSync(path.join(scratch, "feature-gates-"));
  for (const file of PANEL_FILES)
    if (file !== "panel-stubs.js")
      fs.copyFileSync(path.join(from, file), path.join(dir, file));
  fs.writeFileSync(path.join(dir, "panel-stubs.js"), STUBS);
  fs.writeFileSync(path.join(dir, "params.js"), "");
  if (replace) {
    const [file, anchor, to] = replace;
    const source = fs.readFileSync(path.join(dir, file), "utf8");
    assert.ok(source.includes(anchor), "anchor missing: " + anchor);
    fs.writeFileSync(path.join(dir, file), source.replace(anchor, to));
  }
  return dir;
}

function runPanel(dir) {
  const env = load(dir, PANEL_FILES);
  const last = () => env.window.gates.ribbons[env.window.gates.ribbons.length - 1];
  const root = env.roots.panel;
  assert.ok(textOf(root).includes("Offline"), "offline at first");
  env.kit.updateSession({ connected: true, port: "Tempest", fake: false });
  assert.ok(
    !textOf(root).includes("Offline") && textOf(root).includes("Not synced"),
    "connected: the sync status no longer says Offline",
  );
  env.kit.updateSession({ connected: false });
  assert.ok(textOf(root).includes("Offline"), "and back when it drops");
  env.kit.updateSession({ connected: true, port: "Tempest", fake: false });
  assert.strictEqual(last().on, false);
  assert.ok(
    !/\d\.\d\.\d\.\d/.test(last().why),
    "no version is guessed before the first state: " + last().why,
  );

  env.receive(event("state", 0, null, false));
  assert.strictEqual(last().on, false);
  assert.ok(last().why.includes("1.6.3.0 or later"), last().why);
  assert.ok(last().why.includes("unknown to the app"), last().why);

  env.receive(event("installed", 8, reported("1.6.5.0"), true));
  assert.deepStrictEqual({ ...last() }, { on: true, why: "" });

  env.receive(event("installed", 1, reported("1.6.2.0"), true));
  assert.strictEqual(last().on, false);
  assert.ok(last().why.includes("1.6.3.0 or later"), last().why);
  assert.ok(!last().why.includes("unknown"), last().why);

  env.receive(event("installed", 8, reported("1.6.5.0"), true));
  assert.strictEqual(last().on, true, "a new report opens it again");
  env.receive(event("installed", 0, null, false));
  assert.strictEqual(last().on, false, "forgetting the version closes it");
  assert.ok(last().why.includes("unknown to the app"), last().why);
}

/* ---------- The pad-channel panel of the Machine page ---------- */
const MACHINE_FILES = [
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
];

function runPadChannels(dir) {
  const env = load(dir, MACHINE_FILES);
  const root = env.roots.machine;
  const gate = () => {
    const text = textOf(root);
    const at = text.indexOf("MIDI channel per pad needs");
    return at < 0 ? "" : text.slice(at, at + 200);
  };
  env.kit.updateSession({ connected: true, port: "Tempest", fake: false });
  assert.ok(gate().includes("version not known yet"), gate());
  assert.ok(!gate().includes("1.6.3.0"), "no default version: " + gate());

  env.receive(event("state", 0, null, false));
  assert.ok(gate().includes("1.6.5.0 or later"), gate());
  assert.ok(gate().includes("unknown to the app"), gate());

  env.receive(event("installed", 7, reported("1.6.4.0"), true));
  assert.ok(gate().includes("1.6.5.0 or later"), gate());
  assert.ok(!gate().includes("unknown"), gate());

  env.receive(event("installed", 8, reported("1.6.5.0"), true));
  assert.strictEqual(gate(), "", "available: nothing to explain");
}

function run(dir) {
  runPanel(panelFolder(dir));
  runPadChannels(dir);
}

run(uiDir);

const read = (file) => fs.readFileSync(path.join(uiDir, file), "utf8");
const broken = [
  [
    "page-panel.js",
    "        say(status, S.notSynced);\n        probeRemote();",
    "        probeRemote();",
  ],
  [
    "page-panel.js",
    "      if (m.features) {\n        features.set(m.features, m.os_known);",
    "      if (m.features) {\n        features.set(m.features);",
  ],
  [
    "page-panel.js",
    "      if (!m.features) return;\n      features.set(m.features, m.os_known);",
    "      if (!m.features) return;\n      features.set(m.features);",
  ],
  [
    "pad-channels-panel.js",
    "features.set(m.features, m.os_known);",
    "features.set(m.features);",
  ],
  [
    "panel-perform.js",
    'since\n        ? LABELS[name] + " Tempester " + since + " or later"',
    'true\n        ? LABELS[name] + " Tempester " + (since || "1.6.3.0") + " or later"',
  ],
  [
    "panel-perform.js",
    '(osKnown === false ? S.unknown : "")',
    '""',
  ],
];
let refused = 0;
for (const [file, from, to] of broken) {
  assert.ok(read(file).includes(from), "counter-proof anchor missing: " + from);
  const copy = fs.mkdtempSync(path.join(scratch, "feature-gates-ui-"));
  for (const f of fs.readdirSync(uiDir))
    if (f.endsWith(".js")) fs.copyFileSync(path.join(uiDir, f), path.join(copy, f));
  fs.writeFileSync(path.join(copy, file), read(file).replace(from, to));
  assert.throws(() => run(copy), undefined, "broken copy accepted: " + from);
  refused++;
}
console.log(
  `ok: feature gates follow the reported version and the unknown OS; ${refused} broken copies refused`,
);
