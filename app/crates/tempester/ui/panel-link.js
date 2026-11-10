/* What the Panel page sends to the machine, one function per kind of control. Both drawings use it; nothing here
   knows which drawing is shown, so switching views cannot change what a control sends.

   Tempester.panelLink.create({bridge, connected, setTempo, remote}) ->
     knob(index, value)        sound parameter NRPN (Full view: always the pad selected on the machine)
     pad(pad, down)            pad 0-31 (A1-B16) pressed / released: Rust plays its note from the machine's pad note
                               table (pad-notes.js), the page never computes a note
     key(code, down)           remote panel key (CC 102 / 103)
     local(name, value)        BPM (app clock); Volume (control 8, the knob's 0-127 doubled); Distort and Compress as the
                               machine's own 0-255 knob: controls 9 and 2 when the remote panel answers (`remote()`),
                               else CC 12 / 13, which the stock OS reads as 0-127 (the value is cut at 127)
     transport(start)          MIDI Start / Stop
     control(index, value)     absolute control 0-9 of the remote panel, 0-255 (NRPN 220 + index)
     encoder(index, step)      encoder 0-13 turned by `step` detents, -16..15 (NRPN 200 + index, relative)
     beatWide(index, position) beat-wide parameter 0-7 at 0-254 (n°29: CC 20-27 + LSB 52-59)
     ribbon(r, position, pressure)  FX ribbon 0-1 touched at 0-127 (pressure 0-127 or undefined), or released
                               (position null) (n°31: CC 108-113); touch and release share one throttle, so that
                               a release is never overtaken by an older touch
     probability(pad, percent) play probability of pad 0-15 (n°36: the pad's press, then CC 14), once per gesture */
"use strict";
window.Tempester = window.Tempester || {};
window.Tempester.panelLink = (function () {
  const NRPN_THROTTLE_MS = 30;
  const CC_OF_LOCAL = { distort: 12, compress: 13 };
  const CONTROL_OF_LOCAL = { distort: 9, compress: 2 };
  const CC_MAX = 127;
  /* Volume is the absolute control 8: 0-255, of which the machine uses the upper 7 bits, so the knob's 0-127 goes
     out doubled. */
  const VOLUME_CONTROL = 8;
  const CONTROL_PER_KNOB_STEP = 2;
  const PAD_VELOCITY = 100;

  function create({ bridge, connected, setTempo, remote = () => false }) {
    const throttled = (key, ...command) =>
      bridge.sendThrottled(key, NRPN_THROTTLE_MS, ...command);
    const control = (index, value) => {
      if (connected()) throttled("control" + index, "control", index, value);
    };
    return {
      knob(index, value) {
        if (connected()) throttled("nrpn" + index, "nrpn", index, value);
      },
      pad(pad, down) {
        if (!connected()) return;
        bridge.send("pad", down ? "on" : "off", pad, down ? PAD_VELOCITY : 0);
      },
      key(code, down) {
        if (connected()) bridge.send("key", down ? "press" : "release", code);
      },
      local(name, value) {
        if (name === "bpm") {
          setTempo(value, "panel");
          return;
        }
        if (!connected()) return;
        if (name === "volume") {
          throttled(
            "control" + VOLUME_CONTROL,
            "control",
            VOLUME_CONTROL,
            value * CONTROL_PER_KNOB_STEP,
          );
          return;
        }
        if (remote()) {
          control(CONTROL_OF_LOCAL[name], value);
          return;
        }
        const controller = CC_OF_LOCAL[name];
        throttled("cc" + controller, "cc", controller, Math.min(value, CC_MAX));
      },
      transport(start) {
        if (connected()) bridge.send("transport", start ? "start" : "stop");
      },
      control,
      beatWide(index, position) {
        if (connected())
          throttled("beat_wide" + index, "beat_wide", index, position);
      },
      ribbon(r, position, pressure) {
        if (!connected()) return;
        if (position === null) throttled("ribbon" + r, "ribbon", r, "release");
        else if (pressure === undefined)
          throttled("ribbon" + r, "ribbon", r, "touch", position);
        else throttled("ribbon" + r, "ribbon", r, "touch", position, pressure);
      },
      /* Not throttled: one message per gesture, and each one presses the pad. */
      probability(pad, percent) {
        if (connected()) bridge.send("pad_probability", pad, percent);
      },
      /* Never throttled: relative steps add up on the machine, a dropped one is a lost detent. */
      encoder(index, step) {
        if (connected() && step !== 0) bridge.send("encoder", index, step);
      },
    };
  }

  return { create, NRPN_THROTTLE_MS };
})();
