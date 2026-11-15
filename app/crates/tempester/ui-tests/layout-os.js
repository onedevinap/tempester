/* The version cards of the OS page in real browsers (Chrome and WebKit), at the app's narrowest, default and wide
   widths (900, 1100, 1320, 1920 px), with the real manifests (Main and Voice, through the standalone preview) and the
   longest Voice installed line (a word two versions share): every card header holds its own parts (title, chips,
   buttons: none sticks out of the row), the title is never clipped, and the body starts below the header (a long
   title pushes it down, never over it); the Voice group's lead stays inside its box. Nothing is sent: the preview has
   a stub bridge.

   usage: node layout-os.js <ui dir> <work dir> <repo root> [--quick] [--shots <dir>]
   --quick: Chrome only (the counter-proof). --shots: also saves the Voice and Main filters at 1320 and 900 px.
   layout-os-counter-proof.js runs this test on broken copies. */
"use strict";
const path = require("path");
const { withPreview, launch, receive } = require("./layout-common");

const [uiDir, workDir, repo] = process.argv.slice(2, 5);
const QUICK = process.argv.includes("--quick");
const SHOTS = process.argv.includes("--shots") ? process.argv[process.argv.indexOf("--shots") + 1] : null;
const WIDTHS = [900, 1100, 1320, 1920];
const SHARED = ["1.5.1.0", "1.5.1.1"];
const INSTALLED = {
  event: "installed",
  voice_installed: SHARED.map((version) => ({ version, count: 6, of: 6, same_word: SHARED })),
};

/* Runs inside the page: what sticks out of a card header, a clipped title, a body under its header. */
function findProblems() {
  const out = [];
  const shown = (n) => n && n.getClientRects().length > 0;
  const name = (n) => `"${n.textContent.trim().slice(0, 40)}"`;
  const cards = [...document.querySelectorAll(".vcard")].filter(shown);
  if (!cards.length) out.push("no version card shown");
  for (const card of cards) {
    const head = card.querySelector(".vcard-head");
    const title = head.querySelector("h4");
    const h = head.getBoundingClientRect();
    const label = name(title);
    for (const part of [...head.children].filter(shown)) {
      const r = part.getBoundingClientRect();
      if (r.top < h.top - 0.5 || r.bottom > h.bottom + 0.5 || r.left < h.left - 0.5 || r.right > h.right + 0.5)
        out.push(`card ${label}: ${part.tagName.toLowerCase()} ${name(part)} sticks out of its header`);
    }
    if (title.scrollWidth > title.clientWidth + 0.5)
      out.push(`card ${label}: the title is clipped (${title.scrollWidth} px of text in ${title.clientWidth} px)`);
    // The header box and each of its parts (a title spilling out of a fixed-height row) must end above the body.
    const body = card.querySelector(".vcard-body");
    if (shown(body)) {
      const top = body.getBoundingClientRect().top;
      if (top < h.bottom - 0.5) out.push(`card ${label}: the header overlaps the body by ${Math.round(h.bottom - top)} px`);
      for (const part of [...head.children].filter(shown)) {
        const bottom = part.getBoundingClientRect().bottom;
        if (bottom > top + 0.5)
          out.push(`card ${label}: its ${part.tagName.toLowerCase()} overlaps the body by ${Math.round(bottom - top)} px`);
      }
    }
    const c = card.getBoundingClientRect();
    if (h.right > c.right + 0.5 || h.bottom > c.bottom + 0.5) out.push(`card ${label}: its header leaves the card`);
  }
  for (const lead of [...document.querySelectorAll(".vlead")].filter(shown)) {
    const box = lead.getBoundingClientRect();
    for (const p of lead.children) {
      const r = p.getBoundingClientRect();
      if (r.right > box.right + 0.5 || r.bottom > box.bottom + 0.5 || p.scrollWidth > p.clientWidth + 0.5)
        out.push(`Voice lead: ${name(p)} leaves its box`);
    }
  }
  // The chooser only: the top bar's own width at 900 px is another matter (TODO.md).
  const chooser = document.querySelector(".vchooser");
  if (chooser.getBoundingClientRect().right > window.innerWidth + 0.5 || chooser.scrollWidth > chooser.clientWidth + 0.5)
    out.push("the version chooser is wider than the window");
  return out;
}

async function filter(page, label) {
  await page.getByRole("button", { name: label, exact: true }).click();
  await page.waitForTimeout(100);
}

/* The whole chooser of each OS, on a window tall enough to hold it (the page's column scrolls otherwise). */
async function shoot(browser, port, engine, width) {
  if (!SHOTS || engine !== "chrome" || (width !== 1320 && width !== 900)) return;
  const page = await browser.newPage({ viewport: { width, height: 2600 } });
  await page.goto(`http://127.0.0.1:${port}/index.html#os`);
  await page.waitForTimeout(500);
  await receive(page, INSTALLED);
  for (const [label, os] of [
    ["Show the Voice only", "voice"],
    ["Show the Main only", "main"],
  ]) {
    await filter(page, label);
    await page.locator(".vchooser").screenshot({ path: path.join(SHOTS, `os-${os}-${width}.png`) });
  }
  await page.close();
}

async function checkWidth(browser, engine, port, width) {
  const page = await browser.newPage({ viewport: { width, height: 900 } });
  await page.goto(`http://127.0.0.1:${port}/index.html#os`);
  await page.waitForTimeout(500);
  await receive(page, INSTALLED);
  await page.waitForTimeout(150);
  const found = [];
  found.push(...(await page.evaluate(findProblems)));
  // Each OS alone too: the filter changes nothing of a card's layout, but each group is measured on its own.
  for (const label of ["Show the Voice only", "Show the Main only"]) {
    await filter(page, label);
    found.push(...(await page.evaluate(findProblems)));
  }
  await page.close();
  await shoot(browser, port, engine, width);
  return [...new Set(found)];
}

async function run(port) {
  const failures = [];
  const report = [];
  for (const engine of QUICK ? ["chrome"] : ["chrome", "webkit"]) {
    const browser = await launch(engine);
    try {
      for (const width of WIDTHS) {
        const found = await checkWidth(browser, engine, port, width);
        report.push(`${engine}/os/${width}px: ${found.length} problems`);
        for (const p of found) failures.push(`${engine}/os/${width}px: ${p}`);
      }
    } finally {
      await browser.close();
    }
  }
  return { failures, report };
}

(async () => {
  const { failures, report } = await withPreview(uiDir, workDir, repo, run);
  report.forEach((line) => console.log(line));
  if (failures.length) {
    console.log(failures.join("\n"));
    console.log(failures.length + " OS card problems");
    process.exit(1);
  }
  console.log("OS cards: headers hold their parts, titles whole, bodies below, 4 widths");
})().catch((e) => {
  console.error(e);
  process.exit(2);
});
