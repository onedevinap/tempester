/* MIDI learn on the face of the Panel page (Full view). Rust owns the controller port and the table; this script only
   (1) tells which control the user clicked in learn mode, (2) moves a control when a learned controller turns, through
   the same path as a hand on the knob: the control is set on both drawings and `link.knob` / `link.local` send it
   (same NRPN or CC, same 30 ms rate limit), and (3) outlines the controls that have a controller (no text is added
   to the face, so nothing can overlap).

   Tempester.editLearn:
     targetOf(node, host)         the target id under `node` ("n34", "distort", "compress", "volume", "bpm") or null
     drive(id, value, parts)      moves target `id` to `value`; parts = {panel, machine, link}
     attach(host, {onPick})       learn-mode click capture on `host`; returns {setOn(on), mark(ids, armed)} */
"use strict";
window.Tempester = window.Tempester || {};
window.Tempester.editLearn = (function () {
  /* The four knobs that are not sound parameters, by the label panel.js gives them. */
  const LOCALS = {
    BPM: "bpm",
    Distort: "distort",
    Compress: "compress",
    Volume: "volume",
  };
  const CONTROL_CLASSES = ["pot", "fen", "bt"];

  const parentOf = (node) => node.parentNode || node.parent || null;
  const classesOf = (node) =>
    String((node.getAttribute && node.getAttribute("class")) || "").split(
      /\s+/,
    );

  /* The control around `node`: a knob (.pot), an OLED menu (.fen, id "fen-<index>") or a switch (.bt); the first one
     met going up, never past `host`. */
  function targetOf(node, host) {
    for (let n = node; n && n !== host; n = parentOf(n)) {
      if (!n.getAttribute) continue;
      const kind = classesOf(n).find((c) => CONTROL_CLASSES.includes(c));
      if (!kind) continue;
      const param = n.getAttribute("data-param");
      if (param !== null && param !== "") return "n" + Number(param);
      const id = n.getAttribute("id") || "";
      if (kind === "fen" && id.startsWith("fen-"))
        return "n" + Number(id.slice(4));
      if (kind === "pot") return localOf(n);
      return null;
    }
    return null;
  }

  function localOf(pot) {
    const slider = Array.from(pot.children || []).find(
      (c) => c.getAttribute && c.getAttribute("role") === "slider",
    );
    const label = slider ? slider.getAttribute("aria-label") : null;
    return LOCALS[label] || null;
  }

  /* A learned controller moved target `id`: exactly what a hand on the control does (panel.js `regler` /
     `reglerLocal` then the link), so the machine receives the same message at the same rate. */
  function drive(id, value, { panel, machine, link }) {
    const param = /^n(\d+)$/.exec(id);
    if (param) {
      const index = Number(param[1]);
      panel.setValue(index, value);
      machine.setParam(index, value);
      link.knob(index, value);
      return true;
    }
    if (!Object.values(LOCALS).includes(id)) return false;
    panel.setLocal(id, value);
    link.local(id, value);
    return true;
  }

  function attach(host, { onPick }) {
    let on = false;
    /* Capture phase: the control never sees the click, so learning never changes a value. */
    const swallow = (e) => {
      /* Escape (leave learn mode) and Tab (move the focus) still go through. */
      if (!on || e.key === "Escape" || e.key === "Tab") return;
      e.preventDefault();
      e.stopPropagation();
    };
    host.addEventListener(
      "pointerdown",
      (e) => {
        if (!on) return;
        swallow(e);
        const id = targetOf(e.target, host);
        if (id) onPick(id);
      },
      true,
    );
    for (const type of ["click", "dblclick", "wheel", "keydown"])
      host.addEventListener(type, swallow, { capture: true, passive: false });

    function mark(ids, armed) {
      if (!host.querySelectorAll) return;
      const mapped = new Set(ids);
      for (const node of host.querySelectorAll(".pot, .fen, .bt")) {
        const id = targetOf(node, host);
        node.classList.toggle("learned", on && !!id && mapped.has(id));
        node.classList.toggle("learn-armed", on && !!id && id === armed);
      }
    }
    return {
      setOn(value) {
        on = !!value;
        host.classList.toggle("learning", on);
      },
      isOn: () => on,
      mark,
    };
  }

  return { targetOf, drive, attach, LOCALS };
})();
