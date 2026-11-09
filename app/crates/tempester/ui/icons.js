/* Inline SVG icons shared by every page, with no dependency: one consistent style (24 px grid, 1.8 stroke, round
   caps, `currentColor`), icon buttons whose label is both the accessible name and the hover/focus tooltip, the
   small "i" badge that opens a popover with an explanation, and the one placement rule both follow (above the
   element, below only when there is no room). Loaded before kit.js. */
"use strict";
window.Tempester = window.Tempester || {};
window.Tempester.icons = (function () {
  /* The drawing of each icon: the inside of a 24 x 24 <svg>. */
  const PATHS = {
    fetch:
      '<path d="M12 3v11"/><path d="m7.5 10 4.5 4.5 4.5-4.5"/><path d="M4 17v2.5A1.5 1.5 0 0 0 5.5 21h13a1.5 1.5 0 0 0 1.5-1.5V17"/>',
    send: '<path d="M12 15V4"/><path d="m7.5 8 4.5-4.5L16.5 8"/><path d="M4 17v2.5A1.5 1.5 0 0 0 5.5 21h13a1.5 1.5 0 0 0 1.5-1.5V17"/>',
    save: '<path d="M5 3h11l4 4v12.5A1.5 1.5 0 0 1 18.5 21h-13A1.5 1.5 0 0 1 4 19.5v-15A1.5 1.5 0 0 1 5.5 3z"/><path d="M8 3v5h7V3"/><rect x="8" y="13" width="8" height="8" rx="1"/>',
    open: '<path d="M14 3H6.5A1.5 1.5 0 0 0 5 4.5v15A1.5 1.5 0 0 0 6.5 21h11a1.5 1.5 0 0 0 1.5-1.5V8z"/><path d="M14 3v5h5"/><path d="M9 14h6"/>',
    import:
      '<path d="M14 3H6.5A1.5 1.5 0 0 0 5 4.5v15A1.5 1.5 0 0 0 6.5 21h11a1.5 1.5 0 0 0 1.5-1.5V8z"/><path d="M14 3v5h5"/><path d="M12 11v6"/><path d="M9 14h6"/>',
    folder:
      '<path d="M3 6.5A1.5 1.5 0 0 1 4.5 5h4.2l2 2.5h8.8A1.5 1.5 0 0 1 21 9v9.5a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 18.5z"/>',
    reveal:
      '<path d="M3 6.5A1.5 1.5 0 0 1 4.5 5h4.2l2 2.5h8.8A1.5 1.5 0 0 1 21 9v9.5a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 18.5z"/><path d="m10 16 5-5"/><path d="M11.5 11H15v3.5"/>',
    move: '<path d="M3 6.5A1.5 1.5 0 0 1 4.5 5h4.2l2 2.5h8.8A1.5 1.5 0 0 1 21 9v9.5a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 18.5z"/><path d="M8 13.5h8"/><path d="m13 10.5 3 3-3 3"/>',
    rename:
      '<path d="M4 20h4L19 9a2.1 2.1 0 0 0-3-3L5 17z"/><path d="m14 8 3 3"/>',
    tag: '<path d="M3 12.2V4.5A1.5 1.5 0 0 1 4.5 3h7.7a1.5 1.5 0 0 1 1 .4l7.4 7.4a1.5 1.5 0 0 1 0 2.1l-7.7 7.7a1.5 1.5 0 0 1-2.1 0L3.4 13.2a1.5 1.5 0 0 1-.4-1z"/><circle cx="8" cy="8" r="1.3"/>',
    star: '<path d="m12 3.5 2.7 5.5 6 .9-4.4 4.2 1 6-5.3-2.8-5.3 2.8 1-6L3.3 9.9l6-.9z"/>',
    "star-filled":
      '<path d="m12 3.5 2.7 5.5 6 .9-4.4 4.2 1 6-5.3-2.8-5.3 2.8 1-6L3.3 9.9l6-.9z" fill="currentColor"/>',
    history:
      '<path d="M3.5 12a8.5 8.5 0 1 0 2.6-6.1L3.5 8.5"/><path d="M3.5 3.5v5h5"/><path d="M12 7.5V12l3 2"/>',
    trash:
      '<path d="M4 7h16"/><path d="M9 7V4.5A1.5 1.5 0 0 1 10.5 3h3A1.5 1.5 0 0 1 15 4.5V7"/><path d="M6 7l1 12.5A1.5 1.5 0 0 0 8.5 21h7a1.5 1.5 0 0 0 1.5-1.5L18 7"/><path d="M10 11v6"/><path d="M14 11v6"/>',
    restore: '<path d="M4 9h10a5.5 5.5 0 0 1 0 11H9"/><path d="m8 5-4 4 4 4"/>',
    duplicate:
      '<rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V5.5A1.5 1.5 0 0 0 14.5 4h-9A1.5 1.5 0 0 0 4 5.5v9A1.5 1.5 0 0 0 5.5 16H8"/>',
    search: '<circle cx="10.5" cy="10.5" r="6.5"/><path d="m20 20-4.8-4.8"/>',
    filter: '<path d="M3.5 5h17l-6.5 8v6l-4 2v-8z"/>',
    sort: '<path d="M8 4v16"/><path d="m4 8 4-4 4 4"/><path d="M16 20V4"/><path d="m12 16 4 4 4-4"/>',
    info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v5.5"/><path d="M12 7.6v.2" stroke-width="2.4"/>',
    refresh: '<path d="M20 12a8 8 0 1 1-2.5-5.8"/><path d="M20 4v5h-5"/>',
    connect:
      '<path d="M9 3v5"/><path d="M15 3v5"/><path d="M6.5 8h11v3.5a5.5 5.5 0 0 1-11 0z"/><path d="M12 17v4"/>',
    disconnect:
      '<path d="M9 3v5"/><path d="M15 3v5"/><path d="M6.5 8h11v3.5a5.5 5.5 0 0 1-11 0z"/><path d="M12 17v4"/><path d="m3 3 18 18"/>',
    play: '<path d="M7 4.5v15l12-7.5z"/>',
    stop: '<rect x="6" y="6" width="12" height="12" rx="1.5"/>',
    check: '<path d="m4.5 12.5 5 5 10-11"/>',
    warning:
      '<path d="M12 3.5 2.8 19.5a1 1 0 0 0 .9 1.5h16.6a1 1 0 0 0 .9-1.5z"/><path d="M12 10v4.5"/><path d="M12 17.4v.2" stroke-width="2.4"/>',
    close: '<path d="m6 6 12 12"/><path d="M18 6 6 18"/>',
    more: '<circle cx="5.5" cy="12" r="1.2" fill="currentColor"/><circle cx="12" cy="12" r="1.2" fill="currentColor"/><circle cx="18.5" cy="12" r="1.2" fill="currentColor"/>',
    compare:
      '<path d="M8 4v16"/><path d="M16 4v16"/><path d="m4 8 4-4 4 4"/><path d="m12 16 4 4 4-4"/>',
    "select-all":
      '<rect x="4" y="4" width="16" height="16" rx="2.5"/><path d="m8 12.5 3 3 5-6"/>',
    "chevron-down": '<path d="m6 9 6 6 6-6"/>',
    "chevron-up": '<path d="m6 15 6-6 6 6"/>',
    "chevron-right": '<path d="m9 6 6 6-6 6"/>',
    sound:
      '<path d="M3 12h2"/><path d="M7 8v8"/><path d="M11 4v16"/><path d="M15 8v8"/><path d="M19 10v4"/><path d="M21.5 12h.01"/>',
    beat: '<rect x="3.5" y="3.5" width="7" height="7" rx="1.5"/><rect x="13.5" y="3.5" width="7" height="7" rx="1.5"/><rect x="3.5" y="13.5" width="7" height="7" rx="1.5"/><rect x="13.5" y="13.5" width="7" height="7" rx="1.5"/>',
    project:
      '<path d="m12 3.5 9 4.5-9 4.5L3 8z"/><path d="m3 12 9 4.5 9-4.5"/><path d="m3 16 9 4.5 9-4.5"/>',
    list: '<path d="M8 6h12"/><path d="M8 12h12"/><path d="M8 18h12"/><path d="M4 6h.01"/><path d="M4 12h.01"/><path d="M4 18h.01"/>',
    "sort-asc": '<path d="M12 19V5"/><path d="m6 11 6-6 6 6"/>',
    "sort-desc": '<path d="M12 5v14"/><path d="m6 13 6 6 6-6"/>',
    power: '<path d="M12 3v8"/><path d="M6.6 6.8a8 8 0 1 0 10.8 0"/>',
    pause: '<path d="M8 5v14"/><path d="M16 5v14"/>',
    clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3.2 2"/>',
    copy: '<rect x="6" y="4.5" width="12" height="16.5" rx="2"/><path d="M9.5 4.5V4A1 1 0 0 1 10.5 3h3a1 1 0 0 1 1 1v.5"/><path d="M9.5 11h5"/><path d="M9.5 15h5"/>',
    identity:
      '<rect x="3" y="5" width="18" height="14" rx="2"/><circle cx="9" cy="11" r="2"/><path d="M5.8 16.2a3.4 3.4 0 0 1 6.4 0"/><path d="M14.5 10h4"/><path d="M14.5 14h4"/>',
    latest: '<path d="M12 4v13"/><path d="m6.5 12 5.5 5.5 5.5-5.5"/><path d="M5 21h14"/>',
    "arrow-in": '<path d="M20 12H5"/><path d="m10 6.5-5.5 5.5 5.5 5.5"/>',
    "arrow-out": '<path d="M4 12h15"/><path d="m14 6.5 5.5 5.5-5.5 5.5"/>',
    export:
      '<path d="M14 3H6.5A1.5 1.5 0 0 0 5 4.5v15A1.5 1.5 0 0 0 6.5 21h11a1.5 1.5 0 0 0 1.5-1.5V8z"/><path d="M14 3v5h5"/><path d="M8.5 15h7"/><path d="m13 12 3 3-3 3"/>',
    kit: '<rect x="3" y="3" width="3.6" height="3.6" rx=".8"/><rect x="8.1" y="3" width="3.6" height="3.6" rx=".8"/><rect x="13.2" y="3" width="3.6" height="3.6" rx=".8"/><rect x="18.3" y="3" width="3.6" height="3.6" rx=".8"/><rect x="3" y="8.1" width="3.6" height="3.6" rx=".8"/><rect x="8.1" y="8.1" width="3.6" height="3.6" rx=".8"/><rect x="13.2" y="8.1" width="3.6" height="3.6" rx=".8"/><rect x="18.3" y="8.1" width="3.6" height="3.6" rx=".8"/><rect x="3" y="13.2" width="3.6" height="3.6" rx=".8"/><rect x="8.1" y="13.2" width="3.6" height="3.6" rx=".8"/><rect x="13.2" y="13.2" width="3.6" height="3.6" rx=".8"/><rect x="18.3" y="13.2" width="3.6" height="3.6" rx=".8"/><rect x="3" y="18.3" width="3.6" height="3.6" rx=".8"/><rect x="8.1" y="18.3" width="3.6" height="3.6" rx=".8"/><rect x="13.2" y="18.3" width="3.6" height="3.6" rx=".8"/><rect x="18.3" y="18.3" width="3.6" height="3.6" rx=".8"/>',
    dice: '<rect x="4" y="4" width="16" height="16" rx="3"/><circle cx="8.8" cy="8.8" r="1.1"/><circle cx="15.2" cy="8.8" r="1.1"/><circle cx="12" cy="12" r="1.1"/><circle cx="8.8" cy="15.2" r="1.1"/><circle cx="15.2" cy="15.2" r="1.1"/>',
    swap: '<path d="M4 8h15"/><path d="m15 4 4 4-4 4"/><path d="M20 16H5"/><path d="m9 12-4 4 4 4"/>',
    plus: '<path d="M12 5v14"/><path d="M5 12h14"/>',
    minus: '<path d="M5 12h14"/>',
    "view-full":
      '<rect x="3.5" y="4" width="17" height="4.5" rx="1"/><rect x="3.5" y="10" width="17" height="4.5" rx="1"/><rect x="3.5" y="16" width="17" height="4" rx="1"/>',
    "view-machine":
      '<rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="8" cy="9" r="1.7"/><circle cx="13" cy="9" r="1.7"/><rect x="6" y="13.5" width="3" height="3" rx=".6"/><rect x="10.5" y="13.5" width="3" height="3" rx=".6"/><rect x="15" y="13.5" width="3" height="3" rx=".6"/>',
    expand: '<path d="m6 9 6 6 6-6"/>',
    undo: '<path d="M9 14 4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 0 1 0 11H11"/>',
    redo: '<path d="m15 14 5-5-5-5"/><path d="M20 9H9.5a5.5 5.5 0 0 0 0 11H13"/>',
    learn:
      '<circle cx="9" cy="10" r="5.5"/><path d="M9 10V6.5"/><path d="M14.5 13.5 19 18"/><path d="M17 20.5 21.5 16"/><path d="M4 20.5h7"/>',
    tools:
      '<path d="M4 6h9"/><path d="M17 6h3"/><circle cx="15" cy="6" r="2"/><path d="M4 12h3"/><path d="M11 12h9"/><circle cx="9" cy="12" r="2"/><path d="M4 18h11"/><path d="M19 18h1"/><circle cx="17" cy="18" r="2"/>',
    build:
      '<path d="m14.5 6.5 3 3"/><path d="M5 19 15.5 8.5"/><path d="M13 4.5a4.5 4.5 0 0 1 6.4 5.9l-1.4-1.4-3-3z"/><path d="m4 20 2.5-.5"/>',
  };

  const NAMES = Object.keys(PATHS);

  function svgText(name) {
    if (!PATHS[name]) throw new Error("no icon named " + name);
    return (
      '<svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="1.8" ' +
      'stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false">' +
      PATHS[name] +
      "</svg>"
    );
  }

  /* The icon alone, decorative: a span holding the drawing. */
  function icon(name) {
    const node = document.createElement("span");
    node.setAttribute("class", "icon");
    node.setAttribute("aria-hidden", "true");
    node.innerHTML = svgText(name);
    return node;
  }

  /* A square button with an icon. `label` is its accessible name AND its tooltip (shown on hover and on keyboard
     focus), so a button without a label is refused: an icon alone says nothing to a screen reader. Options:
     `variant` ("primary" | "danger"), `pressed` (a toggle), `text` (a short visible text next to the icon). */
  function button(name, label, onclick, options) {
    if (!label) throw new Error("an icon button needs a label: " + name);
    const opts = options || {};
    const node = document.createElement("button");
    node.setAttribute("type", "button");
    node.setAttribute(
      "class",
      "ibtn" +
        (opts.variant ? " " + opts.variant : "") +
        (opts.text ? " with-text" : ""),
    );
    node.setAttribute("aria-label", label);
    node.setAttribute("data-tip", label);
    if (opts.pressed !== undefined)
      node.setAttribute("aria-pressed", String(!!opts.pressed));
    node.append(icon(name));
    if (opts.text) {
      const text = document.createElement("span");
      text.setAttribute("class", "ibtn-text");
      text.append(opts.text);
      node.append(text);
    }
    if (onclick) node.addEventListener("click", onclick);
    return node;
  }

  /* Changes the label (accessible name and tooltip) of an icon-only button, and its drawing when `name` is given:
     for toggles whose meaning flips (Connect / Disconnect). Text buttons keep their own text. */
  function relabel(node, label, name) {
    if (!label) throw new Error("an icon button needs a label");
    node.setAttribute("aria-label", label);
    node.setAttribute("data-tip", label);
    if (name) node.replaceChildren(icon(name));
  }

  /* Where a tooltip or a popover goes. Above its element, centred, `GAP` px away; below only when the room above
     (up to the bottom of the sticky top bar, `view.top`) is too small; moved sideways, never clipped, to stay at
     least `EDGE` px inside the window. Pure: rectangles in, position out, so that it can be tested without a
     browser. `anchor` {left, top, width, height}, `box` {width, height}, `view` {width, height, top}. */
  const GAP = 8;
  const EDGE = 8;
  function placeBox(anchor, box, view) {
    const centred = anchor.left + (anchor.width - box.width) / 2;
    const left = Math.max(
      EDGE,
      Math.min(centred, view.width - EDGE - box.width),
    );
    const above = anchor.top - GAP - box.height;
    const below = anchor.top + anchor.height + GAP;
    const fitsAbove = above >= view.top;
    const fitsBelow = below + box.height <= view.height;
    const roomAbove = anchor.top - view.top;
    const roomBelow = view.height - (anchor.top + anchor.height);
    const side =
      fitsAbove || (!fitsBelow && roomAbove >= roomBelow) ? "above" : "below";
    const wanted = side === "above" ? above : below;
    const top = Math.max(
      view.top,
      Math.min(wanted, view.height - box.height),
    );
    return { left, top, side };
  }

  /* Measures `box` (displayed, position: fixed) and `anchor`, then moves the box next to the anchor. The box is
     first parked at 0, 0: at the window edge its width would otherwise be squeezed by its previous position. A fixed
     box leaves the layout alone, so showing it never moves anything. */
  function placeNear(box, anchor) {
    box.style.left = "0px";
    box.style.top = "0px";
    const size = box.getBoundingClientRect();
    const bar = document.getElementById("topbar");
    const found = placeBox(
      anchor.getBoundingClientRect(),
      { width: size.width, height: size.height },
      {
        width: window.innerWidth,
        height: window.innerHeight,
        top: bar ? bar.getBoundingClientRect().bottom : 0,
      },
    );
    box.style.left = found.left + "px";
    box.style.top = found.top + "px";
    box.setAttribute("data-side", found.side);
  }

  /* One tooltip for the whole page, drawn from the `data-tip` of the element hovered or focused with the keyboard. */
  let tipBox = null;
  function showTip(anchor) {
    const text = anchor.getAttribute("data-tip");
    if (!text) return;
    if (!tipBox) {
      tipBox = document.createElement("div");
      tipBox.setAttribute("class", "tip");
      tipBox.setAttribute("aria-hidden", "true");
      document.body.append(tipBox);
    }
    tipBox.textContent = text;
    tipBox.hidden = false;
    placeNear(tipBox, anchor);
  }
  function hideTip() {
    if (tipBox) tipBox.hidden = true;
  }

  /* Closes the open popover on Escape or a click elsewhere (one listener for the page). */
  let openPopover = null;
  function closePopover() {
    if (!openPopover) return;
    openPopover.badge.setAttribute("aria-expanded", "false");
    openPopover.box.hidden = true;
    openPopover = null;
  }
  const tipOwner = (e) =>
    e.target && e.target.closest ? e.target.closest("[data-tip]") : null;
  if (document.addEventListener) {
    document.addEventListener("mouseover", (e) => {
      const owner = tipOwner(e);
      if (owner) showTip(owner);
    });
    document.addEventListener("mouseout", () => hideTip());
    document.addEventListener("focusin", (e) => {
      const owner = tipOwner(e);
      /* A mouse click also focuses a button: only keyboard focus shows the tip. */
      if (owner && (!owner.matches || owner.matches(":focus-visible")))
        showTip(owner);
    });
    document.addEventListener("focusout", () => hideTip());
    document.addEventListener("keydown", (e) => {
      if (e.key !== "Escape") return;
      hideTip();
      closePopover();
    });
    document.addEventListener("click", (e) => {
      hideTip();
      if (openPopover && !openPopover.wrap.contains(e.target)) closePopover();
    });
    /* Scrolling moves the element under the tip: drop the tip, keep an open popover next to its badge. */
    document.addEventListener(
      "scroll",
      () => {
        hideTip();
        if (openPopover) placeNear(openPopover.box, openPopover.badge);
      },
      true,
    );
  }
  if (window.addEventListener) {
    window.addEventListener("resize", () => {
      hideTip();
      if (openPopover) placeNear(openPopover.box, openPopover.badge);
    });
  }

  /* The "i" badge: a click or Enter shows `text` in a small popover above it (below when there is no room), so that
     long explanations stay out of the page until asked for. It explains what a control does; results the user must
     read belong in the page itself. `text` is a string or a node (kept by the caller to update it). */
  function info(text, label) {
    const wrap = document.createElement("span");
    wrap.setAttribute("class", "info");
    const box = document.createElement("span");
    box.setAttribute("class", "popover");
    box.setAttribute("role", "tooltip");
    box.hidden = true;
    box.append(text);
    const badge = button("info", label || "More information", (e) => {
      const wasOpen = openPopover && openPopover.box === box;
      hideTip();
      closePopover();
      if (wasOpen) return;
      box.hidden = false;
      placeNear(box, badge);
      badge.setAttribute("aria-expanded", "true");
      openPopover = { badge, box, wrap };
      if (e && e.stopPropagation) e.stopPropagation();
    });
    badge.setAttribute("class", "ibtn info-badge");
    badge.setAttribute("aria-expanded", "false");
    wrap.append(badge, box);
    return wrap;
  }

  return {
    icon,
    button,
    relabel,
    info,
    names: NAMES,
    svgText,
    closePopover,
    placeBox,
  };
})();
