/* Counter-proof of project-send.test.js: copies of the page scripts and styles with one behaviour broken must make
   the checks fail. `node project-send-counter-proof.js <ui folder> <scratch folder>` exits 1 when a variant is
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
const DIALOGS = "lib-dialogs.js";
const FLOW = "lib-flow.js";
const BROWSER = "lib-browser.js";
const PROJECT = "project-ui.js";

const variants = {
  "the project row does not start the check": [
    BROWSER,
    'if (item.kind === "projects") flow.projectCheck(item.id, null);',
    "void flow;",
  ],
  "a version of an auto backup is checked without its number": [
    BROWSER,
    "() => flow.projectCheck(item.id, v.index)",
    "() => flow.projectCheck(item.id, null)",
  ],
  "the project row send stays disabled": [
    BROWSER,
    "sendButton.disabled = noSend;",
    "sendButton.disabled = true;",
  ],
  "a project can be opened in a card": [
    BROWSER,
    "openButton.disabled = noOpen;",
    "openButton.disabled = item.slot;",
  ],
  "the confirmed send is not sent": [FLOW, '"lib_project_send"', '"x"'],
  "the version is dropped from the send": [
    FLOW,
    'plan.version === null ? "-" : plan.version',
    '"-"',
  ],
  "the chosen mode is replaced by a plain send": [
    FLOW,
    "plan.token,\n        mode,",
    'plan.token,\n        "normal",',
  ],
  "a declined confirmation still sends": [FLOW, "if (mode)\n", "if (true)\n"],
  "a risky OS is offered a plain send": [
    DIALOGS,
    "if (risky) {",
    "if (false) {",
  ],
  "the warning box is empty": [
    DIALOGS,
    "paragraphs([...plan.warning, ...plan.without_lists])",
    "[]",
  ],
  "an identical project is asked about": [
    DIALOGS,
    "      return null;\n    }\n    const risky",
    '      return "normal";\n    }\n    const risky',
  ],
  "a silent machine is sent to without the explicit label": [
    DIALOGS,
    'label: silent ? "Send without a backup" : "Send",',
    'label: "Send",',
  ],
  "the no-warning reason is not said": [
    DIALOGS,
    "if (PROJECT_GUARD_NOTE[plan.guard]) lines.push(PROJECT_GUARD_NOTE[plan.guard]);",
    "",
  ],
  "the differences are not listed": [DIALOGS, "        ...shown,\n", ""],
  "a long list of differences is not cut": [
    DIALOGS,
    "const more = (result.differences || []).length - shown.length;",
    "const more = 0;",
  ],
  "cancel stops the receive instead of the send": [
    PROJECT,
    'sending ? "lib_project_cancel" : "project_cancel"',
    '"project_cancel"',
  ],
  "a cancelled send can be verified": [
    PROJECT,
    'if (m.state === "done") verifiable = true;',
    "verifiable = true;",
  ],
  "the fetch-back is never started": [
    PROJECT,
    'if (ok) bridge.send("lib_project_verify");',
    "void ok;",
  ],
  "the progress does not name the beat": [
    PROJECT,
    '"Sending the project: beat " + m.message + " of " + m.beats + "."',
    '"Sending the project."',
  ],
  "the page says again that the send waits for the fix": [
    PROJECT,
    'in a folder named after the project.",',
    'in a folder named after the project. Sending a project back is not available yet.",',
  ],
  "the fake defect switch sends nothing": [
    PROJECT,
    'bridge.send("fake_project_defect", defect ? 0 : 1)',
    'bridge.send("x")',
  ],
  "the warning has no box": ["library.css", ".lib-warning {", ".lib-warnx {"],
};

let failed = 0;
for (const [name, [file, from, to]] of Object.entries(variants)) {
  const dir = fs.mkdtempSync(path.join(scratch, "project-send-variant-"));
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
    [path.join(__dirname, "project-send.test.js"), dir],
    { encoding: "utf8" },
  );
  if (run.status === 0) {
    console.log("broken page accepted: " + name);
    failed += 1;
  }
}
console.log(Object.keys(variants).length + " variants checked");
process.exit(failed ? 1 : 0);
