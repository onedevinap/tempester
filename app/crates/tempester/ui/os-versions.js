/* The version chooser of the OS update page, one group per OS (Main, Voice; Panel and SAM join the `offered` list of `show`
   later) with a filter on top, every group, card and history foldable (state kept across new lists), and
   the group of the Main OS holds: three recommended cards (the latest release, the corrections-only
   version, back to the original DSI software) and, below, the History of every release, newest first, to read only:
   the author offers the latest and the corrections-only versions, not every older one (10/11/2026). Each card and
   each history row shows the one-line summary and the changes as short lists (fixes, additions, changes), with a
   Details toggle for the technical text; each can be built. The list comes from the manifests (`os_versions`), already
   sorted newest first, with `kind` "release" or "corrections". Building needs the user's DSI file: `setReady(false)`
   disables every build button. The Voice group (`voice_versions` of the same event, newest first, with the same
   history rows labelled Voice) offers the Tempester Voice OS with cards of the same shape as the Main ones (short
   title, date chip, Build in the header, summary, change lists, Details): it is built from the user's own DSI Voice
   1.5 file, a different file, so `setVoiceReady` enables its build buttons apart. What is true of every Voice version
   is said once, under the group header: the note (all six chips at once, the full backups) and what the chips report
   running. Two versions that answer the same version word (1.5.1.0 and 1.5.1.1) cannot be told apart by the chips:
   that line names both and no card claims to be the one installed. A Voice version of `kind` "superseded" (1.5.1.0,
   replaced by 1.5.1.1 with the same fixes: author, 15/11/2026) keeps its history row, marked, and gets no card: it
   is no longer offered (Rust refuses to build or send it too). */
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
      "The original DSI software of the Main, rebuilt from your own file. Voice chips on Tempester Voice keep it until you send the official Voice file.",
    originalDetails:
      "Prepares your own Tempest_Main_1.5.0.2.syx, whose SHA-256 is checked, to be sent as it is. Only the Main is changed: the Panel and SAM software were never touched, and the Voice chips keep the Voice OS they run. To put DSI Voice 1.5 back, check your Tempest_Voice_1.5.syx with « Choose an OS .syx file to check » and send it.",
    historyTitle: "History",
    historyLead:
      "Every Tempester release, newest first, to read: open one for what it changed since the release before it. Only the latest and the corrections-only versions above can be built.",
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
    supersededChip: "Superseded",
    previous: (version) => "Previous (" + version + ")",
    voiceInstalledChip: (count, of) => "Installed on " + count + "/" + of + " chips",
    voiceNotReported: "Installed: not reported yet (Tempester 1.6.7.0 and later report the Voice chips' versions).",
    voiceNone: "Installed: no chip runs Tempester Voice.",
    voiceOne: (count, of, version) =>
      "Installed on " + count + "/" + of + " chips: Tempester Voice " + version + ".",
    voiceShared: (count, of, versions) =>
      "Installed on " + count + "/" + of + " chips: Tempester Voice " + versions.join(" or ") +
      ". They answer the same version word, so the chips cannot say which of them runs.",
    voiceNote:
      "The Voice OS of the six sound chips, built from your own Tempest_Voice_1.5.syx (step 1). One update goes to all six chips at once. Make a full backup before and after (before Tempester 1.6.8.0, power the machine off and on between the two).",
    buildVoiceLabel: (version) => "Build Tempester Voice " + version,
    filterLabel: "Show",
    filterAll: "All",
    filterAllLabel: "Show every OS",
    filterOnlyLabel: (os) => "Show the " + os + " only",
    foldGroup: (os) => os + ": show or hide",
    foldCard: (title) => "Show or hide " + title,
    foldHistory: (os) => "History of the " + os + ": show or hide",
    count: (cards, releases) =>
      cards + (cards === 1 ? " choice, " : " choices, ") + releases + (releases === 1 ? " release" : " releases"),
    historyOf: (os) => "History, " + os,
  };

  /* The OSes the page can offer, in the order they are shown. Panel and SAM join this list when the app can build
     them: a group, a filter button and a history appear with the entry, nothing else changes. */
  const MAIN = { id: "main", title: "Main OS", short: "Main", detail: "" };
  const VOICE = {
    id: "voice",
    title: "Voice OS",
    short: "Voice",
    detail: "Voice ",
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

  function create({ onBuild, onOriginal, onBuildVoice }) {
    const buttons = [];
    const voiceButtons = [];
    let ready = false,
      voiceReady = false;
    /* The chips that reported a Tempester Voice version, as Rust counted them (`voice_installed`), or null when the
       machine did not report them: the Voice group's line says what runs, a card's badge only an unshared version. */
    let voiceInstalled = null,
      lastLists = null;
    /* What the author folded and which OS the filter shows, kept for this chooser: a new list from Rust (every
       `os_versions` event) redraws the groups without opening what was closed. */
    const memory = { filter: "all", collapsed: {} };
    const filterBar = el("div", {
      class: "seg",
      role: "group",
      "aria-label": S.filterLabel,
    });
    const groupsBox = el("div", { class: "vgroups" });
    const node = el("div", { class: "vchooser" }, filterBar, groupsBox);

    /* A toggle that shows or hides `target` and remembers it under `key`; `makeToggle(flip)` builds the button. */
    function foldable(key, target, makeToggle) {
      let toggle = null;
      const sync = () => {
        const open = !memory.collapsed[key];
        target.hidden = !open;
        toggle.setAttribute("aria-expanded", String(open));
      };
      toggle = makeToggle(() => {
        memory.collapsed[key] = !memory.collapsed[key];
        sync();
      });
      sync();
      return toggle;
    }

    function card({
      title,
      chip,
      badge,
      summary,
      groups,
      details,
      version,
      action,
      os = MAIN,
      bucket = buttons,
    }) {
      const more = detailsBlock(title, details);
      const build = action || buildButton(version, onBuild, "primary");
      bucket.push(build);
      const content = el(
        "div",
        { class: "vcard-body" },
        el("p", { class: "vsummary" }, summary),
        groups,
        el("div", { class: "toolbar" }, more.toggle),
        more.body,
      );
      const fold = foldable("card:" + os.id + ":" + title, content, (flip) =>
        button("expand", S.foldCard(title), flip),
      );
      return el(
        "section",
        { class: "vcard", "aria-label": title },
        el(
          "div",
          { class: "vcard-head" },
          el("h4", {}, title),
          chip ? el("span", { class: "chip" }, chip) : null,
          badge ? el("span", { class: "chip on" }, badge) : null,
          /* The Build button lives in the header row, not in the body that folds: a folded card still builds. */
          build,
          fold,
        ),
        content,
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

    /* A Tempester Voice version, as a Main release card: the newest is the latest, the others come before it (the list
       is newest first). Built from the DSI Voice file, sent to the six chips at once (the group's note says it). */
    function voice(v, index) {
      const label = S.buildVoiceLabel(v.version);
      const action = button("build", label, () => onBuildVoice(v.version), {
        variant: "primary",
        text: S.build,
      });
      return card({
        title: index === 0 ? S.latest(v.version) : S.previous(v.version),
        chip: v.date,
        badge: voiceBadge(v.version),
        summary: v.summary,
        groups: groupList(v.changes || {}),
        details: v.description,
        action,
        os: VOICE,
        bucket: voiceButtons,
      });
    }

    /* What the six chips report, once for the Voice group: one sentence per version word a Tempester Voice version
       answers (a shared word names every version that answers it). */
    function voiceLead() {
      return el(
        "div",
        { class: "vlead" },
        el("p", { class: "vnote" }, S.voiceNote),
        el("p", { class: "vstatus" }, voiceStatus()),
      );
    }

    function voiceStatus() {
      if (!voiceInstalled) return S.voiceNotReported;
      const said = new Set();
      const lines = [];
      for (const found of voiceInstalled) {
        const shared = found.same_word || [];
        if (found.count <= 0 || said.has(found.version)) continue;
        if (shared.length > 1) {
          shared.forEach((v) => said.add(v));
          lines.push(S.voiceShared(found.count, found.of, shared));
        } else {
          said.add(found.version);
          lines.push(S.voiceOne(found.count, found.of, found.version));
        }
      }
      return lines.length ? lines.join(" ") : S.voiceNone;
    }

    /* One history row: a toggle line (version, date, name); the changes open under it. No build button: the history
       is read only. */
    function row(v, os) {
      const body = el("div", { class: "vbody", hidden: true });
      const more = detailsBlock(os.detail + v.version, v.description);
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
          "aria-label": S.open(os.detail + v.version),
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
        v.kind === "superseded"
          ? el("span", { class: "chip" }, S.supersededChip)
          : null,
        el("span", { class: "chip vos-chip" }, os.short),
      );
      return el(
        "li",
        { class: "vrow" },
        el("div", { class: "vrow-head" }, toggle),
        body,
      );
    }

    const historyRow = (os) => (v) => row(v, os);

    /* The History heading of one OS: a fold toggle (the list is the long part of the page) and the explanation. */
    function historyHead(os, list) {
      const fold = foldable("history:" + os.id, list, (flip) =>
        el(
          "button",
          {
            type: "button",
            class: "vfold",
            "aria-label": S.foldHistory(os.title),
            onclick: flip,
          },
          icon("expand"),
          el("h4", {}, S.historyOf(os.short)),
        ),
      );
      return el("div", { class: "vhead" }, fold, info(S.historyLead, S.historyTitle));
    }

    /* One OS: its header (a fold toggle with the count of what it offers), what is said once for all its choices, its
       choices, its history. */
    function osGroup({ os, lead, cards, rows, releases }) {
      const history = el("ul", { class: "vhistory" }, rows);
      const content = el(
        "div",
        { class: "vos-body" },
        lead,
        el("div", { class: "vcards" }, cards),
        historyHead(os, history),
        history,
      );
      const fold = foldable("group:" + os.id, content, (flip) =>
        el(
          "button",
          {
            type: "button",
            class: "vfold vos-fold",
            "aria-label": S.foldGroup(os.title),
            onclick: flip,
          },
          icon("expand"),
          el("h3", {}, os.title),
          el("span", { class: "vdate" }, S.count(cards.length, releases)),
        ),
      );
      return el(
        "section",
        { class: "vos", "aria-label": os.title, "data-os": os.id },
        fold,
        content,
      );
    }

    /* The filter shows every OS, or one: the groups of the others are hidden, not removed (their folds stay). */
    function applyFilter(groups) {
      for (const { os, node: group } of groups)
        group.hidden = memory.filter !== "all" && memory.filter !== os.id;
      for (const b of filterBar.children)
        b.setAttribute(
          "aria-pressed",
          String(b.getAttribute("data-filter") === memory.filter),
        );
    }

    function filterButton(id, text, label, groups) {
      return el(
        "button",
        {
          type: "button",
          "data-filter": id,
          "aria-label": label,
          "aria-pressed": "false",
          onclick: () => {
            memory.filter = id;
            applyFilter(groups);
          },
        },
        text,
      );
    }

    /* A card says Installed only when its version is the only one that answers the chips' word: with a shared word
       (1.5.1.0 and 1.5.1.1 both answer 0x16) the group line names both and no card claims it. */
    function voiceBadge(version) {
      const found = (voiceInstalled || []).find((v) => v.version === version);
      const shared = (found && found.same_word) || [];
      return found && found.count > 0 && shared.length < 2
        ? S.voiceInstalledChip(found.count, found.of)
        : null;
    }

    function show(list, voiceList = []) {
      lastLists = [list, voiceList];
      buttons.length = 0;
      voiceButtons.length = 0;
      const latest = list.find((v) => v.kind !== "corrections");
      const corrections = list.find((v) => v.kind === "corrections");
      const offered = [
        {
          os: MAIN,
          cards: [
            latest ? release(latest, S.latest(latest.version)) : null,
            corrections
              ? release(corrections, S.corrections(corrections.version))
              : null,
            original(),
          ].filter(Boolean),
          rows: list.length
            ? list.map(historyRow(MAIN))
            : [el("li", { class: "vnone" }, S.historyNone)],
          releases: list.length,
        },
        {
          os: VOICE,
          lead: voiceLead(),
          cards: voiceList.filter((v) => v.kind !== "superseded").map(voice),
          rows: voiceList.map(historyRow(VOICE)),
          releases: voiceList.length,
        },
      ].filter((group) => group.cards.length);
      if (!offered.some(({ os }) => os.id === memory.filter))
        memory.filter = "all";
      const groups = offered.map((group) => ({
        os: group.os,
        node: osGroup(group),
      }));
      groupsBox.replaceChildren(...groups.map(({ node: group }) => group));
      filterBar.replaceChildren(
        filterButton("all", S.filterAll, S.filterAllLabel, groups),
        ...groups.map(({ os }) =>
          filterButton(os.id, os.short, S.filterOnlyLabel(os.short), groups),
        ),
      );
      applyFilter(groups);
      setReady(ready);
      setVoiceReady(voiceReady);
    }

    function setReady(on) {
      ready = on;
      for (const b of buttons) b.disabled = !on;
    }

    function setVoiceReady(on) {
      voiceReady = on;
      for (const b of voiceButtons) b.disabled = !on;
    }

    function setVoiceInstalled(list) {
      voiceInstalled = Array.isArray(list) ? list : null;
      if (lastLists) show(...lastLists);
    }

    return { node, show, setReady, setVoiceReady, setVoiceInstalled };
  }

  return { create };
})();
