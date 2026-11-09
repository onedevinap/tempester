/* The remote panel of Tempester 1.6.0.0 on the drawn machine: the keys press and release by key code, the LEDs of the
   machine light the keys and the pads, and everything stays greyed until Rust reports that the machine answered the
   LED probe (the identity reply cannot tell 1.6.0.0 from other versions).

   Tempester.panelRemote.attach(root, onKey)   binds every [data-key] button and [data-led] pad; onKey(code, down)
                                               goes to Rust. Called once per drawing (Full, Machine): the keys and
                                               pads of every drawing share the LED mirror and the availability.   Tempester.panelRemote.setAvailable(on)      probe result: enables or greys the keys
   Tempester.panelRemote.setLeds(changes)      [[index, 0|1], ...] from the machine
   Tempester.panelRemote.releaseAll()          releases every held key
   Tempester.panelRemote.grey(node, message) / ungrey(node)

   A key still down on the machine when its release is lost stays down: every way a press can end (pointer up, leave
   or cancel, key up, focus lost, window blur, hidden page, closing page, probe lost) releases it. Rust releases the
   held keys again when the port closes and when the app exits. */
"use strict";
window.Tempester = window.Tempester || {};
window.Tempester.panelRemote = (function () {
  const NEEDS =
    "Needs Tempester 1.6.0.0 and Remote Pad OUT not Off: the remote panel did not answer";
  const keys = [];
  const pads = [];
  const held = new Set();
  const ledState = new Map();
  let available = false;
  let listening = false;
  let sendKey = () => {};

  function box(node) {
    return node.closest(".bt, .tb, .pot") || node;
  }
  function isButton(node) {
    return node.tagName.toLowerCase() === "button";
  }
  function grey(node, message) {
    box(node).classList.add("inactif");
    box(node).title = message;
    node.title = message;
    node.setAttribute("aria-disabled", "true");
    if (!isButton(node)) node.removeAttribute("tabindex");
  }
  function ungrey(node, message = "") {
    box(node).classList.remove("inactif");
    box(node).title = message;
    node.title = message;
    node.removeAttribute("aria-disabled");
    if (!isButton(node)) node.setAttribute("tabindex", "0");
  }

  function press(key) {
    if (!available || held.has(key.code)) return;
    held.add(key.code);
    sendKey(key.code, true);
  }
  function release(key) {
    if (!held.has(key.code)) return;
    held.delete(key.code);
    sendKey(key.code, false);
  }
  function releaseAll() {
    const codes = Array.from(held);
    held.clear();
    codes.forEach((code) => sendKey(code, false));
  }

  function bindPressing(key) {
    const b = key.button;
    b.addEventListener("pointerdown", (e) => {
      e.preventDefault();
      press(key);
    });
    for (const end of ["pointerup", "pointerleave", "pointercancel"])
      b.addEventListener(end, () => release(key));
    b.addEventListener("keydown", (e) => {
      if ((e.key === "Enter" || e.key === " ") && !e.repeat) {
        e.preventDefault();
        press(key);
      }
    });
    b.addEventListener("keyup", (e) => {
      if (e.key === "Enter" || e.key === " ") release(key);
    });
    b.addEventListener("blur", () => release(key));
  }

  function attach(root, onKey) {
    sendKey = onKey;
    const table = new Map(window.Tempester.panelKeys.map((k) => [k[0], k]));
    root.querySelectorAll("[data-key]").forEach((button) => {
      const row = table.get(button.dataset.key);
      if (!row) throw new Error("panel: no key named " + button.dataset.key);
      const transport = !!button.closest("[data-transport]");
      const key = { button, code: row[1], leds: row[2], transport };
      keys.push(key);
      /* Play and Stop stay MIDI Start and Stop (they also work for a machine slaved to the app's clock); only the
         LED of Play is mirrored. */
      if (!transport) bindPressing(key);
    });
    root.querySelectorAll("[data-led]").forEach((button) => {
      pads.push({ button, led: +button.dataset.led });
    });
    if (!listening) {
      listening = true;
      window.addEventListener("blur", releaseAll);
      window.addEventListener("pagehide", releaseAll);
      document.addEventListener("visibilitychange", () => {
        if (document.hidden) releaseAll();
      });
    }
    setAvailable(available);
  }

  function showLeds() {
    for (const key of keys) {
      if (available && key.leds.length)
        key.button.setAttribute(
          "aria-pressed",
          key.leds.some((led) => ledState.get(led)),
        );
      else key.button.removeAttribute("aria-pressed");
    }
    for (const pad of pads) {
      if (ledState.has(pad.led))
        pad.button.setAttribute("aria-pressed", ledState.get(pad.led));
      else pad.button.removeAttribute("aria-pressed");
    }
  }

  function setAvailable(on) {
    available = on;
    if (!on) {
      releaseAll();
      ledState.clear();
    }
    for (const key of keys) {
      if (key.transport) continue;
      if (on) ungrey(key.button);
      else grey(key.button, NEEDS);
    }
    showLeds();
  }

  function setLeds(changes) {
    for (const [index, on] of changes) ledState.set(index, !!on);
    showLeds();
  }

  return { attach, setAvailable, setLeds, releaseAll, grey, ungrey, NEEDS };
})();
