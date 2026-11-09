/* Library page, one screen: a compact strip with the current sound, the current beat and the whole project (status
   as one short line, actions as icon buttons, long explanations in info badges), and under it the library browser
   (lib-browser.js): every saved sound, beat and project with search, filters, sort, multi-select and a details
   panel. Sending to the machine always goes through a look at what the machine holds (lib-flow.js). */
"use strict";
(function () {
  const { el, toast, errorText, formatBytes } = window.Tempester.kit;
  const bridge = window.Tempester.bridge;
  const kit = window.Tempester.kit;
  const icons = window.Tempester.icons;
  const project = window.Tempester.project;
  const flow = window.Tempester.libflow;

  const S = {
    title: "Library",
    lead: "The sound and the beat on the machine, the whole project, and every file you saved. Fetching needs a Tempester OS (1.5.2.0 or later). Everything that writes or replaces says what, where, older or newer, and how to undo.",
    unnamed: "(unnamed)",
    nameLabel: "File name",
  };

  const SOUND = {
    kind: "sound",
    list: "sounds",
    title: "Current sound",
    lead: "The sound of the pad selected on the machine. Fetch it, name it and save it to the library; or open a .syx file here and send it to the machine (the machine's own sound is looked at and backed up first when it is not in the library).",
    nothing: "No sound fetched yet.",
    fetchLabel: "Fetch the current sound from the machine",
    sendLabel: "Send the opened file to the machine...",
    commands: { fetch: "fetch_sound", save: "save_sound", open: "open_sound" },
    events: { fetched: "sound", opened: "sound_opened" },
    fetchedInfo: (m) => 'Fetched: "' + (m.name || S.unnamed) + '"',
    openedInfo: (m) =>
      m.file +
      ': sound "' +
      (m.name || S.unnamed) +
      '", ' +
      formatBytes(m.size) +
      ".",
  };

  const BEAT_STRINGS = {
    assignTitle: "Voice Assign by pad (0 = none, 1-6 = voice)",
    progress: (m) => "Sending " + Math.round((m.sent * 100) / m.total) + " %",
    summary: (m) =>
      '"' +
      (m.name || S.unnamed) +
      '": ' +
      m.events +
      " events, " +
      m.assigned +
      " of " +
      m.pads +
      " pads with a Voice Assign, " +
      formatBytes(m.size) +
      ".",
  };

  function assignStrip(values) {
    return el(
      "div",
      {
        class: "assign strip",
        role: "img",
        "aria-label": BEAT_STRINGS.assignTitle,
      },
      values.map((voice, pad) =>
        el("span", {
          class: voice ? "on" : "",
          title: "Pad " + (pad + 1) + ": voice " + voice,
        }),
      ),
    );
  }

  const BEAT = {
    kind: "beat",
    list: "beats",
    title: "Current beat",
    lead: "The beat loaded on the machine, with its events and its 32 sounds. Fetch it and save it to the library; or open a .syx file here and restore it to the machine (the machine's own beat is looked at and backed up first when it is not in the library).",
    nothing: "No beat fetched yet.",
    fetchLabel: "Fetch the current beat from the machine",
    sendLabel: "Restore the opened file to the machine...",
    hasProgress: true,
    exportLabel: "Export the fetched beat as a MIDI file (one track per pad)",
    commands: { fetch: "fetch_beat", save: "save_beat", open: "open_beat" },
    events: { fetched: "beat", opened: "beat_opened" },
    fetchedInfo: (m) => BEAT_STRINGS.summary(m),
    openedInfo: (m) => m.file + ": " + BEAT_STRINGS.summary(m),
  };

  /* A button that opens the file picker and hands the picked file's {name, hex} to `onFile`. */
  function filePickerButton(label, onFile) {
    const input = el("input", {
      type: "file",
      accept: ".syx",
      class: "hidden-input",
      tabindex: "-1",
    });
    input.addEventListener("change", async () => {
      const file = input.files[0];
      input.value = "";
      if (file) onFile(await bridge.readFileHex(file));
    });
    return el(
      "span",
      {},
      icons.button("open", label, () => input.click()),
      input,
    );
  }

  /* One compact card for the current sound or beat; `cfg` is SOUND or BEAT. */
  function currentCard(cfg) {
    const els = {};
    let fetched = false;
    let opened = false;

    function refreshButtons(session) {
      els.fetch.disabled = !session.connected;
      els.save.disabled = !fetched;
      if (els.export) els.export.disabled = !fetched;
      els.send.disabled = !opened || !session.connected;
    }

    els.status = el(
      "p",
      { class: "lib-card-status", role: "status" },
      cfg.nothing,
    );
    els.name = el("input", {
      type: "text",
      id: cfg.kind + "-name",
      maxlength: "80",
      "aria-label": S.nameLabel,
      placeholder: S.nameLabel,
    });
    els.fetch = icons.button(
      "fetch",
      cfg.fetchLabel,
      () => bridge.send(cfg.commands.fetch),
      { variant: "primary" },
    );
    els.save = icons.button(
      "save",
      "Save to the library (checks for duplicates first)",
      () => bridge.send(cfg.commands.save, els.name.value),
    );
    els.export = cfg.exportLabel
      ? icons.button("export", cfg.exportLabel, () =>
          bridge.send("lib_export_midi", ""),
        )
      : null;
    els.send = icons.button(
      "send",
      cfg.sendLabel,
      () => flow.sendCheck(cfg.list, ""),
      { variant: "danger" },
    );
    els.progress = el("span", { class: "chip", hidden: true });
    els.assign = el("div", { class: "lib-assign" });

    const node = el(
      "div",
      { class: "card lib-card compact" },
      el(
        "div",
        { class: "lib-card-head" },
        icons.icon(cfg.kind === "sound" ? "sound" : "beat"),
        el("h3", {}, cfg.title),
        cfg.hasProgress ? els.assign : null,
        icons.info(cfg.lead, "About the " + cfg.kind),
      ),
      els.status,
      el(
        "div",
        { class: "toolbar" },
        els.fetch,
        els.name,
        els.save,
        els.export,
        filePickerButton("Open a .syx file here", (file) =>
          bridge.send(cfg.commands.open, file.name, file.hex),
        ),
        els.send,
        els.progress,
      ),
    );

    /* One line, cut with an ellipsis, that tells the latest of "fetched" and "opened"; the tooltip tells both. */
    const told = { fetched: "", opened: "" };
    function say(which, text) {
      told[which] = text;
      els.status.textContent = text;
      els.status.setAttribute(
        "title",
        [told.fetched, told.opened].filter(Boolean).join("\n"),
      );
    }

    bridge.on(cfg.events.fetched, (m) => {
      fetched = true;
      els.name.value = m.name || "";
      say("fetched", cfg.fetchedInfo(m));
      if (cfg.hasProgress)
        els.assign.replaceChildren(assignStrip(m.voice_assign));
      refreshButtons(kit.session);
    });
    bridge.on(cfg.events.opened, (m) => {
      opened = true;
      say("opened", cfg.openedInfo(m));
      refreshButtons(kit.session);
    });
    bridge.on("sent", (m) => {
      if (m.kind !== cfg.kind) return;
      els.progress.hidden = true;
    });
    bridge.on("progress", (m) => {
      if (m.kind !== cfg.kind) return;
      els.progress.hidden = false;
      els.progress.textContent = BEAT_STRINGS.progress(m);
    });
    kit.onSession(refreshButtons);
    refreshButtons(kit.session);
    return node;
  }

  function build(root) {
    const browser = window.Tempester.libbrowser.create();
    root.append(
      el(
        "div",
        { class: "lib-title-row" },
        el("h2", {}, S.title),
        icons.info(S.lead, "About the library"),
      ),
      el(
        "div",
        { class: "lib-top" },
        currentCard(SOUND),
        currentCard(BEAT),
        project.card(),
      ),
      browser.build(),
    );
    const during = [SOUND, BEAT].flatMap((cfg) => Object.values(cfg.commands));
    during.push("open_library");
    bridge.on("error", (m) => {
      if (during.includes(m.during)) toast(errorText(m));
    });
    window.Tempester.libpage = { browser };
  }

  kit.registerPage({
    id: "library",
    title: S.title,
    build,
    onShow: () => window.Tempester.libpage.browser.requestAll(),
  });
})();
