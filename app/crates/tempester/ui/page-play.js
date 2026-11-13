/* Play page: 16 on-screen pads (pads A1-A16 on the chosen channel, each played by Rust with its note in the machine's
   pad note table, pad-notes.js), laid out like the machine's (two rows of
   eight, pads 1-8 on top), the clock master for a Tempest set to slave (Start, clock at a tempo, Stop) and, under each
   pad, its Euclidean pattern: steps, pulses, rotation and probability, played on the app clock (the timing is all in
   Rust, on the clock thread). One screen: a transport strip over the grid, which scrolls inside itself if the window
   is short. */
"use strict";
(function () {
  const { el, toast, errorText } = window.Tempester.kit;
  const { button, info } = window.Tempester.icons;
  const bridge = window.Tempester.bridge;
  const kit = window.Tempester.kit;

  const S = {
    title: "Play",
    padsTitle: "Pads",
    padsLead:
      "Play the pads from the screen, on the MIDI channel chosen on the Machine page, and drive the Tempest's tempo as its clock master.",
    channelChip: (channel) => "Ch " + channel + " · pads A1-A16",
    seqTitle: "Euclidean sequencer",
    seqLead:
      "Under each pad: Len = steps (16th notes), Hit = pulses spread evenly over the steps, Rot = rotation (steps the pattern is delayed), % = probability of each hit. Pads with no hit stay silent; lengths may differ (polymetric). It plays the pads as notes on the app clock, so press Start the clock first: it does not follow an incoming clock. Velocity is the one of the strip.",
    seqOn: "Euclidean sequencer on: click to stop it",
    seqOff: "Euclidean sequencer off: click to play the patterns on the clock",
    velocity: "Velocity",
    clockTitle: "Clock master",
    clockLead:
      "Set the Tempest to slave its clock to MIDI (System > MIDI Clock source), then press Start: the app sends MIDI Start and 24 clocks per quarter note at the tempo, and Stop when you press Stop. It cannot run during an OS update.",
    bpm: "Tempo (BPM)",
    start: "Start the clock",
    stop: "Stop the clock",
    running: (bpm) => "Clock running at " + bpm + " BPM.",
    stopped: "Clock stopped.",
    notConnected: "Connect to the machine to play.",
  };

  const PADS = 16;
  const padNotes = window.Tempester.padNotes;
  const FIELDS = [
    { key: "steps", short: "Len", label: "steps", min: 1, max: 32 },
    { key: "pulses", short: "Hit", label: "pulses", min: 0, max: 32 },
    { key: "rotation", short: "Rot", label: "rotation", min: 0, max: 31 },
    { key: "probability", short: "%", label: "probability %", min: 0, max: 100 },
  ];
  const els = { pads: [], cells: [], seqOn: false, bpmRunning: 0, message: "" };

  function velocity() {
    return Number(els.velocity.value);
  }

  function press(k, down) {
    els.pads[k].classList.toggle("down", down);
    bridge.send("pad", down ? "on" : "off", k, down ? velocity() : 0);
  }

  /* The pad's machine note, once Rust has sent the table. */
  function padLabel(k) {
    const note = padNotes.note(k);
    return (
      "Pad " +
      (k + 1) +
      (note === null ? "" : ", note " + note + " " + padNotes.noteName(note))
    );
  }

  function buildPad(k) {
    const pad = el(
      "button",
      {
        class: "playpad",
        type: "button",
        "aria-label": padLabel(k),
      },
      String(k + 1),
    );
    pad.addEventListener("pointerdown", (e) => {
      pad.setPointerCapture(e.pointerId);
      press(k, true);
    });
    for (const end of ["pointerup", "pointercancel"])
      pad.addEventListener(end, () => {
        if (pad.classList.contains("down")) press(k, false);
      });
    pad.addEventListener("keydown", (e) => {
      if ((e.key === " " || e.key === "Enter") && !e.repeat) {
        e.preventDefault();
        press(k, true);
      }
    });
    pad.addEventListener("keyup", (e) => {
      if (
        (e.key === " " || e.key === "Enter") &&
        pad.classList.contains("down")
      )
        press(k, false);
    });
    return pad;
  }

  /* The four numbers of a pad as the page holds them, brought into range the way Rust checks them (pulses within
     steps, rotation below steps); null while a field is empty or not a number. */
  function readPattern(k) {
    const raw = {};
    for (const f of FIELDS) {
      const value = Number.parseInt(els.cells[k].inputs[f.key].value, 10);
      if (!Number.isFinite(value)) return null;
      raw[f.key] = Math.min(f.max, Math.max(f.min, value));
    }
    raw.pulses = Math.min(raw.pulses, raw.steps);
    raw.rotation = Math.min(raw.rotation, raw.steps - 1);
    return raw;
  }

  function showPattern(k, m) {
    const cell = els.cells[k];
    for (const f of FIELDS) cell.inputs[f.key].value = String(m[f.key]);
    const dots = Array.from(m.pattern, (c) =>
      el("span", { class: c === "x" ? "on" : "" }),
    );
    cell.dots.style.gridTemplateColumns = "repeat(" + m.steps + ", 1fr)";
    cell.dots.replaceChildren(...dots);
    cell.dots.setAttribute("title", m.pattern);
  }

  function buildCell(k) {
    const inputs = {};
    const fields = FIELDS.map((f) => {
      inputs[f.key] = el("input", {
        type: "number",
        min: String(f.min),
        max: String(f.max),
        value: f.key === "steps" ? "16" : f.key === "probability" ? "100" : "0",
        "aria-label": "Pad " + (k + 1) + ", " + f.label,
        title: f.label,
      });
      inputs[f.key].addEventListener("change", () => {
        const p = readPattern(k);
        if (!p) return;
        for (const g of FIELDS) inputs[g.key].value = String(p[g.key]);
        bridge.send("seq_pad", k, p.steps, p.pulses, p.rotation, p.probability);
      });
      return el(
        "label",
        { title: f.label },
        el("span", {}, f.short),
        inputs[f.key],
      );
    });
    const dots = el("div", {
      class: "seqdots",
      role: "img",
      "aria-label": "Pad " + (k + 1) + " pattern",
    });
    els.cells[k] = { inputs, dots };
    return el(
      "div",
      { class: "playcell" },
      els.pads[k],
      el("div", { class: "seqctl" }, fields),
      dots,
    );
  }

  function onSeqRun(m) {
    els.seqOn = m.enabled;
    els.seq.setAttribute("aria-pressed", String(m.enabled));
    const label = m.enabled ? S.seqOn : S.seqOff;
    els.seq.setAttribute("aria-label", label);
    els.seq.setAttribute("data-tip", label);
    els.velocity.value = String(m.velocity);
  }

  function refresh(session) {
    els.pads.forEach((pad) => {
      pad.disabled = !session.connected;
    });
    els.start.disabled = !session.connected || els.running;
    els.stop.disabled = !els.running;
    els.channel.textContent = S.channelChip(session.channel);
    if (!session.connected) els.message = "";
    showStatus(session);
  }

  /* One sentence from the whole state, so a connection after the page was built updates it too. */
  function showStatus(session) {
    els.status.textContent = !session.connected
      ? S.notConnected
      : els.running
        ? S.running(els.bpmRunning)
        : els.message || S.stopped;
  }

  function onClock(m) {
    els.running = m.running;
    els.bpmRunning = m.bpm;
    els.message = m.message || "";
    if (m.running) kit.setTempo(m.bpm, "clock");
    refresh(kit.session);
  }

  function buildTransport() {
    els.bpm = el("input", {
      type: "number",
      id: "bpm",
      min: "20",
      max: "300",
      value: "120",
      "aria-label": S.bpm,
    });
    els.bpm.addEventListener("input", () => {
      if (els.bpm.validity.valid) kit.setTempo(Number(els.bpm.value), "play");
    });
    kit.onTempo((bpm, source) => {
      if (source !== "play") els.bpm.value = bpm;
      if (els.running) bridge.sendThrottled("bpm", 100, "clock_bpm", bpm);
    });
    els.velocity = el("input", {
      type: "number",
      id: "velocity",
      min: "1",
      max: "127",
      value: "100",
      "aria-label": S.velocity,
    });
    els.velocity.addEventListener("input", () => {
      if (els.velocity.validity.valid)
        bridge.sendThrottled(
          "seqvel",
          100,
          "seq_run",
          els.seqOn ? 1 : 0,
          velocity(),
        );
    });
    els.seq = button(
      "dice",
      S.seqOff,
      () => bridge.send("seq_run", els.seqOn ? 0 : 1, velocity()),
      { pressed: false, text: "Seq" },
    );
    els.start = button(
      "play",
      S.start,
      () => bridge.send("clock_start", els.bpm.value),
      {
        variant: "primary",
      },
    );
    els.stop = button("stop", S.stop, () => bridge.send("clock_stop"));
    els.status = el(
      "span",
      { class: "statusline", role: "status" },
      S.notConnected,
    );
    els.channel = el("span", { class: "chip" });
    return el(
      "div",
      { class: "strip" },
      el("label", { class: "field", for: "bpm" }, "BPM"),
      els.bpm,
      els.start,
      els.stop,
      els.status,
      info(S.clockLead, S.clockTitle),
      el("span", { class: "sep" }),
      el("label", { class: "field", for: "velocity" }, "Vel"),
      els.velocity,
      els.channel,
      info(S.padsLead, S.padsTitle),
      el("span", { class: "sep" }),
      els.seq,
      info(S.seqLead, S.seqTitle),
    );
  }

  function build(root) {
    for (let k = 0; k < PADS; k++) els.pads.push(buildPad(k));
    padNotes.onChange(() =>
      els.pads.forEach((pad, k) => pad.setAttribute("aria-label", padLabel(k))),
    );
    root.append(
      buildTransport(),
      el(
        "div",
        { class: "playpads" },
        els.pads.map((_, k) => buildCell(k)),
      ),
    );
    kit.onSession(refresh);
    refresh(kit.session);
    bridge.on("clock", onClock);
    bridge.on("seq_run", onSeqRun);
    bridge.on("seq_pad", (m) => showPattern(m.pad, m));
    bridge.send("seq_state");
    bridge.on("error", (m) => {
      if (["pad", "clock_start", "clock_bpm", "seq_pad", "seq_run"].includes(
          m.during,
        ))
        toast(errorText(m));
    });
  }

  kit.registerPage({ id: "play", title: S.title, fit: true, build });
})();
