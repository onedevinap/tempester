/* The library browser: one list per kind (Sounds / Beats / Projects) with search as you type, filters, sort, counts,
   keyboard navigation, multi-select, per-row icon actions and a details panel for the selected item (metadata, tags,
   note, history with compare and restore). The decisions (what matches, what order, what is selected) are in
   lib-model.js; the dialogs in lib-dialogs.js; the messages in lib-flow.js. Rows have a fixed height and columns of
   fixed width: nothing moves when a name, a date or a badge changes. */
"use strict";
window.Tempester = window.Tempester || {};
window.Tempester.libbrowser = (function () {
  const { el } = window.Tempester.kit;
  const bridge = window.Tempester.bridge;
  const icons = window.Tempester.icons;
  const model = window.Tempester.libmodel;
  const dialogs = window.Tempester.dialogs;
  const flow = window.Tempester.libflow;

  const KINDS = ["sounds", "beats", "projects"];
  const LABEL = { sounds: "Sounds", beats: "Beats", projects: "Projects" };
  const ONE = { sounds: "sound", beats: "beat", projects: "project" };
  const KIND_ICON = { sounds: "sound", beats: "beat", projects: "project" };
  const SORT_LABEL = { name: "Name", date: "Date modified", folder: "Folder" };

  const S = {
    search: "Search name, tag, note, folder...",
    filters: "Filters",
    none: "Nothing in the library yet. Fetch something from the machine, or import a .syx file.",
    noMatch: "Nothing matches the search and the filters.",
    details:
      "Select an item to see its details, or press the arrow keys in the list.",
  };

  function clickHandler(fn) {
    return (e) => {
      if (e && e.stopPropagation) e.stopPropagation();
      fn(e || {});
    };
  }

  function typeText(item) {
    if (item.slot) return "backups";
    if (item.kind === "projects" || item.beats !== null)
      return item.beats + " beats";
    return item.sound_type || (item.osc.length ? "" : "beat");
  }

  function create() {
    const state = {
      kind: "sounds",
      items: { sounds: [], beats: [], projects: [] },
      query: "",
      filters: Object.assign({}, model.NO_FILTERS),
      sort: { key: "name", dir: "asc" },
      sel: model.createSelection(),
      showFilters: false,
      trash: [],
      history: {},
      compare: {},
    };
    const els = {};
    let shown = [];
    let menu = null;
    let detailsKey = "";

    const idsShown = () => shown.map((i) => i.id);
    const selected = () => shown.filter((i) => state.sel.ids.has(i.id));
    const active = () => shown.find((i) => i.id === state.sel.active) || null;

    /* ---- commands ------------------------------------------------------------------------------------------- */

    const send = (name, ...args) => bridge.send(name, ...args);
    const idsOf = (items) => items.map((i) => i.id).join(",");

    function open(item) {
      if (item.kind === "projects" || item.slot) return;
      send("lib_open", item.id);
    }

    /* A project goes back whole (its file, never the as-received capture); an auto backup is sent from its history. */
    function sendToMachine(item) {
      if (item.slot) return;
      if (item.kind === "projects") flow.projectCheck(item.id, null);
      else flow.sendCheck(item.kind, item.id);
    }

    async function rename(item) {
      const project = item.kind === "projects";
      const name = await dialogs.prompt({
        title: 'Rename "' + item.name + '"',
        lines: project
          ? [
              "A project holds no name of its own: only its file name changes (" +
                item.path +
                ").",
            ]
          : [
              "The name inside the data (the 20 characters the machine reads) and the file name change together. The previous version is kept in the item's history.",
            ],
        label: project
          ? "New name"
          : "New name (20 characters at most, printable ASCII)",
        value: item.name,
        okLabel: "Rename",
        validate: (text) =>
          project
            ? text.trim()
              ? null
              : "the name is empty"
            : model.nameRefusal(text),
      });
      if (name !== null && name !== item.name)
        send("lib_rename", item.id, name);
    }

    async function tag(items) {
      const text = await dialogs.prompt({
        title: "Add a tag to " + items.length + " item(s)",
        lines: [
          "Tags are kept in the library index only; the files are not touched.",
        ],
        label: "Tag",
        value: "",
        okLabel: "Add tag",
        options: model.facetOptions(state.items[state.kind]).tags,
        validate: (t) =>
          !t.trim() || t.includes(",") || t.length > 32
            ? "1 to 32 characters, no commas"
            : null,
      });
      if (text !== null) send("lib_tag", idsOf(items), "add", text);
    }

    async function move(items) {
      const folders = model.facetOptions(state.items[state.kind]).folders;
      const folder = await dialogs.prompt({
        title: "Move " + items.length + " item(s) to a folder",
        lines: [
          "The files move to a folder of " +
            LABEL[state.kind] +
            ". Nothing is overwritten: if a file of the same name is already there, nothing moves. Leave empty for the main folder.",
        ],
        label: "Folder",
        value: "",
        okLabel: "Move",
        options: folders,
        validate: () => null,
      });
      if (folder !== null) send("lib_move", idsOf(items), folder);
    }

    async function remove(items) {
      const ok = await dialogs.confirm({
        title: "Move " + items.length + " item(s) to the trash?",
        lines: [
          items.length === 1
            ? '"' + items[0].name + '" leaves the list.'
            : items.length + " items leave the list.",
          "Nothing is deleted for good: they wait in the trash (history included) and can be restored from there.",
        ],
        okLabel: "Move to the trash",
        danger: true,
      });
      if (ok) send("lib_delete", idsOf(items));
    }

    function favourite(items) {
      const all = items.every((i) => i.favourite);
      send("lib_favourite", idsOf(items), all ? "0" : "1");
    }

    async function restoreVersion(item, version) {
      const v = state.history[item.id].versions[version];
      const ok = await dialogs.confirm({
        title: "Restore the version of " + model.formatDate(v.date) + "?",
        lines: [
          '"' + item.name + '" goes back to that version (' + v.label + ").",
          "The current version (" +
            model.formatDate(item.modified) +
            ") is kept in the history first: nothing is lost, and you can restore it again.",
        ],
        okLabel: "Restore",
        danger: true,
      });
      if (ok) send("lib_restore", item.id, String(version));
    }

    function showTrash() {
      dialogs.trash(state.trash, {
        restore: (id) => send("lib_restore_trash", id),
        empty: (count) => send("lib_empty_trash", String(count)),
      });
    }

    function showLog() {
      const rows = flow.log.length
        ? flow.log.map((e) =>
            el(
              "li",
              { class: e.bad ? "older" : "" },
              el("strong", {}, e.title),
              e.lines.map((line) => el("div", { class: "mid" }, line)),
            ),
          )
        : [el("li", {}, "Nothing yet.")];
      dialogs.ask({
        title: "Messages",
        className: "wide",
        body: [el("ul", { class: "lib-entries log" }, rows)],
        buttons: [{ label: "Close", value: "cancel" }],
      });
    }

    /* ---- menu ----------------------------------------------------------------------------------------------- */

    function closeMenu() {
      if (!menu) return;
      if (menu.remove) menu.remove();
      menu = null;
    }

    function openMenu(anchor, entries) {
      closeMenu();
      menu = el(
        "div",
        { class: "lib-menu", role: "menu" },
        entries.map(([iconName, label, run, danger]) =>
          el(
            "button",
            {
              class: "lib-menu-item" + (danger ? " danger" : ""),
              role: "menuitem",
              type: "button",
              onclick: () => {
                closeMenu();
                run();
              },
            },
            icons.icon(iconName),
            label,
          ),
        ),
      );
      const box = anchor.getBoundingClientRect
        ? anchor.getBoundingClientRect()
        : null;
      if (box) {
        const up = box.bottom + 8 * 38 > window.innerHeight;
        menu.style.top =
          (up
            ? Math.max(8, box.top - entries.length * 38 - 8)
            : box.bottom + 4) + "px";
        menu.style.left = Math.max(8, box.right - 220) + "px";
      }
      (document.body || els.root).append(menu);
    }

    function rowMenu(item, anchor) {
      const entries = [];
      if (!item.slot) {
        entries.push(["rename", "Rename...", () => rename(item)]);
        entries.push(["tag", "Add a tag...", () => tag([item])]);
        entries.push(["move", "Move to a folder...", () => move([item])]);
        entries.push([
          "duplicate",
          "Duplicate",
          () => send("lib_duplicate", item.id),
        ]);
      }
      if (item.kind === "beats" && !item.slot)
        entries.push([
          "export",
          "Export as a MIDI file",
          () => send("lib_export_midi", item.id),
        ]);
      entries.push(["history", "History", () => showHistory(item)]);
      if (!item.slot)
        entries.push([
          "trash",
          "Move to the trash...",
          () => remove([item]),
          true,
        ]);
      openMenu(anchor, entries);
    }

    if (document.addEventListener) {
      document.addEventListener("keydown", (e) => {
        if (e.key === "Escape") closeMenu();
      });
      document.addEventListener("click", (e) => {
        if (menu && !(menu.contains && menu.contains(e.target))) closeMenu();
      });
    }

    /* ---- the list ------------------------------------------------------------------------------------------- */

    function chips(item) {
      const first = item.tags
        .slice(0, 2)
        .map((t) => el("span", { class: "chip" }, t));
      if (item.tags.length > 2)
        first.push(
          el(
            "span",
            { class: "chip", title: item.tags.slice(2).join(", ") },
            "+" + (item.tags.length - 2),
          ),
        );
      return first;
    }

    function row(item) {
      const picked = state.sel.ids.has(item.id);
      const star = icons.button(
        item.favourite ? "star-filled" : "star",
        item.favourite ? "Remove from favourites" : "Add to favourites",
        clickHandler(() => favourite([item])),
      );
      if (item.favourite)
        star.setAttribute("class", star.getAttribute("class") + " star-on");
      const box = el("input", {
        type: "checkbox",
        "aria-label": "Select " + item.name,
      });
      box.checked = picked;
      box.addEventListener("click", (e) => {
        if (e && e.stopPropagation) e.stopPropagation();
        select(item, { ctrl: true });
      });
      const noOpen = item.kind === "projects" || item.slot;
      const noSend = item.slot;
      const node = el(
        "div",
        {
          class:
            "row-item lib-row" +
            (item.id === state.sel.active ? " active" : ""),
          role: "option",
          "aria-selected": String(picked),
          "data-id": item.id,
        },
        el("span", { class: "col-check" }, box),
        star,
        el(
          "span",
          {
            class: "col-name",
            title: item.name + (item.path ? "  (" + item.path + ")" : ""),
          },
          item.slot ? icons.icon("history") : null,
          item.name,
        ),
        el(
          "span",
          { class: "col-folder mid", title: item.folder },
          item.folder || "-",
        ),
        el("span", { class: "col-type mid" }, typeText(item)),
        el("span", { class: "col-date mid" }, model.formatDate(item.modified)),
        el("span", { class: "col-tags" }, chips(item)),
        el(
          "span",
          { class: "col-act" },
          icons.button(
            "open",
            noOpen
              ? "Open (not available for " +
                  (item.slot
                    ? "auto backups: open a version from its history"
                    : "projects") +
                  ")"
              : "Open in the card above",
            clickHandler(() => open(item)),
            {},
          ),
          icons.button(
            "send",
            noSend
              ? "Send (open a version from its history)"
              : item.kind === "projects"
                ? "Send the whole project to the machine..."
                : "Send to the machine...",
            clickHandler(() => sendToMachine(item)),
          ),
          icons.button(
            "more",
            "More actions",
            clickHandler((e) => rowMenu(item, (e && e.currentTarget) || node)),
          ),
        ),
      );
      const [openButton, sendButton] = Array.from(node.children[7].children);
      openButton.disabled = noOpen;
      sendButton.disabled = noSend;
      node.libBox = box;
      node.addEventListener("click", (e) => select(item, e || {}));
      node.addEventListener("dblclick", () => open(item));
      return node;
    }

    function select(item, mods) {
      model.selectClick(state.sel, idsShown(), item.id, {
        shift: !!mods.shiftKey || !!mods.shift,
        ctrl: !!mods.ctrlKey || !!mods.metaKey || !!mods.ctrl,
      });
      afterSelection();
    }

    /* A selection change only re-marks the rows (the list keeps its scroll position and focus). */
    function markRows() {
      let activeNode = null;
      for (const node of Array.from(els.list.children)) {
        const id = node.getAttribute("data-id");
        if (id === null) continue;
        const picked = state.sel.ids.has(id);
        const isActive = id === state.sel.active;
        node.setAttribute("aria-selected", String(picked));
        node.setAttribute(
          "class",
          "row-item lib-row" + (isActive ? " active" : ""),
        );
        if (node.libBox) node.libBox.checked = picked;
        if (isActive) activeNode = node;
      }
      return activeNode;
    }

    function afterSelection() {
      const node = markRows();
      renderDetails();
      renderBar();
      if (node && node.scrollIntoView)
        node.scrollIntoView({ block: "nearest" });
      const it = active();
      if (it && !it.slot && !state.history[it.id] && it.history > 0)
        send("lib_history", it.id);
    }

    function renderList() {
      const total = state.items[state.kind].length;
      if (!shown.length) {
        els.list.replaceChildren(
          el("div", { class: "lib-empty mid" }, total ? S.noMatch : S.none),
        );
        return;
      }
      els.list.replaceChildren(...shown.map(row));
    }

    function renderBar() {
      const total = state.items[state.kind].length;
      const count = state.sel.ids.size;
      els.count.textContent =
        shown.length +
        " of " +
        total +
        " " +
        LABEL[state.kind].toLowerCase() +
        (count ? " - " + count + " selected" : "");
      const items = selected().filter((i) => !i.slot);
      for (const button of els.batch) button.disabled = !items.length;
      els.clear.disabled = !count;
      els.filterButton.setAttribute("aria-pressed", String(state.showFilters));
      const n = model.activeFilterCount(state.filters);
      els.filterCount.textContent = n ? String(n) : "";
      els.filterCount.hidden = !n;
      els.filters.hidden = !state.showFilters;
      els.dir.replaceChildren(
        icons.icon(state.sort.dir === "asc" ? "sort-asc" : "sort-desc"),
      );
      els.dir.setAttribute(
        "aria-label",
        "Sort direction: " +
          (state.sort.dir === "asc" ? "ascending" : "descending"),
      );
      els.dir.setAttribute("data-tip", els.dir.getAttribute("aria-label"));
      for (const [key, node] of Object.entries(els.heads))
        node.setAttribute(
          "aria-sort",
          state.sort.key === key
            ? state.sort.dir === "asc"
              ? "ascending"
              : "descending"
            : "none",
        );
    }

    /* ---- the details panel ---------------------------------------------------------------------------------- */

    function fact(label, value) {
      return [el("dt", {}, label), el("dd", { title: String(value) }, value)];
    }

    function versionRows(item) {
      const known = state.history[item.id];
      if (!known)
        return [
          el(
            "li",
            { class: "mid" },
            item.history ? "Loading..." : "No older version.",
          ),
        ];
      if (!known.versions.length)
        return [el("li", { class: "mid" }, "No older version.")];
      return known.versions.map((v) => {
        const diff = state.compare[item.id + ":" + v.index];
        return el(
          "li",
          { class: "lib-version" },
          el(
            "div",
            { class: "lib-version-head" },
            el(
              "span",
              { title: v.label },
              model.formatDate(v.date) +
                (v.same_as_current ? " (same as now)" : ""),
            ),
            icons.button("compare", "Compare with the current version", () =>
              send("lib_compare", item.id, String(v.index)),
            ),
            item.kind === "projects"
              ? icons.button(
                  "send",
                  "Send this version to the machine as a whole project...",
                  () => flow.projectCheck(item.id, v.index),
                )
              : icons.button("open", "Open this version in the card above", () =>
                  send("lib_open_version", item.id, String(v.index)),
                ),
            item.slot
              ? null
              : icons.button("restore", "Restore this version...", () =>
                  restoreVersion(item, v.index),
                ),
          ),
          el("div", { class: "mid" }, v.label),
          diff
            ? el(
                "ul",
                { class: "lib-diff" },
                diff.map((line) => el("li", {}, line)),
              )
            : null,
        );
      });
    }

    function showHistory(item) {
      state.sel.ids = new Set([item.id]);
      state.sel.active = item.id;
      delete state.history[item.id];
      send("lib_history", item.id);
      afterSelection();
    }

    function singleDetails(item) {
      const actions = el(
        "div",
        { class: "toolbar lib-actions" },
        item.slot
          ? null
          : icons.button("open", "Open in the card above", () => open(item)),
        item.slot
          ? null
          : icons.button(
              "send",
              item.kind === "projects"
                ? "Send the whole project to the machine..."
                : "Send to the machine...",
              () => sendToMachine(item),
            ),
        item.kind === "beats" && !item.slot
          ? icons.button("export", "Export as a MIDI file", () =>
              send("lib_export_midi", item.id),
            )
          : null,
        item.slot
          ? null
          : icons.button("rename", "Rename...", () => rename(item)),
        item.slot
          ? null
          : icons.button("tag", "Add a tag...", () => tag([item])),
        item.slot
          ? null
          : icons.button("move", "Move to a folder...", () => move([item])),
        item.slot
          ? null
          : icons.button("duplicate", "Duplicate", () =>
              send("lib_duplicate", item.id),
            ),
        item.slot
          ? null
          : icons.button(
              "trash",
              "Move to the trash...",
              () => remove([item]),
              { variant: "danger" },
            ),
      );
      if (item.kind === "projects" && !item.slot)
        actions.children[0].disabled = true;
      const note = el("textarea", {
        class: "lib-note",
        rows: "3",
        "aria-label": "Note",
        maxlength: "2000",
      });
      note.value = item.note;
      note.addEventListener("blur", () => {
        if (note.value !== item.note) send("lib_note", item.id, note.value);
      });
      const tagNodes = item.tags.map((t) =>
        el(
          "button",
          {
            class: "chip on",
            type: "button",
            title: 'Remove the tag "' + t + '"',
            onclick: () => send("lib_tag", item.id, "remove", t),
          },
          t + " x",
        ),
      );
      return [
        el(
          "div",
          { class: "lib-title" },
          icons.icon(KIND_ICON[item.kind]),
          el("h3", { title: item.name }, item.name),
          item.slot
            ? null
            : icons.button(
                item.favourite ? "star-filled" : "star",
                item.favourite ? "Remove from favourites" : "Add to favourites",
                () => favourite([item]),
              ),
        ),
        actions,
        el(
          "dl",
          { class: "lib-facts" },
          item.slot ? [] : fact("File", item.kind + "/" + item.path),
          item.slot ? [] : fact("Folder", item.folder || "(main folder)"),
          fact("Modified", model.formatDate(item.modified)),
          fact("Created", model.formatDate(item.created)),
          item.fetched ? fact("Fetched", model.formatDate(item.fetched)) : [],
          fact("Source", item.source),
          item.slot ? [] : fact("Size", item.size + " bytes"),
          item.hash ? fact("Hash", item.hash.slice(0, 16)) : [],
          item.sound_type ? fact("Sound Type", item.sound_type) : [],
          item.osc.length ? fact("Oscillators", item.osc.join(", ")) : [],
          item.beats !== null && item.kind === "projects"
            ? fact("Beats", item.beats + " of 16")
            : [],
          item.capture ? fact("Capture", item.capture) : [],
        ),
        item.slot
          ? el("p", { class: "mid" }, item.note)
          : el(
              "div",
              { class: "lib-tags" },
              tagNodes.length
                ? tagNodes
                : el("span", { class: "mid" }, "No tag"),
            ),
        item.slot ? null : note,
        el(
          "div",
          { class: "lib-hist-head" },
          el("strong", {}, "History (" + item.history + ")"),
          icons.info(
            "Older versions of this item: what an update, a rename or a restore replaced. Compare one with the current version, open it in the card above, or restore it (the current one is kept first).",
          ),
        ),
        el(
          "ul",
          { class: "lib-versions" },
          item.history || item.slot
            ? versionRows(item)
            : [el("li", { class: "mid" }, "No older version.")],
        ),
      ];
    }

    function manyDetails(items) {
      return [
        el(
          "div",
          { class: "lib-title" },
          el("h3", {}, items.length + " items selected"),
        ),
        el(
          "div",
          { class: "toolbar lib-actions" },
          icons.button("tag", "Add a tag to all...", () => tag(items)),
          icons.button("star", "Favourite / unfavourite all", () =>
            favourite(items),
          ),
          icons.button("move", "Move all to a folder...", () => move(items)),
          icons.button(
            "trash",
            "Move all to the trash...",
            () => remove(items),
            { variant: "danger" },
          ),
        ),
        el("p", { class: "mid" }, "Press Escape to clear the selection."),
      ];
    }

    function renderDetails() {
      const picked = selected();
      const item = active();
      let content;
      if (picked.length > 1)
        content = manyDetails(picked.filter((i) => !i.slot));
      else if (item) content = singleDetails(item);
      else content = [el("p", { class: "mid" }, S.details)];
      /* A child that is null would be written as the text "null" by replaceChildren. */
      els.details.replaceChildren(...content.filter(Boolean));
      /* A different selection starts at the top of the panel; a refresh of the same one keeps its place. */
      const key = picked.length > 1 ? "many" : item ? item.id : "";
      if (key !== detailsKey) els.details.scrollTop = 0;
      detailsKey = key;
    }

    /* ---- filters and the toolbar ---------------------------------------------------------------------------- */

    function menuSelect(label, key, options, allLabel) {
      const select = el("select", { "aria-label": label });
      select.addEventListener("change", () => {
        state.filters[key] = select.value;
        refresh();
      });
      els.filterSelects[key] = { select, options, allLabel };
      return select;
    }

    function fillSelect(key, values, extra) {
      const { select, allLabel } = els.filterSelects[key];
      const current = state.filters[key];
      const all = [
        ["", allLabel],
        ...(extra || []),
        ...values.map((v) => [v, v]),
      ];
      select.replaceChildren(
        ...all.map(([value, text]) => el("option", { value }, text)),
      );
      select.value = all.some(([value]) => value === current) ? current : "";
      state.filters[key] = select.value;
    }

    function fillFilters() {
      const f = model.facetOptions(state.items[state.kind]);
      fillSelect(
        "folder",
        f.folders,
        f.hasRoot ? [["(none)", "Main folder"]] : [],
      );
      fillSelect("tag", f.tags);
      fillSelect("source", f.sources);
      fillSelect("soundType", f.soundTypes);
      fillSelect("osc", f.oscs);
      const sounds = state.kind === "sounds";
      els.soundOnly.forEach((node) => (node.hidden = !sounds));
    }

    function buildFilters() {
      els.filterSelects = {};
      const dateInput = (key, label) => {
        const input = el("input", { type: "date", "aria-label": label });
        input.addEventListener("change", () => {
          state.filters[key] = input.value;
          refresh();
        });
        return input;
      };
      const fav = el(
        "button",
        { class: "chip", type: "button", "aria-pressed": "false" },
        "Favourites only",
      );
      fav.addEventListener("click", () => {
        state.filters.favourites = !state.filters.favourites;
        fav.setAttribute("aria-pressed", String(state.filters.favourites));
        fav.setAttribute(
          "class",
          "chip" + (state.filters.favourites ? " on" : ""),
        );
        refresh();
      });
      const type = menuSelect("Sound Type", "soundType", [], "Any Sound Type");
      const osc = menuSelect(
        "Oscillator type",
        "osc",
        [],
        "Any oscillator type",
      );
      els.soundOnly = [type, osc];
      els.favChip = fav;
      els.from = dateInput("from", "Modified from");
      els.to = dateInput("to", "Modified until");
      const clear = icons.button("close", "Clear all filters", () => {
        state.filters = Object.assign({}, model.NO_FILTERS);
        els.from.value = "";
        els.to.value = "";
        fav.setAttribute("class", "chip");
        fav.setAttribute("aria-pressed", "false");
        fillFilters();
        refresh();
      });
      return el(
        "div",
        { class: "lib-filters", hidden: true },
        fav,
        menuSelect("Folder or project", "folder", [], "Any folder"),
        menuSelect("Tag", "tag", [], "Any tag"),
        menuSelect("Source", "source", [], "Any source"),
        type,
        osc,
        el("label", {}, "Modified from"),
        els.from,
        el("label", {}, "until"),
        els.to,
        clear,
      );
    }

    function setSort(key) {
      if (state.sort.key === key)
        state.sort.dir = state.sort.dir === "asc" ? "desc" : "asc";
      else state.sort = { key, dir: key === "date" ? "desc" : "asc" };
      els.sortSelect.value = state.sort.key;
      refresh();
    }

    function buildToolbar() {
      els.seg = {};
      const seg = el("div", {
        class: "seg",
        role: "group",
        "aria-label": "Library",
      });
      for (const kind of KINDS) {
        els.seg[kind] = el(
          "button",
          { type: "button", onclick: () => switchKind(kind) },
          LABEL[kind],
        );
        seg.append(els.seg[kind]);
      }
      els.search = el("input", {
        type: "text",
        id: "lib-search",
        class: "lib-search",
        placeholder: S.search,
        "aria-label": "Search the library",
      });
      els.search.addEventListener("input", () => {
        state.query = els.search.value;
        refresh();
      });
      els.filterButton = icons.button(
        "filter",
        S.filters,
        () => {
          state.showFilters = !state.showFilters;
          renderBar();
        },
        { pressed: false },
      );
      els.filterCount = el("span", {
        class: "chip on lib-fcount",
        hidden: true,
      });
      els.sortSelect = el(
        "select",
        { "aria-label": "Sort by" },
        Object.entries(SORT_LABEL).map(([value, text]) =>
          el("option", { value }, "Sort: " + text),
        ),
      );
      els.sortSelect.value = "name";
      els.sortSelect.addEventListener("change", () => {
        state.sort.key = els.sortSelect.value;
        refresh();
      });
      els.dir = icons.button("sort-asc", "Sort direction", () => {
        state.sort.dir = state.sort.dir === "asc" ? "desc" : "asc";
        refresh();
      });
      const picker = el("input", {
        type: "file",
        accept: ".syx",
        multiple: true,
        class: "hidden-input",
        tabindex: "-1",
      });
      picker.addEventListener("change", async () => {
        const files = Array.from(picker.files || []);
        picker.value = "";
        flow.importFiles(
          await Promise.all(files.map((f) => bridge.readFileHex(f))),
        );
      });
      els.kitButton = icons.button(
        "kit",
        "Kit builder: put sounds of the library on the pads of the machine's beat...",
        () => window.Tempester.libkit.open(state.items.sounds),
      );
      els.kitButton.disabled = !window.Tempester.kit.session.connected;
      window.Tempester.kit.onSession((session) => {
        els.kitButton.disabled = !session.connected;
      });
      els.trashButton = icons.button("trash", "Trash", () => showTrash());
      els.trashCount = el("span", { class: "chip", hidden: true });
      els.logButton = icons.button(
        "list",
        "Messages: what the library did",
        () => showLog(),
      );
      return el(
        "div",
        { class: "toolbar lib-toolbar" },
        seg,
        el("span", { class: "sep" }),
        icons.icon("search"),
        els.search,
        els.filterButton,
        els.filterCount,
        els.sortSelect,
        els.dir,
        el("span", { class: "grow" }),
        el(
          "span",
          {},
          icons.button("import", "Import .syx files into the library...", () =>
            picker.click(),
          ),
          picker,
        ),
        els.kitButton,
        icons.button("refresh", "Refresh the list", () => requestAll()),
        icons.button("reveal", "Show this folder in the file manager", () =>
          send("reveal", state.kind),
        ),
        el("span", { class: "with-count" }, els.trashButton, els.trashCount),
        els.logButton,
      );
    }

    function buildBar() {
      els.count = el("span", { class: "count", role: "status" });
      els.batch = [
        icons.button("tag", "Add a tag to the selection...", () =>
          tag(selected().filter((i) => !i.slot)),
        ),
        icons.button("star", "Favourite / unfavourite the selection", () =>
          favourite(selected().filter((i) => !i.slot)),
        ),
        icons.button("move", "Move the selection to a folder...", () =>
          move(selected().filter((i) => !i.slot)),
        ),
        icons.button(
          "trash",
          "Move the selection to the trash...",
          () => remove(selected().filter((i) => !i.slot)),
          { variant: "danger" },
        ),
      ];
      els.clear = icons.button("close", "Clear the selection", () => {
        state.sel = model.createSelection();
        afterSelection();
      });
      const all = icons.button("select-all", "Select all shown", () => {
        model.selectAll(state.sel, idsShown());
        afterSelection();
      });
      return el(
        "div",
        { class: "toolbar lib-bar" },
        els.count,
        el("span", { class: "grow" }),
        all,
        ...els.batch,
        els.clear,
      );
    }

    function buildHeads() {
      els.heads = {};
      const head = (key, text, sortable) => {
        const node = sortable
          ? el(
              "button",
              {
                type: "button",
                class: "lib-head-btn",
                onclick: () => setSort(key),
              },
              text,
            )
          : el("span", {}, text);
        els.heads[key] = node;
        return node;
      };
      return el(
        "div",
        { class: "lib-head" },
        el("span", {}),
        head("favourite", "Fav", true),
        head("name", "Name", true),
        head("folder", "Folder", true),
        head("type", "Type", false),
        head("date", "Modified", true),
        head("tags", "Tags", false),
        el("span", {}),
      );
    }

    /* ---- wiring ---------------------------------------------------------------------------------------------- */

    function switchKind(kind) {
      state.kind = kind;
      state.sel = model.createSelection();
      state.filters = Object.assign({}, model.NO_FILTERS);
      els.from.value = "";
      els.to.value = "";
      els.favChip.setAttribute("class", "chip");
      refresh();
    }

    function refresh() {
      fillFilters();
      const all = state.items[state.kind];
      shown = model.sortItems(
        model.filterItems(all, state.query, state.filters),
        state.sort.key,
        state.sort.dir,
      );
      model.pruneSelection(state.sel, idsShown());
      for (const kind of KINDS) {
        els.seg[kind].textContent =
          LABEL[kind] + " (" + state.items[kind].length + ")";
        els.seg[kind].setAttribute("aria-pressed", String(kind === state.kind));
      }
      renderList();
      renderDetails();
      renderBar();
    }

    function requestAll() {
      for (const kind of KINDS) send("library", kind);
      send("lib_trash");
    }

    function onKey(e) {
      const ids = idsShown();
      const keys = {
        ArrowDown: "down",
        ArrowUp: "up",
        Home: "home",
        End: "end",
        PageDown: "pageDown",
        PageUp: "pageUp",
      };
      const stop = () => e.preventDefault && e.preventDefault();
      if (keys[e.key]) {
        stop();
        model.selectMove(state.sel, ids, keys[e.key], !!e.shiftKey);
        afterSelection();
      } else if (e.key === " " && state.sel.active) {
        stop();
        model.selectClick(state.sel, ids, state.sel.active, { ctrl: true });
        afterSelection();
      } else if ((e.key === "a" || e.key === "A") && (e.ctrlKey || e.metaKey)) {
        stop();
        model.selectAll(state.sel, ids);
        afterSelection();
      } else if (e.key === "Escape") {
        state.sel = model.createSelection();
        afterSelection();
      } else if (e.key === "Enter" && active()) {
        stop();
        open(active());
      } else if (
        (e.key === "Delete" || e.key === "Backspace") &&
        selected().length
      ) {
        stop();
        remove(selected().filter((i) => !i.slot));
      }
    }

    function build() {
      els.filters = buildFilters();
      els.root = el("div", { class: "card lib-browser" });
      els.list = el("div", {
        class: "lib-list dense",
        role: "listbox",
        tabindex: "0",
        "aria-multiselectable": "true",
        "aria-label": "Library items",
      });
      els.list.addEventListener("keydown", onKey);
      els.details = el("aside", {
        class: "pane-side lib-details",
        "aria-label": "Details",
      });
      els.status = el("div", { class: "lib-status", role: "status" });
      els.root.append(
        buildToolbar(),
        els.filters,
        buildBar(),
        el(
          "div",
          { class: "split lib-split" },
          el("div", { class: "lib-listwrap" }, buildHeads(), els.list),
          els.details,
        ),
        els.status,
      );
      flow.onLog((log) => {
        const last = log[0];
        const text = last ? last.title + ": " + (last.lines[0] || "") : "";
        const more =
          last && last.lines.length > 1
            ? el("span", { class: "chip" }, "+" + (last.lines.length - 1) + " more")
            : null;
        els.status.replaceChildren(
          ...[
            icons.icon(last && last.bad ? "warning" : "check"),
            el("span", { class: "lib-status-text", title: text }, text),
            more,
          ].filter(Boolean),
        );
        els.status.setAttribute(
          "class",
          "lib-status" + (last && last.bad ? " bad" : ""),
        );
      });
      els.status.append(
        icons.icon("info"),
        el(
          "span",
          { class: "lib-status-text mid" },
          "Messages about what the library did appear here.",
        ),
      );
      bridge.on("library", (m) => {
        if (!state.items[m.kind]) return;
        /* Rust sends the file path as `name` and the internal name as `display`; the page says name and path. */
        state.items[m.kind] = m.files.map((f) =>
          Object.assign({}, f, { path: f.name, name: f.display }),
        );
        if (typeof m.trash === "number") setTrashCount(m.trash);
        refresh();
      });
      bridge.on("lib_trash", (m) => {
        state.trash = m.items;
        setTrashCount(m.items.length);
      });
      bridge.on("lib_history", (m) => {
        state.history[m.id] = m;
        renderDetails();
      });
      bridge.on("lib_compare", (m) => {
        state.compare[m.id + ":" + m.version] = m.lines;
        renderDetails();
      });
      bridge.on("lib_done", () => {
        state.history = {};
        state.compare = {};
        const it = active();
        if (it && it.history) send("lib_history", it.id);
      });
      refresh();
      return els.root;
    }

    function setTrashCount(n) {
      if (!els.trashCount) return;
      els.trashCount.textContent = String(n);
      els.trashCount.hidden = !n;
      state.trashCount = n;
    }

    return {
      build,
      state,
      requestAll,
      refresh,
      actions: {
        rename,
        tag,
        move,
        remove,
        favourite,
        open,
        sendToMachine,
        restoreVersion,
      },
      onKey,
      showLog,
      showTrash,
    };
  }

  return { create };
})();
