/* Counter-proof of os-cards.test.js: a copy of the page scripts with one behaviour broken must make the checks fail.
   `node os-cards-counter-proof.js <ui folder> <scratch folder>` exits 1 when a variant is accepted or does not change
   the script. A variant is [file, [[from, to], ...]]: every pair must apply. */
"use strict";
const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

const [uiDir, scratch] = process.argv.slice(2);
const FILES = fs.readdirSync(uiDir).filter((f) => f.endsWith(".js"));
const V = "os-versions.js";

const variants = {
  "build: in the folding body, hidden with it": [
    V,
    [
      ["          build,\n          fold,", "          fold,"],
      [
        'el("div", { class: "toolbar" }, more.toggle),',
        'el("div", { class: "toolbar" }, more.toggle, build),',
      ],
    ],
  ],
  "build: left out of a folded card": [
    V,
    [
      [
        "          build,\n          fold,",
        "          fold,",
      ],
    ],
  ],
  "build: taken out of the tab order": [
    V,
    [["      bucket.push(build);", '      bucket.push(build);\n      build.setAttribute("tabindex", "-1");']],
  ],
  "build: disabled while its card is folded": [
    V,
    [
      [
        "        target.hidden = !open;",
        "        target.hidden = !open;\n        if (!open) for (const b of buttons.concat(voiceButtons)) b.disabled = true;",
      ],
    ],
  ],
  "build: hidden while its card is folded": [
    V,
    [
      [
        "        target.hidden = !open;",
        "        target.hidden = !open;\n        if (!open) for (const b of buttons.concat(voiceButtons)) b.hidden = true;",
      ],
    ],
  ],
  "voice card: the long title again": [
    V,
    [["title: index === 0 ? S.latest(v.version) : S.previous(v.version),", 'title: "Tempester Voice " + v.version,']],
  ],
  "voice card: no date chip": [
    V,
    [["        chip: v.date,\n        badge: voiceBadge(v.version),", "        badge: voiceBadge(v.version),"]],
  ],
  "voice card: the installed line as its chip": [
    V,
    [["        chip: v.date,\n        badge: voiceBadge(v.version),", "        chip: voiceStatus(),\n        badge: voiceBadge(v.version),"]],
  ],
  "voice card: no change lists": [
    V,
    [["        groups: groupList(v.changes || {}),\n        details: v.description,\n        action,", "        groups: [],\n        details: v.description,\n        action,"]],
  ],
  "voice card: the note in every card again": [
    V,
    [[
      "        groups: groupList(v.changes || {}),\n        details: v.description,\n        action,",
      '        groups: [el("p", { class: "vsummary" }, S.voiceNote), ...groupList(v.changes || {})],\n        details: v.description,\n        action,',
    ]],
  ],
  "voice group: no note": [
    V,
    [['        el("p", { class: "vnote" }, S.voiceNote),\n', ""]],
  ],
  "voice group: no lead at all": [
    V,
    [["          lead: voiceLead(),\n", ""]],
  ],
  "installed: a shared word claimed on every card": [
    V,
    [["found && found.count > 0 && shared.length < 2", "found && found.count > 0"]],
  ],
  "installed: a shared word names one version": [
    V,
    [["lines.push(S.voiceShared(found.count, found.of, shared));", "lines.push(S.voiceOne(found.count, found.of, found.version));"]],
  ],
  "installed: a shared word said once per version": [
    V,
    [["if (found.count <= 0 || said.has(found.version)) continue;", "if (found.count <= 0) continue;"]],
  ],
  "installed: always says all six": [
    V,
    [["S.voiceInstalledChip(found.count, found.of)", "S.voiceInstalledChip(found.of, found.of)"]],
  ],
  "installed: a count of zero is installed": [
    V,
    [
      ["found && found.count > 0 && shared.length < 2", "found && shared.length < 2"],
      ["if (found.count <= 0 || said.has(found.version)) continue;", "if (said.has(found.version)) continue;"],
    ],
  ],
  "installed: any version marks the card": [
    V,
    [["(voiceInstalled || []).find((v) => v.version === version)", "(voiceInstalled || [])[0]"]],
  ],
  "installed: not reported reads as none": [
    V,
    [["if (!voiceInstalled) return S.voiceNotReported;", "if (!voiceInstalled) return S.voiceNone;"]],
  ],
  "installed: a new report does not redraw": [
    V,
    [["      if (lastLists) show(...lastLists);", ""]],
  ],
  "installed: a new list forgets it": [
    V,
    [["      lastLists = [list, voiceList];", "      lastLists = [list, voiceList];\n      voiceInstalled = null;"]],
  ],
  "installed: the installed event is not read": [
    "page-os.js",
    [['    bridge.on("installed", voiceInstalled);\n', ""]],
  ],
  "installed: the state event is not read": [
    "page-os.js",
    [['    bridge.on("state", voiceInstalled);\n', ""]],
  ],
};

let failed = 0;
for (const [name, [file, pairs]] of Object.entries(variants)) {
  const dir = fs.mkdtempSync(path.join(scratch, "os-cards-variant-"));
  for (const f of FILES) fs.copyFileSync(path.join(uiDir, f), path.join(dir, f));
  let source = fs.readFileSync(path.join(dir, file), "utf8");
  let applied = true;
  for (const [from, to] of pairs) {
    if (!source.includes(from)) applied = false;
    source = source.replace(from, () => to);
  }
  if (!applied) {
    console.log("variant did not apply: " + name);
    failed += 1;
    continue;
  }
  fs.writeFileSync(path.join(dir, file), source);
  const run = spawnSync(
    "node",
    [path.join(__dirname, "os-cards.test.js"), dir],
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
