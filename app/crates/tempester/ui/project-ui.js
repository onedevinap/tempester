/* The whole project, for the Library page: the compact "Project" card (fetch it from the machine, or wait for the
   menu's Send Project, with progress; and the progress, cancel and « fetch it back and compare » of a project sent
   back from the library) whose long explanations are in info badges. What to keep of what arrives is decided in
   the review dialog, what a send replaces in the project dialog (lib-dialogs.js). Loaded before the page. */
"use strict";
window.Tempester = window.Tempester || {};
window.Tempester.project = (function () {
  const { el, toast } = window.Tempester.kit;
  const bridge = window.Tempester.bridge;
  const kit = window.Tempester.kit;
  const icons = window.Tempester.icons;

  const S = {
    title: "Project",
    lead: "Bring the whole project (up to 16 beats and their sounds) from the machine into the library, in one click. Needs Tempester 1.6.1.0 or later; there is nothing to do on the machine. Before anything is written, a review says what is new, identical to something already in the library, or a modified version of it.",
    receiveHint:
      'For an older OS: click "Wait for Send Project", then on the machine open the menu and choose Send Project (on the MIDI cable set in System).',
    note: "A whole project is kept as two files: the exact capture of what the machine sent (evidence, never sent back) and the same project in the standard layout. Its beats and sounds are listed under Beats and Sounds, in a folder named after the project.",
    sendNote:
      "To send a project back, use the send button on its row in the Projects list: the standard file goes (never the capture), after the machine's own project is looked at and backed up when the library does not know it. Beats then arrive one by one, and you can fetch the project back and compare it with what was sent.",
    fetch: "Fetch the project from the machine",
    receive: "Wait for Send Project from the machine's menu",
    cancel: "Cancel",
    fakeButton: "Play the machine's Send Project (fake machine)",
    fakePartialButton: "Play a Send Project with absent beats (fake machine)",
    fakeDefectOff:
      "The fake machine reads Play Lists correctly. Click to make it read them like the stock OS (n°28 defect).",
    fakeDefectOn:
      "The fake machine reads Play Lists like the stock OS (n°28 defect). Click to make it read them correctly.",
    cancelSend: "Cancel the project send",
    verify: "Fetch the project back from the machine and compare it with what was sent",
    sending: (m) =>
      m.message
        ? "Sending the project: beat " + m.message + " of " + m.beats + "."
        : "Sending the project: the 5E header.",
    sent: (m) =>
      "Project sent: the 5E header and " +
      m.beats +
      " beat(s). Fetch it back and compare to check it.",
    sendCancelled: (m) =>
      "Send cancelled after " +
      m.sent +
      " bytes: the machine's project is a mix of the sent project and its previous one.",
    listening:
      "Waiting for the machine. Now open its menu and choose Send Project.",
    asking: "Asking the machine for its project. Nothing to do on the machine.",
    receiving: (m) => "Receiving the project: beat " + m.beat + " received.",
    header: "Project header received, waiting for the first beat.",
    review:
      "Project received. Choose what to keep in the window that opened: nothing is written before.",
    absent: (m) =>
      " PARTIAL PROJECT, not a complete one: beat" +
      (m.absent_beats.length > 1 ? "s " : " ") +
      m.absent_beats.join(", ") +
      " not sent (not marked present on the machine, or the transfer stopped).",
    saved: (m) =>
      "Project reviewed: " +
      m.written +
      " written, " +
      m.updated +
      " updated, " +
      m.skipped +
      " skipped (" +
      m.beats +
      " of " +
      m.total +
      " beats, " +
      m.sounds +
      " distinct sounds" +
      (m.duplicates
        ? ", " +
          m.duplicates +
          " repeated sounds left out; every beat file still holds all its sounds"
        : "") +
      "). Where each file went is in Messages." +
      (m.partial ? S.absent(m) : ""),
    idle: "Nothing fetched yet.",
    cancelled: "Cancelled: nothing was written.",
  };

  function card() {
    const els = {};
    els.status = el("p", { class: "lib-card-status", role: "status" }, S.idle);
    els.status.setAttribute("title", S.idle);
    /* One line with an ellipsis: the full text is the tooltip. */
    function say(text) {
      els.status.textContent = text;
      els.status.setAttribute("title", text);
    }
    els.bar = el("div");
    els.meter = el("div", { class: "progress slim", hidden: true }, els.bar);
    els.fetch = icons.button(
      "fetch",
      S.fetch,
      () => bridge.send("fetch_project"),
      { variant: "primary", text: "Fetch" },
    );
    els.receive = icons.button(
      "play",
      S.receive,
      () => bridge.send("receive_project"),
      { text: "Wait" },
    );
    let sending = false;
    els.cancel = icons.button("close", S.cancel, () =>
      bridge.send(sending ? "lib_project_cancel" : "project_cancel"),
    );
    els.cancel.hidden = true;
    els.verify = icons.button(
      "compare",
      S.verify,
      () => bridge.send("lib_project_verify"),
    );
    els.verify.disabled = true;
    let defect = false;
    els.defect = icons.button("warning", S.fakeDefectOff, () =>
      bridge.send("fake_project_defect", defect ? 0 : 1),
    );
    els.defect.hidden = true;
    els.fake = icons.button("play", S.fakeButton, () =>
      bridge.send("fake_project_send"),
    );
    els.fakePartial = icons.button("warning", S.fakePartialButton, () =>
      bridge.send("fake_project_send_partial"),
    );
    els.fake.hidden = true;
    els.fakePartial.hidden = true;
    const node = el(
      "div",
      { class: "card lib-card compact" },
      el(
        "div",
        { class: "lib-card-head" },
        icons.icon("project"),
        el("h3", {}, S.title),
        icons.info(
          S.lead + " " + S.receiveHint + " " + S.note + " " + S.sendNote,
          "About the project",
        ),
      ),
      els.status,
      el(
        "div",
        { class: "toolbar" },
        els.fetch,
        els.receive,
        els.cancel,
        els.verify,
        el("span", { class: "grow lib-meter" }, els.meter),
        els.fake,
        els.fakePartial,
        els.defect,
      ),
    );

    let verifiable = false;
    function busy(on) {
      els.receive.disabled = on || !kit.session.connected;
      els.fetch.disabled = on || !kit.session.connected;
      els.cancel.hidden = !on;
      els.verify.disabled = on || !verifiable || !kit.session.connected;
    }

    /* After a send: offer to fetch the project back and compare (it changes nothing on the machine). */
    async function offerVerify() {
      const ok = await window.Tempester.dialogs.confirm({
        title: "Project sent",
        lines: [
          "The project was sent to the machine. Fetch it back and compare it with what was sent? This only reads the machine: nothing is changed on it.",
        ],
        okLabel: "Fetch it back and compare",
        noLabel: "Not now",
      });
      if (ok) bridge.send("lib_project_verify");
    }

    function onSend(m) {
      const running = m.state === "started" || m.state === "sending";
      sending = running;
      if (m.state === "started") verifiable = false;
      if (m.state === "done") verifiable = true;
      busy(running);
      els.meter.hidden = !running;
      els.bar.style.width = ((m.sent * 100) / Math.max(m.total, 1)).toFixed(1) + "%";
      if (running) say(S.sending(m));
      else if (m.state === "done") {
        say(S.sent(m));
        offerVerify();
      }
      else if (m.state === "cancelled")
        say(S.sendCancelled(m));
      else {
        say(m.message);
        toast(m.message);
      }
    }

    function onProject(m) {
      const running = m.state === "listening" || m.state === "receiving";
      busy(running);
      els.meter.hidden = !running;
      els.bar.style.width = ((m.beat * 100) / m.total).toFixed(1) + "%";
      if (m.state === "listening")
        say(m.mode === "fetch" ? S.asking : S.listening);
      else if (m.state === "receiving")
        say(m.beat ? S.receiving(m) : S.header);
      else if (m.state === "review") say(S.review);
      else if (m.state === "failed") {
        say(m.message);
        toast(m.message);
      } else if (m.state === "idle") say(S.cancelled);
    }

    bridge.on("project", onProject);
    bridge.on("project_send", onSend);
    bridge.on("fake_project_defect", (m) => {
      defect = m.on;
      const label = m.on ? S.fakeDefectOn : S.fakeDefectOff;
      els.defect.setAttribute("title", label);
      els.defect.setAttribute("aria-label", label);
      els.defect.setAttribute("aria-pressed", String(m.on));
    });
    bridge.on("project_saved", (m) => {
      say(S.saved(m));
    });
    bridge.on("error", (m) => {
      if (
        ["receive_project", "fetch_project", "fake_project_send"].includes(
          m.during,
        )
      )
        toast(kit.errorText(m));
    });
    kit.onSession((session) => {
      busy(!els.cancel.hidden);
      els.fake.hidden = !session.fake;
      els.fakePartial.hidden = !session.fake;
      els.defect.hidden = !session.fake;
    });
    busy(false);
    return node;
  }

  return { card };
})();
