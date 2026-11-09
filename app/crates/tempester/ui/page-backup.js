/* Backup page: guided capture of the Debug-menu sends. The machine sends its own OS (Main, Voice, Panel) as SysEx;
   the app listens, checks each message, flags constants and saves the good ones in a dated folder with a report.
   One screen: a toolbar (the steps and the warning behind info badges), the received images, the saved files. */
"use strict";
(function () {
  const { el, toast, errorText, formatBytes } = window.Tempester.kit;
  const { button, info } = window.Tempester.icons;
  const bridge = window.Tempester.bridge;
  const kit = window.Tempester.kit;

  const S = {
    title: "Backup",
    stepsTitle: "How to back up",
    steps: [
      'Connect with the plug button in the top bar, then press "Start listening" here.',
      "On the Tempest, hold both SHIFT and SYSTEM to open the Debug menu.",
      'Choose "Send Main OS", wait until it is done, then "Send Voice OS" and "Send Panel OS" the same way (the MIDI cable choice in System decides USB or DIN).',
      'Each message shows up below as it arrives. Press "Save backup" when you have what you need.',
    ],
    start: "Start listening",
    stop: "Stop listening",
    save: "Save backup",
    idle: "Not listening.",
    listening: "Listening for the Debug-menu sends...",
    nothingYet: "Nothing received yet.",
    constantTitle: "About constant images",
    constantWarning:
      "A Voice or Panel send can come back as a constant (every byte 0xFF, or every byte 0x10): the machine then sent no real content. Such a message is flagged, saved under a name that says so, and is not a usable backup.",
    verdicts: { ok: "OK", constant: "constant", invalid: "refused" },
    cols: { chip: "Chip", size: "Size", verdict: "Check", detail: "Detail" },
    receivedTitle: "Received",
    savedTitle: "Saved",
    nothingSaved: "Nothing saved yet.",
    savedIn: (dir) => "Saved in " + dir,
    flagged:
      "At least one image is a constant: it is not a real backup of that chip. Try the send again, or read the chip with a programmer.",
    showFolder: "Show the backups folder",
    fakeButton: "Play the Debug-menu sends (fake machine)",
    fakeText: "Fake sends",
  };

  const els = {};
  let listening = false,
    pieces = [];

  function refreshButtons(session) {
    els.start.disabled = !session.connected || listening;
    els.stop.disabled = !listening;
    els.save.disabled = !pieces.some((p) => p.verdict !== "invalid");
    els.status.textContent = listening ? S.listening : S.idle;
    els.status.setAttribute("class", "statusline" + (listening ? " good" : ""));
    els.fake.hidden = !session.fake;
  }

  function showPieces(list) {
    pieces = list;
    els.table.replaceChildren(
      ...(list.length
        ? list.map((p) =>
            el(
              "tr",
              {},
              el("td", {}, p.chip + " (0x" + p.id.toString(16) + ")"),
              el("td", { class: "mid" }, formatBytes(p.size)),
              el(
                "td",
                {},
                el(
                  "span",
                  { class: "badge " + p.verdict },
                  S.verdicts[p.verdict],
                ),
              ),
              el("td", { class: "mid" }, p.detail),
            ),
          )
        : [
            el(
              "tr",
              {},
              el("td", { class: "mid", colspan: "4" }, S.nothingYet),
            ),
          ]),
    );
  }

  function onBackup(m) {
    listening = m.state === "listening";
    showPieces(m.pieces);
    refreshButtons(kit.session);
  }

  function onSaved(m) {
    els.saved.setAttribute(
      "class",
      "statusbox grow " + (m.flagged ? "bad" : "good"),
    );
    els.saved.replaceChildren(
      el("p", {}, S.savedIn(m.dir)),
      ...m.files.map((f) => el("p", { class: "mono mid" }, f)),
      m.flagged ? el("p", {}, S.flagged) : null,
    );
  }

  function stepList() {
    return el(
      "ol",
      { class: "steps" },
      S.steps.map((step) => el("li", {}, step)),
    );
  }

  function buildToolbar() {
    els.start = button("play", S.start, () => bridge.send("backup_start"), {
      variant: "primary",
      text: S.start,
    });
    els.stop = button("stop", S.stop, () => bridge.send("backup_stop"));
    els.save = button("save", S.save, () => bridge.send("backup_save"), {
      text: S.save,
    });
    els.status = el("span", { class: "statusline", role: "status" }, S.idle);
    els.fake = button(
      "play",
      S.fakeButton,
      () => bridge.send("fake_debug_send"),
      { text: S.fakeText },
    );
    els.fake.hidden = true;
    return el(
      "div",
      { class: "strip" },
      els.start,
      els.stop,
      els.save,
      button("reveal", S.showFolder, () => bridge.send("reveal", "backups")),
      info(stepList(), S.stepsTitle),
      info(S.constantWarning, S.constantTitle),
      els.status,
      els.fake,
    );
  }

  function build(root) {
    els.table = el("tbody");
    els.saved = el("div", { class: "statusbox grow" }, S.nothingSaved);
    root.append(
      buildToolbar(),
      el(
        "div",
        { class: "split fill" },
        el(
          "section",
          { class: "panel", "aria-label": S.receivedTitle },
          el("div", { class: "panel-head" }, el("h3", {}, S.receivedTitle)),
          el(
            "div",
            { class: "scroll" },
            el(
              "table",
              { class: "list" },
              el(
                "thead",
                {},
                el(
                  "tr",
                  {},
                  Object.values(S.cols).map((c) => el("th", {}, c)),
                ),
              ),
              els.table,
            ),
          ),
        ),
        el(
          "section",
          { class: "panel pane-side", "aria-label": S.savedTitle },
          el("div", { class: "panel-head" }, el("h3", {}, S.savedTitle)),
          els.saved,
        ),
      ),
    );
    kit.onSession(refreshButtons);
    showPieces([]);
    refreshButtons(kit.session);
    bridge.on("backup", onBackup);
    bridge.on("backup_saved", onSaved);
    bridge.on("error", (m) => {
      if (m.during && m.during.startsWith("backup")) toast(errorText(m));
    });
  }

  kit.registerPage({ id: "backup", title: S.title, fit: true, build });
})();
