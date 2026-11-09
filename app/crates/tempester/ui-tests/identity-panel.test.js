/* The identity answer shown in full (ui/identity-panel.js) and its short summary chip in the top bar, on a fake DOM,
   run by tools/test_app.sh: `node identity-panel.test.js <ui folder>`. Every field is visible text after an
   identity event, nothing sits in a popover, the chip opens the Machine page. Then the same checks on copies of the
   script with one behaviour broken, each of which must fail. */
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

function run(dir) {
  const { env, panel, switched, chip } = setup(dir);
  assert.strictEqual(value(panel, "Status"), "Not checked");
  assert.strictEqual(value(panel, "Version word"), "-");

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
  assert.strictEqual(value(panel, "Came from"), "Tempest");
  assert.match(value(panel, "Checked at"), /^\d\d:\d\d:\d\d$/);
  assert.strictEqual(value(panel, "Sound and beat requests"), "Available");
  assert.match(value(panel, "NRPN reception"), /^From 1\.5\.3\.0/);
  assert.match(value(panel, "Remote panel"), /^From 1\.6\.0\.0/);
  assert.match(value(panel, "Project request"), /^From 1\.6\.1\.0/);
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

  env.kit.updateSession({ fake: true, port: "fake" });
  env.receive(DSI);
  assert.strictEqual(value(panel, "OS"), "DSI OS");
  assert.strictEqual(value(panel, "Device id"), "0x04");
  assert.strictEqual(value(panel, "Version word"), "0x1501");
  assert.strictEqual(value(panel, "Came from"), "fake (fake machine)");
  for (const name of [
    "Sound and beat requests",
    "NRPN reception",
    "Remote panel",
    "Project request",
  ])
    assert.strictEqual(value(panel, name), "Not in the DSI OS", name);
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

run(uiDir);
const source = fs.readFileSync(path.join(uiDir, "identity-panel.js"), "utf8");
const broken = [
  ["const PROVEN_FROM = { 0x7f2:", "const PROVEN_FROM = { 0x7f9:"],
  ['if (!m.tempester) return [S.featuresNo, "off"];', ""],
  ["clock(checked)", "S.none"],
  ["unchecked ? S.none : port,", "S.none,"],
  ['if (!state.connected) render(null, "unchecked");', ""],
  ['m ? "0x" + identity.hex(m.version) : S.none', "S.none"],
  ["m ? S.family : S.none", "S.none"],
  [
    ': status === "none"\n        ? identity.hint',
    ': status === "none"\n        ? S.notChecked',
  ],
  ['render(m, "answered", new Date(), portName())', "void m"],
];
let refused = 0;
for (const [from, to] of broken) {
  assert.ok(source.includes(from), "counter-proof anchor missing: " + from);
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "identity-panel-"));
  for (const f of FILES)
    fs.copyFileSync(path.join(uiDir, f), path.join(dir, f));
  fs.writeFileSync(
    path.join(dir, "identity-panel.js"),
    source.replace(from, to),
  );
  assert.throws(() => run(dir), undefined, "broken copy accepted: " + from);
  refused++;
}
console.log(
  `ok: identity shown in full, no popover, chip opens Machine; ${refused} broken copies refused`,
);
