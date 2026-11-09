/* View and zoom of the Panel page: which of the two drawings is shown (Full: every sound parameter; Machine: only what
   the physical Tempest has, where it has it), and how big. Both drawings are fixed-width pictures scaled to the
   width of their frame; the zoom is a percentage of that fit-to-width scale.

   Tempester.panelView.create({releaseKeys, onView, onZoom, save})
        -> {view(), zoom(), setView(name), setZoom(percent), zoomIn(), zoomOut(), zoomFit(), restore(view, zoom)}
        releaseKeys()        called BEFORE the other drawing is shown: a key held on the old drawing would otherwise
                             never see its release
        onView(name), onZoom(percent)   the page shows the drawing / refits it
        save(view, zoom)     the user chose: the page has Rust keep it; `restore` (what Rust remembers) never saves
   Tempester.panelView.zoomShortcut(event)   "in", "out", "fit" or null for Cmd/Ctrl + - 0
   Tempester.panelView.applyScale(cadre, machine, design, maxFit)   sizes one drawing in its frame
   Tempester.panelView.zoomPercent()         the zoom now (read by the drawings when they refit) */
"use strict";
window.Tempester = window.Tempester || {};
window.Tempester.panelView = (function () {
  const VIEWS = ["full", "machine"];
  const ZOOM_MIN = 60,
    ZOOM_MAX = 200,
    ZOOM_STEP = 10,
    ZOOM_FIT = 100;
  let zoomNow = ZOOM_FIT;

  /* A zoom is a multiple of the step inside the range; anything else (a hand-edited settings file) is the fit. */
  function clampZoom(percent) {
    const n = Number(percent);
    if (!Number.isFinite(n)) return ZOOM_FIT;
    const stepped = Math.round(n / ZOOM_STEP) * ZOOM_STEP;
    return Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, stepped));
  }

  function zoomShortcut(e) {
    if (!(e.metaKey || e.ctrlKey) || e.altKey) return null;
    if (e.key === "+" || e.key === "=") return "in";
    if (e.key === "-" || e.key === "_") return "out";
    if (e.key === "0") return "fit";
    return null;
  }

  function create({ releaseKeys, onView, onZoom, save }) {
    let view = "full";
    function setView(name) {
      if (!VIEWS.includes(name) || name === view) return;
      releaseKeys();
      view = name;
      onView(view);
      save(view, zoomNow);
    }
    function setZoom(percent) {
      const next = clampZoom(percent);
      if (next === zoomNow) return;
      zoomNow = next;
      onZoom(zoomNow);
      save(view, zoomNow);
    }
    function restore(savedView, savedZoom) {
      zoomNow = clampZoom(savedZoom);
      view = VIEWS.includes(savedView) ? savedView : "full";
      onView(view);
      onZoom(zoomNow);
    }
    return {
      view: () => view,
      zoom: () => zoomNow,
      setView,
      setZoom,
      zoomIn: () => setZoom(zoomNow + ZOOM_STEP),
      zoomOut: () => setZoom(zoomNow - ZOOM_STEP),
      zoomFit: () => setZoom(ZOOM_FIT),
      restore,
    };
  }

  /* The drawing keeps its design width and is scaled: the fit scale fills the frame (up to maxFit), the zoom
     multiplies it. Past the frame width the frame scrolls sideways, never the page; below it the drawing is centred.
     `--s` lets the stylesheet keep text at a fixed size on screen whatever the scale. */
  function applyScale(cadre, machine, design, maxFit) {
    if (!cadre.clientWidth)
      return 0; /* hidden: the observer fires again when shown */
    const fit = Math.min(maxFit, cadre.clientWidth / design);
    const scale = (fit * zoomNow) / ZOOM_FIT;
    machine.style.transform = `scale(${scale})`;
    machine.style.setProperty("--s", String(scale));
    machine.style.marginLeft =
      Math.max(0, (cadre.clientWidth - design * scale) / 2) + "px";
    /* A scaled-down drawing still takes its full width in the frame's scroll area: nothing to scroll then. */
    cadre.style.overflowX =
      design * scale <= cadre.clientWidth + 1 ? "hidden" : "auto";
    const height = Math.ceil(machine.offsetHeight * scale);
    cadre.style.height = height + "px";
    /* A classic scrollbar takes room from the frame: give it back so that the drawing is not cut. */
    const bar = cadre.offsetHeight - cadre.clientHeight;
    if (bar > 0) cadre.style.height = height + bar + "px";
    return scale;
  }

  return {
    VIEWS,
    ZOOM_MIN,
    ZOOM_MAX,
    ZOOM_STEP,
    ZOOM_FIT,
    clampZoom,
    zoomShortcut,
    create,
    applyScale,
    zoomPercent: () => zoomNow,
  };
})();
