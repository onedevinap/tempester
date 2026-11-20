/* Counter-proof of os-voice.test.js: a copy of the page scripts with one behaviour broken must make the checks fail.
   `node os-voice-counter-proof.js <ui folder> <scratch folder>` exits 1 when a variant is accepted or does not
   change the script. */
"use strict";
const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

const [uiDir, scratch] = process.argv.slice(2);
const FILES = fs.readdirSync(uiDir).filter((f) => f.endsWith(".js"));

const variants = {
  "card: the Voice versions are ignored": [
    "page-os.js",
    "els.versions.show(list, voiceList || []);",
    "els.versions.show(list, []);",
  ],
  "card: shown before the Main ones": [
    "os-versions.js",
    "      ].filter((group) => group.cards.length);",
    "      ].reverse().filter((group) => group.cards.length);",
  ],
  "card: it does not say it goes to all six chips": [
    "os-versions.js",
    "One update goes to all six chips at once.",
    "One update goes to the chips.",
  ],
  "group: nothing reported is not said": [
    "os-versions.js",
    "if (!voiceInstalled) return S.voiceNotReported;",
    'if (!voiceInstalled) return "";',
  ],
  "group: the note left out of the group": [
    "os-versions.js",
    '        el("p", { class: "vnote" }, S.voiceNote),\n',
    "",
  ],
  "card: the Voice OS joins the history": [
    "os-versions.js",
    "? list.map(historyRow(MAIN))",
    "? list.concat(voiceList).map(historyRow(MAIN))",
  ],
  "build: enabled without the Voice file": [
    "os-versions.js",
    "for (const b of voiceButtons) b.disabled = !on;",
    "for (const b of voiceButtons) b.disabled = false;",
  ],
  "build: the Main file enables it": [
    "page-os.js",
    "els.versions.setVoiceReady(!!dsiVoice);",
    "els.versions.setVoiceReady(!!dsi || !!dsiVoice);",
  ],
  "build: sends the Main file": [
    "page-os.js",
    'bridge.send("os_build_voice", version, dsiVoice.hex)',
    'bridge.send("os_build_voice", version, dsi.hex)',
  ],
  "build: sends another command": [
    "page-os.js",
    'bridge.send("os_build_voice", version, dsiVoice.hex)',
    'bridge.send("os_build", version, dsiVoice.hex)',
  ],
  "build: a new list forgets the Voice file": [
    "os-versions.js",
    "      setReady(ready);\n      setVoiceReady(voiceReady);",
    "      setReady(ready);",
  ],
  "confirm: a Voice file gets the Main confirmation": [
    "page-os.js",
    "const voice = !!readyInfo.all_chips;",
    "const voice = false;",
  ],
  "confirm: no mention of the six chips": [
    "page-os.js",
    "The update is broadcast: there is no trying one chip first, and the app cannot read a chip back from here.",
    "The update is checked first.",
  ],
  "confirm: no word on a power cut": [
    "page-os.js",
    "A power cut, a cable pulled or a crash during the first 29 of the 41 pages (about 70 % of the transfer) can leave a Voice chip dead: only a programmer (PICkit) on its ICSP header brings it back, and the MIDI full backup alone restores it only partly. After those pages the software is complete.",
    "Take care.",
  ],
  "confirm: no backup advice": [
    "page-os.js",
    "Make a full machine backup (Backup page, Tempester 1.6.6.0 or later) right before and right after; before Tempester 1.6.8.0, power the machine off and on between the two (one full backup per power-on).",
    "Good luck.",
  ],
  "confirm: no power cycle between the two backups": [
    "page-os.js",
    "; before Tempester 1.6.8.0, power the machine off and on between the two (one full backup per power-on).",
    ".",
  ],
  "card: no power cycle between the two backups": [
    "os-versions.js",
    " (before Tempester 1.6.8.0, power the machine off and on between the two).",
    ".",
  ],
  "confirm: the Main warning of Voice 1.5.1.2 left out": [
    "page-os.js",
    '      ...(m.main_warning ? ["Warning: " + m.main_warning] : []),\n',
    "",
  ],
  "confirm: a Main warning on every Voice version": [
    "page-os.js",
    '...(m.main_warning ? ["Warning: " + m.main_warning] : []),',
    '...["Warning: " + m.main_warning],',
  ],
  "confirm: the Main file gets the Voice confirmation": [
    "page-os.js",
    "const voice = !!readyInfo.all_chips;",
    "const voice = true;",
  ],
  "confirm: sent without waiting for the answer": [
    "page-os.js",
    "    if (ok) bridge.send(\"os_send\");",
    "    bridge.send(\"os_send\");",
  ],
  "end: no backup suggestion": [
    "page-os.js",
    "then make a full machine backup (Backup page): it compares pages 0-40 of every chip with the file just sent and the calibration page with your previous backup, chip by chip.",
    "Done.",
  ],
  "end: a Main update gets the Voice message": [
    "page-os.js",
    "sendingVoice = m.chip === \"Voice\";",
    "sendingVoice = true;",
  ],
  "cancel: the Voice warning is the Main one": [
    "page-os.js",
    "lines: sendingVoice ? S.cancelVoiceLines : S.cancelLines,",
    "lines: S.cancelLines,",
  ],
  "one piece: cancel offered": [
    "page-os.js",
    "els.cancel.disabled = !sending || onePiece;",
    "els.cancel.disabled = !sending;",
  ],
  "one piece: shown as a progress": [
    "page-os.js",
    "(onePiece ? S.sendingOnePiece : S.sending)(",
    "S.sending(",
  ],
  "one piece: the flag kept from the last send": [
    "page-os.js",
    "onePiece = !!m.in_one_piece;",
    "onePiece = onePiece || !!m.in_one_piece;",
  ],
};

let failed = 0;
for (const [name, [file, from, to]] of Object.entries(variants)) {
  const dir = fs.mkdtempSync(path.join(scratch, "os-voice-variant-"));
  for (const f of FILES) fs.copyFileSync(path.join(uiDir, f), path.join(dir, f));
  const source = fs.readFileSync(path.join(dir, file), "utf8");
  const broken = source.replace(from, to);
  if (broken === source) {
    console.log("variant did not change the script: " + name);
    failed += 1;
    continue;
  }
  fs.writeFileSync(path.join(dir, file), broken);
  const run = spawnSync("node", [path.join(__dirname, "os-voice.test.js"), dir], {
    encoding: "utf8",
  });
  if (run.status === 0) {
    console.log("broken page accepted: " + name);
    failed += 1;
  }
  fs.rmSync(dir, { recursive: true, force: true });
}
console.log(Object.keys(variants).length + " variants checked");
process.exit(failed ? 1 : 0);
