#!/usr/bin/env node
/* Screenshots of the Library page in Chrome and in WebKit at 1320 px, from the preview made by
   `python3 -I tools/preview_ui.py <out.html> --start library --library-demo` and served over HTTP:

     python3 -m http.server 8123 --bind 127.0.0.1 --directory <folder of out.html>
     node tools/preview_capture.js http://127.0.0.1:8123/<out.html> <screenshot folder> [<playwright-core folder>]

   The example library (320 sounds, 64 beats, 3 projects, a history, a project review, a look at the machine before
   a send) is made up by the stub of preview_ui.py: nothing real is shown or sent. Besides the pictures it prints
   measurements: row heights stay the same when the selection or the search changes, nothing scrolls sideways, the
   smallest text size in the list. Exit code 1 when one of these checks fails.
   Needs Playwright (`playwright-core`) with Chrome and WebKit installed; no dependency in the project itself. */
"use strict";
const fs = require("fs");
const path = require("path");

const [url, outDir, pwPath] = process.argv.slice(2);
if (!url || !outDir) {
  console.error(
    "usage: node preview_capture.js <url> <out folder> [<playwright-core folder>]",
  );
  process.exit(2);
}
const playwright = require(pwPath || "playwright-core");
fs.mkdirSync(outDir, { recursive: true });

const VIEW = { width: 1320, height: 900 };
let failures = 0;
const check = (ok, text) => {
  console.log((ok ? "  ok: " : "  FAILED: ") + text);
  if (!ok) failures += 1;
};

async function rowBoxes(page) {
  return page.$$eval(".lib-row", (rows) =>
    rows.slice(0, 12).map((r) => {
      const b = r.getBoundingClientRect();
      return [
        Math.round(b.height),
        Math.round(b.width),
        ...Array.from(r.children, (c) =>
          Math.round(c.getBoundingClientRect().left),
        ),
      ];
    }),
  );
}

async function run(engineName, launcher, options) {
  console.log(engineName);
  const browser = await launcher.launch(options);
  const page = await browser.newPage({ viewport: VIEW });
  const shot = (name) =>
    page.screenshot({ path: path.join(outDir, `${engineName}-${name}.png`) });
  await page.goto(url);
  await page.waitForSelector(".lib-row");
  await page.waitForTimeout(200);
  await shot("1-overview");

  const before = await rowBoxes(page);
  check(
    before.every((r) => r[0] === 44),
    "every row is 44 px high",
  );
  check(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
    "no horizontal scroll at 1320 px",
  );
  const smallest = await page.$$eval(".lib-row *", (nodes) =>
    Math.min(
      ...nodes
        .filter((n) => n.children.length === 0 && n.textContent.trim())
        .map((n) => parseFloat(getComputedStyle(n).fontSize)),
    ),
  );
  check(smallest >= 15, `smallest text in the list is ${smallest}px`);
  const tiny = await page.$$eval(
    ".ibtn",
    (nodes) =>
      nodes.filter(
        (n) => n.offsetParent && (n.offsetWidth < 36 || n.offsetHeight < 36),
      ).length,
  );
  check(
    tiny === 0,
    `every visible icon button is at least 36 px (${tiny} smaller)`,
  );

  // selection and search do not move the cells
  await page.click(".lib-row:nth-child(3)");
  await page.keyboard.press("ArrowDown");
  const afterSelect = await rowBoxes(page);
  check(
    JSON.stringify(before) === JSON.stringify(afterSelect),
    "selecting rows moves nothing",
  );
  await page.fill("#lib-search", "kick");
  await page.waitForTimeout(100);
  const rowCount = await page.$$eval(".lib-row", (r) => r.length);
  await shot("2-search-kick");
  const searched = await rowBoxes(page);
  check(
    searched.every(
      (r) =>
        r[0] === 44 &&
        JSON.stringify(r.slice(2)) === JSON.stringify(before[0].slice(2)),
    ),
    `search (${rowCount} rows): same height and same column positions`,
  );

  // filters
  await page.fill("#lib-search", "");
  await page.click('[aria-label="Filters"]');
  await page.selectOption('[aria-label="Sound Type"]', "Snares");
  await page.click("text=Favourites only");
  await page.waitForTimeout(100);
  await shot("3-filters");
  await page.click('[aria-label="Clear all filters"]');
  await page.click('[aria-label="Filters"]');

  // details with history and compare
  await page.fill("#lib-search", "Kick 100 v1");
  await page.click(".lib-row:nth-child(1)");
  await page.waitForTimeout(150);
  await page.click('[aria-label="Compare with the current version"] >> nth=0');
  await page.waitForTimeout(150);
  await shot("4-details-history");
  await page.fill("#lib-search", "");

  // multi-select
  await page.click(".lib-row:nth-child(4)");
  await page.click(".lib-row:nth-child(7)", { modifiers: ["Shift"] });
  await page.waitForTimeout(100);
  await shot("5-multi-select");

  // the review of a project
  await page.click('[aria-label="Fetch the project from the machine"]');
  await page.waitForSelector("dialog[open]");
  await page.click('[aria-label^="Show the"] >> nth=2');
  await page.waitForTimeout(100);
  await shot("6-review");
  await page.click('dialog[open] button[value="cancel"]');

  // what a send would replace
  await page.click(
    '.lib-row:nth-child(5) [aria-label="Send to the machine..."]',
  );
  await page.waitForSelector("dialog[open]");
  await shot("7-send");
  await page.click('dialog[open] button[value="cancel"]');

  // the trash and the messages
  await page.click('[aria-label="Trash"]');
  await page.waitForSelector("dialog[open]");
  await shot("8-trash");
  await page.click('dialog[open] button[value="cancel"]');
  await page.click(".lib-row:nth-child(6)");
  await page.click('[aria-label="Rename..."]');
  await page.waitForSelector("dialog[open]");
  await page.fill(".lib-prompt", "Kick 100 v2 renamed");
  await page.waitForTimeout(100);
  await shot("9-rename");
  await page.fill(".lib-prompt", "A name that is much too long");
  await page.waitForTimeout(100);
  await shot("10-rename-refused");
  check(
    await page.$eval('dialog[open] button[value="ok"]', (b) => b.disabled),
    "a name that is too long cannot be confirmed",
  );
  await page.click('dialog[open] button[value="cancel"]');
  await browser.close();
}

(async () => {
  await run("chrome", playwright.chromium, { channel: "chrome" });
  await run("webkit", playwright.webkit, {});
  console.log(failures ? failures + " check(s) failed" : "all checks passed");
  process.exit(failures ? 1 : 0);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
