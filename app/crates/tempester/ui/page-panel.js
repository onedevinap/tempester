/* Panel page: the machine's panel wired to the machine, in two views. Full: every sound parameter, one knob each (a
   knob moved on screen sends an NRPN, throttled, last value wins; a knob turned on the machine arrives as an NRPN and
   moves the knob here). Machine: only what the physical Tempest has, where it has it, acting like the physical
   controls through the remote panel of Tempester 1.6.0.0 (keys, LEDs, encoders, absolute knobs). "Sync" fetches the
   current sound and looks again for the remote panel. The chosen view and zoom are kept by Rust (settings file). */
"use strict";
(function () {
  const { el, toast, errorText } = window.Tempester.kit;
  const bridge = window.Tempester.bridge;
  const kit = window.Tempester.kit;
  const { panel, panelMachine, panelMode, panelView, panelLink, panelPerform } =
    window.Tempester;

  const S = {
    title: "Panel",
    sync: "Sync from machine",
    syncing: "Asking the machine...",
    synced: (name) => "Synced: " + (name || "(unnamed sound)"),
    infoButton: "Info",
    infoTitle: "About the Panel page",
    close: "Close",
    info: [
      "Full view: every sound parameter, one knob each. The parameters are NRPN, which need a Tempester Main 1.5.3.0 or later (the original DSI OS ignores them). They always change the pad selected on the machine. Distort and Compress go through the remote panel when it answers (the machine's own 0-255 knob), else they are MIDI CC 12 and 13 (up to 127), the 16 pads play notes, and Play and Stop send MIDI Start and Stop for a Tempest set to slave its clock to MIDI. The BPM knob sets the tempo of this app's clock (Play page), used when the app is the clock master.",
      "Machine view: only what the physical Tempest has, at the same place. Absolute knobs, encoders (endless, shown without a position) and keys act like the machine's own, so the machine decides what each one does: in 16 Beats and 16 Mutes the sound knobs are beat-wide, as on the machine, and the controls the machine ignores there are greyed. An encoder shows its turning (a dot, the ring it crossed, a count); the parameter it changed, with its value, appears under it when the machine reports it. It needs Tempester 1.6.0.0 with Remote Pad IN set to the channel of this app and Remote Pad OUT not Off; without it the controls stay greyed.",
      "Sync works from Tempester 1.5.2.0. With Tempester 1.6.0.0 the machine's LEDs light the keys and pads here, and Volume sets the machine's Volume.",
      "Tempester 1.6.2.0: in 16 Beats and 16 Mutes the Machine view's beat-wide knobs show and set the beat-wide positions (127 = neutral). Tempester 1.6.3.0: the FX ribbons of both views touch the machine's ribbons and show its own; the dice button shows each pad's play probability (read by Sync) and lets you change it in 16 Sounds. A control the installed OS lacks is greyed and its tooltip names the version that brings it.",
      "Tempester 1.6.4.0: the machine reports what its own knobs and encoders change (Level, Pan, Delay Repeats, Volume, Distort, Compress, and Delay Send, Delay Time, BPM, Swing) and sends them all again when the app asks for its state (Sync). The Machine view moves the matching knobs, and shows an encoder's number in its bubble for a moment; the Full view follows Volume, Distort and Compress. A knob you are turning is not moved by the machine.",
      "Zoom: the - and + buttons, or Cmd/Ctrl with - and +; Cmd/Ctrl 0 fits the width of the window. A zoomed panel scrolls inside its frame.",
    ],
    remoteLooking: "Remote panel: looking...",
    remoteOn: "Remote panel: on",
    remoteOff: "Remote panel: not detected",
    notConnected: "Not connected",
    views: { full: "Full view", machine: "Machine view" },
    viewLabel: "Panel view",
    zoomLabel: "Zoom",
    zoomOut: "Zoom out (Cmd/Ctrl -)",
    zoomIn: "Zoom in (Cmd/Ctrl +)",
    zoomFit: "Fit the width of the window (Cmd/Ctrl 0)",
    mode: (name) => "Pad Function: " + name,
    probability: "Play probability of each pad",
    probabilityOff: "Back to playing the pads",
    beatWide:
      "16 Beats and 16 Mutes: these knobs change the selected pad only. The machine's own sound knobs are beat-wide in this mode (every sound of the beat): use the Machine view to turn them like the machine (with Tempester 1.6.2.0 it shows their positions).",
  };
  const BEAT_WIDE_MODES = panelMachine.BEAT_WIDE_MODES;

  let status = null,
    remoteStatus = null,
    syncButton = null;

  function build(root) {
    const params = window.TEMPEST_PARAMS.map(([index, max, def]) => ({
      index,
      max,
      default: def,
    }));
    const maxOf = Object.fromEntries(params.map((p) => [p.index, p.max]));
    const link = panelLink.create({
      bridge,
      connected: () => kit.session.connected,
      setTempo: kit.setTempo,
      remote: () => remoteOn,
    });
    /* The machine reports back what the app sends (n°38): a Full view knob the user holds or just moved ignores the
       report, by the same rule as the Machine view's. */
    const reportGuard = panelMachine.createReportGuard();

    const icons = window.Tempester.icons;
    syncButton = icons.button("fetch", S.sync, sync, { variant: "primary" });
    status = el("span", { class: "statusline slot-status", role: "status" });
    say(status, S.notConnected);
    remoteStatus = el("span", { class: "chip slot-remote", role: "status" });
    const modeChip = el("span", {
      class: "chip slot-mode",
      role: "status",
      hidden: true,
    });
    const info = infoDialog();
    const infoButton = icons.button("info", S.infoButton, () =>
      info.showModal(),
    );
    infoButton.setAttribute("aria-haspopup", "dialog");
    const viewButtons = {};
    const viewSwitch = el(
      "div",
      { class: "seg", role: "group", "aria-label": S.viewLabel },
      Object.entries(S.views).map(([name, label]) => {
        viewButtons[name] = icons.button(
          "view-" + name,
          label,
          () => view.setView(name),
          { pressed: false },
        );
        return viewButtons[name];
      }),
    );
    const zoomFit = el(
      "button",
      {
        class: "ibtn zoomfit",
        type: "button",
        "aria-label": S.zoomFit,
        "data-tip": S.zoomFit,
        onclick: () => view.zoomFit(),
      },
      "100%",
    );
    const probButton = icons.button(
      "dice",
      S.probability,
      () => setProbMode(!perform.probMode()),
      { pressed: false },
    );
    const zoomBox = el(
      "div",
      { class: "seg", role: "group", "aria-label": S.zoomLabel },
      icons.button("minus", S.zoomOut, () => view.zoomOut()),
      zoomFit,
      icons.button("plus", S.zoomIn, () => view.zoomIn()),
    );
    const notice = el("p", { class: "notice", role: "status", hidden: true });
    notice.textContent = S.beatWide;
    const fullHost = el("div", {});
    const machineHost = el("div", { hidden: true });
    const bar = el(
      "div",
      { class: "panelbar" },
      syncButton,
      status,
      remoteStatus,
      modeChip,
      viewSwitch,
      zoomBox,
      probButton,
      infoButton,
    );
    root.append(
      bar,
      fullHost,
      machineHost,
      notice,
      info,
    );

    panel.mount(fullHost, params, link.knob, {
      onPad: link.pad,
      onLocal: (name, value) => {
        reportGuard.touch(name);
        link.local(name, value);
      },
      onHold: (name, down) =>
        down ? reportGuard.hold(name) : reportGuard.release(name),
      onTransport: link.transport,
      onKey: link.key,
      onMessage: toast,
    });
    panel.setLocal("bpm", kit.tempo.bpm);
    kit.onTempo((bpm, source) => {
      if (source !== "panel") panel.setLocal("bpm", bpm);
    });
    const machine = panelMachine.mount(machineHost, {
      link,
      onKey: link.key,
      onPad: link.pad,
      onTransport: link.transport,
      remote: window.Tempester.panelRemote,
      panel: panel.parts,
      paramMax: (index) => maxOf[index],
      paramName: (index) => window.Tempester.panelLabels.NAMES[index],
    });
    /* The Full view's machine-only controls are the Machine view's own knobs (panel.js `fvslot`): one state for both. */
    fullHost.querySelectorAll(".fvslot").forEach((s) =>
      s.prepend(machine.knobFor(s.dataset.kind, +s.dataset.n)),
    );
    /* The editing tools (edit-tools.js): one slot of the bar and its popover; what they change is drawn on both views,
       and a learned controller moves a control through `link`, like a hand. */
    const tools = window.Tempester.editTools.create({
      bridge,
      names: window.Tempester.panelLabels.NAMES,
      learnHost: fullHost,
      shown: () => root.classList.contains("shown"),
      onValues: (values) => {
        panel.setValues(values);
        machine.setValues(values);
      },
      drive: (id, value) =>
        window.Tempester.editLearn.drive(id, value, { panel, machine, link }),
    });
    bar.insertBefore(tools.slot, infoButton);
    root.append(tools.popover);

    const features = panelPerform.createFeatures();
    fetchBeatToo = () => features.status("probability").on;
    const perform = panelPerform.attach({
      hosts: [fullHost, machineHost],
      link,
      remote: window.Tempester.panelRemote,
    });
    let remoteOn = false;
    function setProbMode(on) {
      perform.setProbMode(on);
      probButton.setAttribute("aria-pressed", String(on));
      icons.relabel(probButton, on ? S.probabilityOff : S.probability);
    }
    /* What the installed OS and the connection allow, after any change of either. */
    function refreshFeatures() {
      const connected = kit.session.connected;
      const ribbons = features.status("ribbons");
      perform.setRibbonsUsable(
        connected && ribbons.on,
        connected ? ribbons.why : S.notConnected,
      );
      machine.setBeatWide(connected && features.status("beat_wide").on);
      const prob = features.status("probability");
      const usable = connected && prob.on;
      probButton.disabled = !usable;
      if (!usable) {
        setProbMode(false);
        icons.relabel(probButton, connected ? prob.why : S.notConnected);
      } else if (!perform.probMode()) icons.relabel(probButton, S.probability);
      const sounds = remoteOn && mode.mode() === 2;
      perform.setPadsEditable(usable && sounds, panelPerform.S.padsMode);
    }
    function showNotice() {
      const wide = BEAT_WIDE_MODES.includes(mode.mode());
      notice.hidden = !(view.view() === "full" && wide);
    }
    const mode = panelMode.create({
      onMode: (index) => {
        const name = index === null ? "" : panelMode.NAMES[index];
        modeChip.textContent = name;
        modeChip.setAttribute("data-tip", name ? S.mode(name) : "");
        modeChip.hidden = !name;
        machine.setScreen({ mode: name });
        machine.setMode(index);
        showNotice();
        refreshFeatures();
      },
    });
    const kept = { view: "full", zoom: panelView.ZOOM_FIT };
    const view = panelView.create({
      releaseKeys: () => {
        panel.releaseKeys();
        perform.releaseRibbons();
      },
      onView: (name) => {
        fullHost.hidden = name !== "full";
        machineHost.hidden = name !== "machine";
        for (const [id, button] of Object.entries(viewButtons))
          button.setAttribute("aria-pressed", String(id === name));
        tools.setView(name);
        showNotice();
        refit();
      },
      onZoom: (percent) => {
        zoomFit.textContent = percent + "%";
        refit();
      },
      /* Only what changed goes to Rust: a zoom click does not rewrite the view. */
      save: (name, percent) => {
        if (name !== kept.view) bridge.send("panel_view", name);
        if (percent !== kept.zoom) bridge.send("panel_zoom", percent);
        Object.assign(kept, { view: name, zoom: percent });
      },
    });
    function refit() {
      panel.refit();
      machine.refit();
    }
    view.restore("full", panelView.ZOOM_FIT);
    document.addEventListener("keydown", (e) => {
      if (!root.classList.contains("shown")) return;
      const action = panelView.zoomShortcut(e);
      if (!action) return;
      e.preventDefault();
      if (action === "in") view.zoomIn();
      else if (action === "out") view.zoomOut();
      else view.zoomFit();
    });

    const setRemote = (on) => {
      remoteOn = on;
      panel.setRemote(on);
      machine.setRemote(on);
      if (!on) {
        mode.reset();
        machine.setShift(false);
      }
      refreshFeatures();
    };
    let wasConnected = false;
    kit.onSession((session) => {
      syncButton.disabled = !session.connected;
      tools.setConnected(session.connected);
      if (!session.connected) {
        say(status, S.notConnected);
        sayRemote("");
        setRemote(false);
      } else if (!wasConnected) probeRemote();
      wasConnected = session.connected;
      features.setFake(session.fake);
      if (!session.connected) perform.probabilities.reset();
      refreshFeatures();
    });
    syncButton.disabled = !kit.session.connected;
    bridge.on("state", (m) => {
      if (m.features) {
        features.set(m.features);
        refreshFeatures();
      }
      if (m.panel_full === undefined) return;
      view.restore(m.panel_full ? "full" : "machine", m.panel_zoom);
      Object.assign(kept, { view: view.view(), zoom: view.zoom() });
    });
    bridge.on("nrpn", (m) => {
      panel.setValue(m.number, m.value);
      machine.setParam(m.number, m.value);
    });
    bridge.on("installed", (m) => {
      if (!m.features) return;
      features.set(m.features);
      refreshFeatures();
    });
    bridge.on("identity", (m) => {
      features.setIdentity(!!m.tempester);
      refreshFeatures();
    });
    bridge.on("mixer", (m) => {
      machine.reportMixer(m.item, m.value);
      panelMachine.followInFullView(reportGuard, panel.setLocal, m.item, m.value);
    });
    bridge.on("beat_wide", (m) => machine.reportBeatWide(m.index, m.position));
    bridge.on("ribbon", (m) => perform.reportRibbon(m.ribbon, m.position));
    bridge.on("beat", (m) => {
      perform.probabilities.fromBeat(m.play_probability);
      perform.showPads();
    });
    bridge.on("pad_probability", (m) => {
      perform.probabilities.report(m.pad, m.percent);
      perform.showPads();
    });
    bridge.on("remote", (m) => {
      setRemote(m.available);
      sayRemote(m.available ? S.remoteOn : S.remoteOff);
    });
    bridge.on("leds", (m) => {
      const shift = m.changes.filter(([i]) => i === panelMachine.SHIFT_LED);
      if (shift.length) machine.setShift(shift[shift.length - 1][1] === 1);
      panel.setLeds(m.changes);
      mode.feed(m.changes);
    });
    bridge.on("pad_mode", (m) => mode.announce(m.mode));
    bridge.on("sound", (message) => {
      panel.setValues(message.values);
      panel.setSoundName(message.name);
      machine.setValues(message.values);
      machine.setScreen({ name: message.name });
      say(status, S.synced(message.name));
    });
    bridge.on("error", (m) => {
      if (m.during === "fetch_sound") {
        say(status, errorText(m));
      } else if (
        [
          "cc",
          "transport",
          "key",
          "control",
          "encoder",
          "panel_state",
          "beat_wide",
          "ribbon",
          "pad_probability",
        ].includes(m.during)
      ) {
        toast(errorText(m));
      }
    });
  }

  /* The explanations that used to fill the top of the page. A modal <dialog>: Escape closes it, the focus comes
     back to the button. */
  function infoDialog() {
    const dialog = el(
      "dialog",
      { class: "panel-info", "aria-labelledby": "panel-info-title" },
      el("h2", { id: "panel-info-title" }, S.infoTitle),
      S.info.map((text) => el("p", {}, text)),
      el(
        "div",
        { class: "buttons" },
        el("button", { class: "btn primary", type: "button" }, S.close),
      ),
    );
    dialog
      .querySelector("button")
      .addEventListener("click", () => dialog.close());
    return dialog;
  }

  /* A readout of the bar has a fixed slot: its text is cut with an ellipsis, so the whole text is also its tooltip. */
  function say(node, text) {
    node.textContent = text;
    node.setAttribute("data-tip", text);
  }
  /* The remote readout is a short chip ("Remote on"); the long sentence is its tooltip. */
  const REMOTE_SHORT = {
    [S.remoteLooking]: ["Remote...", ""],
    [S.remoteOn]: ["Remote on", "ok"],
    [S.remoteOff]: ["No remote", "warn"],
  };
  function sayRemote(text) {
    const [short, tone] = REMOTE_SHORT[text] || ["", ""];
    remoteStatus.textContent = short;
    remoteStatus.className = "chip slot-remote" + (tone ? " " + tone : "");
    remoteStatus.setAttribute("data-tip", text);
  }

  /* The full LED state, pads included: the machine answers only with Tempester 1.6.0.0 and Remote Pad OUT on (with
     n°40, its Pad Function mode too). And the exact version (n°39): the features then follow it, not the install
     record; a machine without n°39 does not answer. */
  function probeRemote() {
    sayRemote(S.remoteLooking);
    bridge.send("panel_state", 1);
    bridge.send("tempester_version");
  }

  /* The beat holds the pads' play probabilities: fetched too when the installed OS has them. */
  let fetchBeatToo = () => false;
  function sync() {
    say(status, S.syncing);
    bridge.send("fetch_sound");
    if (fetchBeatToo()) bridge.send("fetch_beat");
    probeRemote();
  }

  kit.registerPage({ id: "panel", title: S.title, build });
})();
