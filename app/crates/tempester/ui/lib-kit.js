/* The kit builder: put sounds of the library on the pads of the machine's current beat. A dialog with the sounds of the
   library on one side (search as you type) and the pads on the other (16 or 32, with each pad's sound name and type).
   A sound goes to a pad by selecting it and then clicking the pad, or by dragging it onto the pad; "Fill" puts the
   listed sounds on the pads in order. Nothing is written here: « Write to the machine's beat » asks Rust to look at the
   machine's beat (`lib_kit_check`); what would be replaced is confirmed in the usual send dialog (lib-flow.js), and
   only the sounds change in the beat. The decisions are plain functions (`createKit`, `assign`, `ids`...) so that the
   tests can drive them without a dialog. Loaded after lib-flow.js. */
"use strict";
window.Tempester = window.Tempester || {};
window.Tempester.libkit = (function () {
  const { el } = window.Tempester.kit;
  const icons = window.Tempester.icons;
  const dialogs = window.Tempester.dialogs;
  const flow = window.Tempester.libflow;

  const PADS = 32;
  const KEEP = "_";
  const S = {
    title: "Kit builder",
    lead: "Choose a sound for each pad, then write them to the machine's current beat. Only those sounds change: the beat's events, tempo and effects, and the pads you leave empty, stay as they are. The machine's own beat is looked at first, and kept as an auto backup when the library does not hold it.",
    search: "Search the sounds...",
    list: "Sounds of the library",
    none: "No sound in the library: fetch or import some first.",
    noMatch: "No sound matches the search.",
    empty: "empty",
    fill: "Fill the pads in order with the listed sounds",
    clear: "Clear all the pads",
    write: "Write to the machine's beat...",
    cancel: "Cancel",
    pads16: "16 pads",
    pads32: "32 pads",
  };

  /* ---- the decisions ---------------------------------------------------------------------------------------- */

  /* A kit: one slot per pad (a library sound item or null) and how many pads are in use (16 or 32). */
  function createKit(count) {
    return { slots: new Array(PADS).fill(null), count: count || 16 };
  }

  function assign(kitState, pad, item) {
    if (pad >= 0 && pad < kitState.count) kitState.slots[pad] = item;
  }

  function clear(kitState, pad) {
    kitState.slots[pad] = null;
  }

  function clearAll(kitState) {
    kitState.slots.fill(null);
  }

  /* The listed sounds on the pads from pad 1, as many as there are pads in use or sounds; pads beyond the list keep
     what they have. */
  function fill(kitState, items) {
    for (let pad = 0; pad < kitState.count && pad < items.length; pad++)
      kitState.slots[pad] = items[pad];
  }

  /* Pads 17-32 left out when only 16 are in use: their sounds are not kept, so they cannot be written. */
  function setCount(kitState, count) {
    kitState.count = count;
    for (let pad = count; pad < PADS; pad++) kitState.slots[pad] = null;
  }

  function filled(kitState) {
    return kitState.slots.slice(0, kitState.count).filter(Boolean).length;
  }

  /* What `lib_kit_check` takes: 32 entries, an item id per pad or `_` to keep the pad's sound. */
  function ids(kitState) {
    return kitState.slots
      .map((item, pad) => (item && pad < kitState.count ? item.id : KEEP))
      .join(",");
  }

  /* The sounds that match the search (name, type, folder), by name. */
  function matching(sounds, query) {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    return sounds
      .filter((item) => !item.slot)
      .filter((item) => {
        const text = (
          item.display +
          " " +
          (item.sound_type || "") +
          " " +
          item.folder
        ).toLowerCase();
        return words.every((w) => text.includes(w));
      })
      .sort((a, b) => a.display.localeCompare(b.display));
  }

  /* ---- the dialog ------------------------------------------------------------------------------------------- */

  function typeOf(item) {
    return item.sound_type || "";
  }

  async function open(sounds) {
    const state = createKit(16);
    const view = { query: "", selected: null };
    const els = {};

    function listed() {
      return matching(sounds, view.query);
    }

    function renderList() {
      const items = listed();
      els.list.replaceChildren(
        ...(items.length
          ? items.map((item) => soundRow(item))
          : [el("li", { class: "lib-empty" }, sounds.length ? S.noMatch : S.none)]),
      );
    }

    function soundRow(item) {
      const row = el(
        "li",
        {
          class: "row-item kit-sound",
          role: "option",
          draggable: "true",
          "aria-selected": String(view.selected === item.id),
          title: item.display + " " + typeOf(item),
        },
        el("span", {}, item.display),
        el("span", { class: "mid" }, typeOf(item)),
      );
      row.addEventListener("click", () => {
        view.selected = view.selected === item.id ? null : item.id;
        renderList();
      });
      row.addEventListener("dragstart", (e) => {
        if (e.dataTransfer) e.dataTransfer.setData("text/plain", item.id);
        view.selected = item.id;
      });
      return row;
    }

    const byId = (id) => sounds.find((s) => s.id === id) || null;

    function padTile(pad) {
      const item = state.slots[pad];
      const tile = el(
        "button",
        {
          type: "button",
          class: "kit-pad" + (item ? " set" : ""),
          "aria-label":
            "Pad " + (pad + 1) + ": " + (item ? item.display + " " + typeOf(item) : S.empty),
          title: item ? item.display + " " + typeOf(item) : S.empty,
        },
        el("span", { class: "kit-pad-no" }, String(pad + 1)),
        el("span", { class: "kit-pad-name" }, item ? item.display : S.empty),
        el("span", { class: "kit-pad-type mid" }, item ? typeOf(item) : ""),
      );
      tile.addEventListener("click", () => {
        const chosen = view.selected ? byId(view.selected) : null;
        if (chosen) assign(state, pad, chosen);
        else clear(state, pad);
        renderGrid();
      });
      tile.addEventListener("dragover", (e) => {
        if (e.preventDefault) e.preventDefault();
      });
      tile.addEventListener("drop", (e) => {
        if (e.preventDefault) e.preventDefault();
        const id = e.dataTransfer ? e.dataTransfer.getData("text/plain") : "";
        const dropped = byId(id);
        if (dropped) assign(state, pad, dropped);
        renderGrid();
      });
      return tile;
    }

    function renderGrid() {
      els.grid.replaceChildren(
        ...Array.from({ length: state.count }, (_, pad) => padTile(pad)),
      );
      els.count.textContent =
        filled(state) + " of " + state.count + " pads have a sound.";
      if (els.write) els.write.disabled = filled(state) === 0;
      for (const [n, button] of Object.entries(els.seg))
        button.setAttribute("aria-pressed", String(Number(n) === state.count));
    }

    els.search = el("input", {
      type: "text",
      class: "lib-search",
      placeholder: S.search,
      "aria-label": S.search,
    });
    els.search.addEventListener("input", () => {
      view.query = els.search.value;
      renderList();
    });
    els.seg = {};
    const seg = el("div", { class: "seg", role: "group", "aria-label": "Pads" });
    for (const [count, label] of [
      [16, S.pads16],
      [32, S.pads32],
    ]) {
      els.seg[count] = el("button", { type: "button" }, label);
      els.seg[count].addEventListener("click", () => {
        setCount(state, count);
        renderGrid();
      });
      seg.append(els.seg[count]);
    }
    els.list = el("ul", { class: "dense kit-list", role: "listbox", "aria-label": S.list });
    els.grid = el("div", { class: "kit-grid" });
    els.count = el("p", { class: "kit-count", role: "status" });
    renderList();
    renderGrid();

    const body = [
      el("p", {}, S.lead),
      el(
        "div",
        { class: "toolbar" },
        els.search,
        seg,
        icons.button("kit", S.fill, () => {
          fill(state, listed());
          renderGrid();
        }),
        icons.button("trash", S.clear, () => {
          clearAll(state);
          renderGrid();
        }),
      ),
      el("div", { class: "kit-cols" }, els.list, els.grid),
      els.count,
    ];
    const answer = await dialogs.ask({
      title: S.title,
      className: "wide kit",
      body,
      buttons: [
        { label: S.cancel, value: "cancel" },
        { label: S.write, value: "write", variant: "primary", disabled: true },
      ],
      onReady: ({ buttons }) => {
        els.write = buttons.write;
        renderGrid();
      },
    });
    if (answer === "write" && filled(state) > 0) flow.kitCheck(ids(state));
    return answer === "write" ? ids(state) : null;
  }

  return { open, createKit, assign, clear, clearAll, fill, setCount, filled, ids, matching };
})();
