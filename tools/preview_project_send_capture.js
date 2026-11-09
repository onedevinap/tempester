#!/usr/bin/env node
/* Screenshots of sending a whole project back (Library page) in Chrome and in WebKit at 1320 px, from the preview made
   by `python3 -I tools/preview_ui.py <out.html> --start library --library-demo` and served over HTTP:

     python3 -m http.server 8123 --bind 127.0.0.1 --directory <folder of out.html>
     node tools/preview_project_send_capture.js http://127.0.0.1:8123/<out.html> <screenshot folder> [<playwright-core folder>]

   The preview stub plays the machine (EXAMPLE data: nothing real is shown or sent). Walks through: the warning when
   the OS may misread Play Lists (project 1), « send without Play Lists », the progress and the cancel, a project
   with no used list (project 2: no warning, said so), a project of 12 beats on an OS with the fix (project 3), the
   offer to fetch back and compare, and the answer. Prints measurements and exits 1 when one fails: the Project card
   keeps its size and its first buttons stay where they are through idle, sending, cancelled and done; the dialogs fit
   the window; nothing scrolls sideways. Needs Playwright (`playwright-core`) with Chrome and WebKit installed; no
   dependency in the project itself. */
"use strict";
const fs = require("fs");
const path = require("path");

const [url, outDir, pwPath] = process.argv.slice(2);
if (!url || !outDir) {
  console.error(
    "usage: node preview_project_send_capture.js <url> <out folder> [<playwright-core folder>]",
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

/* Size of the Project card and left edge of its first two buttons: they must not move between states. */
async function cardGeometry(page) {
  return page.evaluate(() => {
    const card = [...document.querySelectorAll(".lib-card")].find((c) =>
      c.querySelector('[aria-label="Fetch the project from the machine"]'),
    );
    const box = card.getBoundingClientRect();
    const left = (label) =>
      Math.round(
        card.querySelector(`[aria-label="${label}"]`).getBoundingClientRect()
          .left,
      );
    return [
      Math.round(box.width),
      Math.round(box.height),
      left("Fetch the project from the machine"),
      left("Wait for Send Project from the machine's menu"),
    ];
  });
}

async function dialogFits(page) {
  return page.$eval("dialog[open]", (d) => {
    const b = d.getBoundingClientRect();
    return (
      b.left >= 0 &&
      b.right <= innerWidth &&
      b.top >= 0 &&
      b.bottom <= innerHeight
    );
  });
}

async function run(engineName, launcher, options) {
  console.log(engineName);
  const browser = await launcher.launch(options);
  const page = await browser.newPage({ viewport: VIEW });
  const shot = (name) =>
    page.screenshot({ path: path.join(outDir, `${engineName}-${name}.png`) });
  const row = (n) => `.lib-row:nth-child(${n})`;
  const sendIcon = '[aria-label="Send the whole project to the machine..."]';
  await page.goto(url);
  await page.waitForSelector(".lib-row");
  await page.click("text=/^Projects/");
  await page.waitForTimeout(150);
  await shot("1-projects");
  const idle = await cardGeometry(page);

  // project 1: the OS may misread Play Lists
  await page.click(`${row(1)} ${sendIcon}`);
  await page.waitForSelector("dialog[open]");
  await page.waitForTimeout(100);
  await shot("2-warning");
  check(await dialogFits(page), "the warning dialog fits the window");
  const buttons = await page.$$eval("dialog[open] button", (b) =>
    b.map((x) => x.textContent.trim()),
  );
  check(
    buttons.length === 3 &&
      buttons[1].startsWith("Send anyway") &&
      buttons[2].startsWith("Send without Play Lists"),
    "the risky OS offers « Send anyway » and « Send without Play Lists » only: " +
      buttons.join(" | "),
  );
  await page.click('dialog[open] button[value="no_lists"]');
  await page.waitForTimeout(700);
  await shot("3-progress");
  const sending = await cardGeometry(page);
  check(
    JSON.stringify(idle) === JSON.stringify(sending),
    `the card keeps its size and buttons while sending (${idle} / ${sending})`,
  );
  await page.click('.lib-card [aria-label="Cancel"]');
  await page.waitForTimeout(200);
  await shot("4-cancelled");
  const cancelled = await cardGeometry(page);
  check(
    JSON.stringify(idle) === JSON.stringify(cancelled),
    "the card is unchanged after a cancel",
  );

  // project 2: no used list, no warning, said so
  await page.click(`${row(2)} ${sendIcon}`);
  await page.waitForSelector("dialog[open]");
  await shot("5-no-warning");
  check(
    (await page.$eval("dialog[open]", (d) => d.textContent)).includes(
      "No Play List of this project can be misread",
    ),
    "a project with no used list says there is no warning",
  );
  await page.click('dialog[open] button[value="cancel"]');

  // project 3: 12 beats, OS with the fix; send, fetch back and compare
  await page.click(`${row(3)} ${sendIcon}`);
  await page.waitForSelector("dialog[open]");
  await shot("6-partial-beats");
  check(
    (await page.$eval("dialog[open]", (d) => d.textContent)).includes(
      "keep their current content",
    ),
    "a project of fewer than 16 beats says the other beats stay",
  );
  await page.click('dialog[open] button[value="normal"]');
  await page.waitForSelector("dialog[open]", { timeout: 8000 });
  await shot("7-offer-compare");
  const done = await cardGeometry(page);
  check(
    JSON.stringify(idle) === JSON.stringify(done),
    "the card is unchanged when the send is done",
  );
  await page.click('dialog[open] button[value="ok"]');
  await page.waitForSelector("dialog[open]");
  await page.waitForTimeout(100);
  await shot("8-compare-differs");
  check(await dialogFits(page), "the comparison dialog fits the window");
  await page.click('dialog[open] button[value="ok"]');
  check(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
    "no horizontal scroll at 1320 px",
  );
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
