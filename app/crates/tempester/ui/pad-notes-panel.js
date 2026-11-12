/* The pad note table on the Machine page: which MIDI note the Tempest plays each pad A1-B16 with (System > MIDI Note
   : Pad). A button of the strip opens a panel over the page (fixed, so opening it moves nothing): one number per pad
   with its note name, « Learn » (tap the pads on the machine, the app fills the table from what it sends) and
   « Reset » (the default: the author's machine, A1-A15 white keys from C2, A16 61, B 62-77). Each note plays one pad only:
   Rust refuses a note another pad has, and the entry goes back to the table.

   Tempester.padNotesPanel.build() -> {button, panel}   both go in the page; Rust owns the table (pad-notes.js) */
"use strict";
window.Tempester = window.Tempester || {};
window.Tempester.padNotesPanel = (function () {
  const { el, errorText } = window.Tempester.kit;
  const { button, info } = window.Tempester.icons;
  const bridge = window.Tempester.bridge;
  const padNotes = window.Tempester.padNotes;

  const S = {
    open: "Pad notes: the MIDI note of each pad",
    title: "Pad notes",
    lead: "The Tempest plays each pad with its own MIDI note (System > MIDI Note : Pad A1-B16), not 36 + pad. The app plays, sequences and exports every pad with the note shown here. Learn: set Pad Function to 16 Sounds on the Tempest (Remote Pad OUT on, Tempester 1.6.0.0 or later for the pad LEDs) and tap the pads; press Sound Bank B first for B1-B16. The app also learns on its own whenever a pad is tapped. Each note plays one pad only.",
    learnOn: "Stop learning",
    learnOff: "Learn: tap the pads on the machine",
    learnText: "Learn",
    learning:
      "Tap the pads on the Tempest in 16 Sounds (Sound Bank B lit for B1-B16).",
    learned: (pad, note) =>
      "Pad " +
      padNotes.padName(pad) +
      " = note " +
      note +
      " " +
      padNotes.noteName(note) +
      ".",
    reset: "Back to the default notes",
    close: "Close",
    entry: (pad) => "Note of pad " + padNotes.padName(pad),
    unknown: "Waiting for the app's table...",
  };

  const els = { inputs: [], names: [], cells: [], learning: false };

  function show(notes) {
    notes.forEach((note, pad) => {
      els.inputs[pad].value = String(note);
      els.names[pad].textContent = padNotes.noteName(note);
    });
  }

  function setLearning(on) {
    els.learning = on;
    els.learn.setAttribute("aria-pressed", String(on));
    els.learn.setAttribute("aria-label", on ? S.learnOn : S.learnOff);
    els.learn.setAttribute("data-tip", on ? S.learnOn : S.learnOff);
    els.status.textContent = on ? S.learning : "";
    if (on) els.cells.forEach((cell) => cell.classList.remove("learned"));
  }

  function onLearned(m) {
    if (!els.learning) return;
    els.cells[m.pad].classList.add("learned");
    els.status.textContent = S.learned(m.pad, m.note);
  }

  /* A refused note: the entry shows the table again (Rust did not change it). */
  function onError(m) {
    if (m.during !== "pad_note") return;
    els.status.textContent = errorText(m);
    els.inputs.forEach((input, pad) => {
      const note = padNotes.note(pad);
      input.value = note === null ? "" : String(note);
    });
  }

  function cell(pad) {
    const input = el("input", {
      type: "number",
      min: "0",
      max: "127",
      class: "padnote-in",
      "aria-label": S.entry(pad),
    });
    input.addEventListener("change", () => {
      const note = Number.parseInt(input.value, 10);
      if (Number.isInteger(note) && note >= 0 && note <= 127)
        bridge.send("pad_note", pad, note);
      else onError({ during: "pad_note", message: S.entry(pad) + ": 0-127" });
    });
    els.inputs[pad] = input;
    els.names[pad] = el("span", { class: "padnote-name" });
    els.cells[pad] = el(
      "label",
      { class: "padnote" },
      el("span", { class: "padnote-pad" }, padNotes.padName(pad)),
      input,
      els.names[pad],
    );
    return els.cells[pad];
  }

  function build() {
    els.status = el("span", { class: "statusline", role: "status" }, S.unknown);
    els.learn = button("learn", S.learnOff, () => setLearning(!els.learning), {
      pressed: false,
      text: S.learnText,
    });
    const grid = el(
      "div",
      { class: "padnote-grid" },
      Array.from({ length: padNotes.PADS }, (_, pad) => cell(pad)),
    );
    const panel = el(
      "div",
      { class: "padnotes", role: "group", "aria-label": S.title },
      el(
        "div",
        { class: "toolbar" },
        el("strong", {}, S.title),
        info(S.lead, S.title),
        els.learn,
        button("restore", S.reset, () => bridge.send("pad_notes_reset")),
        el("span", { class: "grow" }),
        button("close", S.close, () => toggle(false)),
      ),
      els.status,
      grid,
      window.Tempester.padChannelsPanel.build().section,
    );
    panel.hidden = true;
    const opener = button("kit", S.open, () => toggle(panel.hidden), {
      pressed: false,
    });
    function toggle(open) {
      panel.hidden = !open;
      opener.setAttribute("aria-pressed", String(open));
      if (open) bridge.send("pad_notes");
      else setLearning(false);
    }
    padNotes.onChange((notes) => {
      if (!els.learning) els.status.textContent = "";
      show(notes);
    });
    bridge.on("pad_learned", onLearned);
    bridge.on("error", onError);
    return { button: opener, panel };
  }

  return { build };
})();
