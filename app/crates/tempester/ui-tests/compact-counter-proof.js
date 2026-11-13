/* Counter-proof of compact-pages.test.js: a copy of the page scripts with one command removed, one confirmation
   weakened or one control dropped must make the checks fail. `node compact-counter-proof.js <ui folder> <scratch
   folder>` exits 1 when a variant is accepted or does not change the script. */
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

const send = (file, command) => [
  file,
  `bridge.send("${command}"`,
  `bridge.send("x"`,
];

const variants = {
  "top bar: connect removed": send("conn-bar.js", "connect"),
  "top bar: disconnect removed": send("conn-bar.js", "disconnect"),
  "top bar: refresh of the ports removed": [
    "conn-bar.js",
    'button("refresh", S.refresh, () => bridge.send("ports"))',
    'button("refresh", S.refresh, () => {})',
  ],
  "top bar: identity answer not shown": [
    "conn-bar.js",
    'bridge.on("identity"',
    'bridge.on("identity-x"',
  ],
  "top bar: reported release not shown": [
    "conn-bar.js",
    'if (running) showChip(S.os(running), "ok", S.osTip(running));',
    'if (false) showChip(S.os(running), "ok", S.osTip(running));',
  ],
  "top bar: label instead of the short release": [
    "conn-bar.js",
    'os: (running) => "OS: " + running.version',
    'os: (running) => "OS: " + running.label',
  ],
  "machine: version word kept after the release is reported": [
    "conn-bar.js",
    "running\n        ? running.label",
    "false\n        ? running.label",
  ],
  "machine: release not repainted when reported after the answer": [
    "page-machine.js",
    "showAnswer(view.running);",
    "",
  ],
  "play: status not refreshed on connection": [
    "page-play.js",
    "    showStatus(session);\n",
    "    if (!session.connected) showStatus(session);\n",
  ],
  "machine: set_channel removed": send("page-machine.js", "set_channel"),
  "machine: identity removed": send("page-machine.js", "identity"),
  "machine: version request removed": send("page-machine.js", "tempester_version"),
  "machine: reveal of the data folder removed": [
    "page-machine.js",
    'bridge.send("reveal", "data")',
    "void 0",
  ],
  "monitor: monitor switch removed": [
    "monitor-ui.js",
    'bridge.send("monitor", state.on',
    'bridge.send("x", state.on',
  ],
  "monitor: clock switch removed": send("monitor-ui.js", "monitor_clock"),
  "monitor: Clear does nothing": [
    "monitor-ui.js",
    'button("trash", S.clear, clear)',
    'button("trash", S.clear, () => {})',
  ],
  "backup: backup_start removed": send("page-backup.js", "backup_start"),
  "backup: backup_stop removed": send("page-backup.js", "backup_stop"),
  "backup: backup_save removed": send("page-backup.js", "backup_save"),
  "backup: reveal removed": [
    "page-backup.js",
    'bridge.send("reveal", "backups")',
    "void 0",
  ],
  "backup: fake helper removed": send("page-backup.js", "fake_debug_send"),
  "backup: steps dropped": [
    "page-backup.js",
    "info(stepList(), S.stepsTitle),",
    "",
  ],
  "backup: constant warning dropped": [
    "page-backup.js",
    "info(S.constantWarning, S.constantTitle),",
    "",
  ],
  "os: os_build removed": send("page-os.js", "os_build"),
  "os: os_original removed": send("page-os.js", "os_original"),
  "os: os_validate removed": send("page-os.js", "os_validate"),
  "os: os_versions removed": send("page-os.js", "os_versions"),
  "os: identity removed": send("page-os.js", "identity"),
  "os: version request removed": send("page-os.js", "tempester_version"),
  "os: send without confirmation": [
    "page-os.js",
    'if (ok) bridge.send("os_send");',
    'bridge.send("os_send");',
  ],
  "os: cancel without confirmation": [
    "page-os.js",
    'if (ok) bridge.send("os_cancel");',
    'bridge.send("os_cancel");',
  ],
  "os: confirmation shortened": [
    "page-os.js",
    '". Do not power off or unplug anything while it is sent.",',
    '".",',
  ],
  "os: flasher page sentence dropped": [
    "page-os.js",
    "The flasher page is never touched, so a new update is always possible.",
    "",
  ],
  "play: pad press removed": [
    "page-play.js",
    'bridge.send("pad", down ? "on" : "off", k,',
    'bridge.send("x", down ? "on" : "off", k,',
  ],
  /* The bug seen on the author's machine: the page computing the note as 36 + pad instead of asking for the pad. */
  "play: pad sent as note 36 + pad": [
    "page-play.js",
    'bridge.send("pad", down ? "on" : "off", k,',
    'bridge.send("note", down ? "on" : "off", 36 + k,',
  ],
  "play: pad labels not following the table": [
    "page-play.js",
    'els.pads.forEach((pad, k) => pad.setAttribute("aria-label", padLabel(k))),',
    "void 0,",
  ],
  "pad notes: edited note not sent": send("pad-notes-panel.js", "pad_note"),
  "pad notes: reset not sent": send("pad-notes-panel.js", "pad_notes_reset"),
  "pad notes: a refused note stays shown": [
    "pad-notes-panel.js",
    'input.value = note === null ? "" : String(note);',
    "void note;",
  ],
  "pad notes: learned pad not marked": [
    "pad-notes-panel.js",
    'els.cells[m.pad].classList.add("learned");',
    "",
  ],
  "pad notes: table not shown": [
    "pad-notes-panel.js",
    "els.inputs[pad].value = String(note);",
    "",
  ],
  "pad notes: notes read as 36 + pad": [
    "pad-notes.js",
    "return notes && pad >= 0 && pad < PADS ? notes[pad] : null;",
    "return notes && pad >= 0 && pad < PADS ? 36 + pad : null;",
  ],
  "play: clock_start removed": send("page-play.js", "clock_start"),
  "play: clock_stop removed": send("page-play.js", "clock_stop"),
  "play: tempo while running removed": [
    "page-play.js",
    'bridge.sendThrottled("bpm", 100, "clock_bpm", bpm)',
    "void bpm",
  ],
  "play: seq_pad removed": send("page-play.js", "seq_pad"),
  "play: seq_run removed": send("page-play.js", "seq_run"),
  "play: pulses not kept within the steps": [
    "page-play.js",
    "raw.pulses = Math.min(raw.pulses, raw.steps);",
    "",
  ],
  "play: rotation not kept below the steps": [
    "page-play.js",
    "raw.rotation = Math.min(raw.rotation, raw.steps - 1);",
    "",
  ],
  "play: pads not in the machine's order": [
    "page-play.js",
    "els.pads.map((_, k) => buildCell(k))",
    "els.pads.map((_, k) => buildCell(15 - k))",
  ],
  "play: pattern not drawn": [
    "page-play.js",
    'bridge.on("seq_pad", (m) => showPattern(m.pad, m));',
    "",
  ],
  "play: page is not one screen": ["page-play.js", "fit: true, build", "build"],
};

let failed = 0;
for (const [name, [file, from, to]] of Object.entries(variants)) {
  const dir = fs.mkdtempSync(path.join(scratch, "compact-variant-"));
  for (const f of FILES)
    fs.copyFileSync(path.join(uiDir, f), path.join(dir, f));
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
    [path.join(__dirname, "compact-pages.test.js"), dir],
    { encoding: "utf8" },
  );
  if (run.status === 0) {
    console.log("broken page accepted: " + name);
    failed += 1;
  }
}
console.log(Object.keys(variants).length + " variants checked");
process.exit(failed ? 1 : 0);
