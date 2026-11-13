/* The answer to an identity check, shown in full on the Machine page (not behind an info badge): device id,
   manufacturer and family, version word and what it means (Tempester OS or DSI OS), the exact version the machine
   reports when it does (n°39, from the shared state of os-verify.js), which features that version has, the cable or
   port the answer came from, and when it was checked. The fact rows are always the same, so a new answer never moves
   the page. The top bar only keeps a one-word summary (conn-bar.js). */
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
    wordNote:
      "The version word (0x7F0 to 0x7FF) says this is a Tempester OS, not which release: a feature shown as 'From x on' needs that release or later. The exact version is on the Tempest: System > Main OS Version.",
    reportedNote:
      "The version is the one the machine reports; each feature is resolved from it.",
    unknownNote:
      "The machine reports a version this app does not know: its features cannot be resolved.",
    dsiNote:
      "The version word is the Voice OS version, as the original DSI OS announces it. The features below need a Tempester OS.",
    none: "-",
    featuresNo: "Not in the DSI OS",
    featuresNotHere: "Not in this version",
    featuresMaybe: (since) => "From " + since + " on",
    featuresYes: "Available",
    reported: (running) =>
      running.label +
      " (reported by the machine" +
      (running.known ? ")" : ", not in this app's table)"),
    unknownPort: "unknown",
    fakeSuffix: " (fake machine)",
    rows: {
      status: "Status",
      os: "OS",
      version: "Version",
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

  /* The feature rows (id, name, first release) are not listed here: Rust owns the table (`FEATURES` in
     src/storage/installed.rs) and sends it with every `state` and `installed` event, so a new release's features
     appear without touching this file. The only fact kept here is about the identity word, which Rust cannot know:
     the oldest release a word proves. Words from the early patches (0x7F1) prove nothing: they stay out. */
  const PROVEN_FROM = { 0x7f2: "1.5.2.0" };

  const compare = (a, b) => {
    const left = a.split(".").map(Number);
    const right = b.split(".").map(Number);
    return left.reduce((diff, n, k) => diff || n - right[k], 0);
  };

  /* The text and tone of a feature row. `m`: the identity answer or null; `running`: the OS the machine reports
     or null; `feature`: Rust's {label, on, since}. A reported release the app knows decides alone; otherwise the
     identity word gives only a "from" guess. */
  function featureValue(feature, m, running) {
    if (m && !m.tempester) return [S.featuresNo, "off"];
    if (running && running.known)
      return feature.on ? [S.featuresYes, "good"] : [S.featuresNotHere, "off"];
    if (!m || !feature.since) return [S.none, "off"];
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
  const featureCells = {};
  let featureList = null;
  let note = null;
  /* The last identity answer (or its absence), kept so a change of the shared state re-renders the same card. */
  let current = { m: null, status: "unchecked", checked: null, port: "" };

  function set(cell, text, tone) {
    cell.value.textContent = text;
    cell.value.setAttribute("class", tone || "off");
  }

  /* One row per feature Rust lists, added the first time it is seen; the area keeps its height meanwhile. */
  function ensureFeatureRows(features) {
    for (const [id, feature] of Object.entries(features)) {
      if (featureCells[id]) continue;
      featureCells[id] = row(feature.label);
      featureList.append(
        el(
          "div",
          { class: "idrow" },
          featureCells[id].label,
          featureCells[id].value,
        ),
      );
    }
  }

  function noteText(m, running, status) {
    if (m && !m.tempester) return S.dsiNote;
    if (running) return running.known ? S.reportedNote : S.unknownNote;
    if (m) return S.wordNote;
    return status === "none" ? identity.hint : S.notChecked;
  }

  function render(view) {
    const { m, status, checked, port } = current;
    const running = view.running;
    const unchecked = status === "unchecked";
    const tone = (on, good) => (on ? good : "off");
    set(
      cells.status,
      status === "answered"
        ? S.answered
        : status === "none"
          ? S.noAnswer
          : S.unchecked,
      status === "answered" ? "good" : status === "none" ? "warn" : "off",
    );
    set(cells.os, m ? identity.kind(m) : S.none, tone(m, "good"));
    set(
      cells.version,
      running ? S.reported(running) : S.none,
      running ? (running.known ? "good" : "warn") : "off",
    );
    set(cells.port, unchecked ? S.none : port, unchecked ? "off" : "");
    set(cells.time, unchecked ? S.none : clock(checked), unchecked ? "off" : "");
    set(
      cells.device,
      m ? "0x" + identity.hex(m.device, 2) : S.none,
      tone(m, ""),
    );
    set(cells.manufacturer, m ? S.manufacturer : S.none, tone(m, ""));
    set(cells.family, m ? S.family : S.none, tone(m, ""));
    set(
      cells.word,
      m ? "0x" + identity.hex(m.version) : S.none,
      tone(m, ""),
    );
    ensureFeatureRows(view.features);
    for (const [id, feature] of Object.entries(view.features)) {
      const [text, featureTone] = featureValue(feature, m, running);
      set(featureCells[id], text, featureTone);
    }
    note.textContent = noteText(m, running, status);
    note.setAttribute("class", "idnote" + (status === "none" ? " warn" : ""));
  }

  const portName = () =>
    (session.port || S.unknownPort) + (session.fake ? S.fakeSuffix : "");

  function build() {
    const { osVerify } = window.Tempester;
    for (const [key, label] of Object.entries(S.rows)) cells[key] = row(label);
    featureList = el("dl", { class: "idfeatures" });
    note = el("p", { class: "idnote", role: "status" });
    const answer = (next) => {
      current = next;
      render(osVerify.state());
    };
    bridge.on("identity", (m) =>
      answer({ m, status: "answered", checked: new Date(), port: portName() }),
    );
    bridge.on("error", (m) => {
      if (m.during === "identity" && m.code === "timeout")
        answer({
          m: null,
          status: "none",
          checked: new Date(),
          port: portName(),
        });
    });
    onSession((state) => {
      if (!state.connected)
        answer({ m: null, status: "unchecked", checked: null, port: "" });
    });
    osVerify.watch(render);
    return el(
      "section",
      { class: "panel fixed", "aria-label": S.title },
      el("div", { class: "panel-head" }, el("h3", {}, S.title)),
      el(
        "div",
        { class: "idgrid" },
        column(["status", "os", "version", "port", "time"].map((k) => cells[k])),
        column(
          ["device", "manufacturer", "family", "word"].map((k) => cells[k]),
        ),
      ),
      featureList,
      note,
    );
  }

  window.Tempester.identityPanel = { build };
})();
