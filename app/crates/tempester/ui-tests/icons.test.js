/* The shared icon module on a minimal fake DOM: `node icons.test.js <ui folder>`. Every icon draws a 24 px
   `currentColor` SVG, an icon button always carries its label (accessible name and tooltip), the info badge opens
   and closes its popover. Counter-proof: the same checks on a copy of icons.js without the label guard must fail. */
"use strict";
const fs = require("fs");
const path = require("path");
const vm = require("vm");
const assert = require("assert");

function fakeNode(tag) {
  const listeners = {};
  const node = {
    tagName: tag.toUpperCase(),
    children: [],
    attrs: {},
    hidden: false,
    style: {},
    innerHTML: "",
    getBoundingClientRect: () => ({ left: 100, top: 300, width: 38, height: 38 }),
    setAttribute: (k, v) => (node.attrs[k] = String(v)),
    getAttribute: (k) => (k in node.attrs ? node.attrs[k] : null),
    addEventListener: (type, fn) =>
      (listeners[type] = (listeners[type] || []).concat(fn)),
    fire: (type, event) =>
      (listeners[type] || []).forEach((fn) => fn(event || {})),
    append: (...items) => node.children.push(...items),
    replaceChildren: (...items) => (node.children = items),
    contains: (other) => other === node || node.children.includes(other),
  };
  return node;
}

function load(file) {
  const document = { createElement: fakeNode, getElementById: () => null };
  const window = { innerWidth: 1320, innerHeight: 800 };
  const context = vm.createContext({ window, document });
  vm.runInContext(fs.readFileSync(file, "utf8"), context, { filename: file });
  return context.window.Tempester.icons;
}

function check(icons) {
  assert.ok(icons.names.length >= 40, "enough icons: " + icons.names.length);
  for (const name of [
    "fetch",
    "send",
    "save",
    "open",
    "folder",
    "reveal",
    "rename",
    "tag",
    "star",
    "history",
    "trash",
    "restore",
    "duplicate",
    "search",
    "filter",
    "sort",
    "info",
    "refresh",
    "connect",
    "disconnect",
    "play",
    "stop",
    "check",
    "warning",
    "close",
    "more",
    "power",
    "pause",
    "clock",
    "copy",
    "identity",
    "latest",
    "arrow-in",
    "arrow-out",
    "swap",
    "build",
  ])
    assert.ok(icons.names.includes(name), "icon " + name);
  for (const name of icons.names) {
    const svg = icons.svgText(name);
    assert.ok(svg.includes('viewBox="0 0 24 24"'), name + " viewBox");
    assert.ok(
      svg.includes('stroke="currentColor"'),
      name + " uses currentColor",
    );
    assert.ok(svg.includes('aria-hidden="true"'), name + " is decorative");
  }
  assert.throws(() => icons.svgText("nope"), /no icon/);
  let clicks = 0;
  const button = icons.button("send", "Send to the machine", () => clicks++);
  assert.strictEqual(button.attrs["aria-label"], "Send to the machine");
  assert.strictEqual(button.attrs["data-tip"], "Send to the machine");
  button.fire("click");
  assert.strictEqual(clicks, 1);
  assert.throws(() => icons.button("send", "", null), /needs a label/);
  assert.throws(() => icons.button("send", undefined, null), /needs a label/);
  const toggle = icons.button("star", "Favourite", null, { pressed: true });
  assert.strictEqual(toggle.attrs["aria-pressed"], "true");
  icons.relabel(toggle, "Not a favourite", "close");
  assert.strictEqual(toggle.attrs["aria-label"], "Not a favourite");
  assert.strictEqual(toggle.attrs["data-tip"], "Not a favourite");
  assert.ok(toggle.children[0].innerHTML.includes("<svg"), "relabel redraws");
  assert.throws(() => icons.relabel(toggle, ""), /needs a label/);
  const wrap = icons.info("Explanation text");
  const [badge, box] = wrap.children;
  assert.strictEqual(box.hidden, true, "popover starts closed");
  badge.fire("click", {});
  assert.strictEqual(box.hidden, false, "click opens the popover");
  assert.strictEqual(badge.attrs["aria-expanded"], "true");
  badge.fire("click", {});
  assert.strictEqual(box.hidden, true, "second click closes it");
}

const ui = process.argv[2];
check(load(path.join(ui, "icons.js")));

const source = fs.readFileSync(path.join(ui, "icons.js"), "utf8");
const broken = source.replace(
  'if (!label) throw new Error("an icon button needs a label: " + name);',
  "",
);
assert.notStrictEqual(
  broken,
  source,
  "the counter-proof must change the script",
);
const copy = path.join(
  fs.mkdtempSync(path.join(require("os").tmpdir(), "icons-")),
  "icons.js",
);
fs.writeFileSync(copy, broken);
assert.throws(
  () => check(load(copy)),
  "an icon button without label is accepted",
);
console.log("ok: icons, labelled buttons, popover; unlabelled button refused");
