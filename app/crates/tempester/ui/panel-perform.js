/* The performance controls of Tempester 1.6.2.0 / 1.6.3.0 on the Panel page, in both views:
     - the two FX ribbons (n°31): a drag touches the ribbon at a position (`ribbon r touch p [pressure]`), the end of
       the drag releases it; the machine's own ribbons are mirrored (position, released);
     - the play probability of each pad (n°36): a mode of the pads (toggle in the bar) where each pad shows its
       probability and a drag, the wheel or the arrows change it; the value goes out when the gesture ends
       (`pad_probability pad p`: Rust presses the pad so that the machine selects it, then sends CC 14). The machine
       selects a pad on a press only in 16 Sounds, so editing needs that Pad Function. Values come from a beat dump.
   The beat-wide positions of n°29 are knobs of the Machine view (panel-machine.js); this module only says whether
   the installed OS has them.

   A control the installed OS cannot use is greyed and says which version brings it: the features come from Rust
   (`features` of the `state` and `installed` events: on, since), from the identity answer (a DSI OS has none) and from
   the fake machine (it plays Tempester 1.6.5.0, the newest release).

   Tempester.panelPerform.createFeatures() -> {set(features, osKnown), setFake(on), setIdentity(tempester), status(name)}
        status(name) = {on, why}: name is a key of Rust's table (beat_wide, ribbons, probability, pad_channels...)
   Tempester.panelPerform.createRibbon(r, send) -> {touch(fraction, pressure), release(), report(position), state()}
        send(r, position | null, pressure | undefined)
   Tempester.panelPerform.createProbability(send) -> {fromBeat(list), report(pad, p), value(pad), begin(pad),
        drag(pad, dyUp), nudge(pad, delta), commit(pad), reset()}; send(pad, percent)
   Tempester.panelPerform.attach(deps) binds the drawings; deps = {hosts, link, remote}: returns
        {setRibbonsUsable(on, why), setPadsEditable(on, why), setProbMode(on), probMode(), releaseRibbons(),
        reportRibbon(r, position | null), probabilities (the createProbability model), showPads()} */
"use strict";
window.Tempester = window.Tempester || {};
window.Tempester.panelPerform = (function () {
  const RIBBON_MAX = 127;
  const PROB_MAX = 100;
  /* A drag of 150 px covers the whole range, as for the Machine view's absolute knobs. */
  const PROB_DRAG_PX = 150;
  const PROB_WHEEL_STEP = 5;
  const PROB_PAGE_STEP = 10;
  /* The wheel has no end: its value goes out once it has rested this long. */
  const WHEEL_COMMIT_MS = 400;
  const RIBBON_KEY_STEP = 8;

  const LABELS = {
    beat_wide: "Beat-wide positions need",
    ribbons: "FX ribbons over MIDI need",
    probability: "Play probability needs",
    pad_channels: "MIDI channel per pad needs",
  };
  const S = {
    needs: (name, since) =>
      since
        ? LABELS[name] + " Tempester " + since + " or later"
        : LABELS[name] + " a Tempester OS (version not known yet)",
    unknown:
      " (the OS of the machine is unknown to the app: connect and check the identity, or install a Tempester from the OS page)",
    dsi: " (the machine answered as the DSI OS)",
    notConnected: "Not connected",
    ribbon: (n) =>
      "FX" + n + " ribbon: drag up and down to touch it, let go to release it",
    padsMode:
      "16 Sounds only: set Pad Function to 16 Sounds on the machine (a pad press selects its sound only there)",
    pad: (k, p) =>
      "Pad " +
      (k + 1) +
      ": plays " +
      (p === null ? "?" : p) +
      " % of its notes. Drag, wheel or arrows; sent when you let go (the pad is pressed to select it)",
  };

  /* ---------- What the installed OS can do ---------- */
  function createFeatures() {
    let features = {},
      osKnown = null,
      fake = false,
      tempester = null;
    /* The same rule as Rust's `has_feature` (storage/installed.rs): an OS the app can name decides, an OS it cannot
       name has no feature (greyed, and the tooltip says the OS is unknown), except on the fake machine; an identity
       answer of the DSI OS beats both. `osKnown` is the `os_known` field of the `state` and `installed` events:
       `since` always arrives, so it cannot tell. */
    function status(name) {
      const f = features[name] || {};
      if (tempester === false)
        return { on: false, why: S.needs(name, f.since) + S.dsi };
      if (fake) return { on: true, why: "" };
      if (f.on) return { on: true, why: "" };
      return {
        on: false,
        why: S.needs(name, f.since) + (osKnown === false ? S.unknown : ""),
      };
    }
    return {
      set: (next, known) => {
        features = next || {};
        osKnown = known === undefined ? null : !!known;
      },
      setFake: (on) => (fake = !!on),
      setIdentity: (isTempester) => (tempester = isTempester),
      status,
    };
  }

  /* ---------- One ribbon ---------- */
  const clampInt = (v, max) => Math.max(0, Math.min(max, Math.round(v)));

  function createRibbon(r, send) {
    let touched = false,
      mirrored = false,
      position = null,
      pressure;
    /* `fraction` 0 (bottom) to 1 (top); `force` 0-1 or undefined (a mouse has none worth sending). */
    function touch(fraction, force) {
      const next = clampInt(fraction * RIBBON_MAX, RIBBON_MAX);
      const nextPressure =
        force === undefined
          ? undefined
          : clampInt(force * RIBBON_MAX, RIBBON_MAX);
      if (touched && next === position && nextPressure === pressure)
        return false;
      touched = true;
      position = next;
      pressure = nextPressure;
      send(r, position, pressure);
      return true;
    }
    function release() {
      if (!touched) return false;
      touched = false;
      pressure = undefined;
      send(r, null);
      return true;
    }
    /* The machine's own ribbon; ignored while the user holds this one (the user's finger wins on screen). */
    function report(at) {
      if (touched) return false;
      if (at === null) mirrored = false;
      else {
        mirrored = true;
        position = at;
      }
      return true;
    }
    return {
      touch,
      release,
      report,
      /* What to draw: the level, and whether a finger (here or on the machine) is on it. */
      state: () => ({ position, on: touched || mirrored }),
      nudge: (delta) =>
        touch(((position === null ? 64 : position) + delta) / RIBBON_MAX),
    };
  }

  /* ---------- The 16 pads' probabilities ---------- */
  function createProbability(send) {
    const values = new Array(16).fill(null);
    const sent = new Array(16).fill(null);
    const start = new Array(16).fill(null);
    const set = (pad, v) => {
      const next = clampInt(v, PROB_MAX);
      if (next === values[pad]) return false;
      values[pad] = next;
      return true;
    };
    return {
      /* A beat dump: 32 values, the 16 pads of the panel are the first ones. */
      fromBeat(list) {
        for (let k = 0; k < 16; k++) {
          const v = Array.isArray(list) ? list[k] : undefined;
          values[k] = sent[k] = Number.isInteger(v)
            ? clampInt(v, PROB_MAX)
            : null;
        }
      },
      report(pad, p) {
        values[pad] = sent[pad] = clampInt(p, PROB_MAX);
      },
      value: (pad) => values[pad],
      begin(pad) {
        start[pad] = values[pad] === null ? PROB_MAX : values[pad];
      },
      drag(pad, dyUp) {
        const from = start[pad] === null ? PROB_MAX : start[pad];
        return set(pad, from + (dyUp * PROB_MAX) / PROB_DRAG_PX);
      },
      nudge(pad, delta) {
        const from = values[pad] === null ? PROB_MAX : values[pad];
        return set(pad, from + delta);
      },
      /* The end of a gesture: one message, and only for a value the machine does not have yet. */
      commit(pad) {
        start[pad] = null;
        if (values[pad] === null || values[pad] === sent[pad]) return false;
        sent[pad] = values[pad];
        send(pad, values[pad]);
        return true;
      },
      reset() {
        values.fill(null);
        sent.fill(null);
      },
    };
  }

  /* ---------- Drawing ---------- */
  function attach({ hosts, link, remote }) {
    const ribbons = [0, 1].map((r) => createRibbon(r, link.ribbon));
    const prob = createProbability(link.probability);
    const ribbonNodes = [[], []];
    const padNodes = [];
    let ribbonsUsable = false,
      ribbonsWhy = "",
      padsEditable = false,
      padsWhy = "",
      probMode = false;

    function showRibbon(r) {
      const { position, on } = ribbons[r].state();
      for (const node of ribbonNodes[r]) {
        node.style.setProperty(
          "--niveau",
          position === null ? 0 : position / RIBBON_MAX,
        );
        node.classList.toggle("touche", on);
        node.setAttribute("aria-valuenow", position === null ? 0 : position);
      }
    }
    function showPads() {
      for (const { button, k, label } of padNodes) {
        const p = prob.value(k);
        label.lastChild.textContent = p === null ? "?" : p + "%";
        label.firstChild.style.height = (p === null ? 0 : p) + "%";
        if (probMode) button.title = padsEditable ? S.pad(k, p) : padsWhy;
        else button.removeAttribute("title");
        button.classList.toggle("locked", probMode && !padsEditable);
      }
    }
    function releaseAll() {
      ribbons.forEach((ribbon, r) => {
        if (ribbon.release()) showRibbon(r);
      });
    }

    function bindRibbon(node, r) {
      ribbonNodes[r].push(node);
      node.setAttribute("aria-valuemin", "0");
      node.setAttribute("aria-valuemax", String(RIBBON_MAX));
      const at = (e) => {
        const box = node.getBoundingClientRect();
        return box.height ? (box.bottom - e.clientY) / box.height : 0;
      };
      /* A mouse reports a constant 0.5: only a pen or a finger has a pressure worth sending. */
      const force = (e) =>
        e.pointerType && e.pointerType !== "mouse" ? e.pressure : undefined;
      node.addEventListener("pointerdown", (e) => {
        e.preventDefault();
        node.focus({ preventScroll: true });
        if (!ribbonsUsable) return;
        node.setPointerCapture(e.pointerId);
        ribbons[r].touch(at(e), force(e));
        showRibbon(r);
      });
      node.addEventListener("pointermove", (e) => {
        if (!node.hasPointerCapture(e.pointerId) || !ribbonsUsable) return;
        if (ribbons[r].touch(at(e), force(e))) showRibbon(r);
      });
      for (const end of [
        "pointerup",
        "pointercancel",
        "lostpointercapture",
        "blur",
      ])
        node.addEventListener(end, () => {
          if (ribbons[r].release()) showRibbon(r);
        });
      node.addEventListener("keydown", (e) => {
        const delta = {
          ArrowUp: 1,
          ArrowRight: 1,
          ArrowDown: -1,
          ArrowLeft: -1,
        }[e.key];
        if (!delta || !ribbonsUsable) return;
        e.preventDefault();
        ribbons[r].nudge(delta * RIBBON_KEY_STEP);
        showRibbon(r);
      });
      node.addEventListener("keyup", () => {
        if (ribbons[r].release()) showRibbon(r);
      });
    }

    function bindPads(zone) {
      zone.querySelectorAll(".pad").forEach((button, k) => {
        const label = document.createElement("span");
        label.className = "prob";
        label.setAttribute("aria-hidden", "true");
        label.append(document.createElement("i"), document.createElement("b"));
        button.append(label);
        padNodes.push({ button, k, label });
      });
      const padOf = (e) => {
        const button = e.target.closest && e.target.closest(".pad");
        const found = padNodes.find((p) => p.button === button);
        return found ? found.k : null;
      };
      let held = null;
      /* Capture on the zone: in the probability mode the pads' own press (a note) never runs. */
      zone.addEventListener(
        "pointerdown",
        (e) => {
          if (!probMode) return;
          const k = padOf(e);
          if (k === null) return;
          e.preventDefault();
          e.stopPropagation();
          if (!padsEditable) return;
          held = {
            k,
            y0: e.clientY,
            id: e.pointerId,
            button: e.target.closest(".pad"),
          };
          held.button.setPointerCapture(e.pointerId);
          prob.begin(k);
        },
        true,
      );
      zone.addEventListener(
        "pointermove",
        (e) => {
          if (!probMode || !held || e.pointerId !== held.id) return;
          e.stopPropagation();
          if (prob.drag(held.k, held.y0 - e.clientY)) showPads();
        },
        true,
      );
      const end = (e) => {
        if (!probMode) return;
        e.stopPropagation();
        if (!held) return;
        const k = held.k;
        held = null;
        prob.commit(k);
      };
      for (const name of ["pointerup", "pointercancel"])
        zone.addEventListener(name, end, true);
      zone.addEventListener(
        "pointerleave",
        (e) => {
          if (probMode) e.stopPropagation();
        },
        true,
      );
      const wheelTimers = new Map();
      zone.addEventListener(
        "wheel",
        (e) => {
          if (!probMode) return;
          const k = padOf(e);
          if (k === null) return;
          e.preventDefault();
          if (!padsEditable) return;
          if (prob.nudge(k, e.deltaY < 0 ? PROB_WHEEL_STEP : -PROB_WHEEL_STEP))
            showPads();
          clearTimeout(wheelTimers.get(k));
          wheelTimers.set(
            k,
            setTimeout(() => prob.commit(k), WHEEL_COMMIT_MS),
          );
        },
        { passive: false, capture: true },
      );
      zone.addEventListener(
        "keydown",
        (e) => {
          if (!probMode) return;
          const k = padOf(e);
          const delta = {
            ArrowUp: 1,
            ArrowRight: 1,
            ArrowDown: -1,
            ArrowLeft: -1,
            PageUp: PROB_PAGE_STEP,
            PageDown: -PROB_PAGE_STEP,
          }[e.key];
          if (k === null || !delta) return;
          e.preventDefault();
          e.stopPropagation();
          if (padsEditable && prob.nudge(k, delta)) showPads();
        },
        true,
      );
      zone.addEventListener(
        "keyup",
        (e) => {
          const k = padOf(e);
          if (probMode && k !== null && padsEditable) prob.commit(k);
        },
        true,
      );
    }

    for (const host of hosts) {
      host.querySelectorAll(".ruban").forEach((node) => {
        bindRibbon(node, node.dataset.l === "FX2" ? 1 : 0);
      });
      host.querySelectorAll(".sec.pads").forEach(bindPads);
    }
    window.addEventListener("blur", releaseAll);
    document.addEventListener("visibilitychange", () => {
      if (document.hidden) releaseAll();
    });

    function setRibbonsUsable(on, why) {
      ribbonsUsable = on;
      ribbonsWhy = why;
      if (!on) {
        releaseAll();
        ribbons.forEach((ribbon, r) => ribbon.report(null) && showRibbon(r));
      }
      ribbonNodes.forEach((nodes, r) =>
        nodes.forEach((node) => {
          if (on) remote.ungrey(node, S.ribbon(r + 1));
          else remote.grey(node, ribbonsWhy);
        }),
      );
    }
    function setPadsEditable(on, why) {
      padsEditable = on;
      padsWhy = why;
      showPads();
    }
    function setProbMode(on) {
      probMode = on;
      hosts.forEach((host) => host.classList.toggle("probmode", on));
      showPads();
    }
    setRibbonsUsable(false, S.notConnected);
    showPads();
    return {
      setRibbonsUsable,
      setPadsEditable,
      setProbMode,
      probMode: () => probMode,
      releaseRibbons: releaseAll,
      reportRibbon(r, position) {
        if (ribbons[r] && ribbons[r].report(position)) showRibbon(r);
      },
      probabilities: prob,
      showPads,
    };
  }

  return {
    RIBBON_MAX,
    PROB_MAX,
    PROB_DRAG_PX,
    WHEEL_COMMIT_MS,
    S,
    createFeatures,
    createRibbon,
    createProbability,
    attach,
  };
})();
