/* The line between the page and Rust. Page -> Rust: one line per command, `name<TAB>arg<TAB>arg`, binary data as
   hex. Rust -> page: `Tempester.bridge.receive({event: "...", ...})`, called by Rust with a JSON literal. */
"use strict";
window.Tempester = window.Tempester || {};
window.Tempester.bridge = (function () {
  const listeners = {};
  const timers = {};

  /* A tab or line break would split the line: text is made single-line (file names, names typed by the user). */
  function clean(arg) {
    return String(arg).replace(/[\t\r\n]+/g, " ");
  }

  function send(name, ...args) {
    window.ipc.postMessage([name, ...args.map(clean)].join("\t"));
  }

  /* A script error would otherwise go unseen inside the app's web view (no console): Rust writes it to stderr and
     the page shows it. */
  function reportError(text) {
    try {
      send("page_error", text);
    } catch (e) {
      /* no bridge: nothing more can be done */
    }
    for (const handler of listeners["page_error"] || []) handler({ text });
  }
  window.addEventListener("error", (e) =>
    reportError(`${e.message} (${e.filename}:${e.lineno}:${e.colno})`),
  );
  window.addEventListener("unhandledrejection", (e) =>
    reportError("unhandled rejection: " + String(e.reason)),
  );

  function on(event, handler) {
    (listeners[event] = listeners[event] || []).push(handler);
  }

  function receive(message) {
    for (const handler of listeners[message.event] || []) handler(message);
    for (const handler of listeners["*"] || []) handler(message);
  }

  /* A knob dragged fast makes many values: the first goes out at once, then at most one every `ms`, and the
     last value always goes out (last value wins). One timer per key (the parameter). */
  function sendThrottled(key, ms, name, ...args) {
    const slot = timers[key];
    if (!slot) {
      send(name, ...args);
      startWindow(key, ms);
      return;
    }
    slot.pending = [name, ...args];
  }
  function startWindow(key, ms) {
    timers[key] = {
      pending: null,
      id: setTimeout(() => endWindow(key, ms), ms),
    };
  }
  function endWindow(key, ms) {
    const pending = timers[key].pending;
    delete timers[key];
    if (pending) {
      send(...pending);
      startWindow(key, ms);
    }
  }

  function toHex(buffer) {
    return Array.from(new Uint8Array(buffer), (b) =>
      b.toString(16).padStart(2, "0"),
    ).join("");
  }

  /* The bytes of a file picked with <input type=file>, as {name, hex}. */
  async function readFileHex(file) {
    return { name: file.name, hex: toHex(await file.arrayBuffer()) };
  }

  return { send, on, receive, sendThrottled, readFileHex, clean, toHex };
})();
