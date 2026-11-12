/* Machine page: one settings strip (MIDI channel, pad notes, identity check, data folder), the pad note table over
   the page when opened (pad-notes-panel.js), the identity answer in full
   (identity-panel.js) and the MIDI monitor (monitor-ui.js) taking the rest of the window. The MIDI port and the Connect button live in the top bar (conn-bar.js). */
"use strict";
(function () {
  const { el, toast, errorText } = window.Tempester.kit;
  const { button, info } = window.Tempester.icons;
  const bridge = window.Tempester.bridge;
  const kit = window.Tempester.kit;
  const identity = window.Tempester.identity;

  const S = {
    title: "Machine",
    channelTitle: "MIDI channel",
    channelLead:
      "Channel for the on-screen knobs (NRPN), pads and clock. On the Tempest: System > Remote Pad IN Channel (1-16, or All).",
    channelLabel: "Channel",
    identityButton: "Check the machine identity",
    identityTitle: "Identity",
    identityIdle: "Identity: not checked",
    folderTitle: "Data folder",
    folderLead:
      "Sounds, beats, backups and OS files are saved here, and nowhere else.",
    folderButton: "Show the data folder",
  };

  const els = {};

  function refresh(session) {
    els.identity.disabled = !session.connected;
    els.channel.value = String(session.channel);
    if (!session.connected) showLine(S.identityIdle, "");
  }

  function showLine(text, tone) {
    els.identityLine.textContent = text;
    els.identityLine.setAttribute("class", "statusline " + tone);
  }

  /* The Main OS last sent from the OS update page, and whether a read-back from the machine proved it. */
  function showInstalled(view) {
    const line = window.Tempester.osVerify.installedLine(
      view.record,
      view.running,
      view.verified,
    );
    els.installed.textContent = line.text;
    els.installed.setAttribute("class", "statusline " + line.tone);
    els.installed.setAttribute("title", line.title);
  }

  function buildStrip(padNotes) {
    els.channel = el(
      "select",
      {
        id: "channel",
        "aria-label": S.channelLabel,
        onchange: () => bridge.send("set_channel", els.channel.value),
      },
      Array.from({ length: 16 }, (_, k) =>
        el("option", { value: String(k + 1) }, String(k + 1)),
      ),
    );
    els.identity = button("identity", S.identityButton, () => {
      bridge.send("identity");
      bridge.send("tempester_version");
    });
    els.identityLine = el("span", { class: "statusline", role: "status" });
    els.installed = el("span", { class: "statusline", role: "status" });
    window.Tempester.osVerify.watch(showInstalled);
    els.folder = el("span", { class: "statusline mono", title: "" });
    return el(
      "div",
      { class: "strip" },
      el("label", { class: "field", for: "channel" }, S.channelLabel),
      els.channel,
      info(S.channelLead, S.channelTitle),
      padNotes.button,
      el("span", { class: "sep" }),
      els.identity,
      els.identityLine,
      info(identity.lead, S.identityTitle),
      el("span", { class: "sep" }),
      els.installed,
      el("span", { class: "sep" }),
      button("reveal", S.folderButton, () => bridge.send("reveal", "data")),
      els.folder,
      info(S.folderLead, S.folderTitle),
    );
  }

  function build(root) {
    const padNotes = window.Tempester.padNotesPanel.build();
    root.append(
      buildStrip(padNotes),
      padNotes.panel,
      window.Tempester.identityPanel.build(),
      window.Tempester.monitor.build(),
    );
    kit.onSession(refresh);
    refresh(kit.session);
    bridge.on("identity", (m) => showLine(identity.short(m), "good"));
    bridge.on("state", (m) => {
      els.folder.textContent = m.data_dir || "";
      els.folder.setAttribute("title", m.data_dir || "");
    });
    bridge.on("error", (m) => {
      if (m.during !== "identity") return;
      if (m.code === "timeout")
        showLine("Identity: no answer", "warn");
      else toast(errorText(m));
    });
  }

  kit.registerPage({
    id: "machine",
    title: S.title,
    fit: true,
    build,
    onShow: () => {
      bridge.send("ports");
      window.Tempester.monitor.onShow();
    },
  });
})();
