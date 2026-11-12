/* The pad channel table inside the pad notes panel (n°58, Tempester 1.6.5.0): one choice per pad A1-B16 (Off, channel
   1-16 with the pad's played note, channel 1-16 « Fix » with the pad's own note), « Read » (the machine's 32 values)
   and « Save » (the System settings, only while the machine is stopped). Greyed with the version that brings it when
   the installed OS lacks it; the values themselves are kept by pad-channels.js.

   Tempester.padChannelsPanel.build() -> {section} */
"use strict";
window.Tempester = window.Tempester || {};
window.Tempester.padChannelsPanel = (function () {
  const { el } = window.Tempester.kit;
  const { button, info } = window.Tempester.icons;
  const bridge = window.Tempester.bridge;
  const padNotes = window.Tempester.padNotes;
  const channels = window.Tempester.padChannels;
  const features = window.Tempester.panelPerform.createFeatures();

  const S = {
    title: "MIDI channel of each pad",
    lead: "Tempester 1.6.5.0: each pad can send its notes on a MIDI channel of its own, from the sequencer and when played, to drive several synths. Off = the usual route. A number = that channel with the note the pad plays (chromatic). « Fix » = that channel with the pad's note from the table above (a fixed drum note). Needs Remote Pad IN and OUT set on the machine. Changes act at once and are lost at power-off until you press Save, which stops the machine's transport: save while it is stopped.",
    read: "Read the 32 channels from the machine (needs Remote Pad OUT)",
    readText: "Read",
    save: "Save in the machine's System settings (stop the sequencer first)",
    saveText: "Save",
    entry: (pad) => "MIDI channel of pad " + padNotes.padName(pad),
    noReply: (pad) =>
      "No reply from the machine for pad " +
      padNotes.padName(pad) +
      ": check that Remote Pad OUT is on. The values read so far are shown, the others stay unknown (?).",
    reading: "Reading the 32 channels from the machine...",
    saved:
      "Save sent. The machine ignores it while it plays or records: if so, stop it and save again.",
  };

  const els = { selects: [] };

  function refresh() {
    const { on, why } = features.status("pad_channels");
    const busy = on && channels.reading();
    for (const control of [els.read, els.save, ...els.selects])
      control.disabled = !on || busy;
    els.read.setAttribute("data-tip", busy ? S.reading : on ? S.read : why);
    els.save.setAttribute("data-tip", busy ? S.reading : on ? S.save : why);
    els.unavailable.textContent = on ? "" : why;
  }

  function show(values) {
    values.forEach((v, pad) => {
      els.selects[pad].value = v === null ? "" : String(v);
    });
  }

  function cell(pad) {
    const select = el(
      "select",
      { class: "padchan-in", "aria-label": S.entry(pad) },
      el("option", { value: "", disabled: true }, "?"),
      channels
        .options()
        .map((o) => el("option", { value: String(o.value) }, o.label)),
    );
    select.value = "";
    select.addEventListener("change", () => {
      els.status.textContent = "";
      channels.set(pad, Number.parseInt(select.value, 10));
    });
    els.selects[pad] = select;
    return el(
      "label",
      { class: "padchan" },
      el("span", { class: "padnote-pad" }, padNotes.padName(pad)),
      select,
    );
  }

  function build() {
    els.status = el("span", { class: "statusline", role: "status" });
    els.unavailable = el("span", { class: "statusline", role: "status" });
    els.read = button("fetch", S.read, () => channels.read(), {
      text: S.readText,
    });
    els.save = button(
      "save",
      S.save,
      () => {
        channels.save();
        els.status.textContent = S.saved;
      },
      { text: S.saveText },
    );
    const section = el(
      "div",
      { class: "padchans", role: "group", "aria-label": S.title },
      el(
        "div",
        { class: "toolbar" },
        el("strong", {}, S.title),
        info(S.lead, S.title),
        els.read,
        els.save,
      ),
      els.status,
      els.unavailable,
      el(
        "div",
        { class: "padchan-grid" },
        Array.from({ length: channels.PADS }, (_, pad) => cell(pad)),
      ),
    );
    channels.onChange(show);
    channels.onReading(refresh);
    channels.onTimeout((pad) => {
      els.status.textContent = S.noReply(pad);
    });
    window.Tempester.kit.onSession((session) => {
      features.setFake(session.fake);
      if (!session.connected) channels.forget();
      refresh();
    });
    const take = (m) => {
      if (!m.features) return;
      features.set(m.features);
      refresh();
    };
    bridge.on("state", take);
    bridge.on("installed", take);
    bridge.on("identity", (m) => {
      features.setIdentity(!!m.tempester);
      refresh();
    });
    refresh();
    return { section };
  }

  return { build };
})();
