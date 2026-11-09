/* The machine's note of each pad (System > MIDI Note : Pad A1-B16), as Rust keeps it: the `pad_notes` event (sent
   with `state`, and after every change) carries the 32 notes, A1-A16 then B1-B16. Rust plays a pad by its number
   (`pad on|off <0-31> <velocity>`): the page never computes a note, it only shows the table. Until the first event
   no note is known and the labels name the pad only.

   Tempester.padNotes ->
     note(pad)        the note of pad 0-31, or null while unknown
     noteName(note)   "C2" for 36 (the Tempest's octaves: 60 is C4)
     padName(pad)     "A1" for 0, "B16" for 31
     onChange(fn)     fn(notes) after each table received
     BANK             16 pads per bank */
"use strict";
window.Tempester = window.Tempester || {};
window.Tempester.padNotes = (function () {
  const PADS = 32;
  const BANK = 16;
  const NAMES = [
    "C",
    "C#",
    "D",
    "D#",
    "E",
    "F",
    "F#",
    "G",
    "G#",
    "A",
    "A#",
    "B",
  ];
  let notes = null;
  const listeners = [];

  const valid = (list) =>
    Array.isArray(list) &&
    list.length === PADS &&
    list.every((n) => Number.isInteger(n) && n >= 0 && n <= 127);

  function note(pad) {
    return notes && pad >= 0 && pad < PADS ? notes[pad] : null;
  }

  function noteName(n) {
    return NAMES[n % 12] + (Math.floor(n / 12) - 1);
  }

  function padName(pad) {
    return (pad < BANK ? "A" : "B") + ((pad % BANK) + 1);
  }

  function onChange(fn) {
    listeners.push(fn);
    if (notes) fn(notes.slice());
  }

  window.Tempester.bridge.on("pad_notes", (m) => {
    if (!valid(m.notes)) return;
    notes = m.notes.slice();
    for (const fn of listeners) fn(notes.slice());
  });

  return { note, noteName, padName, onChange, BANK, PADS };
})();
