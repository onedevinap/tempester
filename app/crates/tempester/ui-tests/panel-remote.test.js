/* Behaviour of ui/panel-remote.js on a minimal fake DOM (no dependency, run by tools/test_app.sh with node):
   `node panel-remote.test.js <path of panel-remote.js> <path of panel-keys.js>`. The script under test is a
   parameter so that the counter-proof can run the same checks on a deliberately broken copy: they must fail there. */
"use strict";
const fs = require("fs");
const vm = require("vm");
const assert = require("assert");

function fakeNode(dataset = {}, closestMap = {}) {
  const listeners = {};
  const attrs = {};
  const classes = new Set();
  return {
    dataset,
    tagName: "BUTTON",
    title: "",
    classList: {
      add: (c) => classes.add(c),
      remove: (c) => classes.delete(c),
      has: (c) => classes.has(c),
    },
    attrs,
    setAttribute: (k, v) => (attrs[k] = String(v)),
    removeAttribute: (k) => delete attrs[k],
    addEventListener: (type, fn) =>
      (listeners[type] = (listeners[type] || []).concat(fn)),
    fire: (type, event = {}) =>
      (listeners[type] || []).forEach((fn) =>
        fn({ preventDefault() {}, ...event }),
      ),
    closest: (selector) => closestMap[selector] || null,
  };
}

function load(remotePath, keysPath) {
  const windowListeners = {};
  const documentListeners = {};
  const document = {
    hidden: false,
    addEventListener: (type, fn) =>
      (documentListeners[type] = (documentListeners[type] || []).concat(fn)),
  };
  const window = {
    addEventListener: (type, fn) =>
      (windowListeners[type] = (windowListeners[type] || []).concat(fn)),
  };
  const context = vm.createContext({ window, document });
  vm.runInContext(fs.readFileSync(keysPath, "utf8"), context);
  vm.runInContext(fs.readFileSync(remotePath, "utf8"), context);
  const fire = (target, type) => (target[type] || []).forEach((fn) => fn({}));
  return {
    remote: window.Tempester.panelRemote,
    windowEvent: (type) => fire(windowListeners, type),
    documentEvent: (type) => fire(documentListeners, type),
    document,
  };
}

function setup(remotePath, keysPath) {
  const env = load(remotePath, keysPath);
  const sent = [];
  const play = fakeNode({ key: "Play" }, { "[data-transport]": {} });
  const copy = fakeNode({ key: "Copy" });
  const pad = fakeNode({ led: String(0x2c) });
  const root = {
    querySelectorAll: (selector) =>
      selector === "[data-key]"
        ? [copy, play]
        : selector === "[data-led]"
          ? [pad]
          : [],
  };
  env.remote.attach(root, (code, down) => sent.push([code, down]));
  return { ...env, sent, copy, play, pad };
}

const [remotePath, keysPath] = process.argv.slice(2);
const checks = {
  "keys are greyed until the remote panel is available": () => {
    const t = setup(remotePath, keysPath);
    t.copy.fire("pointerdown");
    assert.deepStrictEqual(t.sent, []);
    assert.strictEqual(t.copy.attrs["aria-disabled"], "true");
    t.remote.setAvailable(true);
    assert.strictEqual(t.copy.attrs["aria-disabled"], undefined);
  },
  "press sends the key code once and release sends it up": () => {
    const t = setup(remotePath, keysPath);
    t.remote.setAvailable(true);
    t.copy.fire("pointerdown");
    t.copy.fire("pointerdown");
    t.copy.fire("pointerup");
    assert.deepStrictEqual(t.sent, [
      [31, true],
      [31, false],
    ]);
  },
  "pointerleave releases a held key": () => {
    const t = setup(remotePath, keysPath);
    t.remote.setAvailable(true);
    t.copy.fire("pointerdown");
    t.copy.fire("pointerleave");
    assert.deepStrictEqual(t.sent, [
      [31, true],
      [31, false],
    ]);
  },
  "pointercancel releases a held key": () => {
    const t = setup(remotePath, keysPath);
    t.remote.setAvailable(true);
    t.copy.fire("pointerdown");
    t.copy.fire("pointercancel");
    assert.deepStrictEqual(t.sent, [
      [31, true],
      [31, false],
    ]);
  },
  "window blur releases a held key": () => {
    const t = setup(remotePath, keysPath);
    t.remote.setAvailable(true);
    t.copy.fire("pointerdown");
    t.windowEvent("blur");
    assert.deepStrictEqual(t.sent, [
      [31, true],
      [31, false],
    ]);
  },
  "a hidden page releases a held key": () => {
    const t = setup(remotePath, keysPath);
    t.remote.setAvailable(true);
    t.copy.fire("pointerdown");
    t.document.hidden = true;
    t.documentEvent("visibilitychange");
    assert.deepStrictEqual(t.sent, [
      [31, true],
      [31, false],
    ]);
  },
  "losing the remote panel releases a held key": () => {
    const t = setup(remotePath, keysPath);
    t.remote.setAvailable(true);
    t.copy.fire("pointerdown");
    t.remote.setAvailable(false);
    assert.deepStrictEqual(t.sent, [
      [31, true],
      [31, false],
    ]);
  },
  "keyboard press and release, ignoring auto-repeat": () => {
    const t = setup(remotePath, keysPath);
    t.remote.setAvailable(true);
    t.copy.fire("keydown", { key: "Enter", repeat: false });
    t.copy.fire("keydown", { key: "Enter", repeat: true });
    t.copy.fire("keyup", { key: "Enter" });
    assert.deepStrictEqual(t.sent, [
      [31, true],
      [31, false],
    ]);
  },
  "a key losing focus releases it": () => {
    const t = setup(remotePath, keysPath);
    t.remote.setAvailable(true);
    t.copy.fire("pointerdown");
    t.copy.fire("blur");
    assert.deepStrictEqual(t.sent, [
      [31, true],
      [31, false],
    ]);
  },
  "LED changes light the key and the pad, and clear when the panel is lost":
    () => {
      const t = setup(remotePath, keysPath);
      t.remote.setAvailable(true);
      t.remote.setLeds([
        [0x1f, 1],
        [0x2c, 1],
      ]);
      assert.strictEqual(t.copy.attrs["aria-pressed"], "true");
      assert.strictEqual(t.pad.attrs["aria-pressed"], "true");
      t.remote.setLeds([[0x1f, 0]]);
      assert.strictEqual(t.copy.attrs["aria-pressed"], "false");
      t.remote.setAvailable(false);
      assert.strictEqual(t.pad.attrs["aria-pressed"], undefined);
    },
  "Play and Stop are never pressed through the remote path": () => {
    const t = setup(remotePath, keysPath);
    t.remote.setAvailable(true);
    t.play.fire("pointerdown");
    t.play.fire("pointerup");
    assert.deepStrictEqual(t.sent, []);
    t.remote.setLeds([[0x24, 1]]);
    assert.strictEqual(t.play.attrs["aria-pressed"], "true");
  },
};

let failed = 0;
for (const [name, check] of Object.entries(checks)) {
  try {
    check();
  } catch (error) {
    failed += 1;
    console.log("FAIL " + name + ": " + error.message.split("\n")[0]);
  }
}
process.exit(failed ? 1 : 0);
