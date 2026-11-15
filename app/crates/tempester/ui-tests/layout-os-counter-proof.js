/* Counter-proof of layout-os.js: on copies of the UI with one defect put back, the test must fail AND name the defect.
   The first variant is the header the author saw on 14/11/2026 (a fixed 40 px row: a title on three lines ran over
   the summary under it) with the long Voice badge that squeezed the title.

   usage: node layout-os-counter-proof.js <ui dir> <work dir> <repo root> */
"use strict";
const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

const [uiDir, workDir, repo] = process.argv.slice(2, 5);
const HEAD_RULE =
  ".vcard-head { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 10px; min-height: 44px; padding-block: 2px; }";

const VARIANTS = [
  {
    name: "the 0.1.1 header: fixed 40 px row, no wrap, the long Voice badge",
    edits: [
      ["compact.css", HEAD_RULE, ".vcard-head { display: flex; align-items: center; gap: 10px; height: 40px; }"],
      [
        "os-versions.js",
        "        chip: v.date,\n        badge: voiceBadge(v.version),",
        '        chip: "Installed on 6/6 chips (or Tempester Voice 1.5.1.0: same version word)",\n        badge: voiceBadge(v.version),',
      ],
      ["os-versions.js", "title: index === 0 ? S.latest(v.version) : S.previous(v.version),", 'title: "Tempester Voice " + v.version,'],
    ],
    expect: ["overlaps the body"],
  },
  {
    name: "a fixed header height, with a title long enough to wrap",
    edits: [
      ["compact.css", HEAD_RULE, HEAD_RULE.replace("min-height: 44px;", "height: 44px;")],
      ["os-versions.js", "title: index === 0 ? S.latest(v.version) : S.previous(v.version),", 'title: v.name + " (" + v.date + ")",'],
    ],
    expect: ["sticks out of its header"],
  },
  {
    name: "a title cut on one line",
    edits: [
      [
        "compact.css",
        ".vcard-head h4 { margin: 0; flex: 1 1 12em; min-width: 0;",
        ".vcard-head h4 { margin: 0; flex: 1 1 0; min-width: 0; white-space: nowrap; overflow: hidden;",
      ],
      ["os-versions.js", "title: index === 0 ? S.latest(v.version) : S.previous(v.version),", 'title: "Tempester Voice " + v.version + ", the dump stream never leaves the flash",'],
    ],
    expect: ["the title is clipped"],
  },
  {
    name: "a header that never wraps its buttons",
    edits: [["compact.css", HEAD_RULE, HEAD_RULE.replace("flex-wrap: wrap;", "flex-wrap: nowrap;").replace("min-height: 44px;", "min-height: 44px; min-width: 760px;")]],
    expect: ["its header leaves the card"],
  },
];

function copyDir(from, to) {
  fs.rmSync(to, { recursive: true, force: true });
  fs.mkdirSync(to, { recursive: true });
  for (const name of fs.readdirSync(from)) fs.copyFileSync(path.join(from, name), path.join(to, name));
}

let failed = 0;
VARIANTS.forEach((variant, i) => {
  const broken = path.join(workDir, "broken-ui-" + i);
  copyDir(uiDir, broken);
  for (const [file, from, to] of variant.edits) {
    const target = path.join(broken, file);
    const text = fs.readFileSync(target, "utf8");
    if (!text.includes(from)) throw new Error(`${file}: the text to break is not there: ${from}`);
    fs.writeFileSync(target, text.replace(from, () => to));
  }
  const run = spawnSync(
    "node",
    [path.join(__dirname, "layout-os.js"), broken, path.join(workDir, "broken-run-" + i), repo, "--quick"],
    { encoding: "utf8" },
  );
  const missing = variant.expect.filter((piece) => !run.stdout.includes(piece));
  if (run.status === 1 && missing.length === 0) console.log(`  broken (${variant.name}): refused, naming the defect`);
  else {
    failed++;
    console.log(`  NOT SEEN (${variant.name}): exit ${run.status}, not named: ${missing.join(" | ")}\n${run.stdout}${run.stderr}`);
  }
});
console.log(`${VARIANTS.length - failed}/${VARIANTS.length} broken OS card layouts refused`);
process.exit(failed ? 1 : 0);
