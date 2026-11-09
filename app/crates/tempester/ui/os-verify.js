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
        ? "The machine runs exactly Tempester " + m.version + " " + TICK
        : "The machine runs exactly the file you sent (" +
          m.label +
          ") " +
          TICK,
    mismatch: (m) =>
      "Mismatch: the machine does not run " +
      m.label +
      ". Its Main image below the flasher page is another one (both hashes below). Update again, or restore your backup.",
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
      found: "Machine, written range",
      flasher: "Machine flasher page",
      shortExpected: "Sent",
      shortFound: "Machine",
    },
  };

  /* "2026-11-08_120000Z" -> "2026-11-08 12:00 UTC" */
  function stampText(stamp) {
    const m = /^(\d{4}-\d{2}-\d{2})_(\d{2})(\d{2})\d{2}Z$/.exec(stamp || "");
    return m ? m[1] + " " + m[2] + ":" + m[3] + " UTC" : String(stamp || "");
  }

  /* The text, tone and full title of the installed-OS line (`record` is null when nothing was sent). */
  function installedLine(record) {
    if (!record) return { text: S.noneInstalled, tone: "", title: "" };
    return {
      text: S.installed(record),
      tone: record.verified_at ? "good" : "",
      title: S.installedTitle(record),
    };
  }

  function stepList() {
    return el(
      "ol",
      { class: "steps" },
      S.steps.map((step) => el("li", {}, step)),
    );
  }

  function create() {
    const els = {};
    let record = null,
      listening = false,
      sending = false;

    function showInstalled() {
      const line = installedLine(record);
      els.installed.textContent = line.text;
      els.installed.setAttribute("class", "statusline " + line.tone);
      els.installed.setAttribute("title", line.title);
    }

    function refresh(session) {
      els.start.disabled =
        !session.connected || !record || listening || sending;
      els.stop.disabled = !listening;
    }

    function showStatus(text, tone) {
      els.status.textContent = text;
      els.status.setAttribute("class", "statusbox" + (tone ? " " + tone : ""));
    }

    function hashes(m) {
      return [
        {
          label: S.evidenceLabels.expected,
          shownAs: S.evidenceLabels.shortExpected,
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

    showInstalled();
    kit.onSession(refresh);
    refresh(kit.session);
    bridge.on("state", (m) => {
      record = m.installed || null;
      showInstalled();
      refresh(kit.session);
    });
    bridge.on("installed", (m) => {
      record = m.installed || null;
      showInstalled();
      refresh(kit.session);
    });
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

  return { installedLine, create, stampText };
})();
