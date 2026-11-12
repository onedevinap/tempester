/* Behaviour of the n°58 pad channel model (pad-channels.js) and the feature gate of its panel on a fake window, no DOM
   (run by tools/test_app.sh with node): `node pad-channels.test.js <ui folder>`. The folder is a parameter so that
   pad-channels-counter-proof.js can run the same checks on deliberately broken copies: they must fail there. */
"use strict";
const fs = require("fs");
const vm = require("vm");
const path = require("path");
const assert = require("assert");

const [uiDir] = process.argv.slice(2);
const sent = [];
const handlers = {};
const window = {
  Tempester: {
    bridge: {
      send: (...message) => sent.push(message),
      on: (name, fn) => (handlers[name] = fn),
    },
  },
};
const context = vm.createContext({ window });
for (const file of ["pad-channels.js", "panel-perform.js"])
  vm.runInContext(fs.readFileSync(path.join(uiDir, file), "utf8"), context, {
    filename: file,
  });
const C = window.Tempester.padChannels;
const plain = (v) => JSON.parse(JSON.stringify(v));
let count = 0;
function check(name, fn) {
  fn();
  count++;
  void name;
}

check("labels follow the machine's page", () => {
  assert.deepStrictEqual(
    [0, 1, 16, 17, 32, 33].map(C.label),
    ["Off", "1", "16", "1 Fix", "16 Fix", "?"],
  );
  assert.strictEqual(C.options().length, 33);
  assert.strictEqual(C.options()[20].label, "4 Fix");
});
check("set sends one command per valid pad and value, nothing otherwise", () => {
  sent.length = 0;
  assert.strictEqual(C.set(31, 32), true);
  assert.deepStrictEqual(plain(sent), [["pad_channel", 31, 32]]);
  sent.length = 0;
  const bad = [
    [32, 0],
    [-1, 0],
    [0, 33],
    [0, -1],
    [0, 1.5],
    [0, NaN],
  ];
  for (const [pad, v] of bad)
    assert.strictEqual(C.set(pad, v), false, pad + "/" + v);
  assert.deepStrictEqual(sent, []);
});
check("read and save send their commands", () => {
  sent.length = 0;
  C.read();
  C.forget(); // the read is over (port closed): a save may go out
  C.save();
  assert.deepStrictEqual(plain(sent), [
    ["pad_channel_read"],
    ["pad_channel_save"],
  ]);
});
check("reports fill the values of their pad only and ignore bad ones", () => {
  const seen = [];
  C.onChange((values) => seen.push(values));
  handlers.pad_channel({ pad: 3, value: 20 });
  handlers.pad_channel({ pad: 32, value: 1 });
  handlers.pad_channel({ pad: 4, value: 33 });
  assert.strictEqual(C.value(3), 20);
  assert.strictEqual(C.value(4), null);
  assert.strictEqual(seen.length, 1);
  C.forget();
  assert.strictEqual(C.value(3), null);
});
check("while a read runs nothing else is sent, and it ends with the 32nd answer", () => {
  C.forget();
  sent.length = 0;
  const flips = [];
  C.onReading((on) => flips.push(on));
  assert.strictEqual(C.read(), true);
  assert.strictEqual(C.reading(), true);
  assert.strictEqual(C.read(), false);
  assert.strictEqual(C.save(), false);
  assert.strictEqual(C.set(3, 1), false);
  assert.deepStrictEqual(plain(sent), [["pad_channel_read"]]);
  for (let pad = 0; pad < 31; pad++) handlers.pad_channel({ pad, value: 1 });
  assert.strictEqual(C.reading(), true);
  handlers.pad_channel({ pad: 31, value: 1 });
  assert.strictEqual(C.reading(), false);
  assert.deepStrictEqual(flips, [true, false]);
  assert.strictEqual(C.save(), true);
  assert.strictEqual(C.set(3, 1), true);
});
check("a read ends on a timeout, an error or a closed port", () => {
  for (const end of [
    () => handlers.pad_channel_timeout({ pad: 2 }),
    () => handlers.error({ during: "pad_channel" }),
    () => C.forget(),
  ]) {
    C.read();
    assert.strictEqual(C.reading(), true);
    end();
    assert.strictEqual(C.reading(), false);
  }
  C.read();
  handlers.error({ during: "other" });
  assert.strictEqual(C.reading(), true);
  C.forget();
});
check("a timeout of the machine is told with the pad that did not answer", () => {
  const seen = [];
  C.onTimeout((pad) => seen.push(pad));
  handlers.pad_channel_timeout({ pad: 0 });
  handlers.pad_channel_timeout({ pad: 40 });
  assert.deepStrictEqual(seen, [0]);
});
check("the gate names 1.6.5.0 on an older OS and opens on 1.6.5.0", () => {
  const f = window.Tempester.panelPerform.createFeatures();
  f.set({ pad_channels: { on: false, since: "1.6.5.0" } });
  const s = f.status("pad_channels");
  assert.strictEqual(s.on, false);
  assert.ok(
    s.why.includes("MIDI channel per pad") && s.why.includes("1.6.5.0"),
    s.why,
  );
  f.set({ pad_channels: { on: true, since: "1.6.5.0" } });
  assert.strictEqual(f.status("pad_channels").on, true);
});
console.log(count + " checks");
