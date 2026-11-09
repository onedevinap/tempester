/* The library manager's page on a minimal fake DOM (no dependency, run by tools/test_app.sh with node):
   `node library-browser.test.js <ui folder>`. The logic (search, filters, sort, selection, name rules), then the page
   with 320 sounds: search as you type, filters, sort, keyboard, multi-select and its batch commands, the review of a
   batch, the look at the machine before a send, and the fixed geometry of the rows. The ui folder is a parameter so
   that library-counter-proof.js can run the same checks on copies with one behaviour broken: they must fail there. */
"use strict";
const fs = require("fs");
const path = require("path");
const vm = require("vm");
const assert = require("assert");

function fakeNode(tag, byId) {
  const listeners = {};
  const classes = new Set();
  const node = {
    tagName: tag.toUpperCase(),
    children: [],
    parent: null,
    attrs: {},
    hidden: false,
    style: {},
    disabled: false,
    value: "",
    checked: false,
    files: null,
    returnValue: "",
    innerHTML: "",
    classList: {
      add: (c) => classes.add(c),
      remove: (c) => classes.delete(c),
      toggle: (c, on) => (on ? classes.add(c) : classes.delete(c)),
      contains: (c) => classes.has(c),
    },
    setAttribute(k, v) {
      node.attrs[k] = String(v);
      if (k === "hidden") node.hidden = true;
      if (k === "id") byId[v] = node;
    },
    getAttribute: (k) => (k in node.attrs ? node.attrs[k] : null),
    removeAttribute: (k) => delete node.attrs[k],
    addEventListener: (type, fn) =>
      (listeners[type] = (listeners[type] || []).concat(fn)),
    fire: (type, event) =>
      (listeners[type] || []).forEach((fn) => fn(event || {})),
    append(...items) {
      for (const item of items) {
        const child =
          typeof item === "string" ? { text: item, children: [] } : item;
        child.parent = node;
        node.children.push(child);
      }
    },
    replaceChildren(...items) {
      node.children = [];
      node.append(...items);
    },
    contains: (other) => all(node).includes(other),
    remove() {
      if (node.parent)
        node.parent.children = node.parent.children.filter((c) => c !== node);
    },
    focus() {},
    scrollIntoView() {},
    showModal() {},
    click() {
      if (isShown(node) && !node.disabled) node.fire("click");
    },
  };
  Object.defineProperty(node, "textContent", {
    get: () => textOf(node),
    set: (text) => {
      node.children = [];
      node.append(String(text));
    },
  });
  return node;
}

const textOf = (node) =>
  node.text !== undefined ? node.text : node.children.map(textOf).join("");
const all = (node) => [node, ...node.children.flatMap(all)];
function isShown(node) {
  for (let n = node; n; n = n.parent) if (n.hidden) return false;
  return true;
}

const FILES = [
  "bridge.js",
  "icons.js",
  "kit.js",
  "project-ui.js",
  "lib-model.js",
  "lib-dialogs.js",
  "lib-flow.js",
  "lib-browser.js",
  "page-library.js",
];

function load(uiDir) {
  const byId = {};
  const sent = [];
  const document = {
    createElement: (tag) => fakeNode(tag, byId),
    getElementById: (id) => byId[id] || (byId[id] = fakeNode("div", byId)),
    addEventListener() {},
  };
  document.body = fakeNode("body", byId);
  const window = {
    addEventListener() {},
    ipc: { postMessage: (line) => sent.push(line) },
    innerHeight: 900,
  };
  const context = vm.createContext({
    window,
    document,
    setTimeout,
    clearTimeout,
  });
  for (const file of FILES)
    vm.runInContext(fs.readFileSync(path.join(uiDir, file), "utf8"), context, {
      filename: file,
    });
  const root = fakeNode("section", byId);
  const kit = window.Tempester.kit;
  kit.pages.forEach((page) => page.build(root));
  return {
    window,
    kit,
    root,
    sent,
    byId,
    T: window.Tempester,
    pages: kit.pages,
  };
}

const tick = () => new Promise((resolve) => setTimeout(resolve, 5));
const labelled = (root, label) =>
  all(root).filter((n) => n.attrs && n.attrs["aria-label"] === label);
const withClass = (root, cls) =>
  all(root).filter((n) =>
    (n.attrs && n.attrs.class ? n.attrs.class.split(" ") : []).includes(cls),
  );

function sound(i) {
  return {
    id: "s" + i,
    kind: "sounds",
    name: (i % 7 ? "" : "Pack/") + "Sound " + i + ".syx",
    display: "Sound " + i,
    folder: i % 7 ? "" : "Pack",
    size: 200,
    hash: "h" + i,
    favourite: i % 10 === 0,
    tags: i % 5 === 0 ? ["kick"] : [],
    note: i === 42 ? "find the needle" : "",
    source: i % 3 === 0 ? "machine" : "file import",
    created: 1e9 + i * 3600,
    modified: 1e9 + i * 86400,
    fetched: null,
    history: 0,
    capture: null,
    beats: null,
    sound_type: ["Kicks", "Snares", "Bass", ""][i % 4],
    osc: ["Shape " + (i % 3), "Shape 0", "Sample/Noise 1", "Sample/Noise 0"],
    slot: false,
  };
}

const SOUNDS = Array.from({ length: 320 }, (_, i) => sound(i));

/* ---- the logic ---------------------------------------------------------------------------------------------- */

function checkModel(M) {
  const ids = (items) => items.map((i) => i.id);
  assert.strictEqual(M.filterItems(SOUNDS, "", {}).length, 320);
  assert.deepStrictEqual(
    ids(M.filterItems(SOUNDS, "needle", {})),
    ["s42"],
    "search finds the note",
  );
  assert.strictEqual(
    M.filterItems(SOUNDS, "KICK", {}).length,
    128,
    "search finds the tag (64) or the Sound Type Kicks (80), any case; 16 have both",
  );
  assert.strictEqual(
    M.filterItems(SOUNDS, "pack", {}).length,
    46,
    "search finds the folder",
  );
  assert.deepStrictEqual(
    ids(M.filterItems(SOUNDS, "sound 42 needle", {})),
    ["s42"],
    "every word must match",
  );
  assert.deepStrictEqual(
    ids(M.filterItems(SOUNDS, "sound 42 nothing", {})),
    [],
    "a missing word excludes",
  );
  assert.strictEqual(
    M.filterItems(SOUNDS, "", { favourites: true }).length,
    32,
    "favourites",
  );
  assert.strictEqual(
    M.filterItems(SOUNDS, "", { folder: "Pack" }).length,
    46,
    "folder",
  );
  assert.strictEqual(
    M.filterItems(SOUNDS, "", { folder: "(none)" }).length,
    274,
    "main folder",
  );
  assert.strictEqual(
    M.filterItems(SOUNDS, "", { tag: "kick" }).length,
    64,
    "tag",
  );
  assert.strictEqual(
    M.filterItems(SOUNDS, "", { source: "machine" }).length,
    107,
    "source",
  );
  assert.strictEqual(
    M.filterItems(SOUNDS, "", { soundType: "Bass" }).length,
    80,
    "Sound Type",
  );
  assert.strictEqual(
    M.filterItems(SOUNDS, "", { osc: "Shape 2" }).length,
    106,
    "oscillator type",
  );
  const day = (n) => M.formatDate(1e9 + n * 86400).slice(0, 10);
  assert.strictEqual(
    M.filterItems(SOUNDS, "", { from: day(10), to: day(19) }).length,
    10,
    "date range, both ends included",
  );
  assert.strictEqual(
    M.filterItems(SOUNDS, "", { from: day(300) }).length,
    20,
    "from only",
  );
  assert.strictEqual(
    M.filterItems(SOUNDS, "", { to: day(4) }).length,
    5,
    "to only",
  );
  assert.strictEqual(
    M.filterItems(SOUNDS, "kick", { favourites: true, soundType: "Kicks" })
      .length,
    16 /* every 20th */,
    "filters and search combine",
  );
  const byName = M.sortItems(SOUNDS, "name", "asc");
  assert.strictEqual(byName[0].display, "Sound 0");
  assert.strictEqual(byName[1].display, "Sound 1");
  assert.strictEqual(byName[2].display, "Sound 10", "text order, not numeric");
  assert.strictEqual(
    M.sortItems(SOUNDS, "name", "desc")[0].display,
    "Sound 99",
  );
  assert.strictEqual(
    M.sortItems(SOUNDS, "date", "desc")[0].id,
    "s319",
    "newest first",
  );
  assert.strictEqual(M.sortItems(SOUNDS, "date", "asc")[0].id, "s0");
  assert.strictEqual(M.sortItems(SOUNDS, "folder", "desc")[0].folder, "Pack");
  assert.strictEqual(
    M.sortItems(SOUNDS, "favourite", "asc")[0].favourite,
    true,
    "favourites first",
  );
  const slot = Object.assign({}, sound(500), {
    slot: true,
    display: "Zzz backups",
  });
  assert.strictEqual(
    M.sortItems([...SOUNDS, slot], "name", "asc")[0].id,
    "s500",
    "the backups item stays first",
  );
  const options = M.facetOptions(SOUNDS);
  assert.deepStrictEqual(Array.from(options.soundTypes), [
    "Bass",
    "Kicks",
    "Snares",
  ]);
  assert.deepStrictEqual(Array.from(options.tags), ["kick"]);
  assert.ok(options.hasRoot && options.folders.includes("Pack"));
  assert.strictEqual(M.activeFilterCount({ favourites: true, tag: "x" }), 2);
  // selection
  const order = ["a", "b", "c", "d", "e", "f"];
  const sel = M.createSelection();
  M.selectClick(sel, order, "b", {});
  M.selectClick(sel, order, "d", { shift: true });
  assert.deepStrictEqual(
    [...sel.ids].sort(),
    ["b", "c", "d"],
    "shift selects the range",
  );
  M.selectClick(sel, order, "f", { ctrl: true });
  assert.deepStrictEqual(
    [...sel.ids].sort(),
    ["b", "c", "d", "f"],
    "ctrl adds",
  );
  M.selectClick(sel, order, "c", { ctrl: true });
  assert.deepStrictEqual(
    [...sel.ids].sort(),
    ["b", "d", "f"],
    "ctrl again removes",
  );
  M.selectClick(sel, order, "e", {});
  assert.deepStrictEqual(
    [...sel.ids],
    ["e"],
    "a plain click selects only that one",
  );
  M.selectMove(sel, order, "down", false);
  assert.strictEqual(sel.active, "f");
  M.selectMove(sel, order, "down", false);
  assert.strictEqual(sel.active, "f", "stops at the end");
  M.selectMove(sel, order, "home", true);
  assert.deepStrictEqual(
    [...sel.ids].sort(),
    order,
    "shift+Home extends to the top",
  );
  M.selectMove(sel, order, "up", false);
  assert.deepStrictEqual(
    [...sel.ids],
    ["a"],
    "a plain move selects only the new row (the top: Up from a stays on a)",
  );
  M.pruneSelection(sel, ["x", "y"]);
  assert.strictEqual(
    sel.ids.size,
    0,
    "rows that disappeared leave the selection",
  );
  // names: the same rules as library::rename::name_refusal in Rust
  for (const bad of [
    "",
    "   ",
    "x".repeat(21),
    "Café",
    "tab\there",
    " lead",
    "trail ",
  ])
    assert.ok(M.nameRefusal(bad), "refused: " + JSON.stringify(bad));
  for (const good of ["Big Kick", "x".repeat(20), "A~z !"])
    assert.strictEqual(M.nameRefusal(good), null, "accepted: " + good);
  assert.ok(M.nameRefusal("x".repeat(24)).includes("24 characters"));
}

/* ---- the page ------------------------------------------------------------------------------------------------ */

function feed(env) {
  const { bridge } = env.T;
  env.kit.updateSession({ connected: true, fake: true });
  bridge.receive({ event: "library", kind: "sounds", files: SOUNDS, trash: 2 });
  bridge.receive({ event: "library", kind: "beats", files: [], trash: 2 });
  bridge.receive({ event: "library", kind: "projects", files: [], trash: 2 });
}

const rowsOf = (env) => withClass(env.root, "lib-row");
const searchBox = (env) => env.byId["lib-search"];
const listbox = (env) =>
  all(env.root).find((n) => n.attrs && n.attrs.role === "listbox");

function type(env, text) {
  searchBox(env).value = text;
  searchBox(env).fire("input");
}

function setFilter(env, label, value) {
  const select = labelled(env.root, label)[0];
  assert.ok(select, "no filter " + label);
  select.value = value;
  select.fire("change");
}

async function checkPage(env) {
  const { dialogs } = env.T;
  const prompts = [];
  const answers = [];
  dialogs.prompt = async (spec) => (prompts.push(spec), answers.shift());
  dialogs.confirm = async (spec) => (prompts.push(spec), answers.shift());
  feed(env);
  assert.strictEqual(rowsOf(env).length, 320, "every sound is listed");
  assert.ok(textOf(env.root).includes("Sounds (320)"), "the segment counts");
  assert.ok(textOf(env.root).includes("320 of 320 sounds"), "the count line");

  type(env, "needle");
  assert.strictEqual(rowsOf(env).length, 1, "search as you type");
  assert.ok(textOf(env.root).includes("1 of 320 sounds"));
  type(env, "zzzz");
  assert.strictEqual(rowsOf(env).length, 0);
  assert.ok(
    textOf(env.root).includes("Nothing matches"),
    "an empty result says so",
  );
  type(env, "");
  assert.strictEqual(rowsOf(env).length, 320);

  setFilter(env, "Sound Type", "Bass");
  assert.strictEqual(rowsOf(env).length, 80, "Sound Type filter");
  setFilter(env, "Sound Type", "");
  setFilter(env, "Tag", "kick");
  assert.strictEqual(rowsOf(env).length, 64, "tag filter");
  setFilter(env, "Tag", "");
  setFilter(env, "Folder or project", "Pack");
  assert.strictEqual(rowsOf(env).length, 46, "folder filter");
  setFilter(env, "Folder or project", "");
  env.byId.x = null;
  withClass(env.root, "chip")
    .find((n) => textOf(n) === "Favourites only")
    .fire("click");
  assert.strictEqual(rowsOf(env).length, 32, "favourites filter");
  labelled(env.root, "Clear all filters")[0].fire("click");
  assert.strictEqual(
    rowsOf(env).length,
    320,
    "clearing the filters restores the list",
  );

  // sort: a header click, then the direction button
  const first = () => textOf(withClass(rowsOf(env)[0], "col-name")[0]);
  assert.strictEqual(first(), "Sound 0");
  withClass(env.root, "lib-head-btn")
    .find((n) => textOf(n) === "Modified")
    .fire("click");
  assert.strictEqual(first(), "Sound 319", "Modified sorts newest first");
  labelled(env.root, "Sort direction: descending")[0].fire("click");
  assert.strictEqual(first(), "Sound 0", "the direction button reverses");
  withClass(env.root, "lib-head-btn")
    .find((n) => textOf(n) === "Name")
    .fire("click");

  // geometry: every row has the same cells whatever it holds, so nothing moves
  const shapes = new Set(
    rowsOf(env).map((r) =>
      r.children.map((c) => (c.attrs.class || c.tagName).split(" ")[0]).join("|"),
    ),
  );
  assert.strictEqual(shapes.size, 1, "all rows have the same cells");

  // keyboard
  const list = listbox(env);
  list.fire("keydown", { key: "ArrowDown" });
  assert.ok(
    textOf(env.root).includes("1 selected"),
    "ArrowDown selects the first row",
  );
  list.fire("keydown", { key: "ArrowDown", shiftKey: true });
  list.fire("keydown", { key: "ArrowDown", shiftKey: true });
  assert.ok(textOf(env.root).includes("3 selected"), "shift+arrows extend");
  assert.ok(
    textOf(env.root).includes("3 items selected"),
    "the panel offers the batch actions",
  );
  list.fire("keydown", { key: "Escape" });
  assert.ok(!/\d+ selected/.test(textOf(env.root)), "Escape clears");
  list.fire("keydown", { key: "a", ctrlKey: true });
  assert.ok(
    textOf(env.root).includes("320 selected"),
    "ctrl+A selects all shown",
  );
  list.fire("keydown", { key: "Escape" });

  // multi-select with the mouse, then each batch command
  env.sent.length = 0;
  rowsOf(env)[1].fire("click", {});
  rowsOf(env)[3].fire("click", { shiftKey: true });
  assert.ok(textOf(env.root).includes("3 selected"));
  answers.push("batch-tag");
  labelled(env.root, "Add a tag to the selection...")[0].fire("click");
  await tick();
  answers.push("Moved Folder");
  labelled(env.root, "Move the selection to a folder...")[0].fire("click");
  await tick();
  labelled(env.root, "Favourite / unfavourite the selection")[0].fire("click");
  answers.push(true);
  labelled(env.root, "Move the selection to the trash...")[0].fire("click");
  await tick();
  const ids = "s1,s10,s100";
  assert.deepStrictEqual(
    Array.from(env.sent),
    [
      "lib_tag\t" + ids + "\tadd\tbatch-tag",
      "lib_move\t" + ids + "\tMoved Folder",
      "lib_favourite\t" + ids + "\t1",
      "lib_delete\t" + ids,
    ],
    "the batch commands carry every selected id",
  );
  assert.ok(
    prompts.at(-1).lines.join(" ").includes("Nothing is deleted for good"),
    "the delete says it goes to the trash",
  );
  // a refused delete sends nothing
  env.sent.length = 0;
  answers.push(false);
  labelled(env.root, "Move the selection to the trash...")[0].fire("click");
  await tick();
  assert.deepStrictEqual(Array.from(env.sent), [], "a refusal deletes nothing");
  list.fire("keydown", { key: "Escape" });

  // one item: rename is validated and carries the new name
  rowsOf(env)[0].fire("click", {});
  answers.push(null);
  labelled(env.root, "Rename...")[0].fire("click");
  await tick();
  const spec = prompts.at(-1);
  assert.ok(
    spec.validate("x".repeat(21)),
    "the rename dialog refuses a long name",
  );
  assert.strictEqual(spec.validate("Fine"), null);
  assert.deepStrictEqual(Array.from(env.sent), [], "dismissed: nothing sent");
  answers.push("New name");
  labelled(env.root, "Rename...")[0].fire("click");
  await tick();
  assert.deepStrictEqual(Array.from(env.sent), ["lib_rename\ts0\tNew name"]);
  assert.ok(
    prompts.at(-1).lines.join(" ").includes("kept in the item's history"),
    "the rename says where the old version goes",
  );
}

/* ---- the dialogs and what they send --------------------------------------------------------------------------- */

async function checkFlows(env) {
  const { dialogs, bridge } = env.T;
  const asked = [];
  let choose = (spec) => "cancel";
  dialogs.ask = async (spec) => (asked.push(spec), choose(spec));
  const plan = {
    event: "lib_plan",
    title: "Project 2026-11-08",
    source: "project",
    new: 2,
    identical: 3,
    modified: 1,
    entries: [
      {
        label: "a",
        kind: "beats",
        verdict: "new",
        name: null,
        place: null,
        in_batch: false,
        existing_date: null,
        candidate_date: 1e9,
        newer: true,
      },
      {
        label: "b",
        kind: "beats",
        verdict: "identical",
        name: "B",
        place: "x/b.syx",
        in_batch: false,
        existing_date: 1e9,
        candidate_date: 1e9,
        newer: true,
      },
      {
        label: "c",
        kind: "beats",
        verdict: "modified",
        name: "C",
        place: "x/c.syx",
        in_batch: false,
        existing_date: 1e9 + 86400 * 5,
        candidate_date: 1e9,
        newer: false,
      },
    ],
  };
  env.sent.length = 0;
  bridge.receive(plan);
  await tick();
  const spec = asked.at(-1);
  const body = textOf({ children: spec.body, text: undefined });
  assert.ok(
    body.includes("2 new") &&
      body.includes("3 identical") &&
      body.includes("1 modified"),
    "the summary counts each group",
  );
  assert.ok(
    body.includes("incoming is OLDER"),
    "an older replacement is warned about, with both dates",
  );
  assert.deepStrictEqual(
    Array.from(env.sent),
    ["lib_cancel"],
    "dismissing the review writes nothing",
  );
  assert.deepStrictEqual(
    Array.from(spec.buttons, (b) => b.value),
    ["cancel", "ok"],
  );
  assert.ok(spec.buttons[0].label.includes("write nothing"));

  env.sent.length = 0;
  choose = () => "ok";
  bridge.receive(plan);
  await tick();
  assert.deepStrictEqual(
    Array.from(env.sent),
    ["lib_apply\twrite\tskip\tupdate"],
    "the defaults: write new, skip identical, update modified",
  );

  // the three groups each carry their own choice
  const selects = all({ children: asked.at(-1).body }).filter(
    (n) => n.tagName === "SELECT",
  );
  assert.strictEqual(selects.length, 3);
  env.sent.length = 0;
  asked.length = 0;
  choose = (s) => {
    all({ children: s.body })
      .filter((n) => n.tagName === "SELECT")
      .forEach((n, i) => (n.value = ["skip", "copy", "new"][i]));
    return "ok";
  };
  bridge.receive(plan);
  await tick();
  assert.deepStrictEqual(
    Array.from(env.sent),
    ["lib_apply\tskip\tcopy\tnew"],
    "one choice per group",
  );

  // sending: the machine is looked at, the dialog says what is replaced, and the token comes back
  const sendPlan = (extra) =>
    Object.assign(
      {
        event: "lib_send_plan",
        kind: "sounds",
        id: "s1",
        status: "unknown",
        changed: false,
        item: "Kick",
        from: "sounds/Kick.syx",
        machine: "Fake",
        token: "a".repeat(64),
        lines: ["It will be saved as an auto backup first."],
      },
      extra,
    );
  env.sent.length = 0;
  asked.length = 0;
  choose = () => "ok";
  bridge.receive(sendPlan({}));
  await tick();
  assert.ok(
    textOf({ children: asked.at(-1).body }).includes("auto backup first"),
    "the confirmation says where the backup goes",
  );
  assert.deepStrictEqual(Array.from(env.sent), [
    "lib_send_apply\tsounds\ts1\t" + "a".repeat(64),
  ]);
  env.sent.length = 0;
  choose = () => "cancel";
  bridge.receive(sendPlan({}));
  await tick();
  assert.deepStrictEqual(Array.from(env.sent), [], "declined: nothing is sent");
  choose = () => "ok";
  bridge.receive(
    sendPlan({
      status: "identical",
      lines: ["The machine already holds exactly this sound: nothing to send."],
    }),
  );
  await tick();
  assert.deepStrictEqual(
    Array.from(env.sent),
    [],
    "identical: nothing to send, said so, no apply",
  );
  assert.ok(asked.at(-1).title.includes("Nothing to send"));
  bridge.receive(
    sendPlan({
      status: "no_answer",
      token: "none",
      lines: [
        "The machine did not answer, so its current sound cannot be backed up.",
      ],
    }),
  );
  await tick();
  assert.ok(
    asked.at(-1).buttons.some((b) => b.label === "Send without a backup"),
    "no answer: explicit consent to send without a backup",
  );
  assert.deepStrictEqual(Array.from(env.sent), [
    "lib_send_apply\tsounds\ts1\tnone",
  ]);

  // the log keeps what the library did
  bridge.receive({
    event: "lib_done",
    title: "Rename",
    lines: ['Renamed sound "A" to "B". To undo: rename it back.'],
  });
  assert.ok(env.T.libflow.log[0].lines[0].includes("To undo"));
  assert.ok(
    textOf(env.root).includes("Rename: Renamed sound"),
    "the last message shows under the list",
  );
  bridge.receive({
    event: "error",
    during: "lib",
    code: "invalid",
    message: '"x" is refused: the name is empty',
  });
  assert.ok(
    env.T.libflow.log[0].bad && textOf(env.root).includes("is refused"),
    "a refusal is shown with its reason",
  );
}

function checkCards(env) {
  // every command of the old Sounds, Beats and Project cards is still sent; sending goes through the check
  const need = [
    "fetch_sound",
    "save_sound\tKick",
    "fetch_beat",
    "save_beat\tGroove",
    "lib_send_check\tsounds\t",
    "lib_send_check\tbeats\t",
    "fetch_project",
    "receive_project",
    "project_cancel",
    "fake_project_send",
    "fake_project_send_partial",
  ];
  const { bridge } = env.T;
  env.sent.length = 0;
  const press = (label) => {
    const found = labelled(env.root, label).filter(
      (n) => n.tagName === "BUTTON" && isShown(n),
    );
    assert.ok(found.length, "no visible button " + label);
    found[0].click();
  };
  bridge.receive({ event: "sound", name: "Kick" });
  bridge.receive({
    event: "sound_opened",
    file: "k.syx",
    name: "Kick",
    size: 300,
  });
  const beat = {
    event: "beat",
    name: "Groove",
    events: 10,
    assigned: 2,
    pads: 32,
    size: 3000,
    voice_assign: new Array(32).fill(1),
  };
  bridge.receive(beat);
  bridge.receive(
    Object.assign({}, beat, { event: "beat_opened", file: "g.syx" }),
  );
  press("Fetch the current sound from the machine");
  env.byId["sound-name"].value = "Kick";
  press("Save to the library (checks for duplicates first)");
  press("Fetch the current beat from the machine");
  env.byId["beat-name"].value = "Groove";
  labelled(
    env.root,
    "Save to the library (checks for duplicates first)",
  )[1].click();
  press("Send the opened file to the machine...");
  press("Restore the opened file to the machine...");
  press("Fetch the project from the machine");
  press("Wait for Send Project from the machine's menu");
  bridge.receive({
    event: "project",
    state: "listening",
    mode: "receive",
    beats: 0,
    beat: 0,
    total: 16,
  });
  press("Cancel");
  press("Play the machine's Send Project (fake machine)");
  press("Play a Send Project with absent beats (fake machine)");
  const sent = new Set(env.sent);
  const missing = need.filter((c) => !sent.has(c));
  assert.deepStrictEqual(missing, [], "commands no longer sent: " + missing);
  return need.length;
}

function checkCss(uiDir) {
  const css = fs.readFileSync(path.join(uiDir, "library.css"), "utf8");
  const compact = fs.readFileSync(path.join(uiDir, "compact.css"), "utf8");
  const columns = css.match(
    /^\.lib-head, \.lib-row \{[^}]*grid-template-columns: ([^;]+);/m,
  );
  assert.ok(
    columns,
    "head and rows share one grid template, so the columns line up",
  );
  assert.ok(
    /\.dense \.row-item \{[^}]*height: 44px/.test(compact),
    "rows have a fixed height",
  );
  assert.ok(
    /\.lib-status \{[^}]*height: 44px/.test(css),
    "the message line has a fixed height",
  );
  assert.ok(
    /\.lib-list \{[^}]*height:/.test(css),
    "the list has a fixed height",
  );
  assert.ok(
    /\.lib-row \.ibtn \{[^}]*width: 36px; height: 36px/.test(css),
    "row icon buttons are at least 36 px",
  );
}

async function main() {
  const uiDir = process.argv[2];
  const env = load(uiDir);
  assert.deepStrictEqual(
    Array.from(env.pages, (p) => p.id),
    ["library"],
    "one Library page",
  );
  checkModel(env.T.libmodel);
  await checkPage(env);
  await checkFlows(env);
  const commands = checkCards(env);
  checkCss(uiDir);
  console.log(
    "ok: logic, 320-item page, flows, " + commands + " card commands, geometry",
  );
}

main().catch((e) => {
  console.error(e.stack);
  process.exit(1);
});
