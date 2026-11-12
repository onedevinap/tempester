/* Counter-proof of os-versions.test.js: a copy of the page scripts with one behaviour broken must make the checks
   fail. `node os-versions-counter-proof.js <ui folder> <scratch folder>` exits 1 when a variant is accepted or does
   not change the script. */
"use strict";
const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

const [uiDir, scratch] = process.argv.slice(2);
const FILES = [
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
  "page-backup.js",
  "page-os.js",
  "page-play.js",
];

const variants = {
  "latest card: the corrections version is taken": [
    "os-versions.js",
    'list.find((v) => v.kind !== "corrections")',
    'list.find((v) => v.kind === "corrections")',
  ],
  "corrections card: missing": [
    "os-versions.js",
    'const corrections = list.find((v) => v.kind === "corrections");',
    "const corrections = null;",
  ],
  "original card: missing": ["os-versions.js", "          original(),\n", ""],
  "details: shown from the start": [
    "os-versions.js",
    'const body = el("p", { class: "vdetails", hidden: true }, text);',
    'const body = el("p", { class: "vdetails" }, text);',
  ],
  "details: the toggle does nothing": [
    "os-versions.js",
    "        body.hidden = !body.hidden;\n        toggle.setAttribute",
    "        body.hidden = body.hidden;\n        toggle.setAttribute",
  ],
  "details: the card shows the summary instead": [
    "os-versions.js",
    "const more = detailsBlock(title, details);",
    "const more = detailsBlock(title, summary);",
  ],
  "summary: not shown": [
    "os-versions.js",
    'el("p", { class: "vsummary" }, summary),',
    'el("p", { class: "vsummary" }, ""),',
  ],
  "groups: an empty group is drawn": [
    "os-versions.js",
    "GROUP_ORDER.filter((key) => (changes[key] || []).length)",
    "GROUP_ORDER.filter(() => true)",
  ],
  "history: the row does not open": [
    "os-versions.js",
    "          onclick: () => {\n            body.hidden = !body.hidden;",
    "          onclick: () => {\n            body.hidden = body.hidden;",
  ],
  "history: oldest first": [
    "os-versions.js",
    "? list.map(row)",
    "? list.slice().reverse().map(row)",
  ],
  "history: the corrections version is left out": [
    "os-versions.js",
    "? list.map(row)",
    '? list.filter((v) => v.kind !== "corrections").map(row)',
  ],
  "history: the corrections chip is missing": [
    "os-versions.js",
    'v.kind === "corrections"\n          ? el("span", { class: "chip on" }',
    'false\n          ? el("span", { class: "chip on" }',
  ],
  "build: a card builds another version": [
    "os-versions.js",
    "() => onBuild(version), {",
    '() => onBuild("0.0.0.0"), {',
  ],
  "history: a row gets a build button again": [
    "os-versions.js",
    'el("div", { class: "vrow-head" }, toggle),',
    'el("div", { class: "vrow-head" }, toggle, buildButton(v.version, onBuild)),',
  ],
  "build: enabled without the DSI file": [
    "os-versions.js",
    "b.disabled = !on;",
    "b.disabled = false;",
  ],
  "build: a new list forgets the DSI file": [
    "os-versions.js",
    "      setReady(ready);\n    }",
    "    }",
  ],
  "page: the original card builds a version": [
    "page-os.js",
    'onOriginal: () => bridge.send("os_original", dsi.hex)',
    'onOriginal: () => bridge.send("os_build", "1.5.0.2", dsi.hex)',
  ],
  "page: the build sends no file": [
    "page-os.js",
    'bridge.send("os_build", version, dsi.hex)',
    'bridge.send("os_build", version, "")',
  ],
};

let failed = 0;
for (const [name, [file, from, to]] of Object.entries(variants)) {
  const dir = fs.mkdtempSync(path.join(scratch, "os-versions-variant-"));
  for (const f of FILES) fs.copyFileSync(path.join(uiDir, f), path.join(dir, f));
  const source = fs.readFileSync(path.join(dir, file), "utf8");
  const broken = source.replace(from, to);
  if (broken === source) {
    console.log("variant did not change the script: " + name);
    failed += 1;
    continue;
  }
  fs.writeFileSync(path.join(dir, file), broken);
  const run = spawnSync(
    "node",
    [path.join(__dirname, "os-versions.test.js"), dir],
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
