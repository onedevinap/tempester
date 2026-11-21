/* Counter-proof of os-one-piece.test.js: a copy of the page scripts with one behaviour broken must make the checks
   fail. `node os-one-piece-counter-proof.js <ui folder> <scratch folder>` exits 1 when a variant is accepted or does
   not change the script. */
"use strict";
const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

const [uiDir, scratch] = process.argv.slice(2);
const FILES = fs.readdirSync(uiDir).filter((f) => f.endsWith(".js"));

const variants = {
  "a time-out calls the send failed past the cable time": [
    "page-os.js",
    "    els.sendingText.textContent = (m.elapsed > m.seconds ? S.onePieceLonger : S.sendingOnePiece)(m);",
    '    if (m.elapsed > m.seconds) showResult(S.failed({ message: "timed out" }), "bad");\n' +
      "    els.sendingText.textContent = S.sendingOnePiece(m);",
  ],
  "the time-out shortened": [
    "page-os.js",
    "m.elapsed > m.seconds ?",
    "m.elapsed > m.seconds / 10 ?",
  ],
  "late before the least time is over": [
    "page-os.js",
    "m.elapsed > m.seconds ?",
    "m.elapsed >= m.seconds ?",
  ],
  "the elapsed time frozen": [
    "page-os.js",
    "    onePieceTimer = setTimeout(showOnePiece, 1000);\n",
    "    onePieceTimer = 0;\n",
  ],
  "the counting goes on after the end": [
    "page-os.js",
    "      const elapsed = onePieceElapsed();\n      stopOnePiece();",
    "      const elapsed = onePieceElapsed();",
  ],
  "the end line left out": [
    "page-os.js",
    "      els.sendingText.textContent = S.onePieceEnded({ label: onePieceSend.label, elapsed });\n",
    "",
  ],
  "the end progress hides the counted time": [
    "page-os.js",
    "    if (onePieceTimer !== null) return;\n",
    "",
  ],
  "the least time ignored": [
    "page-os.js",
    "seconds: m.seconds, since: Date.now()",
    "seconds: 0, since: Date.now()",
  ],
  "quit sent without the user's answer": [
    "page-os.js",
    '    if (ok) bridge.send("quit");',
    '    bridge.send("quit");',
  ],
  "a close in one piece only toasts (an unclosable window)": [
    "page-os.js",
    "if (!(sending && onePiece)) return toast(S.closeBlocked);",
    "return toast(S.closeBlocked);",
  ],
  "quit offered for a send in pieces": [
    "page-os.js",
    "if (!(sending && onePiece)) return toast(S.closeBlocked);",
    "if (!sending) return toast(S.closeBlocked);",
  ],
  "the Voice quit warning is the Main one": [
    "page-os.js",
    "lines: sendingVoice ? S.quitVoiceLines : S.quitLines,",
    "lines: S.quitLines,",
  ],
  "the Main quit text says nothing of the power cycle": [
    "page-os.js",
    "The Tempest then waits for the rest: power it off and on, and send the file again.",
    "Send the file again.",
  ],
  "the other choice reads Cancel": [
    "page-os.js",
    "      cancelLabel: S.keepSending,\n",
    "",
  ],
  "the dialog ignores its cancel label": [
    "kit.js",
    "cancelLabel || STRINGS.cancel;",
    "STRINGS.cancel;",
  ],
  "no word on the cancel that does not exist": [
    "page-os.js",
    "The file is being sent in one piece and cannot be cancelled. ",
    "",
  ],
};

let failed = 0;
for (const [name, [file, from, to]] of Object.entries(variants)) {
  const dir = fs.mkdtempSync(path.join(scratch, "os-one-piece-variant-"));
  for (const f of FILES) fs.copyFileSync(path.join(uiDir, f), path.join(dir, f));
  const source = fs.readFileSync(path.join(dir, file), "utf8");
  const broken = source.replace(from, to);
  if (broken === source) {
    console.log("variant did not change the script: " + name);
    failed += 1;
    continue;
  }
  fs.writeFileSync(path.join(dir, file), broken);
  const run = spawnSync("node", [path.join(__dirname, "os-one-piece.test.js"), dir], {
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
