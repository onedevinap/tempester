/* A minimal fake DOM and page loader shared by the page tests (no dependency, plain node). It is just enough for the
   page scripts of ui/: elements with attributes, classes, listeners, children, text; a window whose `ipc` records
   every command line the page sends to Rust. */
"use strict";
const fs = require("fs");
const path = require("path");
const vm = require("vm");

function textOf(node) {
  return node.text !== undefined
    ? node.text
    : node.children.map(textOf).join("");
}

function isShown(node) {
  for (let n = node; n; n = n.parent) if (n.hidden) return false;
  return true;
}

function all(node) {
  return [node, ...node.children.flatMap(all)];
}

function hasClass(node, name) {
  return (node.attrs.class || "").split(/\s+/).includes(name);
}

function fakeNode(tag, byId, answers) {
  const listeners = {};
  const classes = new Set();
  const node = {
    tagName: tag.toUpperCase(),
    children: [],
    parent: null,
    attrs: {},
    hidden: false,
    style: {},
    disabled: false,
    value: "",
    files: null,
    returnValue: "",
    innerHTML: "",
    scrollTop: 0,
    scrollHeight: 0,
    clientHeight: 0,
    validity: { valid: true },
    classList: {
      add: (c) => classes.add(c),
      remove: (c) => classes.delete(c),
      toggle: (c, on) => (on ? classes.add(c) : classes.delete(c)),
      contains: (c) => classes.has(c),
    },
    setAttribute(k, v) {
      node.attrs[k] = String(v);
      if (k === "hidden") node.hidden = true;
      if (k === "id") byId[v] = node;
      if (k === "value") node.value = String(v);
    },
    getAttribute: (k) => (k in node.attrs ? node.attrs[k] : null),
    removeAttribute: (k) => delete node.attrs[k],
    addEventListener: (type, fn) =>
      (listeners[type] = (listeners[type] || []).concat(fn)),
    fire: (type, event) =>
      (listeners[type] || []).forEach((fn) => fn(event || {})),
    append(...items) {
      for (const item of items) {
        const child =
          typeof item === "string"
            ? { text: item, children: [], attrs: {} }
            : item;
        child.parent = node;
        node.children.push(child);
      }
    },
    insertBefore(child, reference) {
      child.parent = node;
      const at = node.children.indexOf(reference);
      node.children.splice(at < 0 ? node.children.length : at, 0, child);
      return child;
    },
    replaceChildren(...items) {
      node.children = [];
      node.append(...items);
    },
    contains: (other) => all(node).includes(other),
    /* Only the `[attribute]` selector, the one icons.js needs for its tooltips. */
    closest(selector) {
      const name = selector.slice(1, -1);
      for (let n = node; n; n = n.parent)
        if (n.attrs && name in n.attrs) return n;
      return null;
    },
    /* Geometry is set by the test (`node.rect`); a box the page creates is sized by the test. */
    getBoundingClientRect: () =>
      node.rect || { left: 0, top: 0, width: 0, height: 0, bottom: 0 },
    querySelector: (selector) => node.querySelectorAll(selector)[0] || null,
    /* `.class` or a bare tag name. */
    querySelectorAll: (selector) =>
      all(node).filter((n) =>
        selector.startsWith(".")
          ? n.attrs && hasClass(n, selector.slice(1))
          : n.tagName === selector.toUpperCase(),
      ),
    remove() {},
    /* The drawn machine is built from HTML strings: no markup is parsed here, so its controls do not exist. */
    insertAdjacentHTML() {},
    focus() {},
    scrollIntoView() {},
    setPointerCapture() {},
    showModal() {
      node.returnValue = answers.dialog;
      setTimeout(() => node.fire("close"), 0);
    },
    click() {
      if (isShown(node) && !node.disabled) node.fire("click");
    },
  };
  Object.defineProperty(node, "offsetParent", {
    get: () => (isShown(node) ? {} : null),
  });
  Object.defineProperty(node, "textContent", {
    get: () => textOf(node),
    set: (text) => {
      node.children = [];
      node.append(String(text));
    },
  });
  return node;
}

/* Loads `files` of the ui folder in one context, in order, and builds every registered page into its own root.
   `extra`: more globals for the context (location, history...). */
function load(uiDir, files, extra) {
  const byId = {};
  const sent = [];
  const answers = { dialog: "ok" };
  const documentListeners = {};
  const document = {
    addEventListener: (type, fn) =>
      (documentListeners[type] = (documentListeners[type] || []).concat(fn)),
    fire: (type, event) =>
      (documentListeners[type] || []).forEach((fn) => fn(event || {})),
    createElement: (tag) => fakeNode(tag, byId, answers),
    getElementById: (id) =>
      byId[id] || (byId[id] = fakeNode("div", byId, answers)),
    body: fakeNode("body", byId, answers),
  };
  const window = {
    innerWidth: 1320,
    innerHeight: 800,
    addEventListener() {},
    getSelection: () => null,
    ipc: { postMessage: (line) => sent.push(line) },
  };
  const navigator = {};
  const context = vm.createContext({
    window,
    document,
    setTimeout,
    clearTimeout,
    navigator,
    ...extra,
  });
  for (const file of files)
    vm.runInContext(fs.readFileSync(path.join(uiDir, file), "utf8"), context, {
      filename: file,
    });
  const kit = window.Tempester.kit;
  const roots = {};
  for (const page of kit.pages) {
    roots[page.id] = fakeNode("section", byId, answers);
    page.build(roots[page.id]);
  }
  return {
    window,
    document,
    context,
    navigator,
    kit,
    sent,
    answers,
    roots,
    byId,
    receive: (m) => window.Tempester.bridge.receive(m),
  };
}

const tick = () => new Promise((resolve) => setTimeout(resolve, 5));

/* A visible, enabled-or-not control named `label`: an icon button by its accessible name, a plain button by its text. */
function controls(root, label) {
  return all(root).filter(
    (n) =>
      n.tagName === "BUTTON" &&
      isShown(n) &&
      (n.attrs["aria-label"] === label ||
        (n.attrs["aria-label"] === undefined && textOf(n).startsWith(label))),
  );
}

function control(root, label) {
  const found = controls(root, label);
  if (!found.length) throw new Error(`no visible button "${label}"`);
  return found[0];
}

/* Presses a control; a disabled one is a failure (the page would not send anything). */
function press(root, label) {
  const found = control(root, label);
  if (found.disabled) throw new Error(`button "${label}" is disabled`);
  found.click();
}

/* Picks a file with the n-th file picker of `root`. */
async function pick(root, n, name, bytes) {
  const inputs = all(root).filter((node) => node.attrs.type === "file");
  const data = new Uint8Array(bytes || [1, 2]).buffer;
  inputs[n].files = [{ name, arrayBuffer: async () => data }];
  inputs[n].fire("change");
  await tick();
}

module.exports = {
  all,
  textOf,
  isShown,
  load,
  tick,
  controls,
  control,
  press,
  pick,
};
