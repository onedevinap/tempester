/* What happens between the dialogs and Rust for the library manager: a batch to review (`lib_plan`) is shown and
   answered, a send to the machine (`lib_send_plan`) is confirmed or not, every `lib_done` and problem goes to a
   short log the page shows (what was done, where, how to undo), and file imports go one at a time (Rust holds one
   review at once). Loaded after the dialogs, before the browser and the cards. */
"use strict";
window.Tempester = window.Tempester || {};
window.Tempester.libflow = (function () {
  const { toast, errorText } = window.Tempester.kit;
  const bridge = window.Tempester.bridge;
  const dialogs = window.Tempester.dialogs;

  const MAX_LOG = 40;
  const log = [];
  const listeners = [];
  let waiter = null;

  function add(entry) {
    log.unshift(entry);
    if (log.length > MAX_LOG) log.pop();
    listeners.forEach((fn) => fn(log));
    if (waiter) {
      const resolve = waiter;
      waiter = null;
      resolve();
    }
  }

  function short(text) {
    return text.length > 150 ? text.slice(0, 147) + "..." : text;
  }

  bridge.on("lib_plan", async (plan) => {
    const choice = await dialogs.review(plan);
    if (choice)
      bridge.send("lib_apply", choice.new, choice.identical, choice.modified);
    else bridge.send("lib_cancel");
  });

  bridge.on("lib_send_plan", async (plan) => {
    if (await dialogs.sendPlan(plan))
      bridge.send("lib_send_apply", plan.kind, plan.id, plan.token);
  });

  /* A kit (library sounds on the pads of the machine's beat): the sounds asked for are kept here, because the check
     answers with what it saw on the machine, and the apply needs both. */
  let kitIds = "";
  bridge.on("lib_kit_plan", async (plan) => {
    if (await dialogs.sendPlan(plan))
      bridge.send("lib_kit_apply", kitIds, plan.token);
  });

  bridge.on("lib_project_plan", async (plan) => {
    const mode = await dialogs.projectPlan(plan);
    if (mode)
      bridge.send(
        "lib_project_send",
        plan.id,
        plan.version === null ? "-" : plan.version,
        plan.token,
        mode,
      );
  });

  bridge.on("project_verify", (m) => dialogs.projectVerify(m));

  bridge.on("lib_done", (m) => {
    add({ title: m.title, lines: m.lines, bad: false });
    if (m.lines.length) toast(short(m.lines[0]));
  });

  bridge.on("lib_report", (m) => {
    const lines = [];
    if (m.damaged)
      lines.push(
        "The library index was damaged: it was set aside (" +
          m.damaged +
          ") and rebuilt from the files.",
      );
    for (const [key, text] of [
      ["added", "Found in the folder and added"],
      ["dropped", "File missing, entry dropped"],
      ["changed", "Changed outside the app"],
      ["unreadable", "Not a Tempest file here, left alone"],
    ])
      for (const what of m[key] || []) lines.push(text + ": " + what);
    if (lines.length)
      add({ title: "Library check (" + m.kind + ")", lines, bad: false });
  });

  bridge.on("error", (m) => {
    if (m.during !== "lib") return;
    add({ title: "Not done", lines: [errorText(m)], bad: true });
    toast(short(errorText(m)));
  });

  /* Imports the picked files one after the other: each one may open a review that must be answered first. */
  async function importFiles(files) {
    for (const file of files) {
      await new Promise((resolve) => {
        waiter = resolve;
        bridge.send("lib_import", file.name, file.hex);
      });
    }
  }

  /* Asks Rust to look at the machine before sending `id` (an item) or the card's opened file (`id` empty). */
  function sendCheck(kind, id) {
    bridge.send("lib_send_check", kind, id || "");
  }

  /* Asks Rust to look at the machine's beat before these sounds (`ids`: 32 entries, an item id or `_` per pad)
     replace those of its pads. */
  function kitCheck(ids) {
    kitIds = ids;
    bridge.send("lib_kit_check", ids);
  }

  /* Asks Rust to look at the machine's project before a project (an item, or one version of it) replaces it. */
  function projectCheck(id, version) {
    bridge.send("lib_project_check", id, version === null || version === undefined ? "-" : version);
  }

  function onLog(fn) {
    listeners.push(fn);
  }

  return { log, onLog, importFiles, sendCheck, projectCheck, kitCheck };
})();
