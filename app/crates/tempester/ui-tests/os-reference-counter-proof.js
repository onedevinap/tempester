/* Counter-proof of os-reference.test.js: a copy of the page scripts with one behaviour broken must make the checks
   fail. `node os-reference-counter-proof.js <ui folder> <scratch folder>` exits 1 when a variant is accepted or does
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
  "built image: the reference match is not said": [
    "page-os.js",
    'builtMatches: "Built image matches the reference ✓"',
    'builtMatches: "Built"',
  ],
  "known file: not recognised": [
    "page-os.js",
    '"This file is " + m.label + " ✓"',
    '"This file"',
  ],
  "unknown build: not flagged": [
    "page-os.js",
    "if (m.known === false)\n      els.evidence.show({ tone: \"warn\"",
    "if (false)\n      els.evidence.show({ tone: \"warn\"",
  ],
  "unknown build: no extra confirmation": ["page-os.js", "if (!sure) return;", ""],
  "unknown build: sent as a plain send": [
    "page-os.js",
    'if (ok) bridge.send("os_send_unknown");',
    'if (ok) bridge.send("os_send");',
  ],
  "unknown build: extra confirmation first dropped": [
    "page-os.js",
    "const unknown = readyInfo.known === false;",
    "const unknown = false;",
  ],
  "refused build: message not shown": ["page-os.js", "showRefused(m);", "void m;"],
  "full hash copy: nothing copied": [
    "os-hash.js",
    "writeClipboard(copyValue)",
    'writeClipboard("")',
  ],
  "full hash: not in the page": [
    "os-hash.js",
    'el("span", { class: "mono hash-full" }, h.value)',
    'el("span", { class: "mono hash-full" }, "")',
  ],
  "verify: start not sent": [
    "os-verify.js",
    'bridge.send("os_verify_start")',
    'bridge.send("x")',
  ],
  "verify: stop not sent": [
    "os-verify.js",
    'bridge.send("os_verify_stop")',
    'bridge.send("x")',
  ],
  "verify: enabled without a record": [
    "os-verify.js",
    "!session.connected || !record || listening",
    "!session.connected || listening",
  ],
  "verify: match not said": [
    "os-verify.js",
    "The machine runs exactly Tempester",
    "The machine seems to run Tempester",
  ],
  "verify: machine hash not shown on a mismatch": [
    "os-verify.js",
    "value: m.found,",
    "value: m.expected,",
  ],
  "verify: flasher page not reported": [
    "os-verify.js",
    'S.match(m) + " " + S.flasher(m) + " "',
    'S.match(m) + " "',
  ],
  "verify: flasher page called a mismatch": [
    "os-verify.js",
    "Update routine (flasher page): the machine's own",
    "Mismatch (flasher page): the machine's own",
  ],
  "verify: constant dump not flagged": [
    "os-verify.js",
    "Constant dump: every byte is 0x",
    "Dump: every byte is 0x",
  ],
  "verify: incomplete dump stops the listening": [
    "os-verify.js",
    'm.state === "listening" || m.state === "incomplete"',
    'm.state === "listening"',
  ],
  "installed line: verified not said": [
    "os-verify.js",
    '(r.verified_at ? "verified " + TICK : "not verified")',
    '"not verified"',
  ],
  "installed line: not updated by the installed event": [
    "os-verify.js",
    'bridge.on("installed", (m) => {',
    'bridge.on("installed-x", (m) => {',
  ],
  "machine page: installed event ignored": [
    "page-machine.js",
    'bridge.on("installed", (m) => showInstalled(m.installed || null));',
    "",
  ],
  "machine page: installed state ignored": [
    "page-machine.js",
    "showInstalled(m.installed || null);\n    });",
    "});",
  ],
};

let failed = 0;
for (const [name, [file, from, to]] of Object.entries(variants)) {
  const dir = fs.mkdtempSync(path.join(scratch, "os-reference-variant-"));
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
    [path.join(__dirname, "os-reference.test.js"), dir],
    { encoding: "utf8" },
  );
  if (run.status === 0) {
    console.log("broken page accepted: " + name);
    failed += 1;
  }
}
console.log(Object.keys(variants).length + " variants checked");
process.exit(failed ? 1 : 0);
