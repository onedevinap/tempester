#!/usr/bin/env node
/* Screenshots of the OS update page (SHA-256 reference flows) in Chrome and in WebKit at 1320 px, from the preview made
   by `python3 -I tools/preview_ui.py <out.html> --start os` and served over HTTP (file:// is blocked):

     python3 -m http.server 8123 --bind 127.0.0.1 --directory <folder of out.html>
     node tools/preview_os_capture.js http://127.0.0.1:8123/<out.html> <screenshot folder> [<playwright-core folder>]

   The page is driven by made-up events (a built image, an unknown file, a refused build, the read-back of the machine:
   listening, mismatch, constant, match); nothing real is shown or sent. It prints, per browser, the steps at which a
   panel or an evidence row moved or changed size (there must be none) and exits 1 when one did.
   Needs Playwright (`playwright-core`) with Chrome and WebKit installed; no dependency in the project itself. */
"use strict";
const fs = require("fs");
const [url, outDir, pwPath] = process.argv.slice(2);
if (!url || !outDir) {
  console.error("usage: node preview_os_capture.js <url> <out folder> [<playwright-core folder>]");
  process.exit(2);
}
const { chromium, webkit } = require(pwPath || "playwright-core");
fs.mkdirSync(outDir, { recursive: true });
const out = outDir;
const IMAGE = "4fa3c1b2d5e6" + "7a".repeat(26);
const SYX = "9b2e77aa10cd" + "3c".repeat(26);
const OTHER = "e0f1a2b3c4d5" + "9d".repeat(26);
const rec = (v) => ({
  label: "Tempester 1.6.1.1",
  version: "1.6.1.1",
  at: "2026-11-08_120000Z",
  sha256_image: IMAGE,
  verified_at: v,
});
const ready = (x) => ({
  event: "os_ready",
  label: "Tempester 1.6.1.1",
  source: "built",
  chip: "Main",
  id: 0x71,
  blocks: 512,
  checksum: 0x1a2b,
  size: 600000,
  seconds: 192,
  known: true,
  version: "1.6.1.1",
  sha256_image: IMAGE,
  sha256_syx: SYX,
  ...x,
});
const base = {
  event: "os_verify",
  label: "Tempester 1.6.1.1",
  version: "1.6.1.1",
  expected: IMAGE,
};
const steps = [
  ["00-empty", []],
  ["01-built-match", [ready()]],
  [
    "02-unknown-file",
    [
      ready({
        source: "file",
        label: "my build.syx",
        known: false,
        version: null,
      }),
    ],
  ],
  [
    "03-refused",
    [
      {
        event: "error",
        during: "os_build",
        code: "invalid",
        message:
          "the built image does not match the reference of Tempester 1.6.1.1 (SHA-256 expected " +
          IMAGE +
          ", built " +
          OTHER +
          "): nothing will be sent",
      },
    ],
  ],
  [
    "04-installed-listening",
    [
      { event: "state", channel: 1, installed: rec(null) },
      { ...base, state: "listening" },
    ],
  ],
  [
    "05-mismatch",
    [
      {
        ...base,
        state: "mismatch",
        found: OTHER,
        dir: "/Users/example/Tempester/backups/2026-11-08_130000Z",
      },
    ],
  ],
  ["06-constant", [{ ...base, state: "constant", byte: 255, found: OTHER }]],
  [
    "07-match-verified",
    [
      { event: "installed", installed: rec("2026-11-08_130000Z") },
      {
        ...base,
        state: "match",
        found: IMAGE,
        dir: "/Users/example/Tempester/backups/2026-11-08_130000Z",
      },
    ],
  ],
];
async function run(name, type, opts) {
  const browser = await type.launch(opts);
  const page = await browser.newPage({
    viewport: { width: 1320, height: 900 },
  });
  await page.goto(url.replace(/#.*$/, "") + "#os");
  await page.waitForTimeout(500);
  const boxes = async () =>
    page.evaluate(() =>
      [
        ...document.querySelectorAll(
          "#page-os .panel, #page-os .evidence, #page-os .statusbox",
        ),
      ]
        .map((n) => {
          const r = n.getBoundingClientRect();
          return [
            Math.round(r.x),
            Math.round(r.y),
            Math.round(r.width),
            Math.round(r.height),
          ].join(",");
        })
        .join("|"),
    );
  let reference = null;
  let shifted = [];
  for (const [label, events] of steps) {
    for (const e of events)
      await page.evaluate((m) => window.Tempester.bridge.receive(m), e);
    await page.waitForTimeout(150);
    await page.evaluate(() =>
      document.getElementById("toast").classList.remove("visible"),
    );
    if (label === "01-built-match") {
      await page.evaluate(() =>
        document.querySelector("#page-os .evidence .info-badge").click(),
      );
      await page.waitForTimeout(100);
      await page.screenshot({
        path: `${out}/${name}-01b-built-match-full-hash.png`,
      });
      await page.keyboard.press("Escape");
    }
    const b = await boxes();
    if (reference === null) reference = b;
    else if (b !== reference)
      shifted.push(label + " " + b + " vs " + reference);
    await page.screenshot({ path: `${out}/${name}-${label}.png` });
  }
  await page.evaluate(() =>
    [...document.querySelectorAll("nav button, nav a, [role=tab]")]
      .find((n) => n.textContent.trim() === "Machine")
      .click(),
  );
  await page.waitForTimeout(300);
  await page.screenshot({ path: `${out}/${name}-08-machine-page.png` });
  const overflow = await page.evaluate(() => {
    const p = document.querySelector("#page-os");
    return [
      document.documentElement.scrollHeight,
      window.innerHeight,
      p ? p.scrollHeight : null,
    ];
  });
  console.log(
    name,
    "layout shifted at:",
    JSON.stringify(shifted),
    "scroll/inner/page",
    JSON.stringify(overflow),
  );
  await browser.close();
  return shifted.length;
}
(async () => {
  let moved = 0;
  moved += await run("chrome", chromium, { channel: "chrome" });
  moved += await run("webkit", webkit, {});
  process.exit(moved ? 1 : 0);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
