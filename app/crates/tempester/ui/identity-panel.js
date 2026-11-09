/* The answer to an identity check, shown in full on the Machine page (not behind an info badge): device id,
   manufacturer and family, version word and what it means (Tempester OS or DSI OS, and which features follow), the
   cable or port it came from, and when it was checked. The panel always has the same rows, so a new answer never
   moves the page. The top bar only keeps a one-word summary (conn-bar.js). */
"use strict";
(function () {
  const { el, session, onSession } = window.Tempester.kit;
  const bridge = window.Tempester.bridge;
  const identity = window.Tempester.identity;

  const S = {
    title: "Machine identity",
    unchecked: "Not checked",
    answered: "Answered",
    noAnswer: "No answer",
    notChecked: "Check the identity (button above) to fill this in.",
    tempesterNote:
      "The version word (0x7F0 to 0x7FF) says this is a Tempester OS, not which release: a feature shown as 'From x on' needs that release or later. The exact version is on the Tempest: System > Main OS Version.",
    dsiNote:
      "The version word is the Voice OS version, as the original DSI OS announces it. The features on the right need a Tempester OS.",
    none: "-",
    featuresNo: "Not in the DSI OS",
    featuresMaybe: (since) => "From " + since + " on",
    featuresYes: "Available",
    unknownPort: "unknown",
    fakeSuffix: " (fake machine)",
    rows: {
      status: "Status",
      os: "OS",
      port: "Came from",
      time: "Checked at",
      device: "Device id",
      manufacturer: "Manufacturer",
      family: "Family",
      word: "Version word",
    },
    /* Manufacturer and family are the same in every reply the app accepts (tempest-proto parses only these bytes). */
    manufacturer: "Dave Smith Instruments (0x01)",
    family: "Tempest (0x28 0x01)",
  };

  /* What each Tempester release added, and the first release that has it. */
  const FEATURES = [
    { name: "Sound and beat requests", since: "1.5.2.0" },
    { name: "NRPN reception", since: "1.5.3.0" },
    { name: "Remote panel", since: "1.6.0.0" },
    { name: "Project request", since: "1.6.1.0" },
  ];

  /* The oldest release a version word proves. Words from the early patches (0x7F1) prove nothing: they stay out. */
  const PROVEN_FROM = { 0x7f2: "1.5.2.0" };

  const compare = (a, b) => {
    const left = a.split(".").map(Number);
    const right = b.split(".").map(Number);
    return left.reduce((diff, n, k) => diff || n - right[k], 0);
  };

  /* The text and tone of a feature row for an answer `m` (or null: nothing checked). */
  function featureValue(feature, m) {
    if (!m) return [S.none, "off"];
    if (!m.tempester) return [S.featuresNo, "off"];
    const proven = PROVEN_FROM[m.version];
    if (proven && compare(proven, feature.since) >= 0)
      return [S.featuresYes, "good"];
    return [S.featuresMaybe(feature.since), "warn"];
  }

  const clock = (date) =>
    [date.getHours(), date.getMinutes(), date.getSeconds()]
      .map((n) => String(n).padStart(2, "0"))
      .join(":");

  function row(label) {
    const value = el("dd", { class: "off" }, S.none);
    return { label: el("dt", {}, label), value };
  }

  function column(rows) {
    return el(
      "dl",
      { class: "idcol" },
      rows.flatMap((r) => [r.label, r.value]),
    );
  }

  const cells = {};
  let note = null;

  function set(key, text, tone) {
    cells[key].value.textContent = text;
    cells[key].value.setAttribute("class", tone || "off");
  }

  /* `m`: the identity event, or null; `status`: "answered" | "none" | "unchecked"; `checked`: a Date. */
  function render(m, status, checked, port) {
    const unchecked = status === "unchecked";
    set(
      "status",
      status === "answered"
        ? S.answered
        : status === "none"
          ? S.noAnswer
          : S.unchecked,
      status === "answered" ? "good" : status === "none" ? "warn" : "off",
    );
    set("os", m ? identity.kind(m) : S.none, m ? "good" : "off");
    set("port", unchecked ? S.none : port, unchecked ? "off" : "");
    set("time", unchecked ? S.none : clock(checked), unchecked ? "off" : "");
    set(
      "device",
      m ? "0x" + identity.hex(m.device, 2) : S.none,
      m ? "" : "off",
    );
    set("manufacturer", m ? S.manufacturer : S.none, m ? "" : "off");
    set("family", m ? S.family : S.none, m ? "" : "off");
    set("word", m ? "0x" + identity.hex(m.version) : S.none, m ? "" : "off");
    FEATURES.forEach((feature, k) => {
      const [text, tone] = featureValue(feature, m);
      set("feature" + k, text, tone);
    });
    note.textContent = m
      ? m.tempester
        ? S.tempesterNote
        : S.dsiNote
      : status === "none"
        ? identity.hint
        : S.notChecked;
    note.setAttribute("class", "idnote" + (status === "none" ? " warn" : ""));
  }

  const portName = () =>
    (session.port || S.unknownPort) + (session.fake ? S.fakeSuffix : "");

  function build() {
    for (const [key, label] of Object.entries(S.rows)) cells[key] = row(label);
    FEATURES.forEach(
      (feature, k) => (cells["feature" + k] = row(feature.name)),
    );
    note = el("p", { class: "idnote", role: "status" });
    render(null, "unchecked");
    bridge.on("identity", (m) => render(m, "answered", new Date(), portName()));
    bridge.on("error", (m) => {
      if (m.during === "identity" && m.code === "timeout")
        render(null, "none", new Date(), portName());
    });
    onSession((state) => {
      if (!state.connected) render(null, "unchecked");
    });
    return el(
      "section",
      { class: "panel fixed", "aria-label": S.title },
      el("div", { class: "panel-head" }, el("h3", {}, S.title)),
      el(
        "div",
        { class: "idgrid" },
        column(["status", "os", "port", "time"].map((k) => cells[k])),
        column(
          ["device", "manufacturer", "family", "word"].map((k) => cells[k]),
        ),
        column(FEATURES.map((_, k) => cells["feature" + k])),
      ),
      note,
    );
  }

  window.Tempester.identityPanel = { build };
})();
