/* The logic of the library browser, with no DOM: search, filters, sort, counts, selection and keyboard moves, name
   rules, dates. The page code (lib-browser.js) draws what this decides; ui-tests/library-browser.test.js checks it
   on its own and on a fake DOM. Loaded before the page. */
"use strict";
window.Tempester = window.Tempester || {};
window.Tempester.libmodel = (function () {
  const NAME_MAX = 20;

  /* The same rules as the Rust side (library::rename::name_refusal), so that a wrong name is told while typing;
     Rust checks again and has the last word. Returns the reason, or null when the name is fine. */
  function nameRefusal(name) {
    if (!name || !name.trim()) return "the name is empty";
    if (name !== name.trim())
      return "the name starts or ends with a space (the machine pads names with spaces itself)";
    for (const c of name)
      if (c < " " || c > "~")
        return (
          '"' +
          c +
          '" is not allowed: only the printable ASCII characters (space to ~) fit in a machine name'
        );
    if (name.length > NAME_MAX)
      return (
        "the name has " +
        name.length +
        " characters, the machine keeps " +
        NAME_MAX
      );
    return null;
  }

  function pad(n) {
    return String(n).padStart(2, "0");
  }

  /* "2026-11-08 10:00" (UTC: the app keeps no time zone, and says so in the dates it writes in messages). */
  function formatDate(seconds) {
    if (seconds === null || seconds === undefined) return "";
    const d = new Date(seconds * 1000);
    return (
      d.getUTCFullYear() +
      "-" +
      pad(d.getUTCMonth() + 1) +
      "-" +
      pad(d.getUTCDate()) +
      " " +
      pad(d.getUTCHours()) +
      ":" +
      pad(d.getUTCMinutes())
    );
  }

  function dayStart(text) {
    const ms = Date.parse(text + "T00:00:00Z");
    return Number.isNaN(ms) ? null : ms / 1000;
  }

  function haystack(item) {
    return [
      item.display,
      item.path,
      item.folder,
      item.tags.join(" "),
      item.note,
      item.source,
      item.sound_type,
    ]
      .join("\n")
      .toLowerCase();
  }

  /* Every word typed must be found in the name, folder, tags, note, source or Sound Type. */
  function matchesQuery(item, query) {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    if (!words.length) return true;
    const text = haystack(item);
    return words.every((word) => text.includes(word));
  }

  const NO_FILTERS = {
    favourites: false,
    folder: "",
    tag: "",
    source: "",
    from: "",
    to: "",
    soundType: "",
    osc: "",
  };

  function matchesFilters(item, f) {
    if (f.favourites && !item.favourite) return false;
    if (
      f.folder === "(none)"
        ? item.folder !== ""
        : f.folder && item.folder !== f.folder
    )
      return false;
    if (f.tag && !item.tags.includes(f.tag)) return false;
    if (f.source && item.source !== f.source) return false;
    if (f.soundType && item.sound_type !== f.soundType) return false;
    if (f.osc && !item.osc.includes(f.osc)) return false;
    const from = f.from ? dayStart(f.from) : null;
    const to = f.to ? dayStart(f.to) : null;
    if (from !== null && item.modified < from) return false;
    if (to !== null && item.modified >= to + 86400) return false;
    return true;
  }

  function filterItems(items, query, filters) {
    const f = Object.assign({}, NO_FILTERS, filters);
    return items.filter(
      (item) => matchesFilters(item, f) && matchesQuery(item, query),
    );
  }

  const SORTS = {
    name: (a, b) =>
      a.display.localeCompare(b.display, "en", { sensitivity: "base" }),
    date: (a, b) => a.modified - b.modified,
    folder: (a, b) =>
      a.folder.localeCompare(b.folder, "en", { sensitivity: "base" }),
    source: (a, b) =>
      a.source.localeCompare(b.source, "en", { sensitivity: "base" }),
    favourite: (a, b) => Number(b.favourite) - Number(a.favourite),
  };

  /* A sorted copy; the auto-backup slot always stays first, equal items keep a stable name order. */
  function sortItems(items, key, direction) {
    const compare = SORTS[key] || SORTS.name;
    const sign = direction === "desc" ? -1 : 1;
    return items.slice().sort((a, b) => {
      if (a.slot !== b.slot) return a.slot ? -1 : 1;
      return sign * compare(a, b) || SORTS.name(a, b) || (a.id < b.id ? -1 : 1);
    });
  }

  function distinct(values) {
    return Array.from(new Set(values.filter(Boolean))).sort((a, b) =>
      a.localeCompare(b, "en", { sensitivity: "base" }),
    );
  }

  /* The values the filter menus offer, taken from what the library holds. */
  function facetOptions(items) {
    return {
      folders: distinct(items.map((i) => i.folder)),
      hasRoot: items.some((i) => i.folder === ""),
      tags: distinct(items.flatMap((i) => i.tags)),
      sources: distinct(items.map((i) => i.source)),
      soundTypes: distinct(items.map((i) => i.sound_type)),
      oscs: distinct(items.flatMap((i) => i.osc)),
    };
  }

  function activeFilterCount(filters) {
    const f = Object.assign({}, NO_FILTERS, filters);
    return Object.keys(NO_FILTERS).filter((k) => f[k] !== NO_FILTERS[k]).length;
  }

  /* ---- selection --------------------------------------------------------------------------------------------- */

  function createSelection() {
    return { ids: new Set(), anchor: null, active: null };
  }

  /* A click on a row: plain = only this one; ctrl/cmd = toggle it; shift = the range from the anchor. */
  function selectClick(sel, orderedIds, id, mods) {
    const m = mods || {};
    if (m.shift && sel.anchor !== null && orderedIds.includes(sel.anchor)) {
      const a = orderedIds.indexOf(sel.anchor);
      const b = orderedIds.indexOf(id);
      const [lo, hi] = a < b ? [a, b] : [b, a];
      if (!m.ctrl) sel.ids.clear();
      for (const picked of orderedIds.slice(lo, hi + 1)) sel.ids.add(picked);
    } else if (m.ctrl) {
      if (sel.ids.has(id)) sel.ids.delete(id);
      else sel.ids.add(id);
      sel.anchor = id;
    } else {
      sel.ids.clear();
      sel.ids.add(id);
      sel.anchor = id;
    }
    sel.active = id;
  }

  const STEP = { down: 1, up: -1, pageDown: 10, pageUp: -10 };

  /* An arrow, Home, End or page key: moves the active row; with shift it extends the selection, else it selects
     only the new row. */
  function selectMove(sel, orderedIds, key, extend) {
    if (!orderedIds.length) return;
    const at = orderedIds.indexOf(sel.active);
    let next;
    if (key === "home") next = 0;
    else if (key === "end") next = orderedIds.length - 1;
    else if (at < 0)
      next = key === "up" || key === "pageUp" ? orderedIds.length - 1 : 0;
    else next = Math.max(0, Math.min(orderedIds.length - 1, at + STEP[key]));
    const id = orderedIds[next];
    if (extend) {
      if (sel.anchor === null)
        sel.anchor = sel.active === null ? id : sel.active;
      selectClick(sel, orderedIds, id, { shift: true });
    } else {
      selectClick(sel, orderedIds, id, {});
    }
  }

  function selectAll(sel, orderedIds) {
    sel.ids = new Set(orderedIds);
    if (sel.active === null && orderedIds.length) sel.active = orderedIds[0];
  }

  /* Keeps only the ids that are still listed (after a filter or a refresh). */
  function pruneSelection(sel, orderedIds) {
    const present = new Set(orderedIds);
    for (const id of Array.from(sel.ids))
      if (!present.has(id)) sel.ids.delete(id);
    if (sel.active !== null && !present.has(sel.active)) sel.active = null;
    if (sel.anchor !== null && !present.has(sel.anchor)) sel.anchor = null;
  }

  return {
    NAME_MAX,
    NO_FILTERS,
    nameRefusal,
    formatDate,
    matchesQuery,
    filterItems,
    sortItems,
    facetOptions,
    activeFilterCount,
    createSelection,
    selectClick,
    selectMove,
    selectAll,
    pruneSelection,
  };
})();
