/* The MIDI output channel of each pad (n°58, Tempester 1.6.5.0): one value per pad A1-B16, 0 = the usual route, 1-16 =
   that channel with the note the pad plays, 17-32 = channel (value - 16) with the pad's own note ("Fix"). Rust owns the
   conversation with the machine (CC 15 / CC 16 / CC 17, `machine/pad_channel.rs`); this module keeps the values it
   reports (`pad_channel` events) and sends the three commands.

   Tempester.padChannels ->
     value(pad)      0-32 of pad 0-31, or null while unknown
     label(value)    "Off", "5", "5 Fix"
     options()       [{value, label}] for 0-32, in the order of the machine's page
     set(pad, v)     sends `pad_channel pad v`; false (nothing sent) outside 0-31 / 0-32 and while a read runs
     read()          sends `pad_channel_read`: the machine answers one `pad_channel` per pad, one request at a time;
                     false (nothing sent) while a read runs
     save()          sends `pad_channel_save`: the System settings are saved (the machine ignores it while it plays);
                     false (nothing sent) while a read runs: the save makes the machine busy and late answers would
                     stop the read
     reading()       true from `read()` until the 32nd answer, a timeout, an error or a closed port
     onReading(fn)   fn(on) when that changes
     forget()        values unknown again (port closed)
     onChange(fn)    fn(values) after each change
     onTimeout(fn)   fn(pad) when the machine did not answer a read at that pad (the read stops there) */
"use strict";
window.Tempester = window.Tempester || {};
window.Tempester.padChannels = (function () {
  const PADS = 32;
  const MAX = 32;
  const bridge = window.Tempester.bridge;
  const values = Array.from({ length: PADS }, () => null);
  const listeners = [];
  const timeoutListeners = [];
  const readingListeners = [];
  let reading = false;
  let answered = 0;

  const isPad = (pad) => Number.isInteger(pad) && pad >= 0 && pad < PADS;
  const isValue = (v) => Number.isInteger(v) && v >= 0 && v <= MAX;

  function label(v) {
    if (v === 0) return "Off";
    if (v >= 1 && v <= 16) return String(v);
    if (v > 16 && v <= MAX) return v - 16 + " Fix";
    return "?";
  }

  function options() {
    return Array.from({ length: MAX + 1 }, (_, value) => ({
      value,
      label: label(value),
    }));
  }

  const value = (pad) => (isPad(pad) ? values[pad] : null);

  function notify() {
    for (const fn of listeners) fn(values.slice());
  }

  function setReading(on) {
    if (reading === on) return;
    reading = on;
    for (const fn of readingListeners) fn(on);
  }

  function set(pad, v) {
    if (reading || !isPad(pad) || !isValue(v)) return false;
    bridge.send("pad_channel", pad, v);
    return true;
  }

  function read() {
    if (reading) return false;
    answered = 0;
    setReading(true);
    bridge.send("pad_channel_read");
    return true;
  }

  function save() {
    if (reading) return false;
    bridge.send("pad_channel_save");
    return true;
  }

  function onChange(fn) {
    listeners.push(fn);
  }

  function forget() {
    values.fill(null);
    setReading(false);
    notify();
  }

  function onReading(fn) {
    readingListeners.push(fn);
  }

  function onTimeout(fn) {
    timeoutListeners.push(fn);
  }

  /* The machine did not answer the request for `pad`: the read stopped there (Remote Pad OUT off?). */
  bridge.on("pad_channel_timeout", (m) => {
    if (!isPad(m.pad)) return;
    setReading(false);
    for (const fn of timeoutListeners) fn(m.pad);
  });

  bridge.on("error", (m) => {
    if (m.during === "pad_channel") setReading(false);
  });

  bridge.on("pad_channel", (m) => {
    if (!isPad(m.pad) || !isValue(m.value)) return;
    values[m.pad] = m.value;
    if (reading && ++answered >= PADS) setReading(false);
    notify();
  });

  return {
    PADS,
    value,
    label,
    options,
    set,
    read,
    save,
    reading: () => reading,
    forget,
    onChange,
    onTimeout,
    onReading,
  };
})();
