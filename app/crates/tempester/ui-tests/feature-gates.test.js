/* What the Panel page and the pad-channel panel (Machine page) do with the feature table Rust sends (`features` and
   `os_known` of the `state` and `installed` events), on a fake DOM, run by tools/test_app.sh:
   `node feature-gates.test.js <ui folder> <scratch folder>`.
   - before the first state, no control names a version it has been told nothing about;
   - an OS the app cannot name is called unknown; a named OS that lacks a feature is not;
   - a machine that reports 1.6.5.0 (shared state) opens every control, the same page on 1.6.2.0 greys the later ones
     and names the version that brings each;
   - the Release maxima (n°63, n°64) are the ones Rust computes and sends (`release_max` of the `state` and `installed`
     events: 123 under a cap, 127 when the six Voice chips run Tempester Voice 1.5.1.0, the table's maximum on an OS
     without a cap), on both events, for every parameter of `TEMPEST_RELEASE_CAP.params` and no other.
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

/* What Rust sends as `release_max`: [[index, max]] for the five Releases; this page's stub holds 65 and 75. */
const STOCK = [[65, 127], [75, 123]];
const CAPPED = [[65, 123], [75, 123]];
const LIFTED = [[65, 127], [75, 127]];

const TABLE = [
  ["beat_wide", "1.6.2.0"],
  ["ribbons", "1.6.3.0"],
  ["pedal_pad", "1.6.3.0"],
  ["probability", "1.6.3.0"],
  ["mixer_reports", "1.6.4.0"],
  ["long_notes", "1.6.5.0"],
  ["quantize_fine", "1.6.5.0"],
  ["pad_channels", "1.6.5.0"],
  ["full_backup", "1.6.6.0"],
  ["release_cap", "1.6.6.0"],
  ["release_cap_voice", "1.6.7.0"],
  ["voice_versions", "1.6.7.0"],
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
const event = (name, count, running, known, releaseMax) => ({
  event: name,
  features: features(count),
  running,
  os_known: known,
  ...(releaseMax ? { release_max: releaseMax } : {}),
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
const maxima = (window.maxima = []);
T.panel = auto({ parts: {}, setMax: (index, max) => maxima.push([index, max]) });
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
window.TEMPEST_PARAMS = [[65, 127, 0], [75, 123, 80]];
window.TEMPEST_RELEASE_CAP = { params: [65, 75] };
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

  /* Arrays of the page's realm: compared as plain data. */
  const maxima = () => JSON.parse(JSON.stringify(env.window.maxima));
  assert.deepStrictEqual(maxima(), [], "no release_max yet: the table's maxima");
  env.receive(event("installed", 10, reported("1.6.5.0"), true));
  assert.deepStrictEqual(maxima(), [], "an event without release_max changes nothing");
  env.receive(event("installed", 10, reported("1.6.6.0"), true, CAPPED));
  assert.deepStrictEqual(maxima(), [[65, 123]], "1.6.6.0 caps the Release (Amp Release already is)");
  env.receive(event("installed", 12, reported("1.6.7.0"), true, LIFTED));
  assert.deepStrictEqual(
    maxima().slice(1),
    [[65, 127], [75, 127]],
    "1.6.7.0 with six Tempester Voice chips: 127, Amp Release included",
  );
  env.receive(event("installed", 12, reported("1.6.7.0"), true, CAPPED));
  assert.deepStrictEqual(
    maxima().slice(3),
    [[65, 123], [75, 123]],
    "one chip on another Voice OS: back to 123",
  );
  env.receive(event("state", 12, reported("1.6.7.0"), true, LIFTED));
  assert.deepStrictEqual(maxima().slice(5), [[65, 127], [75, 127]], "the state event carries it too");
  env.receive(event("installed", 8, reported("1.6.5.0"), true, STOCK));
  assert.deepStrictEqual(maxima().slice(7), [[75, 123]], "1.6.5.0: DSI maxima (65 was already at 127)");
  const count = maxima().length;
  env.receive(event("installed", 8, reported("1.6.5.0"), true, [[66, 5]]));
  assert.strictEqual(maxima().length, count, "a parameter that is not a Release is left alone");
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
  [
    "page-panel.js",
    "        maxOf[index] = max;\n        panel.setMax(index, max);",
    "        maxOf[index] = baseMax[index];\n        panel.setMax(index, baseMax[index]);",
  ],
  ["page-panel.js", "        panel.setMax(index, max);\n", ""],
  [
    "page-panel.js",
    "if (!releases.has(index) || maxOf[index] === max) continue;",
    "if (maxOf[index] === max) continue;",
  ],
  [
    "page-panel.js",
    "if (!releases.has(index) || maxOf[index] === max) continue;",
    "if (!releases.has(index)) continue;",
  ],
  [
    "page-panel.js",
    '    bridge.on("installed", (m) => {\n      applyReleaseMax(m.release_max);',
    '    bridge.on("installed", (m) => {',
  ],
  [
    "page-panel.js",
    "      applyReleaseMax(m.release_max);\n      if (m.features) {",
    "      if (m.features) {",
  ],
  [
    "page-panel.js",
    "      for (const [index, max] of list) {",
    "      for (const [index, max] of list.slice(0, 1)) {",
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
