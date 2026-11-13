/* "Verify the installed OS": the last Main OS sent from the app (what it was, when, whether a dump read back from
   the machine proved it), and the guided read-back. The user opens the Debug menu on the machine (both SHIFT and
   SYSTEM) and chooses "Send Main OS"; Rust compares the SHA-256 of that image below the flasher page (the range an
   update writes) with the one of the OS it sent and keeps the dump as a dated backup. The flasher page is the
   machine's own and is only reported, never compared. Tempester versions all announce the same identity word, so only this comparison tells
   which one the machine runs. `installedLine` is also used by the Machine page. */
"use strict";
window.Tempester = window.Tempester || {};
window.Tempester.osVerify = (function () {
  const { el, toast, errorText } = window.Tempester.kit;
  const { button, info } = window.Tempester.icons;
  const bridge = window.Tempester.bridge;
  const kit = window.Tempester.kit;

  const TICK = "✓";

  const S = {
    title: "Verify the installed OS",
    start: "Verify the installed OS",
    stop: "Stop listening",
    stepsTitle: "How to verify",
    steps: [
      'Connect with the plug button in the top bar, then press "Verify the installed OS".',
      "Power-cycle the Tempest first if the update just ended, then hold both SHIFT and SYSTEM to open the Debug menu.",
      'Choose "Send Main OS" and wait until it is done (the MIDI cable choice in System decides USB or DIN).',
      "The dump is compared with the OS that was sent, over the range an update writes (the machine's own update routine, after it, is not compared), and kept as a dated backup.",
    ],
    noneInstalled: "Installed: nothing sent from this app yet.",
    installed: (r) =>
      "Installed: " +
      r.label +
      (r.version ? "" : " (unknown build)") +
      ", " +
      (r.verified_at ? "verified " + TICK : "not verified"),
    running: (running, proved) =>
      "The machine runs " +
      running.label +
      (running.known ? "" : " (not in this app's table)") +
      " (it reports it), " +
      (proved ? "verified " + TICK : "not verified"),
    differs: (running, record) =>
      "The machine reports " +
      running.label +
      "; this app last sent " +
      record.label +
      ".",
    sentTitle: (r) => "Last sent by this app: " + r.label + ", " + stampText(r.at),
    installedTitle: (r) =>
      "Sent " +
      stampText(r.at) +
      (r.verified_at ? ", verified " + stampText(r.verified_at) : "") +
      ". Image SHA-256 " +
      r.sha256_image +
      (r.sha256_image_written
        ? ". Written range SHA-256 " + r.sha256_image_written
        : ""),
    idle: "Not verifying. A dump read back from the machine proves what it runs.",
    listening:
      'Listening: on the Tempest hold SHIFT + SYSTEM, then choose "Send Main OS".',
    match: (m) =>
      m.version
        ? "The machine runs exactly Tempester " +
          m.version +
          " " +
          TICK +
          (m.source === "reported" ? S.comparedWithReported : "")
        : "The machine runs exactly the file you sent (" +
          m.label +
          ") " +
          TICK,
    mismatch: (m) =>
      "Mismatch: the machine does not run " +
      m.label +
      (m.also || m.source === "reported" ? S.tag(m.source) : "") +
      (m.also
        ? " nor " + m.also + S.tag(m.source === "reported" ? "sent" : "reported")
        : "") +
      ". Its Main image below the flasher page is another one (both hashes below). Update again, or restore your backup.",
    comparedWithReported:
      " (compared with the image of the version the machine reports)",
    tag: (source) =>
      source === "reported"
        ? " (the version the machine reports)"
        : " (sent by this app)",
    flasher: (m) =>
      "Update routine (flasher page): the machine's own, never rewritten by an update, so it is not compared; it may differ from DSI's file, as expected." +
      (m.flasher ? " SHA-256 " + m.flasher : ""),
    constant: (m) =>
      "Constant dump: every byte is 0x" +
      m.byte.toString(16).toUpperCase().padStart(2, "0") +
      ", the machine sent no real content, so nothing is proved. Send it again.",
    incomplete: (m) =>
      "Incomplete or damaged dump (" +
      m.detail +
      "): nothing is proved. Send it again; still listening.",
    savedIn: (dir) => "Dump kept as a backup in " + dir,
    evidenceLabels: {
      expected: "Sent, written range",
      expectedReported: "Reported version, written range",
      found: "Machine, written range",
      flasher: "Machine flasher page",
      shortExpected: "Sent",
      shortExpectedReported: "Reported",
      shortFound: "Machine",
    },
  };

  /* "2026-11-08_120000Z" -> "2026-11-08 12:00 UTC" */
  function stampText(stamp) {
    const m = /^(\d{4}-\d{2}-\d{2})_(\d{2})(\d{2})\d{2}Z$/.exec(stamp || "");
    return m ? m[1] + " " + m[2] + ":" + m[3] + " UTC" : String(stamp || "");
  }

  /* The text, tone, full title and note of the installed-OS line. `record`: what this app last sent (null when
     nothing); `running`: the OS the machine reports (n°39: version, label, known) or null; `verified`: the reported
     version a read-back has proved, or null. A reported OS is what the machine runs: the record then only says what
     this app last sent, and a note says so when the two differ. */
  function installedLine(record, running, verified) {
    if (!running)
      return !record
        ? { text: S.noneInstalled, tone: "", title: "", note: "" }
        : {
            text: S.installed(record),
            tone: record.verified_at ? "good" : "",
            title: S.installedTitle(record),
            note: "",
          };
    const proved =
      verified === running.version ||
      !!(
        record &&
        record.verified_at &&
        record.version === running.version
      );
    return {
      text: S.running(running, proved),
      tone: proved ? "good" : "",
      title: record ? S.sentTitle(record) : "",
      note:
        record && record.version !== running.version
          ? S.differs(running, record)
          : "",
    };
  }

  /* What the pages show: the record, the reported OS, the reported version a read-back proved and the feature table
     Rust resolved for them (`features`: id -> {label, on, since}). Kept here so the OS page and the Machine page
     always say the same thing. */
  const view = { record: null, running: null, verified: null, features: {} };
  const watchers = [];
  function setView(patch) {
    Object.assign(view, patch);
    watchers.forEach((watcher) => watcher(view));
  }
  function watch(watcher) {
    watchers.push(watcher);
    watcher(view);
  }
  function onMachineState(m) {
    const running = m.running || null;
    setView({
      record: m.installed || null,
      running,
      features: m.features || view.features,
      verified:
        running && running.version === view.verified ? view.verified : null,
    });
  }
  bridge.on("state", onMachineState);
  bridge.on("installed", onMachineState);
  bridge.on("os_verify", (m) => {
    if (m.state === "match" && m.source === "reported")
      setView({ verified: m.version });
  });
  kit.onSession((session) => {
    if (!session.connected) setView({ running: null, verified: null });
  });

  function stepList() {
    return el(
      "ol",
      { class: "steps" },
      S.steps.map((step) => el("li", {}, step)),
    );
  }

  function create() {
    const els = {};
    let listening = false,
      sending = false;

    function showInstalled(state) {
      const line = installedLine(state.record, state.running, state.verified);
      els.installed.textContent = line.text;
      els.installed.setAttribute("class", "statusline " + line.tone);
      els.installed.setAttribute("title", line.title);
      els.note.textContent = line.note;
      els.note.hidden = !line.note;
    }

    /* The read-back compares with the OS the app sent, or with the release the machine reports when it is known. */
    const canVerify = () =>
      !!(view.record || (view.running && view.running.known));

    function refresh(session) {
      els.start.disabled =
        !session.connected || !canVerify() || listening || sending;
      els.stop.disabled = !listening;
    }

    function showStatus(text, tone) {
      els.status.textContent = text;
      els.status.setAttribute("class", "statusbox" + (tone ? " " + tone : ""));
    }

    function hashes(m) {
      const reported = m.source === "reported";
      return [
        {
          label: reported
            ? S.evidenceLabels.expectedReported
            : S.evidenceLabels.expected,
          shownAs: reported
            ? S.evidenceLabels.shortExpectedReported
            : S.evidenceLabels.shortExpected,
          value: m.expected,
        },
        ...(m.found
          ? [
              {
                label: S.evidenceLabels.found,
                shownAs: S.evidenceLabels.shortFound,
                value: m.found,
              },
            ]
          : []),
        ...(m.flasher
          ? [
              {
                label: S.evidenceLabels.flasher,
                value: m.flasher,
                fullOnly: true,
              },
            ]
          : []),
      ];
    }

    function onVerify(m) {
      listening = m.state === "listening" || m.state === "incomplete";
      if (m.state === "idle") {
        showStatus(S.idle, "");
        els.evidence.show({});
      } else if (m.state === "listening") {
        showStatus(S.listening, "");
        els.evidence.show({});
      } else if (m.state === "match") {
        showStatus(
          S.match(m) + " " + S.flasher(m) + " " + S.savedIn(m.dir || ""),
          "good",
        );
        els.evidence.show({ tone: "good", hashes: hashes(m) });
      } else if (m.state === "mismatch") {
        showStatus(
          S.mismatch(m) + " " + S.flasher(m) + " " + S.savedIn(m.dir || ""),
          "bad",
        );
        els.evidence.show({ tone: "bad", hashes: hashes(m) });
      } else if (m.state === "constant") {
        showStatus(S.constant(m), "bad");
        els.evidence.show({ tone: "warn", hashes: hashes(m) });
      } else {
        showStatus(S.incomplete(m), "bad");
        els.evidence.show({});
      }
      refresh(kit.session);
    }

    els.installed = el("span", { class: "statusline", role: "status" });
    els.note = el("p", {
      class: "statusbox warn",
      role: "status",
      hidden: true,
    });
    els.start = button("check", S.start, () => bridge.send("os_verify_start"), {
      text: S.start,
    });
    els.stop = button("stop", S.stop, () => bridge.send("os_verify_stop"));
    els.status = el("p", { class: "statusbox", role: "status" }, S.idle);
    els.evidence = window.Tempester.osHash.evidence();
    const node = el(
      "div",
      { class: "verify-block" },
      el("div", { class: "toolbar" }, els.installed),
      els.note,
      el(
        "div",
        { class: "toolbar" },
        els.start,
        els.stop,
        info(stepList(), S.stepsTitle),
      ),
      els.status,
      els.evidence.node,
    );

    watch((state) => {
      showInstalled(state);
      refresh(kit.session);
    });
    kit.onSession(refresh);
    bridge.on("os_verify", onVerify);
    bridge.on("os_started", () => {
      sending = true;
      onVerify({ state: "idle" });
    });
    bridge.on("os_finished", () => {
      sending = false;
      refresh(kit.session);
    });
    bridge.on("error", (m) => {
      if (m.during !== "os_verify") return;
      toast(errorText(m));
      listening = false;
      refresh(kit.session);
    });
    return { node };
  }

  return { installedLine, watch, state: () => view, create, stampText };
})();
