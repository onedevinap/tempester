/* Behaviour of the top-bar connection controls (ui/conn-bar.js) on a minimal fake DOM, run by tools/test_app.sh:
   `node conn-bar.test.js <ui folder>`. Then the same checks on broken copies of the script, which must fail. */
"use strict";
const fs = require("fs");
const os = require("os");
const path = require("path");
const assert = require("assert");
const { load, press } = require("./fake-dom.js");

const uiDir = process.argv[2];
const FILES = ["bridge.js", "icons.js", "kit.js", "conn-bar.js"];

function run(dir) {
  const env = load(dir, FILES);
  const bar = env.byId.connbar;
  const [select, , toggle] = bar.children;
  const sent = env.sent;
  const setSession = (patch) => env.kit.updateSession(patch);

  assert.deepStrictEqual(
    Array.from(sent),
    ["ports"],
    "asks for the ports at start",
  );
  assert.ok(toggle.disabled, "no port: Connect disabled");
  env.receive({ event: "ports", ports: ["IAC", "Tempest"] });
  assert.strictEqual(select.value, "Tempest", "the Tempest port is preferred");
  assert.ok(!toggle.disabled);
  press(bar, "Connect");
  assert.strictEqual(sent.pop(), "connect\tTempest");
  setSession({ connected: true, port: "Tempest" });
  assert.ok(select.disabled, "port locked while connected");
  assert.strictEqual(toggle.attrs["aria-label"], "Disconnect");
  press(bar, "Disconnect");
  assert.strictEqual(sent.pop(), "disconnect");
  setSession({ connected: false });
  assert.ok(!select.disabled);
  assert.strictEqual(toggle.attrs["aria-label"], "Connect");
  sent.length = 0;
  press(bar, "Refresh the MIDI ports");
  assert.deepStrictEqual(
    Array.from(sent),
    ["ports"],
    "refresh asks for the ports",
  );
}

run(uiDir);
const source = fs.readFileSync(path.join(uiDir, "conn-bar.js"), "utf8");

const broken = [
  ['bridge.send("disconnect")', "void 0"],
  ['bridge.send("ports"));', "void 0);"],
  ["select.disabled = state.connected;", ""],
  ['"Tempest"].find', '"none"].find'],
];
let refused = 0;
for (const [from, to] of broken) {
  assert.ok(source.includes(from), "counter-proof anchor missing: " + from);
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "conn-bar-"));
  for (const f of FILES)
    fs.copyFileSync(path.join(uiDir, f), path.join(dir, f));
  fs.writeFileSync(path.join(dir, "conn-bar.js"), source.replace(from, to));
  try {
    run(dir);
  } catch (e) {
    refused++;
    continue;
  }
  console.error("broken copy accepted: " + from);
  process.exit(1);
}
console.log(`ok, ${refused} broken copies refused`);
