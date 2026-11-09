/* Placement of the tooltips (`data-tip`) and the info popovers (ui/icons.js) on a fake DOM with set geometry, run by
   tools/test_app.sh: `node tooltip.test.js <ui folder>`. Above the element by default with an 8 px gap, below only
   when there is no room above (window top, sticky top bar), shifted sideways instead of clipped, keyboard focus and
   Escape. Then the same checks on copies of icons.js with one rule broken, each of which must fail. */
"use strict";
const fs = require("fs");
const os = require("os");
const path = require("path");
const assert = require("assert");
const { load } = require("./fake-dom.js");

const uiDir = process.argv[2];
const BOX = { left: 0, top: 0, width: 200, height: 30 };
const VIEW = { width: 1320, height: 800, top: 56 };
const rect = (left, top, width, height) => ({
  left,
  top,
  width,
  height,
  bottom: top + height,
});

function checkPure(icons) {
  const place = (anchor) => icons.placeBox(anchor, BOX, VIEW);
  const above = place(rect(600, 400, 38, 38));
  assert.strictEqual(above.side, "above", "above by default");
  assert.strictEqual(above.top, 400 - 8 - 30, "8 px gap above");
  assert.strictEqual(above.left, 600 + 19 - 100, "centred on the element");

  assert.strictEqual(place(rect(600, 9, 38, 38)).side, "below", "top bar");
  const below = place(rect(600, 70, 38, 38));
  assert.strictEqual(below.side, "below", "no room above the page content");
  assert.strictEqual(below.top, 70 + 38 + 8, "8 px gap below");
  assert.strictEqual(place(rect(600, 56 + 8 + 30, 38, 38)).side, "above");
  assert.strictEqual(place(rect(600, 56 + 8 + 29, 38, 38)).side, "below");

  assert.strictEqual(place(rect(0, 400, 38, 38)).left, 8, "left edge");
  assert.strictEqual(place(rect(1290, 400, 38, 38)).left, 1320 - 8 - 200);
  const wide = icons.placeBox(
    rect(600, 400, 38, 38),
    { width: 2000, height: 30 },
    VIEW,
  );
  assert.strictEqual(
    wide.left,
    8,
    "a box wider than the window starts at the edge",
  );

  const low = place(rect(600, 780, 38, 20));
  assert.strictEqual(low.side, "above", "bottom of the window");
  const tiny = { width: 1320, height: 120, top: 56 };
  const none = icons.placeBox(
    rect(600, 60, 38, 38),
    { width: 200, height: 50 },
    tiny,
  );
  assert.ok(
    none.top >= tiny.top && none.top + 50 <= tiny.height,
    "kept inside",
  );
}

function setup(dir) {
  const env = load(dir, ["icons.js", "kit.js"]);
  const icons = env.window.Tempester.icons;
  const make = env.document.createElement;
  env.document.createElement = (tag) => {
    const node = make(tag);
    node.getBoundingClientRect = () =>
      node.rect ||
      (/\b(tip|popover)\b/.test(node.attrs.class || "")
        ? BOX
        : rect(0, 0, 0, 0));
    return node;
  };
  env.byId.topbar = make("header");
  env.byId.topbar.rect = rect(0, 0, 1320, 56);
  return { env, icons, doc: env.document, body: env.document.body };
}

const tip = (body) => body.children.find((n) => n.attrs.class === "tip");
const px = (value) => Number(value.replace("px", ""));

function checkTooltip(dir) {
  const { env, icons, doc, body } = setup(dir);
  const inPage = icons.button("send", "Send to the machine", null);
  inPage.rect = rect(600, 400, 38, 38);
  const inBar = icons.button("connect", "Connect", null);
  inBar.rect = rect(300, 9, 38, 38);
  doc.fire("mouseover", { target: inPage });
  const shown = tip(body);
  assert.ok(shown && !shown.hidden, "hover shows the tooltip");
  assert.strictEqual(shown.textContent, "Send to the machine");
  assert.strictEqual(shown.attrs["data-side"], "above");
  assert.strictEqual(
    400 - (px(shown.style.top) + BOX.height),
    8,
    "gap of 8 px",
  );
  doc.fire("mouseout", { target: inPage });
  assert.ok(shown.hidden, "leaving hides it");
  doc.fire("mouseover", { target: inBar });
  assert.strictEqual(
    shown.attrs["data-side"],
    "below",
    "flips under the top bar",
  );
  /* Pushed down to the bar's lower edge (56) when 8 px under the button would overlap it. */
  assert.strictEqual(px(shown.style.top), 56, "starts under the bar");
  doc.fire("mouseout", { target: inBar });

  inPage.matches = () => false;
  doc.fire("focusin", { target: inPage });
  assert.ok(shown.hidden, "a mouse focus shows nothing");
  inPage.matches = (selector) => selector === ":focus-visible";
  doc.fire("focusin", { target: inPage });
  assert.ok(!shown.hidden, "keyboard focus shows the same tooltip");
  doc.fire("keydown", { key: "Escape" });
  assert.ok(shown.hidden, "Escape hides the tooltip");
  doc.fire("focusin", { target: inPage });
  doc.fire("focusout", { target: inPage });
  assert.ok(shown.hidden, "leaving the focus hides it");
  doc.fire("mouseover", { target: inPage });
  doc.fire("click", { target: inPage });
  assert.ok(shown.hidden, "a click hides it");
  assert.strictEqual(env.sent.length, 0);
}

function checkPopover(dir) {
  const { icons, doc, body } = setup(dir);
  const wrap = icons.info("Explanation", "About this");
  const [badge, box] = wrap.children;
  body.append(wrap);
  badge.rect = rect(600, 400, 36, 36);
  badge.fire("click", {});
  assert.ok(!box.hidden, "click opens");
  assert.strictEqual(box.attrs["data-side"], "above");
  assert.strictEqual(400 - (px(box.style.top) + BOX.height), 8, "gap of 8 px");
  doc.fire("keydown", { key: "Escape" });
  assert.ok(box.hidden, "Escape closes the popover");
  assert.strictEqual(badge.attrs["aria-expanded"], "false");

  badge.rect = rect(1290, 400, 36, 36);
  badge.fire("click", {});
  assert.ok(
    px(box.style.left) + BOX.width <= 1320 - 8,
    "not clipped on the right",
  );
  badge.fire("click", {});
  assert.ok(box.hidden, "second click closes");

  badge.rect = rect(600, 10, 36, 36);
  badge.fire("click", {});
  assert.strictEqual(
    box.attrs["data-side"],
    "below",
    "flips under the top bar",
  );
  assert.strictEqual(px(box.style.top), 56, "starts under the bar");
  badge.rect = rect(600, 400, 36, 36);
  doc.fire("scroll", {});
  assert.strictEqual(
    box.attrs["data-side"],
    "above",
    "follows its badge when scrolled",
  );
  doc.fire("click", { target: body });
  assert.ok(box.hidden, "a click elsewhere closes it");
}

function run(dir) {
  const { icons } = setup(dir);
  checkPure(icons);
  checkTooltip(dir);
  checkPopover(dir);
}

run(uiDir);
const source = fs.readFileSync(path.join(uiDir, "icons.js"), "utf8");
const broken = [
  ["const GAP = 8;", "const GAP = 0;"],
  ['"above" : "below"', '"above" : "above"'],
  ["fitsAbove ||", "!fitsAbove ||"],
  ["Math.min(centred, view.width - EDGE - box.width)", "centred"],
  ["top: bar ? bar.getBoundingClientRect().bottom : 0", "top: 0"],
  ['if (e.key !== "Escape") return;', "return;"],
  [
    'document.addEventListener("focusin"',
    'document.addEventListener("focusin-off"',
  ],
  [
    "        if (openPopover) placeNear(openPopover.box, openPopover.badge);\n      },\n      true,",
    "      },\n      true,",
  ],
];
let refused = 0;
for (const [from, to] of broken) {
  assert.ok(source.includes(from), "counter-proof anchor missing: " + from);
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "tooltip-"));
  fs.copyFileSync(path.join(uiDir, "kit.js"), path.join(dir, "kit.js"));
  fs.writeFileSync(path.join(dir, "icons.js"), source.replace(from, to));
  assert.throws(() => run(dir), undefined, "broken copy accepted: " + from);
  refused++;
}
console.log(
  `ok: above by default, 8 px gap, flip, clamp, focus, Escape; ${refused} broken copies refused`,
);
