/* Overlaps on the face of the Panel page, in real browsers (Chrome and WebKit): no printed text may overlap another
   text, a control (knob with its ring and pointer, key with its LED, pad, ribbon, screen) or a printed line. Both views,
   at 1100, 1320 and 1920 px. A text is the rectangle the browser gives each piece of text (a range over the text node);
   a control is the rectangle of its element, a key's LED is read from its ::after box. Two boxes overlap when they
   share more than OVERLAP_PX on both axes (the limit is 0: touching is allowed, sharing area is not; the epsilon only
   absorbs rounding of scaled boxes).

   usage: node layout-overlap.js <ui dir> <work dir> <repo root> [--list] [--quick]
   --quick: Chrome only, 1320 px only (the counter-proofs). --list prints every overlap and exits 0.
   layout-overlap-counter-proof.js runs this test on deliberately broken copies, which must fail. */
"use strict";
const { withPreview, launch, openPanel } = require("./layout-common");

const [uiDir, workDir, repo] = process.argv.slice(2, 5);
const LIST = process.argv.includes("--list");
const QUICK = process.argv.includes("--quick");
const WIDTHS = QUICK ? [1320] : [1100, 1320, 1920];
const OVERLAP_PX = 0.05;

/* Runs inside the page: every overlap of the view, as readable lines. */
function findOverlaps({ view, epsilon }) {
  const host = document.querySelector(
    view === "machine"
      ? ".panel-machine"
      : "#pages .panel-root:not(.panel-machine)",
  );
  const CONTROLS =
    "button, svg[role=slider], .fen, .ecran, .mvlcd, .ruban, .mvosc, .tled, .prise i";
  /* Text that belongs to something else: the OLED windows and screens (their own content), the counts
     that appear on purpose over their neighbours, the menus. */
  const OWN =
    ".cnt, .ecran, .mvlcd, .fen, .panel-menu, select, script, style";
  const shown = (r) => r.width > 0.5 && r.height > 0.5;
  const box = (r) => ({ x: r.left, y: r.top, w: r.width, h: r.height });
  const controls = [];
  for (const node of host.querySelectorAll(CONTROLS)) {
    if (node.closest("[hidden]")) continue;
    const r = node.getBoundingClientRect();
    if (!shown(r)) continue;
    const name =
      node.getAttribute("aria-label") ||
      node.dataset.l ||
      node.className.baseVal ||
      node.className ||
      node.tagName;
    controls.push({ ...box(r), name: "control " + name, node });
    /* The LED of a key is drawn by ::after, outside the key's own box. */
    const after = getComputedStyle(node, "::after");
    if (
      node.tagName === "BUTTON" &&
      after.content !== "none" &&
      after.position === "absolute"
    ) {
      const k = r.width / node.offsetWidth;
      const left = parseFloat(after.left),
        top = parseFloat(after.top);
      if (Number.isFinite(left) && Number.isFinite(top))
        controls.push({
          x: r.left + left * k,
          y: r.top + top * k,
          w: parseFloat(after.width) * k,
          h: parseFloat(after.height) * k,
          name: "LED of " + name,
          node,
        });
    }
  }
  const texts = [];
  const walker = document.createTreeWalker(host, NodeFilter.SHOW_TEXT);
  for (let t = walker.nextNode(); t; t = walker.nextNode()) {
    const text = t.nodeValue.trim();
    const parent = t.parentElement;
    if (!text || !parent || parent.closest("[hidden]") || parent.closest(OWN))
      continue;
    const style = getComputedStyle(parent);
    if (style.visibility === "hidden" || style.display === "none") continue;
    /* Inside a control the text is the control's own, except a pad's number, which is printed above the pad. */
    const owner = parent.closest(CONTROLS);
    if (owner && !parent.classList.contains("n")) continue;
    const range = document.createRange();
    range.selectNodeContents(t);
    const r = range.getBoundingClientRect();
    if (!shown(r)) continue;
    texts.push({ ...box(r), name: `text "${text}"`, node: parent });
  }
  /* The printed lines and brackets of the Machine view, as segments on screen. */
  const segments = [];
  for (const line of host.querySelectorAll(".mvlines polyline")) {
    const m = line.getScreenCTM();
    const pts = [...line.points].map((p) => ({
      x: m.a * p.x + m.c * p.y + m.e,
      y: m.b * p.x + m.d * p.y + m.f,
    }));
    for (let i = 1; i < pts.length; i++) segments.push([pts[i - 1], pts[i]]);
  }
  const overlap = (a, b) => {
    const ox = Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x);
    const oy = Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y);
    return ox > epsilon && oy > epsilon ? [ox, oy] : null;
  };
  /* Liang-Barsky: does the segment go through the inside of the box? */
  const crosses = ([p, q], b) => {
    let t0 = 0,
      t1 = 1;
    const dx = q.x - p.x,
      dy = q.y - p.y;
    for (const [pp, qq] of [
      [-dx, p.x - b.x],
      [dx, b.x + b.w - p.x],
      [-dy, p.y - b.y],
      [dy, b.y + b.h - p.y],
    ]) {
      if (pp === 0) {
        if (qq <= epsilon) return false;
        continue;
      }
      const r = qq / pp;
      if (pp < 0) {
        if (r > t1) return false;
        if (r > t0) t0 = r;
      } else {
        if (r < t0) return false;
        if (r < t1) t1 = r;
      }
    }
    const length = (t1 - t0) * Math.hypot(dx, dy);
    return t1 > t0 && length > epsilon;
  };
  const out = [];
  const fmt = (v) => v.toFixed(1);
  for (let i = 0; i < texts.length; i++) {
    for (let j = i + 1; j < texts.length; j++) {
      const o = overlap(texts[i], texts[j]);
      if (o)
        out.push(
          `${texts[i].name} overlaps ${texts[j].name} by ${fmt(o[0])} x ${fmt(o[1])} px`,
        );
    }
    for (const c of controls) {
      if (c.node.contains(texts[i].node) || texts[i].node.contains(c.node))
        continue;
      const o = overlap(texts[i], c);
      if (o)
        out.push(
          `${texts[i].name} overlaps ${c.name} by ${fmt(o[0])} x ${fmt(o[1])} px`,
        );
    }
    for (const s of segments)
      if (crosses(s, texts[i]))
        out.push(`${texts[i].name} is crossed by a printed line`);
  }
  return {
    problems: out,
    texts: texts.length,
    controls: controls.length,
    lines: segments.length,
  };
}

async function run(port) {
  const failures = [];
  const report = [];
  for (const engine of QUICK ? ["chrome"] : ["chrome", "webkit"]) {
    const browser = await launch(engine);
    try {
      for (const view of ["full", "machine"]) {
        for (const width of WIDTHS) {
          const page = await openPanel(browser, port, view, width);
          const found = await page.evaluate(findOverlaps, {
            view,
            epsilon: OVERLAP_PX,
          });
          report.push(
            `${engine}/${view}/${width}px: ${found.texts} texts, ${found.controls} controls, ${found.lines} line segments, ${found.problems.length} overlaps`,
          );
          for (const p of found.problems)
            failures.push(`${engine}/${view}/${width}px: ${p}`);
          await page.close();
        }
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
    const unique = [...new Set(failures)];
    console.log(unique.join("\n"));
    console.log(failures.length + " overlaps");
    process.exit(LIST ? 0 : 1);
  }
  console.log("no text overlaps another text, a control or a printed line");
})().catch((e) => {
  console.error(e);
  process.exit(2);
});
