/* The full machine backup on the Backup page (Tempester 1.6.6.0, n°62): « Request » asks the machine for its eight
   messages (six Voice chips, the Main boot flash, the System settings), each row shows what the checks made of it, and
   « Save » writes the dated folder, only when all eight passed (a calibration « warning » passes: the chip uses an
   unusually shaped page as it is). Under each Voice row whose message decoded, refused or not, its 35 calibration words,
   for the user to quote in a bug report. These are backups for a programmer (PICkit) restore: no control of this
   module sends a backup to the machine, and the page says so.

   Tempester.fullBackup.build() -> {section} */
"use strict";
window.Tempester = window.Tempester || {};
window.Tempester.fullBackup = (function () {
  const { el, formatBytes } = window.Tempester.kit;
  const { button, info } = window.Tempester.icons;
  const bridge = window.Tempester.bridge;
  const features = window.Tempester.panelPerform.createFeatures();

  const S = {
    title: "Full machine backup",
    never:
      "These files are backups for a programmer (PICkit) restore. This app never sends them back to the machine.",
    stepsTitle: "How the full backup works",
    steps: [
      "Stop the sequencer on the machine first: during the backup the machine shows nothing, its panel does not answer and it reads no MIDI input, so this app sends it nothing else until the end (about 2 min 35 s on a DIN cable, a few seconds on USB).",
      "Press Request. The machine sends eight messages, in this order: Voice 0 to 5, the Main boot flash and configuration bits, the System settings with the MIDI channel of each pad. Each is checked as it arrives: it decodes, it comes in its place, it is aligned, the code pages and the boot page are present, the calibration page is read where the chip keeps it (35 words).",
      "A calibration page whose values have a shape this app has not seen before is not refused: its row says « warning », it can be saved, and the report says so. The chip uses its page as it is.",
      "Optionally, once the eight have arrived, send the Debug-menu dumps (hold SHIFT and SYSTEM): Main, Panel and Voice 0 join the same folder.",
      "Save writes a dated folder: every message exactly as received, the System settings file and the pad channels taken from the last message, a manifest (versions, SHA-256, the calibration of each Voice) and a readable report. Nothing is saved unless all eight pass (a warning passes).",
      "If a Voice dump is refused as misaligned or not read, power the machine off and on, then request again.",
      "Before Tempester 1.6.8.0, only one full backup per power-on: a second one in the same power-on stops the voice chips (silent until the machine is switched off). The app refuses it until you say the machine was powered off and on. From 1.6.8.0 there is no limit.",
    ],
    onePerPowerOn:
      "One full backup per power-on on this OS (before Tempester 1.6.8.0): a second one stops the voice chips. Power the machine off and on before the next one, then press « I powered it off and on ».",
    cycled: "I powered the machine off and on: allow the next full backup",
    cycledText: "I powered it off and on",
    cycledDone: "Noted: the next full backup is allowed.",
    request: "Request the full backup from the machine",
    requestText: "Request full backup",
    cancel: "Cancel the full backup",
    cancelText: "Cancel",
    save: "Save the full backup in a dated folder",
    saveText: "Save full backup",
    status: {
      idle: "No full backup in progress.",
      waiting: "Request sent. Waiting for the machine (stop the sequencer; this can take a while)...",
      receiving: "Receiving the machine's messages...",
      ready: "All eight messages passed every check. You can save.",
      refused: "Not saved: at least one message is missing or was refused. Nothing is written until all eight pass.",
    },
    readyWarned: (n) =>
      "You can save. " + n + " calibration page(s) with a warning: saved exactly as read, see the rows.",
    verdicts: { ok: "OK", warning: "warning", refused: "refused", missing: "missing", waiting: "waiting" },
    badges: { ok: "ok", warning: "constant", waiting: "" },
    cols: { chip: "Chip", size: "Size", verdict: "Check", detail: "Detail" },
    calibrationNote:
      "Below each Voice row: its 35 calibration words (A oscillator 1, B oscillator 2, C lowpass, then three scalars). They are your machine's own measurements: you may copy them into a bug report. Never post the Voice .syx files: they hold DSI's Voice OS.",
    calibration: (chip, c) =>
      "Calibration of " + chip + ": A " + c.a.join(" ") + " | B " + c.b.join(" ") +
      " | C " + c.c.join(" ") + " | scalars " + c.scalars.join(" "),
    extras: "Also heard (Debug menu), saved in the same folder:",
    savedIn: (dir) => "Saved in " + dir,
    needs: (since) =>
      "The full backup needs Tempester " + (since || "1.6.6.0") + " or later",
    unknown:
      " (the OS of the machine is unknown to the app: connect and check the identity)",
    dsi: " (the machine answered as the DSI OS)",
    notConnected: "Not connected",
  };

  const els = {};
  let state = "idle";
  let warned = 0;

  /* The gate: the same rule as the other features (panel-perform.js), plus a connection. */
  function gate(session) {
    if (!session.connected) return { on: false, why: S.notConnected };
    const { on, why } = features.status("full_backup");
    return { on, why: on ? "" : why || S.needs() };
  }

  function refresh(session) {
    const { on, why } = gate(session);
    const running = state === "waiting" || state === "receiving";
    els.request.disabled = !on || running;
    els.cancel.disabled = state === "idle";
    els.save.disabled = state !== "ready";
    els.request.setAttribute("data-tip", on ? S.request : why);
    els.unavailable.textContent = on ? "" : why;
    /* Before 1.6.8.0 (no `full_backup_repeat`), one backup per power-on: said before any request. */
    const limited = on && !features.status("full_backup_repeat").on;
    els.limit.hidden = !limited;
    els.cycled.hidden = !limited;
    const warn = state === "ready" && warned > 0;
    els.status.textContent = warn ? S.readyWarned(warned) : S.status[state] || S.status.idle;
    els.status.setAttribute(
      "class",
      "statusline" + (warn ? " warn" : state === "ready" ? " good" : state === "refused" ? " bad" : ""),
    );
  }

  /* A chip's row, then, for a Voice chip whose message decoded (accepted, warned or refused), its calibration words. */
  function rowLines(r) {
    const line = el(
      "tr",
      {},
      el("td", {}, r.chip + " (0x" + r.id.toString(16) + ")"),
      el("td", { class: "mid" }, r.size ? formatBytes(r.size) : ""),
      el(
        "td",
        {},
        el(
          "span",
          { class: "badge " + (r.verdict in S.badges ? S.badges[r.verdict] : "invalid") },
          S.verdicts[r.verdict] || r.verdict,
        ),
      ),
      el("td", { class: "mid" }, r.detail),
    );
    if (!r.calibration) return [line];
    return [line, el("tr", {}, el("td", { colspan: "4", class: "calwords" }, S.calibration(r.chip, r.calibration)))];
  }

  function showRows(rows) {
    warned = rows.filter((r) => r.verdict === "warning").length;
    els.table.replaceChildren(...rows.flatMap(rowLines));
    els.calibrationNote.hidden = !rows.some((r) => r.calibration);
  }

  function showExtras(extras) {
    els.extras.replaceChildren(
      ...(extras.length ? [el("p", {}, S.extras)] : []),
      ...extras.map((p) =>
        el("p", { class: "mono mid" }, p.chip + " (0x" + p.id.toString(16) + "): " + p.detail),
      ),
    );
  }

  function onState(m) {
    state = m.state;
    showRows(m.rows || []);
    showExtras(m.extras || []);
    refresh(window.Tempester.kit.session);
  }

  function onSaved(m) {
    els.saved.replaceChildren(
      el("p", {}, S.savedIn(m.dir)),
      ...m.files.map((f) => el("p", { class: "mono mid" }, f)),
    );
  }

  function build() {
    const kit = window.Tempester.kit;
    els.status = el("span", { class: "statusline", role: "status" });
    els.unavailable = el("span", { class: "statusline", role: "status" });
    els.request = button(
      "fetch",
      S.request,
      () => bridge.send("full_backup_start"),
      { variant: "primary", text: S.requestText },
    );
    els.cancel = button("stop", S.cancel, () => bridge.send("full_backup_cancel"), {
      text: S.cancelText,
    });
    els.save = button("save", S.save, () => bridge.send("full_backup_save"), {
      text: S.saveText,
    });
    els.cycled = button("refresh", S.cycled, () => bridge.send("full_backup_power_cycled"), {
      text: S.cycledText,
    });
    els.limit = el("p", { class: "statusbox bad", role: "note" }, S.onePerPowerOn);
    els.table = el("tbody");
    els.calibrationNote = el("p", { class: "statusbox", role: "note" }, S.calibrationNote);
    els.extras = el("div");
    els.saved = el("div", { class: "statusbox" });
    const section = el(
      "section",
      { class: "panel", "aria-label": S.title },
      el(
        "div",
        { class: "panel-head" },
        el("h3", {}, S.title),
        info(
          el("ol", { class: "steps" }, S.steps.map((step) => el("li", {}, step))),
          S.stepsTitle,
        ),
        els.request,
        els.cancel,
        els.save,
        els.cycled,
      ),
      el("p", { class: "statusbox bad", role: "note" }, S.never),
      els.limit,
      els.status,
      els.unavailable,
      els.calibrationNote,
      el(
        "div",
        { class: "scroll" },
        el(
          "table",
          { class: "list" },
          el(
            "thead",
            {},
            el("tr", {}, Object.values(S.cols).map((c) => el("th", {}, c))),
          ),
          els.table,
        ),
      ),
      els.extras,
      els.saved,
    );
    kit.onSession((session) => {
      features.setFake(session.fake);
      refresh(session);
    });
    const take = (m) => {
      if (!m.features) return;
      features.set(m.features, m.os_known);
      refresh(kit.session);
    };
    bridge.on("state", take);
    bridge.on("installed", take);
    bridge.on("identity", (m) => {
      features.setIdentity(!!m.tempester);
      refresh(kit.session);
    });
    bridge.on("full_backup", onState);
    bridge.on("full_backup_saved", onSaved);
    bridge.on("full_backup_power_cycled", () => window.Tempester.kit.toast(S.cycledDone));
    bridge.on("error", (m) => {
      if (m.during && m.during.startsWith("full_backup"))
        window.Tempester.kit.toast(window.Tempester.kit.errorText(m));
    });
    showRows([]);
    refresh(kit.session);
    return { section };
  }

  return { build };
})();
