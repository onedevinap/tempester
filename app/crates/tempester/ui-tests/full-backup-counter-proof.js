/* Counter-proof of full-backup.test.js: each deliberately broken copy of full-backup.js (or of the gate it uses) must
   make the checks fail. `node full-backup-counter-proof.js <ui folder> <scratch folder>` exits 1 when a variant is
   accepted or does not change its script. */
"use strict";
const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

const [uiDir, scratch] = process.argv.slice(2);
const FILES = [
  "bridge.js",
  "icons.js",
  "kit.js",
  "panel-perform.js",
  "full-backup.js",
  "page-backup.js",
];
const variants = {
  "save possible whatever the state": [
    "full-backup.js",
    'els.save.disabled = state !== "ready";',
    "els.save.disabled = false;",
  ],
  "save possible once the answer is complete but refused": [
    "full-backup.js",
    'els.save.disabled = state !== "ready";',
    'els.save.disabled = state === "idle" || state === "waiting";',
  ],
  "request possible on an OS without the feature": [
    "full-backup.js",
    "els.request.disabled = !on || running;",
    "els.request.disabled = running;",
  ],
  "request possible twice while it runs": [
    "full-backup.js",
    "els.request.disabled = !on || running;",
    "els.request.disabled = !on;",
  ],
  "request possible when not connected": [
    "full-backup.js",
    "if (!session.connected) return { on: false, why: S.notConnected };",
    "",
  ],
  "the PICkit notice dropped": [
    "full-backup.js",
    'el("p", { class: "statusbox bad", role: "note" }, S.never),',
    "",
  ],
  "the notice no longer says the files are never sent back": [
    "full-backup.js",
    "This app never sends them back to the machine.",
    "Keep them safe.",
  ],
  "a refusal reason hidden": [
    "full-backup.js",
    'el("td", { class: "mid" }, r.detail),',
    'el("td", { class: "mid" }, ""),',
  ],
  "a refused chip shown as OK": [
    "full-backup.js",
    "S.verdicts[r.verdict] || r.verdict,",
    "S.verdicts.ok,",
  ],
  "the save command sent to the wrong handler": [
    "full-backup.js",
    'bridge.send("full_backup_save")',
    'bridge.send("backup_save")',
  ],
  "the request command wrong": [
    "full-backup.js",
    'bridge.send("full_backup_start")',
    'bridge.send("backup_start")',
  ],
  "cancel possible when nothing runs": [
    "full-backup.js",
    'els.cancel.disabled = state === "idle";',
    "els.cancel.disabled = false;",
  ],
  "the Debug-menu dumps not listed": [
    "full-backup.js",
    "...(extras.length ? [el(\"p\", {}, S.extras)] : []),\n      ...extras.map(",
    "...[].map(",
  ],
  "the saved folder not shown": [
    "full-backup.js",
    "el(\"p\", {}, S.savedIn(m.dir)),",
    "",
  ],
  "an error of the full backup not told": [
    "full-backup.js",
    'if (m.during && m.during.startsWith("full_backup"))',
    "if (false)",
  ],
  "a button that sends the backup back": [
    "full-backup.js",
    'els.cancel = button("stop", S.cancel,',
    'els.cancel = button("stop", "Send the backup to the machine",',
  ],
  "the one-backup-per-power-on rule never said": [
    "full-backup.js",
    'const limited = on && !features.status("full_backup_repeat").on;',
    "const limited = false;",
  ],
  "the rule said on 1.6.8.0 too": [
    "full-backup.js",
    'const limited = on && !features.status("full_backup_repeat").on;',
    "const limited = on;",
  ],
  "the power-cycle confirmation sends nothing": [
    "full-backup.js",
    'bridge.send("full_backup_power_cycled")',
    "undefined",
  ],
  "a calibration warning shown in the refusal colour": [
    "full-backup.js",
    'badges: { ok: "ok", warning: "constant", waiting: "" },',
    'badges: { ok: "ok", waiting: "" },',
  ],
  "a calibration warning shown as OK": [
    "full-backup.js",
    "verdicts: { ok: \"OK\", warning: \"warning\",",
    "verdicts: { ok: \"OK\", warning: \"OK\",",
  ],
  "the warned pages not counted in the status": [
    "full-backup.js",
    'const warn = state === "ready" && warned > 0;',
    "const warn = false;",
  ],
  "the calibration words never shown": [
    "full-backup.js",
    "if (!r.calibration) return [line];",
    "return [line];",
  ],
  "the calibration words hidden for a refused chip": [
    "full-backup.js",
    "if (!r.calibration) return [line];",
    'if (!r.calibration || r.verdict === "refused") return [line];',
  ],
  "a calibration table left out of the words": [
    "full-backup.js",
    '" | C " + c.c.join(" ")',
    '""',
  ],
  "the note on what may be posted dropped": [
    "full-backup.js",
    "      els.calibrationNote,\n",
    "",
  ],
  "the note no longer forbids posting the Voice files": [
    "full-backup.js",
    "Never post the Voice .syx files: they hold DSI's Voice OS.",
    "Post them with the report.",
  ],
  "the gate without the version name": [
    "panel-perform.js",
    'LABELS[name] + " Tempester " + since + " or later"',
    'LABELS[name] + " a newer OS"',
  ],
};

fs.mkdirSync(scratch, { recursive: true });
let failed = 0;
for (const [name, [file, from, to]] of Object.entries(variants)) {
  const dir = fs.mkdtempSync(path.join(scratch, "fullbackup-"));
  for (const f of FILES)
    fs.copyFileSync(path.join(uiDir, f), path.join(dir, f));
  const source = fs.readFileSync(path.join(dir, file), "utf8");
  if (!source.includes(from)) {
    console.error("variant does not apply: " + name);
    failed++;
    continue;
  }
  fs.writeFileSync(path.join(dir, file), source.replace(from, to));
  const run = spawnSync("node", [
    path.join(__dirname, "full-backup.test.js"),
    dir,
  ]);
  if (run.status === 0) {
    console.error("variant accepted: " + name);
    failed++;
  }
}
if (failed) process.exit(1);
console.log(Object.keys(variants).length + " broken copies refused");
