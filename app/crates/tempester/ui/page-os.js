/* OS update page: get a file ready (build a Tempester version from the user's DSI Main 1.5.0.2 file, or validate any
   .syx, or go back to DSI 1.5.0.2), show what its SHA-256 says (equal to the reference, a known version, or an
   unknown build that needs an extra confirmation), confirm, send at the cable rate with progress and a cancel, then
   re-check the identity after the power cycle and read the Main OS back from the machine (os-verify.js). One screen: the four steps are numbered panels, their explanations sit behind
   info badges, the risky actions keep a text and their full confirmation. */
"use strict";
(function () {
  const { el, toast, errorText, confirmDialog, formatBytes, formatDuration } =
    window.Tempester.kit;
  const { button, info } = window.Tempester.icons;
  const bridge = window.Tempester.bridge;
  const kit = window.Tempester.kit;

  const S = {
    title: "OS update",
    lead: "Install a Tempester version, or go back to the original DSI software. The file is checked before anything is sent, and nothing is sent without your confirmation.",
    leadTitle: "About the OS update",
    dsiTitle: "Your DSI file",
    dsiLead:
      "Tempester never ships DSI software: it builds a Tempester version from your own Tempest_Main_1.5.0.2.syx (its SHA-256 is checked).",
    dsiPick: "Choose Tempest_Main_1.5.0.2.syx",
    dsiNone: "No file chosen.",
    dsiChosen: (name, size) => name + " (" + formatBytes(size) + ")",
    versionsTitle: "A Tempester version",
    versionsLead:
      "The recommended choices come first, the full history below. Building needs your DSI file (step 1); the built file is then checked against its recorded SHA-256 before anything can be sent.",
    validateLead:
      "Or check any OS file: id, block count and checksum of a .syx OS file (Main, Voice, Panel or SAM), then prepare it to send.",
    validatePick: "Choose an OS .syx file to check",
    readyTitle: "Ready to send",
    nothingReady: "No file is ready.",
    ready: (m) =>
      m.label +
      ": " +
      m.chip +
      " OS (id 0x" +
      m.id.toString(16) +
      "), " +
      m.blocks +
      " blocks, checksum 0x" +
      m.checksum.toString(16).padStart(4, "0") +
      " OK, " +
      formatBytes(m.size) +
      ", " +
      formatDuration(m.seconds) +
      " to send.",
    refused: (text) => "Refused: " + text,
    builtMatches: "Built image matches the reference ✓",
    fileKnown: (m) => "This file is " + m.label + " ✓",
    fileUnknown: "Unknown build: no known version matches",
    hashLabels: { image: "Image", syx: ".syx file", short: "SHA-256" },
    unknownTitle: "Send an unknown build?",
    unknownLines: (m) => [
      m.label +
        " matches no Tempester version known to this app (image SHA-256 " +
        m.sha256_image +
        "). The app cannot tell what it contains.",
      "Send it only if you made it yourself and know what it does.",
      "A faulty Main OS can leave the machine unable to start; it then needs a programmer (ICSP) to be repaired, because the MIDI update no longer works.",
    ],
    unknownOk: "Send an unknown build",
    send: "Send to the machine...",
    confirmTitle: (m) => "Send " + m.label + " to the Tempest?",
    confirmLines: (m) => [
      "This replaces the " +
        m.chip +
        " software of the machine with " +
        m.label +
        " (" +
        formatBytes(m.size) +
        ").",
      "It takes " +
        formatDuration(m.seconds) +
        ". Do not power off or unplug anything while it is sent.",
      "When the FX1 LEDs stop at the top, power the Tempest off and on.",
      "If you cancel half way, the update stays incomplete. The flasher page is never touched, so a new update is always possible.",
    ],
    confirmOk: "Send update",
    sendingIdle: "Nothing is being sent.",
    sending: (m) =>
      m.label + ": " + formatBytes(m.sent) + " of " + formatBytes(m.total),
    cancel: "Cancel the update",
    cancelTitle: "Cancel the update?",
    cancelLines: [
      "The machine received only part of the file: the update will be incomplete.",
      "The flasher page is never touched and a new update is always possible. Send the file again to finish.",
    ],
    cancelOk: "Cancel update",
    keepSending: "Keep sending",
    noResult: "No update sent yet.",
    done: "All bytes were sent. Now power the Tempest off and on (when the FX1 LEDs stop at the top), then check the identity below.",
    cancelled: (m) =>
      "Cancelled after " +
      formatBytes(m.sent) +
      " of " +
      formatBytes(m.total) +
      ". The update is incomplete: send the file again to finish.",
    failed: (m) =>
      "The send failed: " +
      m.message +
      ". The update is incomplete: reconnect and send the file again.",
    afterTitle: "After the update",
    afterLead:
      "Power-cycle the Tempest, reconnect with the top bar, then ask for its identity.",
    identity: "Check the machine identity",
    identityIdle: "Identity: not checked.",
    identityResult: (m) =>
      "Identity: version word 0x" +
      m.version.toString(16).toUpperCase() +
      (m.tempester ? " (Tempester OS)." : " (DSI original or unknown)."),
    closeBlocked:
      "An OS update is being sent: cancel it first, or wait until it ends, before closing.",
  };

  const els = {};
  let dsi = null,
    readyInfo = null,
    sending = false,
    versionsLoaded = false;

  function refresh(session) {
    els.versions.setReady(!!dsi);
    els.send.disabled = !readyInfo || !session.connected || sending;
    els.cancel.disabled = !sending;
    els.identity.disabled = !session.connected || sending;
  }

  function showVersions(list) {
    versionsLoaded = list.length > 0;
    els.versions.show(list);
  }

  /* What the SHA-256 of the ready file says: a built version equals its reference, a file is a known version or an
     unknown build. An event without `known` (an older Rust side) is treated as known. */
  function showEvidence(m) {
    const hashes = m.sha256_image
      ? [
          {
            label: S.hashLabels.image,
            shownAs: S.hashLabels.short,
            value: m.sha256_image,
          },
          ...(m.sha256_syx
            ? [
                {
                  label: S.hashLabels.syx,
                  value: m.sha256_syx,
                  fullOnly: true,
                },
              ]
            : []),
        ]
      : [];
    if (m.known === false)
      els.evidence.show({ tone: "warn", text: S.fileUnknown, hashes });
    else if (m.source === "built")
      els.evidence.show({ tone: "good", text: S.builtMatches, hashes });
    else els.evidence.show({ tone: "good", text: S.fileKnown(m), hashes });
  }

  function showReady(m) {
    readyInfo = m;
    els.ready.textContent = S.ready(m);
    els.ready.setAttribute("class", "statusbox");
    els.ready.removeAttribute("title");
    showEvidence(m);
    refresh(kit.session);
  }

  /* A refused file leaves nothing ready: the message replaces the "ready" text. */
  function showRefused(m) {
    readyInfo = null;
    const text = S.refused(m.message || errorText(m));
    els.ready.textContent = text.replace(
      /[0-9a-f]{64}/g,
      window.Tempester.osHash.short,
    );
    els.ready.setAttribute("title", text);
    els.ready.setAttribute("class", "statusbox bad");
    els.evidence.show({});
    refresh(kit.session);
  }

  async function confirmSend() {
    const unknown = readyInfo.known === false;
    if (unknown) {
      const sure = await confirmDialog({
        title: S.unknownTitle,
        lines: S.unknownLines(readyInfo),
        okLabel: S.unknownOk,
        danger: true,
      });
      if (!sure) return;
    }
    const ok = await confirmDialog({
      title: S.confirmTitle(readyInfo),
      lines: S.confirmLines(readyInfo),
      okLabel: S.confirmOk,
      danger: true,
    });
    if (unknown) {
      if (ok) bridge.send("os_send_unknown");
      return;
    }
    if (ok) bridge.send("os_send");
  }

  async function confirmCancel() {
    const ok = await confirmDialog({
      title: S.cancelTitle,
      lines: S.cancelLines,
      okLabel: S.cancelOk,
      danger: true,
    });
    if (ok) bridge.send("os_cancel");
  }

  function showResult(text, tone) {
    els.result.textContent = text;
    els.result.setAttribute("class", "statusbox" + (tone ? " " + tone : ""));
  }

  function onStarted(m) {
    sending = true;
    readyInfo = null;
    showResult(S.noResult, "");
    els.bar.style.width = "0%";
    els.sendingText.textContent = S.sending({
      label: m.label,
      sent: 0,
      total: m.total,
    });
    els.ready.textContent = S.nothingReady;
    els.evidence.show({});
    refresh(kit.session);
  }

  function onProgress(m) {
    els.bar.style.width = ((m.sent * 100) / m.total).toFixed(1) + "%";
    els.sendingText.textContent = S.sending({
      label: els.label || "",
      sent: m.sent,
      total: m.total,
    });
  }

  function onFinished(m) {
    sending = false;
    els.bar.style.width = ((m.sent * 100) / m.total).toFixed(1) + "%";
    showResult(
      m.state === "done"
        ? S.done
        : m.state === "cancelled"
          ? S.cancelled(m)
          : S.failed(m),
      m.state === "done" ? "good" : "bad",
    );
    refresh(kit.session);
  }

  function pickDsi(file) {
    dsi = file;
    els.dsiName.textContent = S.dsiChosen(file.name, file.hex.length / 2);
    els.dsiName.setAttribute("title", file.name);
    refresh(kit.session);
  }

  function head(number, title, lead) {
    return el(
      "div",
      { class: "panel-head" },
      number ? el("span", { class: "step-no" }, String(number)) : null,
      el("h3", {}, title),
      info(lead, title),
    );
  }

  function buildSourcePanels() {
    els.dsiName = el("span", { class: "statusline" }, S.dsiNone);
    els.versions = window.Tempester.osVersions.create({
      onBuild: (version) => bridge.send("os_build", version, dsi.hex),
      onOriginal: () => bridge.send("os_original", dsi.hex),
    });
    return [
      el(
        "section",
        { class: "panel fixed" },
        head(1, S.dsiTitle, S.dsiLead),
        el(
          "div",
          { class: "toolbar" },
          kit.filePicker(S.dsiPick, pickDsi, ".syx", "open"),
          els.dsiName,
        ),
        el(
          "div",
          { class: "toolbar" },
          kit.filePicker(
            S.validatePick,
            (file) => bridge.send("os_validate", file.name, file.hex),
            ".syx",
            "open",
          ),
          info(S.validateLead, S.validatePick),
        ),
      ),
      el(
        "section",
        { class: "panel versions-panel" },
        head(2, S.versionsTitle, S.versionsLead),
        el("div", { class: "vscroll" }, els.versions.node),
      ),
    ];
  }

  function buildSendPanels() {
    els.ready = el("p", { class: "statusbox", role: "status" }, S.nothingReady);
    els.evidence = window.Tempester.osHash.evidence();
    els.send = button("send", S.send, confirmSend, {
      variant: "danger",
      text: S.send,
    });
    els.bar = el("div");
    els.sendingText = el(
      "span",
      { class: "statusline", role: "status" },
      S.sendingIdle,
    );
    els.cancel = button("stop", S.cancel, confirmCancel, {
      variant: "danger",
      text: S.cancel,
    });
    els.result = el("p", { class: "statusbox", role: "status" }, S.noResult);
    els.identityResult = el(
      "span",
      { class: "statusline", role: "status" },
      S.identityIdle,
    );
    els.identity = button(
      "identity",
      S.identity,
      () => {
        bridge.send("identity");
        bridge.send("tempester_version");
      },
      {
        text: S.identity,
      },
    );
    return [
      el(
        "section",
        { class: "panel fixed" },
        head(3, S.readyTitle, S.lead),
        els.ready,
        els.evidence.node,
        el("div", { class: "toolbar" }, els.send, els.cancel),
        el("div", { class: "progress" }, els.bar),
        el("div", { class: "toolbar" }, els.sendingText),
        els.result,
      ),
      el(
        "section",
        { class: "panel fixed" },
        head(4, S.afterTitle, S.afterLead),
        el("div", { class: "toolbar" }, els.identity),
        el("div", { class: "toolbar" }, els.identityResult),
        window.Tempester.osVerify.create().node,
      ),
    ];
  }

  function build(root) {
    const [dsiPanel, versionsPanel] = buildSourcePanels();
    const [sendPanel, afterPanel] = buildSendPanels();
    root.append(
      el(
        "div",
        { class: "cols2 fill os-cols" },
        el("div", { class: "col" }, dsiPanel, versionsPanel),
        el("div", { class: "col" }, sendPanel, afterPanel),
      ),
    );
    kit.onSession(refresh);
    refresh(kit.session);
    bridge.on("os_versions", (m) => showVersions(m.versions));
    bridge.on("os_ready", showReady);
    bridge.on("os_started", (m) => {
      els.label = m.label;
      onStarted(m);
    });
    bridge.on("os_progress", onProgress);
    bridge.on("os_finished", onFinished);
    bridge.on("identity", (m) => {
      els.identityResult.textContent = S.identityResult(m);
    });
    bridge.on("close_blocked", () => toast(S.closeBlocked));
    bridge.on("error", (m) => {
      if (m.during === "os_verify") return;
      if (m.during && m.during.startsWith("os_")) toast(errorText(m));
      if (["os_build", "os_validate", "os_original"].includes(m.during))
        showRefused(m);
    });
  }

  kit.registerPage({
    id: "os",
    title: S.title,
    fit: true,
    build,
    onShow: () => {
      if (!versionsLoaded) bridge.send("os_versions");
    },
  });
})();
