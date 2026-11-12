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
    "!session.connected || !canVerify() || listening",
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
    'bridge.on("installed", onMachineState);',
    'bridge.on("installed-x", onMachineState);',
  ],
  "installed line: not updated by the state event": [
    "os-verify.js",
    'bridge.on("state", onMachineState);',
    'bridge.on("state-x", onMachineState);',
  ],
  "machine page: installed line not watched": [
    "page-machine.js",
    "window.Tempester.osVerify.watch(showInstalled);",
    "",
  ],
  "reported: the record still stands for what runs": [
    "os-verify.js",
    "if (!running)\n      return !record",
    "if (true)\n      return !record",
  ],
  "reported: no note when the two differ": [
    "os-verify.js",
    "els.note.hidden = !line.note;",
    "els.note.hidden = true;",
  ],
  "reported: note even when the two agree": [
    "os-verify.js",
    "record && record.version !== running.version\n          ?",
    "record\n          ?",
  ],
  "reported: verify allowed for a version the app does not know": [
    "os-verify.js",
    "(view.running && view.running.known)",
    "view.running",
  ],
  "reported: proof of one version covers another": [
    "os-verify.js",
    "verified === running.version ||",
    "verified ||",
  ],
  "reported: proof kept when the version changes": [
    "os-verify.js",
    "running && running.version === view.verified ? view.verified : null",
    "view.verified",
  ],
  "reported: proof kept after a disconnect": [
    "os-verify.js",
    "setView({ running: null, verified: null })",
    "setView({ verified: null })",
  ],
  "reported: a mismatch counts as a proof": [
    "os-verify.js",
    'm.state === "match" && m.source === "reported"',
    'm.state !== "idle" && m.source === "reported"',
  ],
  "reported: match does not say what it was compared with": [
    "os-verify.js",
    '(m.source === "reported" ? S.comparedWithReported : "")',
    '""',
  ],
  "reported: mismatch does not say it is the reported version": [
    "os-verify.js",
    '(m.also || m.source === "reported" ? S.tag(m.source) : "") +',
    '"" +',
  ],
  "reported: a mismatch does not name the other candidate": [
    "os-verify.js",
    '? " nor " + m.also +',
    '? "" + m.also +',
  ],
  "reported: a match with the sent OS is said to be a comparison with the report": [
    "os-verify.js",
    '(m.source === "reported" ? S.comparedWithReported : "")',
    "S.comparedWithReported",
  ],
  "reported: hash called Sent": [
    "os-verify.js",
    'const reported = m.source === "reported";',
    "const reported = false;",
  ],
  "pill: a version no longer reported stays": [
    "conn-bar.js",
    "running = m.running || null;",
    "running = m.running || running;",
  ],
  "line: a version no longer reported stays": [
    "os-verify.js",
    "const running = m.running || null;",
    "const running = m.running || view.running;",
  ],
  "pill: the reported OS is not named": [
    "conn-bar.js",
    'if (running) showChip(S.os(running.label), "ok");',
    'if (false) showChip(S.os(running.label), "ok");',
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
