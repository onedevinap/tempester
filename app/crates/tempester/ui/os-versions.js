/* The version chooser of the OS update page: three recommended cards (the latest release, the corrections-only
   version, back to the original DSI software) and, below, the History of every release, newest first. Each card and
   each history row shows the one-line summary and the changes as short lists (fixes, additions, changes), with a
   Details toggle for the technical text; each can be built. The list comes from the manifests (`os_versions`), already
   sorted newest first, with `kind` "release" or "corrections". Building needs the user's DSI file: `setReady(false)`
   disables every build button. */
"use strict";
window.Tempester = window.Tempester || {};
window.Tempester.osVersions = (function () {
  const { el } = window.Tempester.kit;
  const { icon, button, info } = window.Tempester.icons;

  const S = {
    latest: (version) => "Latest (" + version + ")",
    corrections: (version) => "Corrections only (" + version + ")",
    original: "Back to DSI 1.5.0.2",
    originalSummary:
      "The original DSI software, rebuilt from your own file. Nothing of Tempester stays on the machine.",
    originalDetails:
      "Prepares your own Tempest_Main_1.5.0.2.syx, whose SHA-256 is checked, to be sent as it is. The Voice, Panel and SAM software are not touched.",
    historyTitle: "History",
    historyLead:
      "Every Tempester release, newest first. Open one to read what it changed since the release before it; each can be built.",
    historyNone: "No version listed.",
    groups: {
      fixes: { title: "Fixes", icon: "check" },
      additions: { title: "Additions", icon: "plus" },
      changes: { title: "Changes", icon: "swap" },
    },
    details: "Details",
    detailsLabel: (version) => "Details of " + version,
    build: "Build",
    buildLabel: (version) => "Build Tempester " + version,
    open: (version) => "Changes of " + version,
    correctionsChip: "Corrections only",
  };

  const GROUP_ORDER = ["fixes", "additions", "changes"];

  function groupList(changes) {
    return GROUP_ORDER.filter((key) => (changes[key] || []).length).map(
      (key) =>
        el(
          "div",
          { class: "vgroup" },
          el(
            "h5",
            {},
            icon(S.groups[key].icon),
            el("span", {}, S.groups[key].title),
          ),
          el(
            "ul",
            {},
            changes[key].map((sentence) => el("li", {}, sentence)),
          ),
        ),
    );
  }

  /* The Details toggle and the technical text it shows; the text is hidden until asked for. */
  function detailsBlock(version, text) {
    const body = el("p", { class: "vdetails", hidden: true }, text);
    const toggle = button(
      "expand",
      S.detailsLabel(version),
      () => {
        body.hidden = !body.hidden;
        toggle.setAttribute("aria-expanded", String(!body.hidden));
      },
      { text: S.details },
    );
    toggle.setAttribute("aria-expanded", "false");
    return { toggle, body };
  }

  function buildButton(version, onBuild, variant) {
    return button("build", S.buildLabel(version), () => onBuild(version), {
      variant,
      text: S.build,
    });
  }

  function create({ onBuild, onOriginal }) {
    const buttons = [];
    let ready = false;
    const cards = el("div", { class: "vcards" });
    const history = el("ul", { class: "vhistory" });
    const historyHead = el(
      "div",
      { class: "vhead" },
      el("h4", {}, S.historyTitle),
      info(S.historyLead, S.historyTitle),
    );
    const node = el("div", { class: "vchooser" }, cards, historyHead, history);

    function card({ title, chip, summary, groups, details, version, action }) {
      const more = detailsBlock(title, details);
      const build = action || buildButton(version, onBuild, "primary");
      buttons.push(build);
      return el(
        "section",
        { class: "vcard", "aria-label": title },
        el(
          "div",
          { class: "vcard-head" },
          el("h4", {}, title),
          chip ? el("span", { class: "chip" }, chip) : null,
        ),
        el("p", { class: "vsummary" }, summary),
        groups,
        el("div", { class: "toolbar" }, more.toggle, build),
        more.body,
      );
    }

    function release(v, title) {
      return card({
        title,
        chip: v.date,
        summary: v.summary,
        groups: groupList(v.changes || {}),
        details: v.description,
        version: v.version,
      });
    }

    function original() {
      const action = button("restore", S.original, onOriginal, {
        text: S.original,
        variant: "primary",
      });
      return card({
        title: S.original,
        summary: S.originalSummary,
        groups: [],
        details: S.originalDetails,
        action,
      });
    }

    /* One history row: a toggle line (version, date, name) and a build button; the changes open under it. */
    function row(v) {
      const body = el("div", { class: "vbody", hidden: true });
      const more = detailsBlock(v.version, v.description);
      body.append(
        el("p", { class: "vsummary" }, v.summary),
        ...groupList(v.changes || {}),
        el("div", { class: "toolbar" }, more.toggle),
        more.body,
      );
      const toggle = el(
        "button",
        {
          type: "button",
          class: "vtoggle",
          "aria-label": S.open(v.version),
          "aria-expanded": "false",
          onclick: () => {
            body.hidden = !body.hidden;
            toggle.setAttribute("aria-expanded", String(!body.hidden));
          },
        },
        icon("expand"),
        el("strong", {}, v.version),
        el("span", { class: "vdate" }, v.date),
        el("span", { class: "vname" }, v.name),
        v.kind === "corrections"
          ? el("span", { class: "chip on" }, S.correctionsChip)
          : null,
      );
      const build = buildButton(v.version, onBuild);
      buttons.push(build);
      return el(
        "li",
        { class: "vrow" },
        el("div", { class: "vrow-head" }, toggle, build),
        body,
      );
    }

    function show(list) {
      buttons.length = 0;
      const latest = list.find((v) => v.kind !== "corrections");
      const corrections = list.find((v) => v.kind === "corrections");
      cards.replaceChildren(
        ...[
          latest ? release(latest, S.latest(latest.version)) : null,
          corrections
            ? release(corrections, S.corrections(corrections.version))
            : null,
          original(),
        ].filter(Boolean),
      );
      history.replaceChildren(
        ...(list.length
          ? list.map(row)
          : [el("li", { class: "vnone" }, S.historyNone)]),
      );
      setReady(ready);
    }

    function setReady(on) {
      ready = on;
      for (const b of buttons) b.disabled = !on;
    }

    return { node, show, setReady };
  }

  return { create };
})();
