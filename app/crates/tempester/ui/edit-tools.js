/* The editing tools of the Panel page: one fixed slot of the toolbar (undo and the Sound tools button: the bar stays
   about half the window wide) and a small popover (redo, A / B, variations, MIDI learn: the controller port and its
   table). Rust keeps the
   history, the snapshots, the variations and the learn table (src/edit, src/machine/editor.rs) and sends to the
   machine; this script only asks and shows. Every button has a fixed size and a tooltip, so nothing moves.

   Tempester.editTools.create({bridge, names, learnHost, shown, onValues, drive}) ->
     {slot, popover, setView(name), setConnected(on)}
     names      {index: label} of the sound parameters (panel-labels.js)
     learnHost  the Full view's face, where learn mode picks a control
     shown()    true while the Panel page is the visible page (keyboard shortcuts)
     onValues   (values) the sound changed by undo, redo, A/B or a variation: redraw both views
     drive      (id, value) a learned controller moved control `id` (edit-learn.js `drive`) */
"use strict";
window.Tempester = window.Tempester || {};
window.Tempester.editTools = (function () {
  const S = {
    undo: "Undo (Cmd/Ctrl Z)",
    undoWhat: (what) => "Undo " + what + " (Cmd/Ctrl Z)",
    redo: "Redo (Shift Cmd/Ctrl Z)",
    redoWhat: (what) => "Redo " + what + " (Shift Cmd/Ctrl Z)",
    ab: "Compare: send the other one of A and B",
    abEmpty: "Compare A and B: store both first (Sound tools)",
    learn: "MIDI learn: click a control, then move a knob of the controller",
    learnOn:
      "MIDI learn on: click a control of the Full view, then move a knob of the controller (Escape: stop)",
    learnNoPort: "MIDI learn: choose the controller's port first (Sound tools)",
    learnFull: "MIDI learn works on the Full view",
    tools: "Sound tools: redo, A/B, variations, MIDI learn",
    history_head: "History",
    steps: (n) => n + (n === 1 ? " step" : " steps"),
    title: "Sound tools",
    info: "A/B, undo and variations send only the parameters that change, as NRPN, like the knobs: they act on the pad selected on the machine, in its memory only (nothing is saved to its flash). They follow the sound fetched by Sync: sending a sound, a beat or a project asks for a new Sync. A learned controller moves its control as your hand would; nothing is ever sent back to the controller.",
    ab_head: "A / B",
    store: (slot) => "Store " + slot,
    storeTip: (slot) => "Keep the current sound as " + slot,
    needSync: "Sync first",
    needSyncTip: "Fetch the sound with Sync: A/B and variations start from it",
    active: (slot) => "On " + slot,
    vary_head: "Variation",
    group: "Parameters to vary",
    amount: "How far, in percent of each range",
    seed: "Seed: the same seed on the same sound gives the same variation",
    generate: "Generate",
    generateTip: "Vary the current sound with this seed and send it (undoable)",
    dice: "New seed, then generate",
    seeded: (seed) => "Seed " + seed,
    learn_head: "MIDI learn",
    port: "Controller port",
    noPort: "No controller",
    refresh: "Look for MIDI inputs again",
    clearAll: "Clear all",
    clearAllTip: "Forget every learned controller",
    clearOne: (cc, name) => "Forget CC " + cc + " (" + name + ")",
    count: (n) => n + " learned",
    fake: "Simulated controller, CC 21",
    learned: (cc, name) => "CC " + cc + " now drives " + name,
    empty:
      "Nothing learned: choose the port, press MIDI learn, click a control, move a knob",
    recallWord: (slot) => "recall " + slot,
    variationWord: (seed) => "variation (seed " + seed + ")",
  };
  const GROUPS = [
    ["pitch", "Pitch"],
    ["env", "Envelopes"],
    ["filter", "Filters"],
    ["mod", "Modulation"],
    ["all", "All four"],
  ];
  const AMOUNTS = [10, 25, 50, 75, 100];
  const SEED_MAX = 99999;
  const FAKE_CC = 21;
  const LOCAL_NAMES = {
    bpm: "BPM",
    distort: "Distort",
    compress: "Compress",
    volume: "Volume",
  };

  function create({ bridge, names, learnHost, shown, onValues, drive }) {
    const { el, toast, errorText } = window.Tempester.kit;
    const icons = window.Tempester.icons;
    const state = {
      synced: false,
      a: false,
      b: false,
      active: null,
      port: null,
      armed: null,
      map: [],
      view: "full",
      connected: false,
    };
    const nameOf = (id) => {
      const param = /^n(\d+)$/.exec(id || "");
      if (param) return names[param[1]] || "Param " + param[1];
      return LOCAL_NAMES[id] || id;
    };
    const stepName = (label) => {
      const [kind, arg] = String(label).split(" ");
      if (kind === "recall") return S.recallWord(arg);
      if (kind === "variation") return S.variationWord(arg);
      return nameOf(kind);
    };

    /* ---- the toolbar slot ---- */
    const undo = icons.button("undo", S.undo, () => bridge.send("edit_undo"));
    const redo = icons.button("redo", S.redo, () => bridge.send("edit_redo"));
    /* A fixed-width text button (edit-tools.css): its text ("A/B", "A", "B") changes, its size does not. */
    const ab = icons.button(
      "compare",
      S.abEmpty,
      () => bridge.send("edit_recall", state.active === "A" ? "b" : "a"),
      { text: "A/B" },
    );
    ab.classList.add("edit-ab");
    const abText = ab.children[1];
    const learnButton = icons.button(
      "learn",
      S.learnNoPort,
      () => setLearn(!learner.isOn()),
      { pressed: false },
    );
    /* In learn mode the popover closes (it would hide the knobs): this button then says so and lights up. */
    const toolsButton = icons.button("tools", S.tools, () => togglePopover());
    toolsButton.setAttribute("aria-haspopup", "dialog");
    toolsButton.setAttribute("aria-expanded", "false");
    const slot = el(
      "div",
      { class: "edit-slot", role: "group", "aria-label": S.title },
      undo,
      toolsButton,
    );

    /* ---- the popover ---- */
    const head = (text) => el("span", { class: "ep-head" }, text);
    const textButton = (iconName, label, tip, onclick) => {
      const b = icons.button(iconName, tip, onclick, { text: label });
      return b;
    };
    const store = {
      A: textButton("save", S.store("A"), S.storeTip("A"), () =>
        bridge.send("edit_store", "a"),
      ),
      B: textButton("save", S.store("B"), S.storeTip("B"), () =>
        bridge.send("edit_store", "b"),
      ),
    };
    const abChip = el("span", { class: "chip ep-chip", role: "status" });
    const stepsChip = el("span", { class: "chip ep-chip", role: "status" });
    const groupSelect = el(
      "select",
      { "aria-label": S.group, "data-tip": S.group },
      GROUPS.map(([value, label]) => el("option", { value }, label)),
    );
    groupSelect.value = "filter";
    const amountSelect = el(
      "select",
      { "aria-label": S.amount, "data-tip": S.amount },
      AMOUNTS.map((n) => el("option", { value: String(n) }, n + " %")),
    );
    amountSelect.value = "25";
    const seedInput = el("input", {
      type: "number",
      min: "0",
      max: String(SEED_MAX),
      class: "ep-seed",
      "aria-label": S.seed,
      "data-tip": S.seed,
    });
    seedInput.value = String(newSeed());
    const generate = textButton("check", S.generate, S.generateTip, vary);
    const dice = icons.button("dice", S.dice, () => {
      seedInput.value = String(newSeed());
      vary();
    });
    const varyChip = el("span", { class: "chip ep-chip", role: "status" });
    const portSelect = el("select", {
      class: "ep-port",
      "aria-label": S.port,
      "data-tip": S.port,
    });
    portSelect.addEventListener("change", () =>
      bridge.send("learn_open", portSelect.value),
    );
    const refresh = icons.button("refresh", S.refresh, () =>
      bridge.send("learn_ports"),
    );
    const clearAll = textButton("trash", S.clearAll, S.clearAllTip, () =>
      bridge.send("learn_clear_all"),
    );
    const countChip = el("span", { class: "chip ep-chip", role: "status" });
    const fakeSlider = el("input", {
      type: "range",
      min: "0",
      max: "127",
      "aria-label": S.fake,
      "data-tip": S.fake,
    });
    fakeSlider.addEventListener("input", () =>
      bridge.sendThrottled(
        "fakecc",
        30,
        "learn_fake_cc",
        FAKE_CC,
        fakeSlider.value,
      ),
    );
    const fakeRow = el(
      "div",
      { class: "ep-row", hidden: true },
      head(""),
      el("span", { class: "ep-label" }, S.fake),
      fakeSlider,
    );
    const mapList = el("ul", { class: "ep-map", "aria-label": S.learn_head });
    mapList.append(el("li", { class: "ep-map-row ep-empty" }, S.empty));
    const popover = el(
      "div",
      {
        class: "edit-pop",
        role: "dialog",
        "aria-label": S.title,
        hidden: true,
      },
      el(
        "div",
        { class: "ep-row" },
        head(S.history_head),
        redo,
        stepsChip,
        el("span", { class: "grow" }),
        icons.info(S.info),
      ),
      el(
        "div",
        { class: "ep-row" },
        head(S.ab_head),
        store.A,
        store.B,
        ab,
        abChip,
      ),
      el(
        "div",
        { class: "ep-row" },
        head(S.vary_head),
        groupSelect,
        amountSelect,
        seedInput,
        dice,
        generate,
        varyChip,
      ),
      el(
        "div",
        { class: "ep-row" },
        head(S.learn_head),
        learnButton,
        portSelect,
        refresh,
        clearAll,
        countChip,
      ),
      fakeRow,
      mapList,
    );

    function newSeed() {
      return Math.floor(Math.random() * (SEED_MAX + 1));
    }
    function vary() {
      const seed = Math.max(
        0,
        Math.min(SEED_MAX, Math.floor(Number(seedInput.value) || 0)),
      );
      seedInput.value = String(seed);
      bridge.send("edit_vary", groupSelect.value, amountSelect.value, seed);
    }

    function togglePopover() {
      if (!popover.hidden) return closePopover();
      popover.hidden = false;
      toolsButton.setAttribute("aria-expanded", "true");
      bridge.send("learn_ports");
      place();
    }
    function closePopover() {
      popover.hidden = true;
      toolsButton.setAttribute("aria-expanded", "false");
    }
    /* Above the tools button, below when there is no room; fixed, so the page never moves. */
    function place() {
      if (!popover.style || !document.getElementById) return;
      popover.style.left = "0px";
      popover.style.top = "0px";
      const size = popover.getBoundingClientRect();
      const bar = document.getElementById("topbar");
      const found = icons.placeBox(
        toolsButton.getBoundingClientRect(),
        { width: size.width, height: size.height },
        {
          width: window.innerWidth,
          height: window.innerHeight,
          top: bar ? bar.getBoundingClientRect().bottom : 0,
        },
      );
      popover.style.left = found.left + "px";
      popover.style.top = found.top + "px";
    }

    /* ---- learn mode ---- */
    const learner = window.Tempester.editLearn.attach(learnHost, {
      onPick: (id) => bridge.send("learn_arm", id),
    });
    function setLearn(on) {
      if (on && !canLearn()) return;
      learner.setOn(on);
      if (learner.isOn()) closePopover();
      learnButton.setAttribute("aria-pressed", String(learner.isOn()));
      toolsButton.classList.toggle("learning", learner.isOn());
      icons.relabel(toolsButton, learner.isOn() ? S.learnOn : S.tools);
      if (!on && state.armed) bridge.send("learn_cancel");
      showLearn();
    }
    const canLearn = () => !!state.port && state.view === "full";
    function showLearn() {
      if (learner.isOn() && !canLearn()) setLearn(false);
      learnButton.disabled = !canLearn();
      icons.relabel(
        learnButton,
        !state.port
          ? S.learnNoPort
          : state.view !== "full"
            ? S.learnFull
            : learner.isOn()
              ? S.learnOn
              : S.learn,
      );
      learner.mark(
        state.map.map(([, id]) => id),
        state.armed,
      );
    }

    /* ---- what Rust says ---- */
    function showEdit(m) {
      Object.assign(state, {
        synced: !!m.synced,
        a: !!m.a,
        b: !!m.b,
        active: m.active || null,
      });
      const live = state.connected;
      undo.disabled = !live || !m.undo;
      redo.disabled = !live || !m.redo;
      icons.relabel(undo, m.undo ? S.undoWhat(stepName(m.undo)) : S.undo);
      icons.relabel(redo, m.redo ? S.redoWhat(stepName(m.redo)) : S.redo);
      const both = state.a && state.b;
      ab.disabled = !live || !state.synced || !both;
      icons.relabel(ab, both ? S.ab : S.abEmpty);
      abText.textContent = state.active || "A/B";
      for (const slotName of ["A", "B"])
        store[slotName].disabled = !state.synced;
      stepsChip.textContent = S.steps(m.depth || 0);
      generate.disabled = dice.disabled = !live || !state.synced;
      abChip.textContent = !state.synced
        ? S.needSync
        : state.active
          ? S.active(state.active)
          : "";
      abChip.setAttribute("data-tip", state.synced ? "" : S.needSyncTip);
      abChip.className = "chip ep-chip" + (state.synced ? "" : " warn");
      const seeded = /^variation (\d+)$/.exec(m.undo || "");
      varyChip.textContent = seeded ? S.seeded(seeded[1]) : "";
      if (m.values) onValues(m.values);
    }
    let lastEdit = {};
    bridge.on("edit", (m) => {
      lastEdit = m;
      showEdit(m);
    });
    bridge.on("learn_ports", (m) => {
      const ports = m.ports || [];
      const options = [el("option", { value: "" }, S.noPort)].concat(
        ports.map((p) => el("option", { value: p }, p)),
      );
      if (state.port && !ports.includes(state.port))
        options.push(el("option", { value: state.port }, state.port));
      portSelect.replaceChildren(...options);
      portSelect.value = state.port || "";
    });
    bridge.on("learn", (m) => {
      state.port = m.port || null;
      state.armed = m.armed || null;
      state.map = m.map || [];
      fakeRow.hidden = !(m.fake && state.port);
      if (
        !Array.from(portSelect.children || []).some(
          (o) => o.value === (state.port || ""),
        )
      )
        portSelect.append(
          el("option", { value: state.port || "" }, state.port || S.noPort),
        );
      portSelect.value = state.port || "";
      clearAll.disabled = !state.map.length;
      countChip.textContent = S.count(state.map.length);
      mapList.replaceChildren(
        ...state.map.map(([cc, id]) =>
          el(
            "li",
            { class: "ep-map-row" },
            el("span", { class: "ep-cc" }, "CC " + cc),
            el("span", { class: "ep-name" }, nameOf(id)),
            icons.button("close", S.clearOne(cc, nameOf(id)), () =>
              bridge.send("learn_clear", id),
            ),
          ),
        ),
      );
      if (!state.map.length)
        mapList.append(el("li", { class: "ep-map-row ep-empty" }, S.empty));
      if (m.learned_cc !== undefined)
        toast(S.learned(m.learned_cc, nameOf(m.learned_target)));
      showLearn();
    });
    bridge.on("learned", (m) => drive(m.target, m.value));
    bridge.on("error", (m) => {
      if (m.during === "edit") toast(errorText(m));
    });

    if (document.addEventListener) {
      document.addEventListener("click", (e) => {
        const inside =
          popover.contains(e.target) || toolsButton.contains(e.target);
        if (!popover.hidden && !inside) closePopover();
      });
      document.addEventListener("keydown", (e) => {
        if (e.key === "Escape") {
          closePopover();
          if (learner.isOn()) setLearn(false);
          return;
        }
        const action = shortcut(e);
        if (!action || !shown()) return;
        const field = e.target && e.target.tagName;
        if (field === "INPUT" || field === "SELECT" || field === "TEXTAREA")
          return;
        e.preventDefault();
        const button = action === "undo" ? undo : redo;
        if (!button.disabled) button.click();
      });
    }

    showEdit(lastEdit);
    showLearn();
    bridge.send("edit_state");
    return {
      slot,
      popover,
      setView(name) {
        state.view = name;
        showLearn();
      },
      setConnected(on) {
        state.connected = !!on;
        showEdit(lastEdit);
      },
    };
  }

  /* Cmd/Ctrl Z: undo; with Shift: redo. */
  function shortcut(e) {
    if (!(e.metaKey || e.ctrlKey) || e.altKey) return null;
    if (String(e.key).toLowerCase() !== "z") return null;
    return e.shiftKey ? "redo" : "undo";
  }

  return { create, shortcut };
})();
