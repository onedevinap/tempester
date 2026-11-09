/* The Machine view of the Panel page: only what the physical Tempest has, where it has it (top face: photo and
   manual). Its controls act like the machine's own through the remote panel of Tempester 1.6.0.0:
     - the 10 absolute knobs send NRPN 220 + k, 0-255 (`control k value`);
     - the 14 encoders, the 4 soft knobs under the screen included, send relative NRPN 200 + e steps
       (`encoder e step`): endless, so they have no position and are drawn without one;
     - the keys go through the key path (CC 102 / 103), the pads send notes; the FX ribbons
       (Tempester 1.6.3.0) are bound by panel-perform.js.
   So the machine decides what a knob does: in 16 Beats and 16 Mutes the sound knobs are beat-wide, exactly as on the
   machine. A sound parameter changed on the machine (per-parameter NRPN) moves the matching absolute knob, and is
   shown under the knob that was just turned (name and value), since the app cannot know which parameter an encoder
   edits (it depends on the selected LFO / envelope / oscillators and on the screen).
   An encoder shows its turning only: a bright dot that moves by ENDLESS_STEP_DEG per detent, the part of the ring the
   last turn crossed, and the count of the burst ("+3", "-1"). In 16 Beats and 16 Mutes the controls the firmware
   ignores there are greyed (INERT_IN_BEAT_WIDE).
   With Tempester 1.6.2.0 (n°29) the 8 controls that are beat-wide in 16 Beats and 16 Mutes (BEAT_WIDE, encoders
   included) show and set the beat-wide position itself (0-254, 127 = neutral) in those modes: `link.beatWide(i, p)`
   sends CC 20 + i (and the LSB 52 + i), and the positions the machine reports move them. The position shows as an
   offset from neutral under the knob.

   The face itself is a fixed canvas (panel-face.js: the machine's arrangement, panel-face.css: placement only; the
   look is the Full view's, panel.css): every element is placed absolutely at its place and in a box of fixed size, so
   that nothing moves whatever a caption, a count or a LED shows (ui-tests/layout-stability.js proves it in Chrome and
   WebKit) and no text meets another text or a control (ui-tests/layout-overlap.js).

   Tempester.panelMachine.createControls(link, now)   the controls without any drawing (the part the tests run):
        link = {control(k, value), encoder(e, step), beatWide(i, position)}; now() = clock in ms (Date.now), a parameter for the tests
   Tempester.panelMachine.mount(container, deps) draws the view; deps = {link, onKey, onPad, onTransport, remote,
        panel (Tempester.panel.parts), paramMax(index), paramName(index)}; returns {setRemote(on),
        setParam(index, value), setValues(values), setScreen({name, mode}), setMode(index | null),
        setBeatWide(on), reportBeatWide(i, position), refit()}
   Tempester.panelMachine.CONTROLS               the knob table, checked against refs/panel_controls.tsv */
"use strict";
window.Tempester = window.Tempester || {};
window.Tempester.panelMachine = (function () {
  const NS = "http://www.w3.org/2000/svg";
  const MAX_FIT = 1.45;

  /* Every knob of the machine: kind (abs = absolute control, enc = encoder), number, the label printed on the
     machine, the tooltip (the firmware's name, the Shift function in brackets). */
  const CONTROLS = [
    ["enc", 0, "Rate", "LFO Rate (Shift: Shape)"],
    ["enc", 1, "Pitch", "Pitch (Shift: Fine)"],
    ["enc", 2, "1/2 Mix", "1/2 Mix (Shift: Level, Pre/Post Filter)"],
    ["enc", 3, "Shape", "Shape (Shift: Sample)"],
    ["enc", 4, "Amount", "LFO Amount (Shift: Destination)"],
    ["enc", 5, "Amount", "Env Amount (Shift: Velocity Amt)"],
    ["enc", 6, "Decay", "Env Decay (Shift: Release)"],
    ["enc", 7, "Attack", "Env Attack (Shift: Sustain Level)"],
    ["enc", 8, "BPM", "BPM (Shift: Swing)"],
    ["enc", 9, "Delay Send", "Delay Send (Shift: Delay Time)"],
    ["enc", 10, "", "Soft knob 4 (Shift: Swing in Beat Events)"],
    ["enc", 11, "", "Soft knob 3 (Shift: Quantize in Beat Events)"],
    ["enc", 12, "", "Soft knob 2"],
    ["enc", 13, "", "Soft knob 1 (Shift: Assign Row in Beat Events)"],
    ["abs", 0, "Feedback", "Feedback"],
    ["abs", 1, "Frequency", "Highpass Frequency"],
    ["abs", 2, "Compress", "Compress"],
    ["abs", 3, "Audio Mod", "Audio Mod"],
    ["abs", 4, "Resonance", "Lowpass Resonance"],
    ["abs", 5, "Frequency", "Lowpass Frequency"],
    ["abs", 6, "Pan", "Pan"],
    ["abs", 7, "Level", "Level"],
    ["abs", 8, "Volume", "Volume"],
    ["abs", 9, "Distort", "Distort"],
  ].map(([kind, n, label, tip]) => ({ kind, n, label, tip }));

  /* The sound parameter each absolute knob is the same control as (Feedback 33, HP Freq 41, Audio Mod 39,
     Resonance 35, LP Freq 34); the others (Compress, Pan, Level, Volume, Distort) are not sound parameters. */
  const PARAM_OF_ABS = { 0: 33, 1: 41, 3: 39, 4: 35, 5: 34 };
  const ABS_OF_PARAM = Object.fromEntries(
    Object.entries(PARAM_OF_ABS).map(([k, p]) => [p, +k]),
  );
  /* Pad Function modes (panel-mode.js) in which the sound controls are beat-wide on the machine. */
  const BEAT_WIDE_MODES = [0, 4];
  /* The controls the firmware ignores in 16 Beats and 16 Mutes, Shift up or down: [kind, number, why]. Proven by
     running the image's own panel handlers in a simulator (one of the firmware simulations, not in the public
     tree). Pan and Delay Send act there with Shift (beat-level Pan, Delay Time) and
     the soft knobs belong to whatever screen is shown: they are NOT in this table. */
  const INERT_IN_BEAT_WIDE = [
    ["enc", 0, "LFO Rate"],
    ["enc", 2, "1/2 Mix"],
    ["enc", 3, "Shape"],
    ["enc", 4, "LFO Amount"],
    ["enc", 5, "Env Amount"],
    ["abs", 7, "Level"],
  ];
  const INERT_MESSAGE = "No effect in 16 Beats / 16 Mutes on the machine";
  /* The controls that are beat-wide in 16 Beats and 16 Mutes, with the beat-wide parameter each one moves (n°29:
     CC 20 + index; Docs/analyses/feature-beat-wide-emission.md, section 2). */
  const BEAT_WIDE = [
    ["enc", 1, 0, "Osc Freq"],
    ["abs", 0, 1, "Feedback"],
    ["abs", 5, 2, "LP Freq"],
    ["abs", 4, 3, "Resonance"],
    ["abs", 3, 4, "Audio Mod"],
    ["abs", 1, 5, "HP Freq"],
    ["enc", 7, 6, "Attack"],
    ["enc", 6, 7, "Decay"],
  ];
  const BW_MAX = 254,
    BW_NEUTRAL = 127;
  const bwTip = (name) =>
    "Beat-wide " + name + ": every sound of the beat, 0-254 (127 = neutral)";
  const formatOffset = (p) => {
    const d = p - BW_NEUTRAL;
    return d > 0 ? "+" + d : d < 0 ? "\u2212" + -d : "0";
  };
  const isInert = (kind, n, mode) =>
    BEAT_WIDE_MODES.includes(mode) &&
    INERT_IN_BEAT_WIDE.some(([k, i]) => k === kind && i === n);

  /* Encoder feedback. */
  const ENDLESS_STEP_DEG = 15, /* 24 detents a turn: a step is visible, a turn is countable */
    MAX_FLASH_SWEEP = 330, /* a long turn lights almost the whole ring, never a full circle that reads as nothing */
    FLASH_MS = 450, /* the lit ring segment */
    COUNT_WINDOW_MS = 700, /* steps closer than this add up in one "+3"; it then fades */
    EXPIRY_POLL_MS = 100;
  /* The machine answers a turn with a per-parameter NRPN within a main-loop pass plus the MIDI transfer (a few
     ms, USB or DIN); 400 ms is far more than that and still shorter than a second, deliberate turn of another knob. */
  const ATTRIBUTION_MS = 400;
  const ABS_MAX = 255,
    ABS_MID = 128,
    /* The machine reads value >> 1: two units are one step of its 7-bit knob. */
    ABS_KEY_STEP = 2,
    ABS_DRAG_PX = 150,
    ENC_PX_PER_STEP = 8,
    ENC_MAX_STEP = 15,
    ENC_MIN_STEP = -16;

  /* ---------- The controls, without any drawing ---------- */
  const angleOf = (detents) =>
    (((detents * ENDLESS_STEP_DEG) % 360) + 360) % 360;
  const formatCount = (sum) =>
    sum > 0 ? "+" + sum : sum < 0 ? "\u2212" + -sum : "0";

  function createControls(link, now = () => Date.now()) {
    const abs = new Array(10).fill(null);
    const pending = new Array(14).fill(0);
    const turned = new Array(14).fill(0);
    const bursts = Array.from({ length: 14 }, () => ({
      sum: 0,
      at: -Infinity,
      flash: null,
      flashStart: 0,
    }));
    const captions = new Map();
    const dragging = new Set();
    let lastTurn = null;
    let mode = null;
    const bw = new Array(BEAT_WIDE.length).fill(null);
    const bwHeld = new Set();
    let bwAvailable = false;
    /* The beat-wide parameter a control sets now, or null: only in 16 Beats / 16 Mutes with n°29. */
    function beatWideOf(kind, n) {
      if (!bwAvailable || !BEAT_WIDE_MODES.includes(mode)) return null;
      const row = BEAT_WIDE.find(([k, i]) => k === kind && i === n);
      return row ? row[2] : null;
    }
    /* The caption of a beat-wide knob appears when the user turns it (8 at once after every report of the machine
       would hide the face's labels); a report then keeps it up to date. */
    function bwCaption(i, onlyIfShown = false) {
      const [kind, n, , name] = BEAT_WIDE.find((row) => row[2] === i);
      if (onlyIfShown && !captions.has(keyOf(kind, n))) return;
      captions.set(keyOf(kind, n), {
        text: { name, value: formatOffset(bw[i]) },
      });
    }
    function setBeatWidePos(i, position) {
      const next = Math.max(0, Math.min(BW_MAX, Math.round(position)));
      if (next === bw[i]) return false;
      bw[i] = next;
      bwCaption(i);
      link.beatWide(i, next);
      return true;
    }
    const clampAbs = (n) => Math.max(0, Math.min(ABS_MAX, Math.round(n)));
    const keyOf = (kind, n) => kind + n;

    /* A control was just turned by the user: the next parameter NRPN from the machine is its answer, and what it
       showed before is stale. */
    function noteTurn(kind, n) {
      lastTurn = { kind, n, at: now(), index: null };
      captions.delete(keyOf(kind, n));
    }

    function setAbs(k, value) {
      const next = clampAbs(value);
      if (next === abs[k]) return false;
      abs[k] = next;
      noteTurn("abs", k);
      link.control(k, next);
      return true;
    }
    /* A drag is relative to where the knob was when it was taken (the middle when its position is not known). */
    function dragAbs(k, startValue, dyUp) {
      const from = startValue === null ? ABS_MID : startValue;
      return setAbs(k, from + (dyUp * ABS_MAX) / ABS_DRAG_PX);
    }
    function nudgeAbs(k, detents) {
      const from = abs[k] === null ? ABS_MID : abs[k];
      return setAbs(k, from + detents * ABS_KEY_STEP);
    }
    /* Steps leave in messages of at most 15 / -16 detents (what one NRPN can carry), none dropped. */
    function stepEncoder(e, detents) {
      let left = Math.trunc(detents);
      if (left !== 0) recordBurst(e, left);
      while (left !== 0) {
        const chunk = Math.max(ENC_MIN_STEP, Math.min(ENC_MAX_STEP, left));
        link.encoder(e, chunk);
        left -= chunk;
      }
      return detents !== 0;
    }
    /* The lit ring segment is the whole stretch turned since the light came on (a drag adds one detent every few
       pixels), restarted when it went out or when the direction reverses. */
    function recordBurst(e, detents) {
      const burst = bursts[e],
        t = now();
      if (t - burst.at > COUNT_WINDOW_MS) burst.sum = 0;
      const restart =
        t - burst.at >= FLASH_MS ||
        burst.flash === null ||
        Math.sign(detents) !== Math.sign(burst.flash.sweep);
      if (restart) burst.flashStart = turned[e];
      turned[e] += detents;
      burst.sum += detents;
      burst.at = t;
      const sweep = (turned[e] - burst.flashStart) * ENDLESS_STEP_DEG;
      burst.flash = {
        from: angleOf(burst.flashStart),
        sweep: Math.max(-MAX_FLASH_SWEEP, Math.min(MAX_FLASH_SWEEP, sweep)),
      };
      noteTurn("enc", e);
    }
    /* What an encoder shows now: the dot, the ring segment of the last turn, the count of the burst. */
    function encoderView(e) {
      const burst = bursts[e],
        age = now() - burst.at;
      return {
        angle: angleOf(turned[e]),
        flash: age < FLASH_MS ? burst.flash : null,
        count: age < COUNT_WINDOW_MS ? formatCount(burst.sum) : "",
      };
    }
    /* A per-parameter NRPN from the machine. The absolute knobs that are a sound parameter own theirs by number;
       any other one answers the control turned within ATTRIBUTION_MS, and only the first parameter it reports
       (an encoder may touch several, the later ones are not its caption). Returns the control or null. */
    function attribute(index, value) {
      const owner = ABS_OF_PARAM[index];
      let target = null;
      if (owner !== undefined) target = { kind: "abs", n: owner };
      else if (
        lastTurn !== null &&
        now() - lastTurn.at <= ATTRIBUTION_MS &&
        (lastTurn.index === null || lastTurn.index === index)
      ) {
        lastTurn.index = index;
        target = lastTurn;
      }
      if (target === null) return null;
      captions.set(keyOf(target.kind, target.n), { index, value });
      return { kind: target.kind, n: target.n };
    }
    function turnEncoder(e, dyUp) {
      pending[e] += dyUp / ENC_PX_PER_STEP;
      const whole = Math.trunc(pending[e]);
      if (whole === 0) return false;
      pending[e] -= whole;
      return stepEncoder(e, whole);
    }
    function paramToAbs(index, value, max) {
      const k = ABS_OF_PARAM[index];
      if (k === undefined || dragging.has(k) || !max) return null;
      abs[k] = clampAbs((value * ABS_MAX) / max);
      return k;
    }
    return {
      abs: (k) => abs[k],
      turned: (e) => turned[e],
      encoderView,
      attribute,
      caption: (kind, n) => captions.get(keyOf(kind, n)) || null,
      clearCaptions: () => captions.clear(),
      setMode: (index) => (mode = index),
      mode: () => mode,
      setBeatWideAvailable: (on) => (bwAvailable = !!on),
      beatWideOf,
      beatWide: (i) => bw[i],
      dragBeatWide: (i, startValue, dyUp) =>
        setBeatWidePos(
          i,
          (startValue === null ? BW_NEUTRAL : startValue) + (dyUp * BW_MAX) / ABS_DRAG_PX,
        ),
      nudgeBeatWide: (i, detents) =>
        setBeatWidePos(i, (bw[i] === null ? BW_NEUTRAL : bw[i]) + detents * ABS_KEY_STEP),
      /* A position the machine reported; ignored while the user holds that knob (the drag wins). */
      reportBeatWide(i, position) {
        if (i < 0 || i >= bw.length || bwHeld.has(i)) return false;
        bw[i] = Math.max(0, Math.min(BW_MAX, position));
        bwCaption(i, true);
        return true;
      },
      holdBeatWide: (i) => bwHeld.add(i),
      releaseBeatWide: (i) => bwHeld.delete(i),
      forgetBeatWide: () => bw.fill(null),
      inert: (kind, n) => isInert(kind, n, mode),
      setAbs,
      dragAbs,
      nudgeAbs,
      stepEncoder,
      turnEncoder,
      paramToAbs,
      hold: (k) => dragging.add(k),
      release: (k) => dragging.delete(k),
      resetPending: (e) => (pending[e] = 0),
    };
  }

  /* ---------- Drawing ---------- */
  const el = (tag, cls, html) => {
    const node = document.createElement(tag);
    if (cls) node.className = cls;
    if (html !== undefined) node.innerHTML = html;
    return node;
  };
  /* Everything on the face sits at a fixed place of the canvas (panel-face.js, photo pixels): nothing flows, so no
     caption, value or state can push a neighbour. */
  function placer(origin) {
    return (node, x, y, w, h) => {
      node.style.left = x - origin.x + "px";
      node.style.top = y - origin.y + "px";
      if (w !== undefined) {
        node.style.width = w + "px";
        node.style.height = h + "px";
      }
      return node;
    };
  }

  function mount(container, deps) {
    const { link, panel } = deps;
    const F = window.Tempester.panelFace;
    const at = placer(F.ORIGIN);
    const controls = createControls(link);
    const draws = [];
    const knobNodes = [];
    let available = false;
    container.classList.add("panel-root", "panel-machine");
    container.innerHTML = "";
    const note = el("p", "mvnote", NOTE);
    note.setAttribute("role", "status");
    const frame = el("div", "cadre", `<div class="machine mv"></div>`);
    container.append(frame, note);
    const face = frame.querySelector(".machine");
    face.style.height = F.SIZE.h + "px";

    function knob(kind, n, cx, cy) {
      const def = CONTROLS.find((c) => c.kind === kind && c.n === n);
      const d = el("div", "pot mvpot");
      at(d, cx - F.KNOB.box / 2, cy - F.KNOB.box / 2, F.KNOB.box, F.KNOB.box);
      const svg = document.createElementNS(NS, "svg");
      svg.setAttribute("viewBox", "0 0 40 40");
      svg.setAttribute("tabindex", "0");
      svg.setAttribute("role", "slider");
      svg.setAttribute("aria-label", def.tip);
      d.append(svg);
      d.title = def.tip;
      const encoder = kind === "enc";
      const count = el("span", "cnt");
      count.setAttribute("aria-hidden", "true");
      if (encoder) d.append(count);
      let expiry = null;
      const draw = () => {
        const wide = controls.beatWideOf(kind, n);
        if (encoder) count.hidden = wide !== null;
        if (wide !== null) {
          const p = controls.beatWide(wide);
          panel.drawKnob(svg, null, p === null ? 0.5 : p / BW_MAX);
          svg.setAttribute(
            "aria-valuetext",
            p === null ? "position unknown" : "beat-wide " + p,
          );
          d.classList.toggle("inconnu", p === null);
          return;
        }
        if (encoder) {
          const view = controls.encoderView(n);
          panel.drawKnob(svg, null, 0, view);
          /* Gone, the count keeps its text so that it fades out instead of blanking. */
          if (view.count) count.textContent = view.count;
          count.classList.toggle("on", view.count !== "");
          clearTimeout(expiry);
          if (view.count) expiry = setTimeout(draw, EXPIRY_POLL_MS);
        } else {
          const value = controls.abs(n);
          /* A position that is not known is drawn in the middle, dimmed (the "inconnu" class). */
          panel.drawKnob(svg, null, value === null ? 0.5 : value / ABS_MAX);
          svg.setAttribute(
            "aria-valuetext",
            value === null ? "position unknown" : String(value),
          );
          d.classList.toggle("inconnu", value === null);
        }
      };
      if (encoder) svg.setAttribute("aria-valuetext", "endless encoder");
      draws.push(draw);
      draw();
      bindKnob(svg, kind, n, draw);
      knobNodes.push({ svg, def });
      return d;
    }
    function usable(svg) {
      return available && svg.getAttribute("aria-disabled") !== "true";
    }
    function bindKnob(svg, kind, n, draw) {
      let y0 = 0,
        v0 = null,
        wide = null;
      const encoder = kind === "enc";
      const nudge = (detents) => {
        if (!usable(svg)) return;
        const i = controls.beatWideOf(kind, n);
        if (i !== null) controls.nudgeBeatWide(i, detents);
        else if (encoder) controls.stepEncoder(n, detents);
        else controls.nudgeAbs(n, detents);
        draw();
      };
      svg.addEventListener("pointerdown", (e) => {
        e.preventDefault();
        svg.focus({ preventScroll: true });
        if (!usable(svg)) return;
        y0 = e.clientY;
        wide = controls.beatWideOf(kind, n);
        if (wide !== null) {
          v0 = controls.beatWide(wide);
          controls.holdBeatWide(wide);
          svg.setPointerCapture(e.pointerId);
          return;
        }
        v0 = encoder ? null : controls.abs(n);
        if (encoder) controls.resetPending(n);
        else controls.hold(n);
        svg.setPointerCapture(e.pointerId);
      });
      svg.addEventListener("pointermove", (e) => {
        if (!svg.hasPointerCapture(e.pointerId) || !available) return;
        if (wide !== null) controls.dragBeatWide(wide, v0, y0 - e.clientY);
        else if (encoder) {
          controls.turnEncoder(n, y0 - e.clientY);
          y0 = e.clientY;
        } else controls.dragAbs(n, v0, y0 - e.clientY);
        draw();
      });
      for (const end of ["pointerup", "pointercancel"])
        svg.addEventListener(end, () => {
          if (wide !== null) controls.releaseBeatWide(wide);
          else controls.release(n);
          wide = null;
        });
      svg.addEventListener(
        "wheel",
        (e) => {
          e.preventDefault();
          nudge(e.deltaY < 0 ? 1 : -1);
        },
        { passive: false },
      );
      svg.addEventListener("keydown", (e) => {
        const detents = {
          ArrowUp: 1,
          ArrowRight: 1,
          ArrowDown: -1,
          ArrowLeft: -1,
        }[e.key];
        if (detents) {
          e.preventDefault();
          nudge(detents);
        }
      });
    }

    /* ----- Sheet metal, lines and silkscreen ----- */
    const add = (node) => face.append(node);
    const bodyHeight = F.SIZE.h;
    add(at(el("div", "bois mvwood l"), 35, 41, 65, bodyHeight));
    add(at(el("div", "bois mvwood r"), 1902, 41, 65, bodyHeight));
    add(at(el("div", "tole mvbody"), 100, 41, 1802, bodyHeight));
    for (const [x0, y0, x1, y1, kind] of F.ZONES)
      add(at(el("div", "sec mvzone " + kind), x0, y0, x1 - x0, y1 - y0));
    face.insertAdjacentHTML("beforeend", F.linesMarkup());
    for (const [text, cx, cy] of F.TITLES) {
      const t = el("span", "mvtitle");
      t.textContent = text;
      add(at(t, cx, cy));
    }
    for (const [text, cx, cy, style] of F.TEXTS) {
      const t = el("span", "mvt " + style);
      t.textContent = text;
      t.setAttribute("aria-hidden", "true");
      add(at(t, cx, cy));
    }
    /* The lettering of the Full view's bar: plain, spaced, no maker's name or mark. */
    const brand = el("div", "marque mvbrand", "<b>TEMPESTER</b> \u00b7 Remote panel");
    brand.setAttribute("aria-hidden", "true");
    add(at(brand, 848, F.TOP + 66));

    /* ----- Keys ----- */
    function key(name, x, y, w, h, led) {
      const d = el("div", "bt mvk");
      at(d, x, y, w, h);
      const b = el("button", "");
      b.type = "button";
      b.setAttribute("aria-label", name);
      b.dataset.key = name;
      if (name.startsWith("Shift")) d.classList.add("clair");
      if (led) {
        const { gap, w: ledW } = F.LED_SIZE;
        const lx =
          led.side === "left"
            ? -ledW - gap
            : led.side === "shift"
              ? w + 5
              : w + gap;
        b.style.setProperty("--lx", lx + "px");
        b.style.setProperty("--ly", (led.dy === null ? (h - F.LED_SIZE.h) / 2 : led.dy) + "px");
        b.style.setProperty("--lw", ledW + "px");
        b.style.setProperty("--lh", F.LED_SIZE.h + "px");
        b.classList.add("hasled");
        if (led.side === "shift") b.classList.add("tiny");
      }
      d.append(b);
      return d;
    }
    for (const [name, x, y, w, h, led] of F.KEYS)
      add(key(name, x, y, w, h, led));
    for (const [led, x, y] of F.OSC_LEDS) {
      const i = el("i", "mvosc");
      i.dataset.led = led;
      i.title = "Oscillator " + (led - 0x26);
      add(at(i, x, y, F.LED_SIZE.w, F.LED_SIZE.h));
    }
    /* The transport keys are the Full view's (`.tb`): « and » narrow, REC, STOP and PLAY larger (panel-face.js);
       Play and Stop also drive the app's clock. Rew and FF carry no glyph: their « » is printed above them, as on the
       machine. */
    const GLYPH = { Rec: "\u25cf", Stop: "\u25a0", Play: "\u25b6" };
    for (const [name, x, y, w, h, kind] of F.TRANSPORT) {
      const d = el("div", "tb mvk " + kind);
      if (name === "Play" || name === "Stop")
        d.dataset.transport = name === "Play" ? "start" : "stop";
      at(d, x, y, w, h);
      const b = el("button", "");
      b.type = "button";
      b.setAttribute("aria-label", name);
      b.dataset.key = name;
      b.textContent = GLYPH[name] || "";
      d.append(b);
      add(d);
    }
    /* The LEDs of REC and PLAY, right of their titles: lit by the machine's LED events like the oscillator LEDs. */
    for (const [led, x, y, colour] of F.TRANSPORT_LEDS) {
      const i = el("i", "tled " + colour);
      i.dataset.led = led;
      i.setAttribute("aria-hidden", "true");
      add(at(i, x, y, F.LED_SIZE.w, F.LED_SIZE.h));
    }

    /* ----- Knobs ----- */
    for (const [kind, n, cx, cy] of F.KNOBS) add(knob(kind, n, cx, cy));

    /* ----- The screen ----- */
    const s = F.SCREEN;
    const display = el("div", "ecran mvlcd");
    display.setAttribute("aria-live", "polite");
    display.innerHTML = F.screenMarkup();
    add(at(display, s.x, s.y, s.w, s.h));

    /* ----- Real Time FX ribbons, then the pads ----- */
    for (const r of F.RIBBONS) {
      const ribbon = el("div", "ruban mvribbon", F.ribbonMarkup());
      ribbon.dataset.l = r.name;
      ribbon.setAttribute("role", "slider");
      ribbon.setAttribute("aria-label", r.name);
      add(at(ribbon, r.x, r.y, r.w, r.h));
    }
    const pads = panel.buildPads(deps.onPad);
    pads.classList.add("mvpads");
    const P = F.PAD;
    pads.style.setProperty("--pad-w", P.w + "px");
    pads.style.setProperty("--pad-h", P.h + "px");
    pads.style.setProperty("--pad-col-gap", P.pitch - P.w + "px");
    pads.style.setProperty("--pad-row-gap", P.y[1] - P.y[0] - P.h + "px");
    add(at(pads, P.x, P.y[0]));

    panel.bindTransport(face, deps.onTransport);
    face
      .querySelectorAll(".mvribbon")
      .forEach((node) => deps.remote.grey(node, panel.NO_MIDI));
    deps.remote.attach(face, deps.onKey);

    /* A knob is greyed when the machine does not answer, or when the firmware ignores it in the Pad Function mode
       shown (an unknown mode greys nothing). */
    function refreshGrey() {
      for (const { svg, def } of knobNodes) {
        const wide = controls.beatWideOf(def.kind, def.n);
        if (!available) deps.remote.grey(svg, deps.remote.NEEDS);
        else if (controls.inert(def.kind, def.n))
          deps.remote.grey(svg, INERT_MESSAGE);
        else if (wide !== null) deps.remote.ungrey(svg, bwTip(BEAT_WIDE.find((row) => row[2] === wide)[3]));
        else deps.remote.ungrey(svg, def.tip);
      }
    }
    /* The mode or the availability changed what a knob is: its drawing and tooltip follow. */
    function refresh() {
      controls.clearCaptions();
      refreshGrey();
      redraw();
    }
    function setRemote(on) {
      available = on;
      note.hidden = on;
      refreshGrey();
    }
    function setMode(index) {
      controls.setMode(index);
      refresh();
    }
    function setBeatWide(on) {
      controls.setBeatWideAvailable(on);
      if (!on) controls.forgetBeatWide();
      refresh();
    }
    function reportBeatWide(i, position) {
      if (controls.reportBeatWide(i, position)) redraw();
    }
    function redraw() {
      draws.forEach((d) => d());
    }
    function setParam(index, value) {
      const moved = controls.paramToAbs(index, value, deps.paramMax(index));
      const answered = controls.attribute(index, value);
      if (moved !== null || answered !== null) redraw();
    }
    /* A fetched sound replaces every value: the captions were about the old one. */
    function setValues(values) {
      controls.clearCaptions();
      values.forEach((value, index) => {
        controls.paramToAbs(index, value, deps.paramMax(index));
      });
      redraw();
    }
    /* `name` and `mode` come from different events: each call changes only what it carries. */
    const shown = { name: "", mode: "" };
    function setScreen(change) {
      Object.assign(shown, change);
      display.querySelector(".nom").textContent =
        shown.name || "No sound fetched";
      display.querySelector(".mode").textContent = shown.mode;
    }
    setScreen({});
    setRemote(false);
    return {
      setRemote,
      setParam,
      setValues,
      setScreen,
      setMode,
      setBeatWide,
      reportBeatWide,
      refit: () => {
        if (!frame.clientWidth) return;
        window.Tempester.panelView.applyScale(frame, face, F.SIZE.w, MAX_FIT);
      },
      frame,
    };
  }

  const NOTE =
    "The Machine view needs Tempester 1.6.0.0 with the remote panel: Remote Pad IN on this app's channel, Remote Pad OUT not Off. Until the machine answers, the controls are greyed.";

  return {
    CONTROLS,
    PARAM_OF_ABS,
    BEAT_WIDE_MODES,
    BEAT_WIDE,
    INERT_IN_BEAT_WIDE,
    INERT_MESSAGE,
    ENDLESS_STEP_DEG,
    ATTRIBUTION_MS,
    isInert,
    MAX_FIT,
    createControls,
    mount,
  };
})();
