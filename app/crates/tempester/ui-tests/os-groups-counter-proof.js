/* Counter-proof of os-groups.test.js: a copy of the page scripts with one behaviour broken must make the checks
   fail. `node os-groups-counter-proof.js <ui folder> <scratch folder>` exits 1 when a variant is accepted or does not
   change the script. */
"use strict";
const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

const [uiDir, scratch] = process.argv.slice(2);
const FILES = fs.readdirSync(uiDir).filter((f) => f.endsWith(".js"));
const V = "os-versions.js";

const variants = {
  "fold: the toggle does nothing": [
    V,
    "memory.collapsed[key] = !memory.collapsed[key];",
    "memory.collapsed[key] = memory.collapsed[key];",
  ],
  "fold: aria-expanded is not kept up to date": [
    V,
    'toggle.setAttribute("aria-expanded", String(open));',
    "void open;",
  ],
  "fold: a new list forgets what was folded": [
    V,
    "      buttons.length = 0;\n      voiceButtons.length = 0;\n      const latest",
    "      memory.collapsed = {};\n      buttons.length = 0;\n      voiceButtons.length = 0;\n      const latest",
  ],
  "fold: every card shares one key": [
    V,
    'foldable("card:" + os.id + ":" + title,',
    'foldable("card:",',
  ],
  "fold: the Main and Voice cards share their keys": [
    V,
    'foldable("card:" + os.id + ":" + title,',
    'foldable("card:" + title.replace(/[0-9.]+/, ""),',
  ],
  "fold: a card fold hides nothing": [
    V,
    'foldable("card:" + os.id + ":" + title, content,',
    'foldable("card:" + os.id + ":" + title, el("div"),',
  ],
  "fold: a history fold hides nothing": [
    V,
    'foldable("history:" + os.id, list,',
    'foldable("history:" + os.id, el("div"),',
  ],
  "fold: a group fold hides nothing": [
    V,
    'foldable("group:" + os.id, content,',
    'foldable("group:" + os.id, el("div"),',
  ],
  "fold: the group button has no name": [
    V,
    '"aria-label": S.foldGroup(os.title),',
    '"aria-label": "",',
  ],
  "group: the Voice group is missing": [
    V,
    'cards: voiceList.filter((v) => v.kind !== "superseded").map(voice),',
    "cards: [],",
  ],
  "card: the superseded Voice 1.5.1.0 is offered again": [
    V,
    'cards: voiceList.filter((v) => v.kind !== "superseded").map(voice),',
    "cards: voiceList.map(voice),",
  ],
  "card: the newest Voice version is left out with the superseded one": [
    V,
    'cards: voiceList.filter((v) => v.kind !== "superseded").map(voice),',
    'cards: voiceList.filter((v) => v.kind === "release" && false).map(voice),',
  ],
  "history: the superseded row is not marked": [
    V,
    '        v.kind === "superseded"\n          ? el("span", { class: "chip" }, S.supersededChip)\n          : null,\n',
    "",
  ],
  "history: the superseded row is left out": [
    V,
    "rows: voiceList.map(historyRow(VOICE)),",
    'rows: voiceList.filter((v) => v.kind !== "superseded").map(historyRow(VOICE)),',
  ],
  "group: the Voice group comes first": [
    V,
    "      ].filter((group) => group.cards.length);",
    "      ].reverse().filter((group) => group.cards.length);",
  ],
  "history: the Voice releases are left out": [
    V,
    "rows: voiceList.map(historyRow(VOICE)),",
    "rows: [],",
  ],
  "history: the Voice rows are labelled Main": [
    V,
    "rows: voiceList.map(historyRow(VOICE)),",
    "rows: voiceList.map(historyRow(MAIN)),",
  ],
  "filter: nothing is hidden": [
    V,
    'group.hidden = memory.filter !== "all" && memory.filter !== os.id;',
    "group.hidden = false;",
  ],
  "filter: the other OS stays and the chosen one goes": [
    V,
    'memory.filter !== "all" && memory.filter !== os.id;',
    'memory.filter !== "all" && memory.filter === os.id;',
  ],
  "filter: the pressed state is not shown": [
    V,
    'String(b.getAttribute("data-filter") === memory.filter),',
    '"false",',
  ],
  "filter: a new list forgets it": [
    V,
    '      if (!offered.some(({ os }) => os.id === memory.filter))\n        memory.filter = "all";',
    '      memory.filter = "all";',
  ],
  "filter: kept on an OS that is no longer offered": [
    V,
    '      if (!offered.some(({ os }) => os.id === memory.filter))\n        memory.filter = "all";',
    "",
  ],
};

let failed = 0;
for (const [name, [file, from, to]] of Object.entries(variants)) {
  const dir = fs.mkdtempSync(path.join(scratch, "os-groups-variant-"));
  for (const f of FILES) fs.copyFileSync(path.join(uiDir, f), path.join(dir, f));
  const source = fs.readFileSync(path.join(dir, file), "utf8");
  const broken = source.replace(from, () => to);
  if (broken === source) {
    console.log("variant did not change the script: " + name);
    failed += 1;
    continue;
  }
  fs.writeFileSync(path.join(dir, file), broken);
  const run = spawnSync(
    "node",
    [path.join(__dirname, "os-groups.test.js"), dir],
    { encoding: "utf8" },
  );
  if (run.status === 0) {
    console.log("broken page accepted: " + name);
    failed += 1;
  }
  fs.rmSync(dir, { recursive: true, force: true });
}
console.log(Object.keys(variants).length + " variants checked");
process.exit(failed ? 1 : 0);
