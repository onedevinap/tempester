/* Small helpers shared by the pages: DOM building, toast, confirmation dialog, formats, error wording, and the
   registry the pages add themselves to. */
"use strict";
window.Tempester = window.Tempester || {};
window.Tempester.kit = (function () {
  const STRINGS = {
    ok: "OK",
    cancel: "Cancel",
    errors: {
      not_connected:
        "Not connected: connect to the Tempest on the Machine page first.",
      timeout:
        'No answer from the machine in time. Check the cable choice (System > "MIDI: Sysex IN+OUT Cable") and that the Tempest runs a Tempester OS (1.5.2.0 or later) for sound and beat requests.',
      busy: "Another operation is running (an OS update or a clock).",
      precondition: "",
      invalid: "",
      io: "",
      midi: "",
      cancelled: "Cancelled.",
    },
  };

  /* el("div", {class: "x", onclick: fn}, child, "text", ...) */
  function el(tag, attrs, ...children) {
    const node = document.createElement(tag);
    for (const [key, value] of Object.entries(attrs || {})) {
      if (key.startsWith("on")) node.addEventListener(key.slice(2), value);
      else if (value === true) node.setAttribute(key, "");
      else if (value !== false && value !== null && value !== undefined)
        node.setAttribute(key, value);
    }
    for (const child of children.flat()) {
      if (child !== null && child !== undefined && child !== false)
        node.append(child);
    }
    return node;
  }

  let toastTimer = null;
  function toast(text) {
    const box = document.getElementById("toast");
    box.textContent = text;
    box.classList.add("visible");
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => box.classList.remove("visible"), 4200);
  }

  /* Asks the user; resolves true on OK. `lines` are paragraphs (text only). */
  function confirmDialog({ title, lines, okLabel, danger }) {
    const dialog = document.getElementById("confirm");
    document.getElementById("confirm-title").textContent = title;
    const body = document.getElementById("confirm-body");
    body.replaceChildren(...lines.map((line) => el("p", {}, line)));
    const ok = document.getElementById("confirm-ok");
    ok.textContent = okLabel || STRINGS.ok;
    ok.classList.toggle("danger", !!danger);
    return new Promise((resolve) => {
      dialog.addEventListener(
        "close",
        () => resolve(dialog.returnValue === "ok"),
        { once: true },
      );
      dialog.returnValue = "cancel";
      dialog.showModal();
      document.getElementById("confirm-cancel").focus();
    });
  }

  function formatBytes(n) {
    if (n < 1024) return n + " B";
    if (n < 1024 * 1024) return (n / 1024).toFixed(1) + " KB";
    return (n / 1024 / 1024).toFixed(2) + " MB";
  }

  function formatDuration(seconds) {
    if (seconds < 90) return "about " + seconds + " s";
    return "about " + Math.round(seconds / 60) + " min";
  }

  /* The sentence shown for an `error` event: the page's wording for the code, else the technical message. */
  function errorText(message) {
    return STRINGS.errors[message.code] || message.message;
  }

  /* A button that opens the file picker and hands the picked file's {name, hex} to `onFile`. `accept`: file
     extensions; `iconName`: optional icon (icons.js) that turns it into an icon button keeping its text. */
  function filePicker(label, onFile, accept, iconName) {
    const input = el("input", {
      type: "file",
      accept: accept || ".syx",
      class: "hidden-input",
      tabindex: "-1",
    });
    input.addEventListener("change", async () => {
      const file = input.files[0];
      input.value = "";
      if (file) onFile(await window.Tempester.bridge.readFileHex(file));
    });
    const open = () => input.click();
    /* With an icon name the picker is an icon button that keeps its text (the file to choose is named in it). */
    const button = iconName
      ? window.Tempester.icons.button(iconName, label, open, { text: label })
      : el("button", { class: "btn", type: "button", onclick: open }, label);
    return el("span", {}, button, input);
  }

  /* What every page needs to know about the link; app.js updates it from the `conn` and `state` events. */
  const session = { connected: false, fake: false, channel: 1, port: null };
  const sessionListeners = [];
  function onSession(listener) {
    sessionListeners.push(listener);
  }
  function updateSession(patch) {
    Object.assign(session, patch);
    sessionListeners.forEach((listener) => listener(session));
  }

  /* The tempo of the app's clock (BPM), shared by the Play page and the panel's BPM knob. `source` names who
     changed it, so that each one only updates the others. */
  const tempo = { bpm: 120, min: 20, max: 300 };
  const tempoListeners = [];
  function onTempo(listener) {
    tempoListeners.push(listener);
  }
  function setTempo(bpm, source) {
    const next = Math.max(tempo.min, Math.min(tempo.max, Math.round(bpm)));
    if (next === tempo.bpm) return;
    tempo.bpm = next;
    tempoListeners.forEach((listener) => listener(next, source));
  }

  /* Pages register themselves: {id, title, build(root), onShow?()} ; app.js mounts them. */
  const pages = [];
  function registerPage(page) {
    pages.push(page);
  }

  /* Going to a page from another one (the identity chip in the top bar opens the Machine page): app.js owns the tabs
     and registers how to switch. */
  let pageSwitcher = null;
  function setPageSwitcher(fn) {
    pageSwitcher = fn;
  }
  function showPage(id) {
    if (pageSwitcher) pageSwitcher(id);
  }

  return {
    el,
    toast,
    confirmDialog,
    formatBytes,
    formatDuration,
    errorText,
    filePicker,
    session,
    onSession,
    updateSession,
    tempo,
    onTempo,
    setTempo,
    pages,
    registerPage,
    setPageSwitcher,
    showPage,
  };
})();
