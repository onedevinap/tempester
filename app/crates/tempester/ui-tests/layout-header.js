/* What the top bar and the status readouts say must be readable whole, in real browsers (Chrome and WebKit), at 1100,
   1320 and 1920 px: the connection pill, the OS chip of the connection controls (with the longest release text the
   machine can report) and the sync status of the Panel bar, once connected to the fake machine. A readout is clipped
   when its content is wider than its box (scrollWidth > clientWidth: an ellipsis counts as clipped); the top bar's
   items must also stay inside the window and not overlap. Nothing is sent: the standalone preview has a stub bridge.

   usage: node layout-header.js <ui dir> <work dir> <repo root> [--quick]
   --quick: Chrome only (the counter-proof). layout-header-counter-proof.js runs this test on broken copies. */
"use strict";
const { withPreview, launch, receive } = require("./layout-common");

const [uiDir, workDir, repo] = process.argv.slice(2, 5);
const QUICK = process.argv.includes("--quick");
const WIDTHS = [1100, 1320, 1920];

const RELEASE = {
  version: "1.6.5.0",
  label: "Tempester 1.6.5.0",
  known: true,
};

/* Runs inside the page: what is clipped or out of place in the top bar and in the readouts of `selectors`. */
function findProblems(selectors) {
  const out = [];
  const name = (n) =>
    `${n.id ? "#" + n.id : n.tagName.toLowerCase() + "." + n.className} "${n.textContent.trim()}"`;
  for (const selector of selectors) {
    const nodes = [...document.querySelectorAll(selector)].filter(
      (n) => n.getClientRects().length > 0 && n.textContent.trim(),
    );
    if (!nodes.length) out.push(`nothing found for ${selector}`);
    for (const n of nodes)
      if (n.scrollWidth > n.clientWidth + 0.5)
        out.push(
          `${name(n)} is clipped (${n.scrollWidth} px of text in ${n.clientWidth} px)`,
        );
  }
  const bar = document.getElementById("topbar");
  const items = [...bar.querySelectorAll("#nav button, #conn, #connbar > *")]
    .filter((n) => n.getClientRects().length > 0)
    .map((n) => ({ n, r: n.getBoundingClientRect() }));
  for (const { n, r } of items)
    if (r.left < -0.5 || r.right > window.innerWidth + 0.5)
      out.push(`${name(n)} leaves the window`);
  for (let i = 0; i < items.length; i++)
    for (let j = i + 1; j < items.length; j++) {
      const a = items[i].r,
        b = items[j].r;
      if (
        Math.min(a.right, b.right) - Math.max(a.left, b.left) > 0.5 &&
        Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top) > 0.5
      )
        out.push(`${name(items[i].n)} overlaps ${name(items[j].n)}`);
    }
  return out;
}

const conn = (state) => ({
  event: "conn",
  state,
  port: "Tempest",
  fake: true,
});

/* The readouts in each state they can be in before a sound is synced (the Panel status ends in a sound name, cut by
   design with the whole text as its tooltip): link down, link just up, then the machine's release known. */
async function checkPage(browser, port, hash, width, selectors) {
  const page = await browser.newPage({ viewport: { width, height: 900 } });
  await page.goto(`http://127.0.0.1:${port}/index.html#${hash}`);
  await page.waitForTimeout(400);
  const found = [];
  const look = async (list) =>
    found.push(...(await page.evaluate(findProblems, list)));
  await receive(page, conn("disconnected"));
  await look(selectors);
  await receive(page, conn("connected"));
  await look(selectors);
  await receive(page, {
    event: "identity",
    device: 0x7f,
    version: 0x7f2,
    tempester: true,
  });
  await receive(page, { event: "state", running: RELEASE, channel: 1 });
  await page.waitForTimeout(150);
  await look(selectors.filter((s) => !s.includes("slot-status")));
  await page.close();
  return found;
}

/* The MIDI channel section of the Pads panel (Machine page): no empty band between its title row and the pad grid
   while nothing is to be said (the author wants no empty space; two empty status lines once left ~70 px there). */
const MAX_GAP = 20;
async function checkPadChannelGap(browser, port, width) {
  const page = await browser.newPage({ viewport: { width, height: 900 } });
  await page.goto(`http://127.0.0.1:${port}/index.html#machine`);
  await page.waitForTimeout(400);
  await receive(page, conn("connected"));
  await receive(page, { event: "state", running: RELEASE, channel: 1 });
  await page.locator("button", { hasText: /^\s*Pads\s*$/ }).first().click();
  await page.waitForTimeout(150);
  const gap = await page.evaluate(() => {
    const bar = document.querySelector(".padchans .toolbar");
    const grid = document.querySelector(".padchans .padchan-grid");
    if (!bar || !grid) return null;
    return grid.getBoundingClientRect().top - bar.getBoundingClientRect().bottom;
  });
  await page.close();
  if (gap === null) return ["pad channel section not found"];
  return gap > MAX_GAP
    ? [`pad channel section: ${Math.round(gap)} px empty between its title row and the grid (max ${MAX_GAP})`]
    : [];
}

const TOP = ["#conn", ".connbar .chip"];
const PAGES = [
  ["panel", [...TOP, ".panelbar .slot-status"]],
  ["machine", TOP],
  ["play", TOP],
];

async function run(port) {
  const failures = [];
  const report = [];
  for (const engine of QUICK ? ["chrome"] : ["chrome", "webkit"]) {
    const browser = await launch(engine);
    try {
      for (const [hash, selectors] of PAGES)
        for (const width of WIDTHS) {
          const found = await checkPage(browser, port, hash, width, selectors);
          report.push(`${engine}/${hash}/${width}px: ${found.length} problems`);
          for (const p of found) failures.push(`${engine}/${hash}/${width}px: ${p}`);
        }
      for (const width of WIDTHS) {
        const found = await checkPadChannelGap(browser, port, width);
        report.push(`${engine}/pads/${width}px: ${found.length} problems`);
        for (const p of found) failures.push(`${engine}/pads/${width}px: ${p}`);
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
    console.log([...new Set(failures)].join("\n"));
    console.log(failures.length + " header problems");
    process.exit(1);
  }
  console.log("top bar and status readouts whole at 3 widths");
})().catch((e) => {
  console.error(e);
  process.exit(2);
});
