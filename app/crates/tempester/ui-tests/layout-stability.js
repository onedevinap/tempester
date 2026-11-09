/* Layout stability of the Panel page, in real browsers (Chrome and WebKit): no control may move or change size
   whatever the page shows. The page is the standalone preview (tools/preview_ui.py, a stub bridge, nothing is sent),
   served on 127.0.0.1 by `python3 -m http.server`; every control of both views is measured, then many interactions
   are driven (turning the knobs, BPM across 2 and 3 digits, parameter reports, LED events, Pad Function mode changes,
   a sound with a long and a short name, counts appearing and expiring), and every control is measured again.
   Any movement or resize above 0.5 px fails. It also checks the rows (alignment), the transport group as on the
   machine (transportRow) and that the LED events light the LEDs of REC and PLAY (transportLeds).

   usage: node layout-stability.js <ui dir> <work dir> <repo root> [--shifts] [--quick]
   env:   PLAYWRIGHT_CORE (path of the playwright-core package; default: the npx cache), CHROME (Chrome binary)
          PLAYWRIGHT_BROWSERS_PATH (where WebKit is installed, when HOME is not the real one)
   --quick: Chrome only, fewer knobs (the counter-proofs).
   --shifts prints the measured shifts and exits 0 (to compare an old UI folder with a new one); layout-counter-proof.js
   runs this test on deliberately broken copies, which must fail. */
"use strict";
const { withPreview, launch, openPanel, receive } = require("./layout-common");

const [uiDir, workDir, repo] = process.argv.slice(2, 5);
const PRINT_ONLY = process.argv.includes("--shifts");
const QUICK = process.argv.includes("--quick");
const TOLERANCE = 0.5;

/* What counts as a control: everything the user can touch or read as part of the face. Captions that appear on
   purpose (value, count) hang over their neighbours and are measured as part of their knob's box. */
const MEASURED =
  "button, svg[role=slider], .pot, .pad, .ruban, .ecran, .mvlcd, .lcd, .prise, .fen, .sec, .mvzone, .mvt, .mvtitle, .tled, .panelbar > :not(.spacer), .cadre";

/* Every measured element, keyed by its place in the document (the DOM is static): [x, y, w, h]. */
async function measure(page) {
  return page.evaluate((selector) => {
    const out = {};
    document
      .querySelectorAll("#pages " + selector.split(", ").join(", #pages "))
      .forEach((node, i) => {
          const r = node.getBoundingClientRect();
        /* The key must not depend on what the element shows: a count changes the text of its box. */
        const where = node.getAttribute("aria-label") || node.dataset.l || "";
        out[
          i +
            " " +
            node.tagName.toLowerCase() +
            "." +
            String(
              node.className && node.className.baseVal === undefined
                ? node.className
                : "",
            ).split(" ")[0] +
            " " +
            where
        ] = [r.x, r.y + scrollY, r.width, r.height];
      });
    return out;
  }, MEASURED);
}

function compare(before, after) {
  const moved = [];
  for (const [name, a] of Object.entries(before)) {
    const b = after[name];
    if (!b) continue; /* the DOM is static: every key is there */
    const shift = Math.max(...a.map((v, i) => Math.abs(v - b[i])));
    if (shift > TOLERANCE)
      moved.push([name, +shift.toFixed(2), a.map(Math.round), b.map(Math.round)]);
  }
  return moved;
}

async function drag(page, selector, index, dy) {
  const box = await (await page.$$(selector))[index].boundingBox();
  const x = box.x + box.width / 2,
    y = box.y + box.height / 2;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x, y + dy, { steps: 4 });
  await page.mouse.up();
}


/* Rows line up: the knobs of a row share one centre line, the keys of a row one top line, the main labels of a row one
   centre line (a row = elements whose lines are within a few px of each other). The machine aligns them, so does the
   drawing: a 1 px spread (on screen) is the most that is accepted. Returns the rows that do not line up. */
const ROW_SPREAD = 1;
async function alignment(page, view) {
  return page.evaluate(
    ({ view, limit }) => {
      const host = document.querySelector(view === "machine" ? ".panel-machine" : "#pages .panel-root:not(.panel-machine)");
      const visible = (n) => {
        const r = n.getBoundingClientRect();
        return r.width > 0 && r.height > 0 && !n.closest("[hidden]");
      };
      const kinds = {
        knobs: [host.querySelectorAll("svg[role=slider]"), (r) => r.top + r.height / 2, 12],
        keys: [host.querySelectorAll("button[data-key]"), (r) => r.top, view === "machine" ? 8 : 40],
        labels: [host.querySelectorAll(view === "machine" ? ".mvt.m, .mvt.mg" : ".pot .l1"), (r) => r.top + r.height / 2, 6],
      };
      const problems = [];
      for (const [kind, [nodes, line, gap]] of Object.entries(kinds)) {
        /* Rows are looked for inside one section of the Full view (a section is one box of the drawing); the
           Machine view is one face. */
        const sections = new Map();
        const items = [...nodes].filter(visible).map((n) => {
          const r = n.getBoundingClientRect();
          const box = view === "machine" ? host : n.closest(".sec") || host;
          if (!sections.has(box)) sections.set(box, sections.size);
          return { y: line(r), section: sections.get(box), name: n.getAttribute("aria-label") || n.textContent.trim().slice(0, 14) };
        });
        items.sort((a, b) => a.section - b.section || a.y - b.y);
        let row = [];
        const close = () => {
          if (row.length > 1) {
            const spread = row[row.length - 1].y - row[0].y;
            if (spread > limit) problems.push(`${kind}: a row of ${row.length} spreads ${spread.toFixed(1)}px (${row[0].name} .. ${row[row.length - 1].name})`);
          }
          row = [];
        };
        for (const it of items) {
          if (row.length && (it.section !== row[0].section || it.y - row[row.length - 1].y > gap)) close();
          row.push(it);
        }
        close();
      }
      /* Keys stacked in a column (same width, the next one less than 90 px under) share their left edge, so their LED
         and their label are on one line too. */
      const keys = [...host.querySelectorAll("button[data-key]")].filter(visible).map((n) => {
        const r = n.getBoundingClientRect();
        /* the section of a key: its frame in the Machine view (the smallest one around it), its box in the Full view */
        const cx = r.left + r.width / 2, cy = r.top + r.height / 2;
        const frames = [...host.querySelectorAll(".mvzone")].filter((z) => {
          const q = z.getBoundingClientRect();
          return cx >= q.left && cx <= q.right && cy >= q.top && cy <= q.bottom;
        });
        frames.sort((p, q) => p.offsetWidth * p.offsetHeight - q.offsetWidth * q.offsetHeight);
        const zone = view === "machine" ? frames[0] || null : n.closest(".sec");
        return { left: r.left, top: r.top, w: r.width, zone, name: n.getAttribute("aria-label") };
      });
      keys.sort((a, b) => a.left - b.left);
      const used = new Set();
      for (const k of keys) {
        if (used.has(k)) continue;
        const column = [k];
        used.add(k);
        for (const o of keys)
          if (!used.has(o) && o.zone === k.zone && Math.abs(o.w - k.w) <= 1.5 && Math.abs(o.left - k.left) <= 12 &&
              column.some((c) => Math.abs(c.top - o.top) < 90)) {
            column.push(o);
            used.add(o);
          }
        const lefts = column.map((c) => c.left);
        const spread = Math.max(...lefts) - Math.min(...lefts);
        if (column.length > 1 && spread > limit)
          problems.push(`key column: ${column.length} keys spread ${spread.toFixed(1)}px (${column[0].name} .. ${column[column.length - 1].name})`);
      }
      return problems;
    },
    { view, limit: ROW_SPREAD },
  );
}

/* The transport as on the machine (author's correction, 2026-11-09), in both views: Rec, Stop and Play one size, on
   one row, evenly spaced; « (Rew) and » (FF) one size, at most SMALL_KEY_RATIO as wide as Rec and taller than wide,
   their top on the line of Rec, with the « |« » print between them; the LED of REC and the LED of PLAY right of their
   title (on its line, within TITLE_LED_GAP title heights of it, touching no text). */
const SMALL_KEY_RATIO = 0.5;
const TITLE_LED_GAP = 1;
async function transportRow(page, view) {
  return page.evaluate(
    ({ view, ratio, ledGap }) => {
      const host = document.querySelector(view === "machine" ? ".panel-machine" : "#pages .panel-root:not(.panel-machine)");
      const names = ["Rew", "FF", "Rec", "Stop", "Play"];
      const boxes = names.map((n) => host.querySelector(`button[data-key="${n}"]`)?.getBoundingClientRect());
      if (boxes.some((b) => !b)) return ["transport: a key of Rew / FF / Rec / Stop / Play is missing"];
      const [rew, ff, rec] = boxes;
      const big = boxes.slice(2);
      const problems = [];
      const spread = (values) => Math.max(...values) - Math.min(...values);
      if (spread(big.map((b) => b.width)) > 0.5) problems.push("transport: Rec, Stop and Play are not one width");
      if (spread(big.map((b) => b.height)) > 0.5) problems.push("transport: Rec, Stop and Play are not one height");
      if (spread(big.map((b) => b.top)) > 0.5) problems.push("transport: Rec, Stop and Play are not on one row");
      const gaps = big.slice(1).map((b, i) => b.left - big[i].right);
      if (spread(gaps) > 0.5) problems.push("transport: Rec, Stop and Play are not evenly spaced");
      if (Math.abs(rew.width - ff.width) > 0.5 || Math.abs(rew.height - ff.height) > 0.5)
        problems.push("transport: « and » are not one size");
      if (rew.width > ratio * rec.width)
        problems.push(`transport: « and » are not narrow keys (${(rew.width / rec.width).toFixed(2)} of the width of Rec)`);
      if (rew.height <= rew.width) problems.push("transport: « and » are not taller than wide");
      if (spread([rew.top, ff.top, rec.top]) > 0.5) problems.push("transport: « and » are not on the top line of Rec");
      if (!(rew.right < ff.left && ff.right < rec.left)) problems.push("transport: the keys are not in the order « » Rec");
      const mark = host.querySelector(".rwmark");
      const m = mark && mark.getBoundingClientRect();
      if (!m || !mark.textContent.includes("|«")) problems.push("transport: no « |« » print between « and »");
      else if (m.left < rew.right || m.right > ff.left || m.top < rew.top - 1 || m.bottom > rew.bottom + 1)
        problems.push("transport: the « |« » print is not between « and »");
      /* The title of a LED: the text node of that exact text closest to it. */
      const titles = (text) => {
        const found = [];
        const walker = document.createTreeWalker(host, NodeFilter.SHOW_TEXT);
        for (let t = walker.nextNode(); t; t = walker.nextNode())
          if (t.nodeValue.trim() === text) {
            const range = document.createRange();
            range.selectNodeContents(t);
            found.push(range.getBoundingClientRect());
          }
        return found;
      };
      for (const [colour, title] of [
        ["rec", view === "machine" ? "REC" : "Rec"],
        ["play", view === "machine" ? "PLAY" : "Play"],
      ]) {
        const led = host.querySelector(`.tled.${colour}`);
        const l = led && led.getBoundingClientRect();
        if (!l || l.width < 1 || l.height < 1) {
          problems.push(`transport: no LED right of the ${title} title`);
          continue;
        }
        const near = titles(title).sort(
          (a, b) => Math.hypot(a.right - l.left, a.top - l.top) - Math.hypot(b.right - l.left, b.top - l.top),
        )[0];
        if (!near) {
          problems.push(`transport: no ${title} title`);
          continue;
        }
        const gap = l.left - near.right;
        const middle = l.top + l.height / 2;
        if (gap < 0) problems.push(`transport: the ${title} LED overlaps its title by ${(-gap).toFixed(1)} px`);
        else if (gap > ledGap * near.height) problems.push(`transport: the ${title} LED is ${gap.toFixed(1)} px from its title, not beside it`);
        if (middle < near.top || middle > near.bottom) problems.push(`transport: the ${title} LED is not on the line of its title`);
      }
      return problems;
    },
    { view, ratio: SMALL_KEY_RATIO, ledGap: TITLE_LED_GAP },
  );
}

/* The machine's LED events light the LEDs of REC (0x23, red) and PLAY (0x24, white), and only those: 0x14, which the
   firmware flashes with 0x23 while armed, lights neither. Each step: the changes sent, then [REC lit, PLAY lit]. */
async function transportLeds(page, view) {
  const host = view === "machine" ? ".panel-machine" : "#pages .panel-root:not(.panel-machine)";
  const read = () =>
    page.evaluate(
      (h) =>
        ["rec", "play"].map((colour) => {
          const n = document.querySelector(`${h} .tled.${colour}`);
          if (!n) return null;
          return {
            on: n.getAttribute("aria-pressed") === "true",
            rgb: getComputedStyle(n).backgroundColor.match(/[\d.]+/g).slice(0, 3).map(Number),
          };
        }),
      host,
    );
  const looks = {
    rec: ([r, g, b]) => r > 200 && g < 150 && b < 150,
    play: ([r, g, b]) => Math.min(r, g, b) > 220,
    off: (rgb) => Math.max(...rgb) < 70,
  };
  const problems = [];
  await receive(page, { event: "remote", available: true });
  for (const [changes, want] of [
    [[[0x23, 1]], [true, false]],
    [[[0x23, 0], [0x24, 1]], [false, true]],
    [[[0x24, 0], [0x14, 1]], [false, false]],
    [[[0x14, 0], [0x23, 1], [0x24, 1]], [true, true]],
    [[[0x23, 0], [0x24, 0]], [false, false]],
  ]) {
    await receive(page, { event: "leds", changes });
    const seen = await read();
    ["rec", "play"].forEach((colour, i) => {
      const s = seen[i];
      const step = JSON.stringify(changes);
      if (!s) problems.push(`LED events: no ${colour} LED`);
      else if (s.on !== want[i]) problems.push(`LED events ${step}: the ${colour} LED is ${s.on ? "lit" : "dark"}`);
      else if (!(want[i] ? looks[colour] : looks.off)(s.rgb))
        problems.push(`LED events ${step}: the ${colour} LED shows rgb(${s.rgb.join(", ")})`);
    });
  }
  await receive(page, { event: "remote", available: false });
  return [...new Set(problems)];
}

/* The bar of the page is a compact toolbar of icon buttons: about half the window at most, every button named (name
   and tooltip), the same width whatever the readouts say. */
async function toolbar(page) {
  return page.evaluate(() => {
    const bar = document.querySelector("#pages .panelbar");
    const problems = [];
    const width = bar.getBoundingClientRect().width;
    if (width > 0.52 * window.innerWidth) problems.push(`toolbar: ${Math.round(width)} px is more than half of the ${window.innerWidth} px window`);
    for (const name of ["Sync from machine", "Full view", "Machine view", "Zoom out (Cmd/Ctrl -)", "Zoom in (Cmd/Ctrl +)", "Info"]) {
      const b = [...bar.querySelectorAll("button")].find((n) => n.getAttribute("aria-label") === name);
      if (!b) problems.push(`toolbar: no button named ${name}`);
      else if (b.getAttribute("data-tip") !== name) problems.push(`toolbar: ${name} has no tooltip`);
    }
    return problems;
  });
}

/* The interactions of one view; `check(label)` measures and compares after each batch. */
async function interactions(page, view, check, coverage) {
  await receive(page, { event: "remote", available: true });
  await check("remote answers");
  /* Measured after each value: a label that follows its value only moves something while its digit count differs from
     the baseline's. */
  for (const bpm of [99, 120, 300, 20, 100]) {
    await page.evaluate((v) => window.Tempester.panel.setLocal("bpm", v), bpm);
    await check("BPM " + bpm);
  }
  const values = window_values();
  await receive(page, {
    event: "sound",
    name: "A much too long sound name to fit in any box of the screen, ever",
    values,
  });
  await check("sound with a long name");
  await receive(page, {
    event: "sound",
    name: "K",
    values: values.map((v) => (v ? 0 : v)),
  });
  await check("sound with a short name");
  for (const [number, value] of [
    [34, 0],
    [34, 127],
    [35, 99],
    [33, 3],
    [41, 100],
    [46, 8],
    [28, 64],
    [156, 1],
  ])
    await receive(page, { event: "nrpn", number, value });
  await check("parameter reports");
  await performance(page, view, check, coverage);
  for (const led of Array.from({ length: 0x3c }, (_, i) => i))
    await receive(page, { event: "leds", changes: [[led, 1]] });
  await check("every LED on");
  for (const led of Array.from({ length: 0x3c }, (_, i) => i))
    await receive(page, { event: "leds", changes: [[led, 0]] });
  await check("every LED off");
  for (const lit of [[0x10], [0x18], [0x10, 0x11], [0x19], [0x11]]) {
    await receive(page, {
      event: "leds",
      changes: [
        [0x10, 0],
        [0x11, 0],
        [0x18, 0],
        [0x19, 0],
        ...lit.map((l) => [l, 1]),
      ],
    });
    await page.waitForTimeout(150);
    await check("mode " + lit.join("+"));
  }
  const knobs = await page.$$(
    view === "machine"
      ? ".panel-machine svg[role=slider]"
      : "#pages svg[role=slider]",
  );
  for (let i = 0; i < Math.min(knobs.length, QUICK ? 8 : 40); i++) {
    await drag(
      page,
      view === "machine"
        ? ".panel-machine svg[role=slider]"
        : "#pages svg[role=slider]",
      i,
      i % 2 ? -34 : 41,
    );
    await receive(page, {
      event: "nrpn",
      number: (i * 7) % 127,
      value: (i * 13) % 100,
    });
    coverage.counts = Math.max(
      coverage.counts,
      await page.evaluate(() => document.querySelectorAll(".cnt.on").length),
    );
    await check("turn knob " + i);
  }
  await page.waitForTimeout(900);
  await check("counts expired");
  await receive(page, { event: "remote", available: false });
  await check("remote lost");
}

/* The controls of Tempester 1.6.2.0 / 1.6.3.0: an OS that has them, beat-wide knobs in 16 Beats with positions
   reported, the machine's ribbons and the user's, the pads' probabilities shown, changed (16 Sounds) and hidden again. */
async function performance(page, view, check, coverage) {
  /* A drag far down the page must happen where the control is on screen; the page is scrolled back afterwards
     (a hidden element measures at 0 + scrollY: the scroll itself would read as a shift). */
  const dragInView = async (selector, index, dy) => {
    const at = await page.evaluate(() => [scrollX, scrollY]);
    await (await page.$$(selector))[index].scrollIntoViewIfNeeded();
    await drag(page, selector, index, dy);
    await page.evaluate(([x, y]) => scrollTo(x, y), at);
  };
  const host =
    view === "machine"
      ? ".panel-machine"
      : "#pages .panel-root:not(.panel-machine)";
  const feature = (since) => ({ on: true, since });
  await receive(page, {
    event: "state",
    features: {
      beat_wide: feature("1.6.2.0"),
      ribbons: feature("1.6.3.0"),
      pedal_pad: feature("1.6.3.0"),
      probability: feature("1.6.3.0"),
    },
  });
  const modeLeds = (lit) =>
    receive(page, {
      event: "leds",
      changes: [
        [0x10, 0],
        [0x11, 0],
        [0x18, 0],
        [0x19, 0],
        [lit, 1],
      ],
    });
  await modeLeds(0x10);
  await page.waitForTimeout(250);
  for (const [index, position] of [
    [0, 0],
    [1, 254],
    [2, 131],
    [3, 127],
    [4, 9],
    [5, 200],
    [6, 64],
    [7, 250],
  ])
    await receive(page, { event: "beat_wide", index, position });
  await check("beat-wide positions in 16 Beats");
  /* Only the Machine view has beat-wide knobs; a Full-view knob turned here would change the BPM the later checks
     rely on (a 3-digit BPM label hides the counter-proof of its fixed slot). */
  if (view === "machine") {
    await dragInView(host + " svg[role=slider][aria-label='Lowpass Frequency']", 0, -60);
    await check("beat-wide knob turned");
  }
  await receive(page, { event: "ribbon", ribbon: 0, position: 100 });
  await receive(page, { event: "ribbon", ribbon: 1, position: 3 });
  coverage.ribbons = await page.evaluate(
    (h) => document.querySelectorAll(h + " .ruban.touche").length,
    host,
  );
  await check("machine's ribbons touched");
  await receive(page, { event: "ribbon", ribbon: 0, position: null });
  await dragInView(host + " .ruban", 1, -30);
  await check("ribbons released and dragged");
  await receive(page, {
    event: "beat",
    name: "Probable",
    events: 12,
    voice_assign: new Array(32).fill(0),
    assigned: 0,
    pads: 32,
    size: 9000,
    play_probability: Array.from({ length: 32 }, (_, k) => (k * 37) % 101),
  });
  await page
    .locator(
      "#pages .panelbar button[aria-label='Play probability of each pad']",
    )
    .click();
  await check("probability mode, 16 Beats");
  await modeLeds(0x18);
  await page.waitForTimeout(250);
  await dragInView(host + " .pad", 3, -50);
  /* Pad 4 came at 10 % in the beat: 50 px up is 33 % more, so the gesture really went through the page. */
  coverage.probability = await page.evaluate(
    (h) => document.querySelectorAll(h + " .pad .prob b")[3].textContent,
    host,
  );
  await receive(page, { event: "pad_probability", pad: 9, percent: 5 });
  await check("probability changed in 16 Sounds");
  await page
    .locator("#pages .panelbar button[aria-label='Back to playing the pads']")
    .click();
  await check("probability mode off");
}

function window_values() {
  return Array.from({ length: 160 }, (_, i) => (i * 37) % 120);
}

async function runEngine(engine, port) {
  const browser = await launch(engine);
  const failures = [];
  const report = [];
  try {
    for (const view of ["full", "machine"]) {
      const page = await openPanel(browser, port, view, 1500);
      page.on("pageerror", (e) =>
        failures.push(`${engine}/${view}: page error ${e.message}`),
      );
      const baseline = await measure(page);
      for (const problem of [
        ...(await alignment(page, view)),
        ...(await transportRow(page, view)),
        ...(await toolbar(page)),
        ...(await transportLeds(page, view)),
      ])
        failures.push(`${engine}/${view}: ${problem}`);
      let worst = 0;
      const movedNames = new Set();
      const coverage = { counts: 0, ribbons: 0, probability: "" };
      await interactions(page, view, async (label) => {
        const moved = compare(baseline, await measure(page));
        for (const [name, s] of moved) {
          worst = Math.max(worst, s);
          movedNames.add(name);
        }
        if (moved.length)
          failures.push(
            `${engine}/${view} after "${label}": ${moved.length} controls moved, e.g. ${moved
              .slice(0, 3)
              .map(([n, s, from, to]) => `${n} by ${s}px ${JSON.stringify(from)} -> ${JSON.stringify(to)}`)
              .join("; ")}`,
          );
      }, coverage);
      if (view === "machine" && coverage.counts === 0)
        failures.push(`${engine}/${view}: the test never saw a count: it would prove nothing`);
      if (coverage.ribbons !== 2 || coverage.probability !== "43%")
        failures.push(
          `${engine}/${view}: the 1.6.3.0 controls did not react (${coverage.ribbons} ribbons lit, pad 4 at ${coverage.probability}): it would prove nothing`,
        );
      report.push(
        `${engine}/${view}: ${Object.keys(baseline).length} elements measured, worst shift ${worst}px, ${movedNames.size} distinct elements moved, up to ${coverage.counts} counts shown`,
      );
      await page.close();
    }
  } finally {
    await browser.close();
  }
  return { failures, report };
}

(async () => {
  const failures = [];
  await withPreview(uiDir, workDir, repo, async (port) => {
    for (const engine of QUICK ? ["chrome"] : ["chrome", "webkit"]) {
      const { failures: f, report } = await runEngine(engine, port);
      report.forEach((line) => console.log(line));
      failures.push(...f);
    }
  });
  if (failures.length) {
    console.log([...new Set(failures)].slice(0, 12).join("\n"));
    console.log(failures.length + " layout shifts");
    process.exit(PRINT_ONLY ? 0 : 1);
  }
  console.log("no control moved or changed size");
})().catch((e) => {
  console.error(e);
  process.exit(2);
});
