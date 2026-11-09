/* The tab bar built by ui/app.js on a fake DOM, run by tools/test_app.sh: `node nav-order.test.js <ui folder>`.
   Tabs in order of use (Panel, Library, Play, Machine, then Backup and OS update apart at the right end), the app
   opens on Panel unless the address names a page, pages are still built in registration order. Then the same checks
   on copies of app.js with one rule broken, each of which must fail. */
"use strict";
const fs = require("fs");
const os = require("os");
const path = require("path");
const vm = require("vm");
const assert = require("assert");
const { load, textOf } = require("./fake-dom.js");

const uiDir = process.argv[2];
const SUPPORT = ["bridge.js", "icons.js", "kit.js"];
/* Registration order = script order of index.html. */
const REGISTERED = [
  ["machine", "Machine"],
  ["panel", "Panel"],
  ["library", "Library"],
  ["backup", "Backup"],
  ["os", "OS update"],
  ["play", "Play"],
];

function start(dir, hash) {
  const history = {
    shown: [],
    replaceState: (_state, _title, url) => history.shown.push(url),
  };
  const env = load(dir, SUPPORT, { location: { hash }, history });
  const built = [];
  for (const [id, title] of REGISTERED)
    env.kit.registerPage({ id, title, build: () => built.push(id) });
  const file = path.join(dir, "app.js");
  vm.runInContext(fs.readFileSync(file, "utf8"), env.context, {
    filename: "app.js",
  });
  return { env, built, history };
}

const cls = (node) => node.attrs.class || "";
const current = (env) =>
  env.byId.nav.children.map((t) => t.attrs["aria-current"] === "page");

function run(dir) {
  const { env, built, history } = start(dir, "");
  const tabs = env.byId.nav.children;
  assert.deepStrictEqual(
    tabs.map(textOf),
    ["Panel", "Library", "Play", "Machine", "Backup", "OS update"],
    "tab order",
  );
  assert.deepStrictEqual(
    built,
    REGISTERED.map(([id]) => id),
    "pages are built in registration order",
  );
  assert.deepStrictEqual(
    tabs.map((t) => cls(t).includes("tool")),
    [false, false, false, false, true, true],
    "Backup and OS update are the tools",
  );
  assert.ok(cls(tabs[4]).includes("tools-start"), "a rule opens the tools");
  assert.ok(!cls(tabs[5]).includes("tools-start"), "one rule only");
  assert.deepStrictEqual(
    current(env),
    [true, false, false, false, false, false],
    "Panel is the current tab",
  );
  const shown = env.byId.pages.children.filter((p) =>
    p.classList.contains("shown"),
  );
  assert.deepStrictEqual(
    shown.map((p) => p.attrs.id),
    ["page-panel"],
    "Panel is shown",
  );
  assert.deepStrictEqual(history.shown, ["#panel"]);
  tabs[3].click();
  assert.deepStrictEqual(current(env).indexOf(true), 3, "a click switches");
  env.kit.showPage("backup");
  assert.deepStrictEqual(current(env).indexOf(true), 4, "showPage switches");

  assert.strictEqual(
    current(start(dir, "#machine").env).indexOf(true),
    3,
    "the address names a page",
  );
  assert.strictEqual(
    current(start(dir, "#nowhere").env).indexOf(true),
    0,
    "an unknown page name falls back to Panel",
  );
}

run(uiDir);
const source = fs.readFileSync(path.join(uiDir, "app.js"), "utf8");
const broken = [
  [
    '["panel", "library", "play", "machine", "backup", "os"]',
    '["machine", "panel", "library", "play", "backup", "os"]',
  ],
  ["orderedPages(pages)[0].id", "pages[0].id"],
  ["pages.some((p) => p.id === wanted)", "false"],
  ['"tool" + (toolsStarted ? "" : " tools-start")', '"tool"'],
  ['const TOOL_PAGES = ["backup", "os"];', "const TOOL_PAGES = [];"],
  ["kit.setPageSwitcher(show);", ""],
];
let refused = 0;
for (const [from, to] of broken) {
  assert.ok(source.includes(from), "counter-proof anchor missing: " + from);
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "nav-order-"));
  for (const f of SUPPORT)
    fs.copyFileSync(path.join(uiDir, f), path.join(dir, f));
  fs.writeFileSync(path.join(dir, "app.js"), source.replace(from, to));
  assert.throws(() => run(dir), undefined, "broken copy accepted: " + from);
  refused++;
}
console.log(
  `ok: tab order, tools apart, Panel by default; ${refused} broken copies refused`,
);
