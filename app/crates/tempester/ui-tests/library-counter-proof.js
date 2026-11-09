/* Counter-proof of library-browser.test.js: copies of the page scripts and styles with one behaviour broken must
   make the checks fail. `node library-counter-proof.js <ui folder> <scratch folder>` exits 1 when a variant is
   accepted or does not change the file. */
"use strict";
const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

const [uiDir, scratch] = process.argv.slice(2);
const FILES = [
  "bridge.js",
  "icons.js",
  "kit.js",
  "project-ui.js",
  "lib-model.js",
  "lib-dialogs.js",
  "lib-flow.js",
  "lib-browser.js",
  "page-library.js",
  "library.css",
  "compact.css",
];
const MODEL = "lib-model.js";
const DIALOGS = "lib-dialogs.js";
const FLOW = "lib-flow.js";
const BROWSER = "lib-browser.js";
const PAGE = "page-library.js";
const PROJECT = "project-ui.js";

const variants = {
  "search matches any word instead of every word": [
    MODEL,
    "words.every(",
    "words.some(",
  ],
  "search ignores the tags": [MODEL, 'item.tags.join(" "),', '"",'],
  "search ignores the note": [MODEL, "item.note,", '"",'],
  "search ignores the folder": [
    MODEL,
    "item.folder,\n      item.tags",
    '"",\n      item.tags',
  ],
  "favourites filter off": [
    MODEL,
    "if (f.favourites && !item.favourite) return false;",
    "",
  ],
  "tag filter off": [
    MODEL,
    "if (f.tag && !item.tags.includes(f.tag)) return false;",
    "",
  ],
  "source filter off": [
    MODEL,
    "if (f.source && item.source !== f.source) return false;",
    "",
  ],
  "Sound Type filter off": [
    MODEL,
    "if (f.soundType && item.sound_type !== f.soundType) return false;",
    "",
  ],
  "oscillator filter off": [
    MODEL,
    "if (f.osc && !item.osc.includes(f.osc)) return false;",
    "",
  ],
  "date range end ignored": [
    MODEL,
    "if (to !== null && item.modified >= to + 86400) return false;",
    "",
  ],
  "date range start ignored": [
    MODEL,
    "if (from !== null && item.modified < from) return false;",
    "",
  ],
  "sort direction ignored": [
    MODEL,
    'const sign = direction === "desc" ? -1 : 1;',
    "const sign = 1;",
  ],
  "backups item no longer first": [
    MODEL,
    "if (a.slot !== b.slot) return a.slot ? -1 : 1;",
    "",
  ],
  "shift click selects one row": [
    MODEL,
    "const [lo, hi] = a < b ? [a, b] : [b, a];",
    "const [lo, hi] = [b, b];",
  ],
  "ctrl click does not toggle": [
    MODEL,
    "if (sel.ids.has(id)) sel.ids.delete(id);",
    "if (false) sel.ids.delete(id);",
  ],
  "select all selects nothing": [
    MODEL,
    "sel.ids = new Set(orderedIds);",
    "sel.ids = new Set();",
  ],
  "name length not checked": [
    MODEL,
    "if (name.length > NAME_MAX)",
    "if (false)",
  ],
  "name characters not checked": [
    MODEL,
    'if (c < " " || c > "~")',
    "if (false)",
  ],
  "name spaces not checked": [MODEL, "if (name !== name.trim())", "if (false)"],
  "review defaults changed": [
    DIALOGS,
    "select.value = group.choices[0][0];",
    "select.value = group.choices[group.choices.length - 1][0];",
  ],
  "review does not warn about an older version": [
    DIALOGS,
    "incoming is OLDER: it would replace a newer version",
    "incoming",
  ],
  "send without backup not asked as such": [
    DIALOGS,
    'okLabel: risky ? "Send without a backup" : "Send"',
    'okLabel: "Send"',
  ],
  "identical send still asks to send": [
    DIALOGS,
    'if (plan.status === "identical") {',
    "if (false) {",
  ],
  "dismissed review not cancelled": [
    FLOW,
    'else bridge.send("lib_cancel");',
    "else void 0;",
  ],
  "review choices not sent": [
    FLOW,
    'bridge.send("lib_apply", choice.new,',
    'bridge.send("lib_apply_x", choice.new,',
  ],
  "send token not returned": [
    FLOW,
    'bridge.send("lib_send_apply", plan.kind, plan.id, plan.token)',
    'bridge.send("lib_send_apply", plan.kind, plan.id, "")',
  ],
  "send check lost": [
    FLOW,
    'bridge.send("lib_send_check", kind, id || "")',
    'bridge.send("lib_send_check_x", kind, id || "")',
  ],
  "problems not logged": [
    FLOW,
    'add({ title: "Not done"',
    'void ({ title: "Not done"',
  ],
  "delete without confirmation": [
    BROWSER,
    'if (ok) send("lib_delete", idsOf(items));',
    'send("lib_delete", idsOf(items));',
  ],
  "rename not validated": [BROWSER, "model.nameRefusal(text)", "null"],
  "batch sends only the first id": [
    BROWSER,
    'items.map((i) => i.id).join(",")',
    'items.slice(0, 1).map((i) => i.id).join(",")',
  ],
  "arrow down does nothing": [BROWSER, 'ArrowDown: "down",', ""],
  "escape does not clear": [
    BROWSER,
    '} else if (e.key === "Escape") {',
    '} else if (e.key === "Escape" && false) {',
  ],
  "search box does nothing": [BROWSER, "state.query = els.search.value;", ""],
  "header click does not sort": [
    BROWSER,
    "onclick: () => setSort(key)",
    "onclick: () => {}",
  ],
  "rows with another cell": [
    BROWSER,
    'el("span", { class: "col-type mid" }, typeText(item)),',
    "",
  ],
  "head and rows no longer share a grid": [
    uiCss("library.css"),
    ".lib-head, .lib-row {",
    ".lib-row {",
  ],
  "rows without a fixed height": [
    uiCss("compact.css"),
    "align-items: center; height: 44px;",
    "align-items: center; height: auto;",
  ],
  "message line without a fixed height": [
    uiCss("library.css"),
    "gap: 10px; height: 44px; margin-top: 8px;",
    "gap: 10px; margin-top: 8px;",
  ],
  "row icon buttons too small": [
    uiCss("library.css"),
    ".lib-row .ibtn { width: 36px; height: 36px; }",
    ".lib-row .ibtn { width: 28px; height: 28px; }",
  ],
  "fetch_sound removed": [PAGE, 'fetch: "fetch_sound"', 'fetch: "x"'],
  "save_sound removed": [PAGE, 'save: "save_sound"', 'save: "x"'],
  "fetch_beat removed": [PAGE, 'fetch: "fetch_beat"', 'fetch: "x"'],
  "save_beat removed": [PAGE, 'save: "save_beat"', 'save: "x"'],
  "card send goes straight out": [
    PAGE,
    'flow.sendCheck(cfg.list, "")',
    "void cfg",
  ],
  "fetch_project removed": [
    PROJECT,
    'bridge.send("fetch_project")',
    'bridge.send("x")',
  ],
  "receive_project removed": [
    PROJECT,
    'bridge.send("receive_project")',
    'bridge.send("x")',
  ],
  "project_cancel removed": [
    PROJECT,
    ': "project_cancel"',
    ': "x"',
  ],
  "fake_project_send removed": [
    PROJECT,
    'bridge.send("fake_project_send")',
    'bridge.send("x")',
  ],
  "fake_project_send_partial removed": [
    PROJECT,
    'bridge.send("fake_project_send_partial")',
    'bridge.send("x")',
  ],
};

function uiCss(name) {
  return name;
}

let failed = 0;
for (const [name, [file, from, to]] of Object.entries(variants)) {
  const dir = fs.mkdtempSync(path.join(scratch, "library-variant-"));
  for (const f of FILES)
    fs.copyFileSync(path.join(uiDir, f), path.join(dir, f));
  const source = fs.readFileSync(path.join(dir, file), "utf8");
  const broken = source.replace(from, to);
  if (broken === source) {
    console.log("variant did not change the file: " + name);
    failed += 1;
    continue;
  }
  fs.writeFileSync(path.join(dir, file), broken);
  const run = spawnSync(
    "node",
    [path.join(__dirname, "library-browser.test.js"), dir],
    { encoding: "utf8" },
  );
  if (run.status === 0) {
    console.log("broken page accepted: " + name);
    failed += 1;
  }
}
console.log(Object.keys(variants).length + " variants checked");
process.exit(failed ? 1 : 0);
