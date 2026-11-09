/* The machine's Pad Function mode, read from the LEDs it sends (Tempester 1.6.0.0, n°22). The firmware keeps the mode
   in bits 2-5 of one RAM byte: 0 16 Beats, 1 16 Time Steps, 2 16 Sounds, 3 16 Tunings, 4 16 Mutes, 5 16 Levels.
   The four mode LEDs tell it: 0x10 alone = Beats, 0x11 alone = Time Steps, 0x18 alone = Sounds, 0x19 alone = Mutes,
   0x10 + 0x11 = Tunings, 0x18 + 0x19 = Levels. On a change the machine turns the four LEDs off, then on again, so the
   state is only trusted once no LED has changed for SETTLE_MS.

   Tempester.panelMode.create({onMode, setTimer, clearTimer, settleMs}) -> {feed(changes), reset(), mode()}
        onMode(index | null)   the settled mode (null = unknown or no single mode lit)
   Tempester.panelMode.decode(lit)  index of the mode for a set of lit LED indexes, or null
   Tempester.panelMode.NAMES */
"use strict";
window.Tempester = window.Tempester || {};
window.Tempester.panelMode = (function () {
  const NAMES = [
    "16 Beats",
    "16 Time Steps",
    "16 Sounds",
    "16 Tunings",
    "16 Mutes",
    "16 Levels",
  ];
  const BEATS = 0x10,
    TIME_STEPS = 0x11,
    SOUNDS = 0x18,
    MUTES = 0x19;
  const MODE_LEDS = [BEATS, TIME_STEPS, SOUNDS, MUTES];
  const SETTLE_MS = 150;

  function decode(lit) {
    const on = (led) => lit.has(led);
    const [beats, steps, sounds, mutes] = MODE_LEDS.map(on);
    if (beats && steps && !sounds && !mutes) return 3;
    if (sounds && mutes && !beats && !steps) return 5;
    if (beats && !steps && !sounds && !mutes) return 0;
    if (steps && !beats && !sounds && !mutes) return 1;
    if (sounds && !beats && !steps && !mutes) return 2;
    if (mutes && !beats && !steps && !sounds) return 4;
    return null;
  }

  function create({
    onMode,
    setTimer = (fn, ms) => setTimeout(fn, ms),
    clearTimer = (id) => clearTimeout(id),
    settleMs = SETTLE_MS,
  }) {
    const lit = new Set();
    let current = null,
      timer = null;

    function settle() {
      timer = null;
      const next = decode(lit);
      if (next === current) return;
      current = next;
      onMode(current);
    }
    function schedule() {
      if (timer !== null) clearTimer(timer);
      timer = setTimer(settle, settleMs);
    }
    function feed(changes) {
      let touched = false;
      for (const [index, on] of changes) {
        if (!MODE_LEDS.includes(index)) continue;
        touched = true;
        if (on) lit.add(index);
        else lit.delete(index);
      }
      if (touched) schedule();
    }
    /* The panel is gone (disconnected, no remote panel): what was read is no longer true. */
    function reset() {
      if (timer !== null) clearTimer(timer);
      timer = null;
      lit.clear();
      if (current !== null) {
        current = null;
        onMode(null);
      }
    }
    return { feed, reset, mode: () => current };
  }

  return { NAMES, MODE_LEDS, SETTLE_MS, decode, create };
})();
