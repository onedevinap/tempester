/* The "MIDI monitor" panel of the Machine page: every message the app sends and the machine sends, as lines.
   Rust does the collecting and the wording (`midi_log` events: {lines: [{t, dir, kind, text, raw}], dropped,
   clock_in, clock_out, backlog}); this file only shows them. Nothing is collected until the switch is on. */
"use strict";
(function () {
  const { el, toast } = window.Tempester.kit;
  const { button, relabel, info } = window.Tempester.icons;
  const bridge = window.Tempester.bridge;

  const MAX_LINES = 2000;
  /* In: arrow towards us, plain; out: arrow away from us, indented. Form and colour both tell them apart. */
  const MARK = { in: "◀ IN", out: "OUT ▶", sys: "•" };

  const S = {
    title: "MIDI monitor",
    lead: "Every message the app sends to the Tempest and every message the Tempest sends back, as it happens. Nothing is collected while the monitor is off.",
    on: "Monitor on (click to stop)",
    off: "Monitor off (click to start)",
    pause: "Pause the list",
    resume: "Resume the list",
    clear: "Clear the list",
    copy: "Copy the selected lines, or every line shown",
    both: "In and out",
    onlyIn: "In only",
    onlyOut: "Out only",
    direction: (shown) => "Showing: " + shown + " (click to change)",
    clockOn: "Clock and active sensing shown (click to hide)",
    clockOff: "Clock and active sensing hidden (click to show)",
    latest: "Jump to the latest line",
    listLabel: "MIDI messages",
    empty: "Nothing yet. Turn the monitor on, then use the Tempest or the app.",
    emptyOff: "The monitor is off.",
    copied: (n) => (n === 1 ? "Copied 1 line." : "Copied " + n + " lines."),
    copyFailed: "The text could not be copied. Select it and use the copy shortcut.",
    nothingToCopy: "No line to copy.",
    counts: (shown, dropped, clockIn, clockOut) =>
      shown +
      " lines · clock bytes " +
      clockIn +
      " in, " +
      clockOut +
      " out · dropped " +
      dropped,
    paused: " · paused (new lines appear on resume)",
  };

  const state = {
    on: false,
    paused: false,
    follow: true,
    direction: "both",
    clockShown: false,
    lines: [],
    dropped: 0,
    clockIn: 0,
    clockOut: 0,
    base: { dropped: 0, clockIn: 0, clockOut: 0 },
    dirty: false,
  };
  const els = {};

  function seconds(ms) {
    return (ms / 1000).toFixed(3);
  }

  function visible(line) {
    return line.dir === "sys" || state.direction === "both" || line.dir === state.direction;
  }

  function lineNode(line) {
    return el(
      "div",
      { class: "mon-line " + line.dir + " k-" + line.kind },
      el("span", { class: "mon-time" }, seconds(line.t)),
      el("span", { class: "mon-dir" }, MARK[line.dir] || ""),
      el("span", { class: "mon-text" }, line.text),
      line.raw ? el("span", { class: "mon-raw" }, line.raw) : null,
    );
  }

  function lineText(line) {
    const dir = line.dir === "sys" ? "--" : line.dir.toUpperCase().padEnd(3);
    return [seconds(line.t), dir, line.text, line.raw ? "[" + line.raw + "]" : ""]
      .filter(Boolean)
      .join("  ");
  }

  function isShown() {
    return els.list.offsetParent !== null;
  }

  function atBottom() {
    const list = els.list;
    return list.scrollHeight - list.scrollTop - list.clientHeight < 8;
  }

  function stick() {
    if (state.follow) els.list.scrollTop = els.list.scrollHeight;
    els.latest.disabled = state.follow;
  }

  function emptyNote() {
    return el("p", { class: "mon-empty mid" }, state.on ? S.empty : S.emptyOff);
  }

  /* Draws the whole list: after a filter change, a resume, or when the page comes back into view. */
  function redraw() {
    state.dirty = false;
    const shown = state.lines.filter(visible);
    els.list.replaceChildren(...(shown.length ? shown.map(lineNode) : [emptyNote()]));
    stick();
    updateCounts(shown.length);
  }

  function updateCounts(shownCount) {
    const shown = shownCount === undefined ? state.lines.filter(visible).length : shownCount;
    els.counts.textContent =
      S.counts(
        shown,
        state.dropped - state.base.dropped,
        state.clockIn - state.base.clockIn,
        state.clockOut - state.base.clockOut,
      ) + (state.paused ? S.paused : "");
  }

  /* New lines: appended to the open list, the oldest ones leave it when the cap is passed. */
  function append(lines) {
    state.lines.push(...lines);
    if (state.lines.length > MAX_LINES) state.lines.splice(0, state.lines.length - MAX_LINES);
    if (state.paused || !isShown()) {
      state.dirty = true;
      return updateCounts();
    }
    const fresh = lines.filter(visible);
    if (!els.list.querySelector(".mon-line")) return redraw();
    els.list.append(...fresh.map(lineNode));
    const nodes = els.list.querySelectorAll(".mon-line");
    for (let k = 0; k < nodes.length - MAX_LINES; k++) nodes[k].remove();
    stick();
    updateCounts();
  }

  function onLog(message) {
    state.dropped = message.dropped;
    state.clockIn = message.clock_in;
    state.clockOut = message.clock_out;
    append(message.lines);
  }

  function clear() {
    state.lines = [];
    state.base = {
      dropped: state.dropped,
      clockIn: state.clockIn,
      clockOut: state.clockOut,
    };
    redraw();
  }

  function setPaused(paused) {
    state.paused = paused;
    relabel(els.pause, paused ? S.resume : S.pause);
    els.pause.setAttribute("aria-pressed", String(paused));
    if (!paused) redraw();
    else updateCounts();
  }

  function showSwitch(message) {
    state.on = message.on;
    relabel(els.toggle, message.on ? S.on : S.off);
    els.toggle.setAttribute("aria-pressed", String(message.on));
    state.clockShown = message.clock;
    relabel(els.clock, message.clock ? S.clockOn : S.clockOff);
    els.clock.setAttribute("aria-pressed", String(message.clock));
    if (!message.on) {
      state.base = { dropped: 0, clockIn: 0, clockOut: 0 };
      state.dropped = state.clockIn = state.clockOut = 0;
    }
    if (!state.paused && isShown()) redraw();
    else state.dirty = true;
  }

  /* What the user selected inside the list, else every line shown. */
  function textToCopy() {
    const selection = window.getSelection();
    const selected = selection ? selection.toString() : "";
    if (selected && els.list.contains(selection.anchorNode)) {
      return { text: selected, count: selected.split("\n").filter(Boolean).length };
    }
    const shown = state.lines.filter(visible);
    return { text: shown.map(lineText).join("\n"), count: shown.length };
  }

  /* The clipboard API may be refused in the web view: a hidden text area and the old command are the fallback. */
  async function writeClipboard(text) {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch (e) {
      const area = el("textarea", { class: "hidden-input", "aria-hidden": "true" });
      area.value = text;
      document.body.append(area);
      area.select();
      const done = document.execCommand("copy");
      area.remove();
      return done;
    }
  }

  async function copy() {
    const { text, count } = textToCopy();
    if (!text) return toast(S.nothingToCopy);
    toast((await writeClipboard(text)) ? S.copied(count) : S.copyFailed);
  }

  const DIRECTIONS = [
    ["both", S.both, "swap"],
    ["in", S.onlyIn, "arrow-in"],
    ["out", S.onlyOut, "arrow-out"],
  ];

  /* The direction filter is one button that goes through the three choices; its drawing says which is on. */
  function cycleDirection() {
    const at = DIRECTIONS.findIndex(([value]) => value === state.direction);
    const [value, shown, icon] = DIRECTIONS[(at + 1) % DIRECTIONS.length];
    state.direction = value;
    relabel(els.direction, S.direction(shown), icon);
    if (!state.paused) redraw();
    else state.dirty = true;
  }

  function build() {
    els.toggle = button("power", S.off, () => bridge.send("monitor", state.on ? "off" : "on"), {
      pressed: false,
    });
    els.pause = button("pause", S.pause, () => setPaused(!state.paused), { pressed: false });
    els.clear = button("trash", S.clear, clear);
    els.copy = button("copy", S.copy, copy);
    els.direction = button("swap", S.direction(S.both), cycleDirection);
    els.clock = button(
      "clock",
      S.clockOff,
      () => bridge.send("monitor_clock", state.clockShown ? "off" : "on"),
      { pressed: false },
    );
    els.latest = button("latest", S.latest, () => {
      state.follow = true;
      stick();
    });
    els.latest.disabled = true;
    els.counts = el("span", { class: "count", role: "status" });
    els.list = el(
      "div",
      { class: "mon-list", role: "log", "aria-label": S.listLabel, tabindex: "0" },
      emptyNote(),
    );
    els.list.addEventListener("scroll", () => {
      state.follow = atBottom();
      els.latest.disabled = state.follow;
    });
    bridge.on("midi_log", onLog);
    bridge.on("monitor", showSwitch);
    updateCounts(0);
    return el(
      "section",
      { class: "mon-panel", "aria-label": S.title },
      el(
        "div",
        { class: "toolbar" },
        el("h3", {}, S.title),
        info(S.lead, S.title),
        el("span", { class: "sep" }),
        els.toggle,
        els.pause,
        els.clear,
        els.copy,
        el("span", { class: "sep" }),
        els.direction,
        els.clock,
        els.latest,
        els.counts,
      ),
      els.list,
    );
  }

  window.Tempester.monitor = {
    build,
    /* The list is not drawn while its page is hidden: draw what came meanwhile. */
    onShow: () => {
      if (state.dirty && !state.paused) redraw();
    },
  };
})();
