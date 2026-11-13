/* Counter-proof of layout-header.js: on copies of the UI with one defect of the 0.1.1 header put back, the test must
   fail AND name the readout the author saw cut. A variant that does not turn it red means the test cannot see that
   defect.

   usage: node layout-header-counter-proof.js <ui dir> <work dir> <repo root> */
"use strict";
const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

const [uiDir, workDir, repo] = process.argv.slice(2, 5);

const VARIANTS = [
  {
    name: "the OS chip shows the whole label (« OS: Tempester 1.6.5.0 »)",
    file: "conn-bar.js",
    from: 'os: (running) => "OS: " + running.version',
    to: 'os: (running) => "OS: " + running.label',
    expect: ["button.chip", "is clipped"],
  },
  {
    name: "the OS chip back to a flex box with centred overflow",
    file: "app.css",
    from: ".connbar .chip { display: block; width: 124px; padding: 0 8px; line-height: 24px; text-align: center;",
    to: ".connbar .chip { display: inline-flex; justify-content: center; width: 60px; padding: 0 8px; line-height: 24px; text-align: center;",
    expect: ["button.chip", "is clipped"],
  },
  {
    name: "the Panel status slot too narrow for its sentences",
    file: "panel.css",
    from: ".panelbar .slot-status { flex: 0 0 100px; width: 100px; }",
    to: ".panelbar .slot-status { flex: 0 0 56px; width: 56px; }",
    expect: ["slot-status", "is clipped"],
  },
  {
    name: "empty status lines of the pad channel section keep their height again",
    file: "compact.css",
    from: ".padchans .statusline:empty { display: none; }",
    to: ".padchans .statusline:empty { display: block; min-height: 22px; }",
    expect: ["pad channel section", "empty between its title row and the grid"],
  },
];

function copyDir(from, to) {
  fs.rmSync(to, { recursive: true, force: true });
  fs.mkdirSync(to, { recursive: true });
  for (const name of fs.readdirSync(from))
    fs.copyFileSync(path.join(from, name), path.join(to, name));
}

let failed = 0;
VARIANTS.forEach((variant, i) => {
  const broken = path.join(workDir, "broken-ui-" + i);
  copyDir(uiDir, broken);
  const target = path.join(broken, variant.file);
  const text = fs.readFileSync(target, "utf8");
  if (!text.includes(variant.from))
    throw new Error(`${variant.file}: the text to break is not there: ${variant.from}`);
  fs.writeFileSync(target, text.replace(variant.from, variant.to));
  const run = spawnSync(
    "node",
    [
      path.join(__dirname, "layout-header.js"),
      broken,
      path.join(workDir, "broken-run-" + i),
      repo,
      "--quick",
    ],
    { encoding: "utf8" },
  );
  const missing = variant.expect.filter((piece) => !run.stdout.includes(piece));
  if (run.status === 1 && missing.length === 0)
    console.log(`  broken (${variant.name}): refused, naming the readout`);
  else {
    failed++;
    console.log(
      `  NOT SEEN (${variant.name}): exit ${run.status}, not named: ${missing.join(" | ")}\n${run.stderr}`,
    );
  }
});
if (failed) process.exit(1);
console.log("every broken variant turns the header test red and names its readout");
