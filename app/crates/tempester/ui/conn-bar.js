/* Connection controls in the top bar, on every page: the MIDI port to use, a refresh of the port list, one
   Connect / Disconnect icon toggle, and a short summary of the identity check (a chip that opens the Machine page,
   where the whole answer is shown: identity-panel.js). There is no other place to connect: the Machine page only asks
   for the identity. */
"use strict";
(function () {
  const kit = window.Tempester.kit;
  const { el, session, onSession } = kit;
  const { button, relabel } = window.Tempester.icons;
  const bridge = window.Tempester.bridge;

  const S = {
    port: "MIDI port",
    noPorts: "No MIDI port",
    refresh: "Refresh the MIDI ports",
    connect: "Connect",
    disconnect: "Disconnect",
    chipTip: "Show the whole identity answer on the Machine page",
    idUnchecked: "OS: unchecked",
    os: (label) => "OS: " + label,
    idTempester: "Tempester OS",
    idDsi: "DSI OS",
    idNoAnswer: "No answer",
    identityLead:
      "Asks the machine who it is. The whole answer is shown below: a quick way to see that the cable and the settings are right and which OS answers.",
    identityHint:
      'No answer. On the Tempest, System > "MIDI: Sysex IN+OUT Cable" must be the cable you use (USB or DIN). The original DSI OS answers the identity request on the DIN cable only; Tempester versions answer on the chosen cable.',
  };

  const hex = (n, width) =>
    n
      .toString(16)
      .toUpperCase()
      .padStart(width || 1, "0");

  /* The wording of an identity answer, shared with the identity panel. */
  const identity = {
    lead: S.identityLead,
    hint: S.identityHint,
    kind: (m) => (m.tempester ? S.idTempester : S.idDsi),
    hex,
    short: (m) => identity.kind(m) + ", version word 0x" + hex(m.version),
  };
  window.Tempester.identity = identity;

  let ports = [];
  let select = null,
    toggle = null,
    chip = null,
    answered = null,
    running = null;

  function fill(list) {
    ports = list;
    const keep = select.value;
    select.replaceChildren(
      ...(list.length
        ? list.map((name) => el("option", { value: name }, name))
        : [el("option", { value: "" }, S.noPorts)]),
    );
    const preferred = [session.port, keep, "Tempest"].find((p) =>
      list.includes(p),
    );
    if (preferred) select.value = preferred;
    refresh(session);
  }

  function showChip(text, tone) {
    chip.textContent = text;
    chip.setAttribute("class", "chip" + (tone ? " " + tone : ""));
  }

  function refresh(state) {
    relabel(
      toggle,
      state.connected ? S.disconnect : S.connect,
      state.connected ? "disconnect" : "connect",
    );
    toggle.classList.toggle("primary", !state.connected);
    toggle.disabled = !state.connected && !ports.length;
    select.disabled = state.connected;
    if (!state.connected) {
      answered = null;
      running = null;
      showChip(S.idUnchecked, "");
    }
  }

  /* The exact version the machine reports (n°39) is more precise than the identity word: it wins when known. */
  function paintChip() {
    if (running) showChip(S.os(running.label), "ok");
    else if (answered) showChip(answered, "ok");
    else showChip(S.idUnchecked, "");
  }

  function onRunning(m) {
    running = m.running || null;
    paintChip();
  }

  function onToggle() {
    if (session.connected) bridge.send("disconnect");
    else if (select.value) bridge.send("connect", select.value);
  }

  function build() {
    const bar = document.getElementById("connbar");
    select = el("select", { id: "connbar-port", "aria-label": S.port });
    const reload = button("refresh", S.refresh, () => bridge.send("ports"));
    toggle = button("connect", S.connect, onToggle);
    chip = el(
      "button",
      {
        type: "button",
        class: "chip",
        "data-tip": S.chipTip,
        onclick: () => kit.showPage("machine"),
      },
      S.idUnchecked,
    );
    bar.append(select, reload, toggle, chip);
    onSession(refresh);
    bridge.on("ports", (m) => fill(m.ports));
    bridge.on("identity", (m) => {
      answered = identity.kind(m);
      paintChip();
    });
    bridge.on("state", onRunning);
    bridge.on("installed", onRunning);
    bridge.on("error", (m) => {
      if (m.during === "identity" && m.code === "timeout")
        showChip(S.idNoAnswer, "warn");
    });
    fill([]);
    bridge.send("ports");
  }

  build();
})();
