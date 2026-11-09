/* The dialogs of the library manager: the review of a batch before anything is written (new / identical / modified
   with one choice per group), what a send to the machine would replace, name and tag prompts, the trash. Each one
   resolves with the user's choice, or null when dismissed (Escape counts as « no »). Built on `ask`, which the tests
   replace to play the user. Loaded before the browser. */
"use strict";
window.Tempester = window.Tempester || {};
window.Tempester.dialogs = (function () {
  const { el } = window.Tempester.kit;
  const icons = window.Tempester.icons;
  const model = window.Tempester.libmodel;

  /* A modal dialog made on the spot: `body` nodes, `buttons` [{label, value, variant}], the first one being the
     « no ». Resolves with the value of the button used, "cancel" for Escape. `onReady({dialog, buttons})` lets the
     caller enable or disable buttons while the user types. */
  function ask(spec) {
    const dialog = el("dialog", {
      class: "lib-dialog " + (spec.className || ""),
      "aria-label": spec.title,
    });
    const buttons = {};
    const row = el("div", { class: "buttons" });
    for (const b of spec.buttons) {
      buttons[b.value] = el(
        "button",
        {
          class: "btn " + (b.variant || ""),
          type: "submit",
          value: b.value,
          disabled: !!b.disabled,
        },
        b.label,
      );
      row.append(buttons[b.value]);
    }
    dialog.append(
      el(
        "form",
        { method: "dialog" },
        el("h2", {}, spec.title),
        el("div", { class: "lib-dialog-body" }, spec.body),
        row,
      ),
    );
    (document.body || document.getElementById("pages")).append(dialog);
    return new Promise((resolve) => {
      dialog.addEventListener(
        "close",
        () => {
          if (dialog.remove) dialog.remove();
          resolve(dialog.returnValue || "cancel");
        },
        { once: true },
      );
      dialog.returnValue = "cancel";
      dialog.showModal();
      if (spec.onReady) spec.onReady({ dialog, buttons });
    });
  }

  function paragraphs(lines) {
    return lines.map((line) => el("p", {}, line));
  }

  /* Yes / no with explanatory lines. Resolves true on the confirming button. */
  async function confirm({ title, lines, okLabel, danger, noLabel }) {
    const answer = await api.ask({
      title,
      body: paragraphs(lines),
      buttons: [
        { label: noLabel || "Cancel", value: "cancel" },
        { label: okLabel, value: "ok", variant: danger ? "danger" : "primary" },
      ],
    });
    return answer === "ok";
  }

  /* Only reads out: one OK button. */
  function inform({ title, lines }) {
    return api.ask({
      title,
      body: paragraphs(lines),
      buttons: [{ label: "OK", value: "ok", variant: "primary" }],
    });
  }

  /* A text field. `validate(text)` returns a reason (shown, OK disabled) or null. `options` fill a suggestion list.
     Resolves with the text, or null. */
  async function prompt({
    title,
    lines,
    label,
    value,
    okLabel,
    validate,
    options,
  }) {
    const input = el("input", {
      type: "text",
      class: "lib-prompt",
      "aria-label": label,
      list: options && options.length ? "lib-prompt-options" : null,
    });
    input.value = value || "";
    const why = el("p", { class: "lib-why", role: "alert" });
    const datalist =
      options && options.length
        ? el(
            "datalist",
            { id: "lib-prompt-options" },
            options.map((o) => el("option", { value: o })),
          )
        : null;
    let buttons = null;
    const check = () => {
      const reason = validate ? validate(input.value) : null;
      why.textContent = input.value && reason ? reason : "";
      if (buttons) buttons.ok.disabled = !!reason;
    };
    input.addEventListener("input", check);
    const answer = await api.ask({
      title,
      body: [
        ...paragraphs(lines || []),
        el("label", {}, label),
        input,
        datalist,
        why,
      ],
      buttons: [
        { label: "Cancel", value: "cancel" },
        { label: okLabel, value: "ok", variant: "primary", disabled: true },
      ],
      onReady: (refs) => {
        buttons = refs.buttons;
        check();
        if (input.focus) input.focus();
      },
    });
    return answer === "ok" ? input.value : null;
  }

  /* ---- the review of a batch ---------------------------------------------------------------------------------- */

  const GROUPS = [
    {
      key: "new",
      verdict: "new",
      title: (n) => n + " new",
      hint: "Not in the library yet.",
      choices: [
        ["write", "Save them"],
        ["skip", "Do not save them"],
      ],
    },
    {
      key: "identical",
      verdict: "identical",
      title: (n) => n + " identical",
      hint: "Same data as something already in the library, whatever its name: nothing new to keep.",
      choices: [
        ["skip", "Skip them (nothing is written)"],
        ["copy", "Keep a second copy of each"],
      ],
    },
    {
      key: "modified",
      verdict: "modified",
      title: (n) => n + " modified",
      hint: "Same name or slot as an item in the library, with other data: a newer or older version of it.",
      choices: [
        ["update", "Update: the old version goes to its history"],
        ["new", "Save as new items, keep the old ones as they are"],
        ["skip", "Skip them: the library keeps what it has"],
      ],
    },
  ];

  function entryLine(entry, group) {
    if (group.verdict === "new") return entry.label;
    if (entry.in_batch)
      return (
        entry.label + ": identical to " + entry.name + " earlier in this batch"
      );
    if (group.verdict === "identical")
      return (
        entry.label +
        ': already in the library as "' +
        entry.name +
        '" (' +
        entry.kind +
        "/" +
        entry.place +
        ", " +
        model.formatDate(entry.existing_date) +
        ")"
      );
    return (
      entry.label +
      ': library "' +
      entry.name +
      '" of ' +
      model.formatDate(entry.existing_date) +
      ", incoming of " +
      model.formatDate(entry.candidate_date) +
      " (" +
      (entry.newer
        ? "incoming is NEWER"
        : "incoming is OLDER: it would replace a newer version") +
      ")"
    );
  }

  function groupSection(plan, group, selects) {
    const entries = plan.entries.filter((e) => e.verdict === group.verdict);
    const select = el(
      "select",
      { "aria-label": "What to do with the " + group.key + " items" },
      group.choices.map(([value, text]) => el("option", { value }, text)),
    );
    select.value = group.choices[0][0];
    selects[group.key] = select;
    const list = el(
      "ul",
      { class: "lib-entries", hidden: true },
      entries.map((e) =>
        el(
          "li",
          { class: e.verdict === "modified" && !e.newer ? "older" : "" },
          entryLine(e, group),
        ),
      ),
    );
    const toggle = icons.button(
      "chevron-down",
      "Show the " + entries.length + " item(s)",
      () => {
        list.hidden = !list.hidden;
      },
    );
    return el(
      "section",
      { class: "lib-group " + group.key },
      el(
        "div",
        { class: "lib-group-head" },
        el("strong", {}, group.title(entries.length)),
        icons.info(group.hint, "What is this group"),
        select,
        toggle,
      ),
      list,
    );
  }

  /* Resolves with {new, identical, modified} (the choices, as `lib_apply` takes them), or null for cancel. */
  async function review(plan) {
    const selects = {};
    const shown = GROUPS.filter((g) => plan[g.key] > 0);
    const body = [
      el(
        "p",
        { class: "lib-summary" },
        shown.map((g) =>
          el("span", { class: "chip " + g.key }, g.title(plan[g.key])),
        ),
      ),
      ...shown.map((g) => groupSection(plan, g, selects)),
      el(
        "p",
        { class: "mid" },
        "Nothing is written until you apply. Replaced versions are kept in each item's history.",
      ),
    ];
    const answer = await api.ask({
      title: plan.title,
      className: "wide",
      body,
      buttons: [
        { label: "Cancel: write nothing", value: "cancel" },
        { label: "Apply", value: "ok", variant: "primary" },
      ],
    });
    if (answer !== "ok") return null;
    const pick = (key, fallback) =>
      selects[key] ? selects[key].value : fallback;
    return {
      new: pick("new", "write"),
      identical: pick("identical", "skip"),
      modified: pick("modified", "update"),
    };
  }

  /* ---- sending to the machine -------------------------------------------------------------------------------- */

  /* What the machine holds and what the send replaces. Resolves true when the user confirms (never for
     `identical`: there is nothing to send). */
  async function sendPlan(plan) {
    if (plan.status === "identical") {
      await inform({ title: "Nothing to send", lines: plan.lines });
      return false;
    }
    const risky = plan.status === "no_answer";
    return confirm({
      title: risky ? "No backup is possible" : "Send to the machine?",
      lines: [
        '"' +
          plan.item +
          '" (' +
          plan.from +
          ") goes to the machine as its current " +
          plan.kind.replace(/s$/, "") +
          ".",
        ...plan.lines,
      ],
      okLabel: risky ? "Send without a backup" : "Send",
      danger: true,
      noLabel: "Do not send",
    });
  }

  /* ---- sending a whole project to the machine ---------------------------------------------------------------- */

  const PROJECT_GUARD_NOTE = {
    no_risk:
      "No Play List of this project can be misread by the machine's OS (it uses none, or only Play List 4): no warning is needed.",
    fixed:
      "The OS the app recorded as installed includes the Play List fix: no warning is needed.",
  };

  /* What the machine holds and what sending a whole project replaces. Resolves with the mode the user chose
     (`normal`, `anyway`, `no_lists`, as `lib_project_send` takes it), or null. Never asks for `identical`. When the
     OS may misread Play Lists the warning is shown in full and the plain « Send » is not offered: only « Send
     anyway » and « Send without Play Lists » (the four lists go empty: safe, but the machine's own lists go). */
  async function projectPlan(plan) {
    if (plan.status === "identical") {
      await inform({ title: "Nothing to send", lines: plan.lines });
      return null;
    }
    const risky = plan.guard === "at_risk";
    const silent = plan.status === "no_answer";
    const without = silent ? " (no backup)" : "";
    const lines = [
      '"' +
        plan.item +
        '" (' +
        plan.from +
        ") goes to the machine as its whole project.",
      ...plan.lines,
    ];
    if (PROJECT_GUARD_NOTE[plan.guard]) lines.push(PROJECT_GUARD_NOTE[plan.guard]);
    const warning = risky
      ? el(
          "div",
          { class: "lib-warning", role: "alert" },
          paragraphs([...plan.warning, ...plan.without_lists]),
        )
      : null;
    const buttons = [{ label: "Do not send", value: "cancel" }];
    if (risky) {
      buttons.push(
        { label: "Send anyway" + without, value: "anyway", variant: "danger" },
        {
          label: "Send without Play Lists" + without,
          value: "no_lists",
          variant: "danger",
        },
      );
    } else {
      buttons.push({
        label: silent ? "Send without a backup" : "Send",
        value: "normal",
        variant: "danger",
      });
    }
    const answer = await api.ask({
      title: risky
        ? "The machine's OS may misread Play Lists"
        : silent
          ? "No backup is possible"
          : "Send the project to the machine?",
      className: "wide",
      body: [...paragraphs(lines), warning],
      buttons,
    });
    return answer === "cancel" ? null : answer;
  }

  const MAX_DIFFERENCES = 30;

  /* The answer of « Fetch it back and compare »: the machine holds what was sent, or exactly what differs. */
  function projectVerify(result) {
    const shown = (result.differences || []).slice(0, MAX_DIFFERENCES);
    const more = (result.differences || []).length - shown.length;
    return inform({
      title:
        result.state === "match"
          ? "The machine holds what was sent"
          : result.state === "differ"
            ? "The machine does NOT hold what was sent"
            : "No answer from the machine",
      lines: [
        ...result.lines,
        ...shown,
        ...(more > 0 ? ["... and " + more + " more difference(s)."] : []),
        ...(result.notes || []),
      ],
    });
  }

  /* ---- the trash ---------------------------------------------------------------------------------------------- */

  /* The trashed items with a restore button each, and « Empty the trash » (asked again, with the count).
     `actions` = {restore(id), empty(count)}. */
  async function trash(items, actions) {
    const rows = items.map((t) =>
      el(
        "li",
        { class: "lib-trash-row" },
        el(
          "span",
          {},
          t.name +
            " (" +
            t.kind +
            "/" +
            t.path +
            ", deleted " +
            model.formatDate(t.trashed) +
            ")",
        ),
        icons.button("restore", "Restore " + t.name, () =>
          actions.restore(t.id),
        ),
      ),
    );
    const answer = await api.ask({
      title: "Trash (" + items.length + ")",
      className: "wide",
      body: items.length
        ? [
            el(
              "p",
              { class: "mid" },
              "Deleted items wait here. Nothing is deleted for good until you empty the trash.",
            ),
            el("ul", { class: "lib-entries" }, rows),
          ]
        : [el("p", {}, "The trash is empty.")],
      buttons: [
        { label: "Close", value: "cancel" },
        ...(items.length
          ? [{ label: "Empty the trash...", value: "empty", variant: "danger" }]
          : []),
      ],
    });
    if (answer !== "empty") return;
    const sure = await confirm({
      title: "Empty the trash for good?",
      lines: [
        items.length +
          " item(s) and their history will be deleted for good. This cannot be undone.",
      ],
      okLabel: "Delete " + items.length + " item(s) for good",
      danger: true,
    });
    if (sure) actions.empty(items.length);
  }

  const api = {
    ask,
    confirm,
    inform,
    prompt,
    review,
    sendPlan,
    projectPlan,
    projectVerify,
    trash,
  };
  return api;
})();
