/* The identity answer shown in full (ui/identity-panel.js) and its short summary chip in the top bar, on a fake DOM,
   run by tools/test_app.sh: `node identity-panel.test.js <ui folder>`. Every field is visible text after an
   identity event, nothing sits in a popover, the chip opens the Machine page. The exact version the machine reports
   (shared state of os-verify.js) fills the Version row and resolves every feature from Rust's table; it is forgotten
   with the shared state. The Voice OS of each of the six chips (n°64, `voice_versions` of Rust's events) fills six
   fixed rows: the label Rust sends, a tone for Tempester / DSI / unknown, "Waiting" while a machine that has the
   feature has not sent them yet, then "Not reported". Then the same checks on copies of the scripts with one
   behaviour broken, each of which must fail. */
"use strict";
const fs = require("fs");
const os = require("os");
const path = require("path");
const assert = require("assert");
const { load, all, textOf, isShown } = require("./fake-dom.js");

const uiDir = process.argv[2];
const FILES = [
  "bridge.js",
  "icons.js",
  "kit.js",
  "conn-bar.js",
  "os-verify.js",
  "identity-panel.js",
];
const TEMPESTER = {
  event: "identity",
  device: 0x7f,
  version: 0x7f2,
  tempester: true,
};
const DSI = {
  event: "identity",
  device: 0x04,
  version: 0x1501,
  tempester: false,
};

/* What Rust sends: every feature of its table (id, label, since) and whether the OS has it. */
const TABLE = [
  ["dump_requests", "Sound and beat requests", "1.5.2.0"],
  ["nrpn_reception", "NRPN reception", "1.5.3.0"],
  ["remote_panel", "Remote panel", "1.6.0.0"],
  ["project_request", "Project request", "1.6.1.0"],
  ["beat_wide", "Wide beat positions", "1.6.2.0"],
  ["ribbons", "FX ribbons", "1.6.3.0"],
  ["pedal_pad", "Pedal as pad", "1.6.3.0"],
  ["probability", "Step probability", "1.6.3.0"],
  ["mixer_reports", "Mixer reports", "1.6.4.0"],
  ["long_notes", "Long notes", "1.6.5.0"],
  ["quantize_fine", "Fine quantize", "1.6.5.0"],
  ["pad_channels", "MIDI channel per pad", "1.6.5.0"],
];
const features = (count) =>
  Object.fromEntries(
    TABLE.map(([id, label, since], k) => [
      id,
      { label, on: k < count, since },
    ]),
  );
const running = (version, known = true) => ({
  version,
  label: "Tempester " + version,
  known,
});
const state = (event, count, run) => ({
  event,
  features: features(count),
  running: run,
});

/* The class (tone) of the value shown next to a label. */
function tone(panel, label) {
  const cells = all(panel).filter(
    (n) => n.tagName === "DT" || n.tagName === "DD",
  );
  const at = cells.findIndex((n) => n.tagName === "DT" && textOf(n) === label);
  assert.ok(at >= 0, "no row " + label);
  return cells[at + 1].attrs.class;
}

/* The visible value shown next to a label, e.g. value(panel, "NRPN reception"). */
function value(panel, label) {
  const cells = all(panel).filter(
    (n) => n.tagName === "DT" || n.tagName === "DD",
  );
  const at = cells.findIndex((n) => n.tagName === "DT" && textOf(n) === label);
  assert.ok(at >= 0, "no row " + label);
  assert.ok(isShown(cells[at + 1]), label + " is hidden");
  return textOf(cells[at + 1]);
}

function setup(dir) {
  const env = load(dir, FILES);
  const panel = env.window.Tempester.identityPanel.build();
  const switched = [];
  env.kit.setPageSwitcher((id) => switched.push(id));
  env.receive({ event: "ports", ports: ["Tempest"] });
  env.kit.updateSession({ connected: true, port: "Tempest", fake: false });
  return { env, panel, switched, chip: env.byId.connbar.children[3] };
}

const popovers = (root) =>
  all(root).filter((n) =>
    /\b(popover|info)\b/.test((n.attrs && n.attrs.class) || ""),
  );

/* The rows and the guesses of an old OS or a machine that does not report its version. */
function runWord(env, panel, switched, chip) {
  assert.strictEqual(value(panel, "Status"), "Not checked");
  assert.strictEqual(value(panel, "Version word"), "-");
  assert.strictEqual(value(panel, "Version"), "-");
  assert.strictEqual(
    TABLE.filter(([, label]) => value(panel, label) === "-").length,
    TABLE.length,
    "every feature row exists from the first state and is empty",
  );

  env.receive(TEMPESTER);
  assert.strictEqual(value(panel, "Status"), "Answered");
  assert.strictEqual(value(panel, "OS"), "Tempester OS");
  assert.strictEqual(value(panel, "Device id"), "0x7F");
  assert.strictEqual(
    value(panel, "Manufacturer"),
    "Dave Smith Instruments (0x01)",
  );
  assert.strictEqual(value(panel, "Family"), "Tempest (0x28 0x01)");
  assert.strictEqual(value(panel, "Version word"), "0x7F2");
  assert.strictEqual(value(panel, "Version"), "-");
  assert.strictEqual(value(panel, "Came from"), "Tempest");
  assert.match(value(panel, "Checked at"), /^\d\d:\d\d:\d\d$/);
  assert.strictEqual(value(panel, "Sound and beat requests"), "Available");
  assert.match(value(panel, "NRPN reception"), /^From 1\.5\.3\.0/);
  assert.match(value(panel, "Remote panel"), /^From 1\.6\.0\.0/);
  assert.match(value(panel, "Project request"), /^From 1\.6\.1\.0/);
  assert.match(value(panel, "Fine quantize"), /^From 1\.6\.5\.0/);
  assert.match(
    textOf(panel),
    /System > Main OS Version/,
    "how to read the exact version",
  );
  assert.strictEqual(popovers(panel).length, 0, "nothing in a popover");
  assert.strictEqual(
    textOf(chip),
    "Tempester OS",
    "the chip is a short summary",
  );
  assert.strictEqual(
    popovers(env.byId.connbar).filter((n) => /popover/.test(n.attrs.class))
      .length,
    0,
  );
  chip.fire("click", {});
  assert.deepStrictEqual(
    switched,
    ["machine"],
    "the chip opens the Machine page",
  );
}

/* The exact version the machine reports resolves every feature and is forgotten with the shared state. */
function runReported(env, panel) {
  env.receive(state("installed", 12, running("1.6.5.0")));
  assert.strictEqual(
    value(panel, "Version"),
    "Tempester 1.6.5.0 (reported by the machine)",
  );
  for (const [, label] of TABLE)
    assert.strictEqual(value(panel, label), "Available", label);
  assert.doesNotMatch(
    textOf(panel),
    /System > Main OS Version/,
    "no look-on-the-machine hint when the version is known",
  );

  env.receive(state("installed", 5, running("1.6.2.0")));
  assert.strictEqual(value(panel, "Wide beat positions"), "Available");
  assert.strictEqual(value(panel, "Long notes"), "Not in this version");
  assert.strictEqual(value(panel, "Mixer reports"), "Not in this version");
  assert.doesNotMatch(textOf(panel), /From \d/, "no guess when resolved");

  env.receive(state("state", 5, running("1.6.2.0")));
  assert.strictEqual(
    value(panel, "Version"),
    "Tempester 1.6.2.0 (reported by the machine)",
  );

  env.receive(state("installed", 0, null));
  assert.strictEqual(value(panel, "Version"), "-", "forgotten by the state");
  assert.match(value(panel, "NRPN reception"), /^From 1\.5\.3\.0/);
  assert.match(textOf(panel), /System > Main OS Version/);

  env.receive(state("installed", 0, running("9.9.9.9", false)));
  assert.match(value(panel, "Version"), /not in this app's table/);
  assert.match(value(panel, "Long notes"), /^From 1\.6\.5\.0/);
  assert.match(textOf(panel), /does not know/);

  env.receive(state("installed", 12, running("1.6.5.0")));
  env.kit.updateSession({ connected: false });
  assert.strictEqual(value(panel, "Version"), "-", "disconnecting forgets it");
  env.kit.updateSession({ connected: true });
  assert.strictEqual(value(panel, "Version"), "-");
}

function runDsi(env, panel, chip) {
  env.kit.updateSession({ fake: true, port: "fake" });
  env.receive(state("installed", 0, null));
  env.receive(DSI);
  assert.strictEqual(value(panel, "OS"), "DSI OS");
  assert.strictEqual(value(panel, "Device id"), "0x04");
  assert.strictEqual(value(panel, "Version word"), "0x1501");
  assert.strictEqual(value(panel, "Came from"), "fake (fake machine)");
  for (const [, label] of TABLE)
    assert.strictEqual(value(panel, label), "Not in the DSI OS", label);
  assert.match(textOf(panel), /need a Tempester OS/);

  env.receive({ event: "error", during: "identity", code: "timeout" });
  assert.strictEqual(value(panel, "Status"), "No answer");
  assert.strictEqual(value(panel, "Version word"), "-");
  assert.match(
    textOf(panel),
    /MIDI: Sysex IN\+OUT Cable/,
    "the hint is shown, not hidden",
  );
  assert.strictEqual(textOf(chip), "No answer");

  env.receive(TEMPESTER);
  env.kit.updateSession({ connected: false });
  assert.strictEqual(
    value(panel, "Status"),
    "Not checked",
    "disconnecting clears it",
  );
  assert.strictEqual(value(panel, "Checked at"), "-");
}

/* n°64: the six Voice rows. `voices`: what Rust sends in `voice_versions` (six {word, label}) or null. */
const LABELS = {
  0x15: "DSI 1.5",
  0x16: "Tempester 1.5.1.0 or 1.5.1.1",
  0x17: "Tempester 1.5.1.2",
  0x18: "unknown (0x0018)",
};
const VOICES = [0x16, 0x16, 0x15, 0x16, 0x18, 0x17].map((word) => ({ word, label: LABELS[word] }));
const voiceEvent = (on, voices, run) => ({
  event: "installed",
  features: {
    voice_versions: {
      label: "Voice versions reported",
      on,
      since: "1.6.7.0",
    },
  },
  running: run,
  voice_versions: voices,
});
const voiceValues = (panel) =>
  [0, 1, 2, 3, 4, 5].map((k) => value(panel, "Voice " + k));

function runVoices(env, panel) {
  env.kit.updateSession({ connected: true, port: "Tempest", fake: false });
  env.receive(voiceEvent(false, null, null));
  assert.deepStrictEqual(voiceValues(panel), Array(6).fill("-"), "no machine: nothing");

  env.receive(voiceEvent(true, VOICES, running("1.6.7.0")));
  assert.deepStrictEqual(
    voiceValues(panel),
    VOICES.map((v) => v.label),
    "each chip shows the label Rust sent, in its own row",
  );
  assert.strictEqual(tone(panel, "Voice 0"), "good");
  assert.strictEqual(tone(panel, "Voice 2"), "off", "DSI 1.5 is shown as it is, not as a problem");
  assert.strictEqual(tone(panel, "Voice 4"), "warn", "an unknown word is flagged");
  assert.strictEqual(tone(panel, "Voice 5"), "good", "Tempester Voice 1.5.1.2 (0x17) is a Tempester Voice");

  env.receive(voiceEvent(true, null, running("1.6.7.0")));
  assert.deepStrictEqual(
    voiceValues(panel),
    Array(6).fill("Waiting for the machine..."),
    "a Main that has n°64 has not sent them yet",
  );

  env.receive(voiceEvent(false, null, running("1.6.6.0")));
  assert.deepStrictEqual(
    voiceValues(panel),
    Array(6).fill("Not reported"),
    "a Main without n°64 never will: no waiting",
  );

  env.receive(voiceEvent(true, VOICES, running("1.6.7.0")));
  env.kit.updateSession({ connected: false });
  assert.deepStrictEqual(voiceValues(panel), Array(6).fill("-"), "disconnecting forgets them");
}

/* The wait for a message that never comes ends: « Not reported ». */
async function runVoiceWait(env, panel, ms) {
  env.window.Tempester.identityPanel.voiceWait.ms = ms;
  env.kit.updateSession({ connected: true, port: "Tempest", fake: false });
  env.receive(voiceEvent(true, null, running("1.6.7.0")));
  assert.deepStrictEqual(voiceValues(panel), Array(6).fill("Waiting for the machine..."));
  await new Promise((resolve) => setTimeout(resolve, ms + 40));
  assert.deepStrictEqual(
    voiceValues(panel),
    Array(6).fill("Not reported"),
    "after the wait the page says so",
  );
  env.receive(voiceEvent(true, VOICES, running("1.6.7.0")));
  assert.strictEqual(
    value(panel, "Voice 0"),
    "Tempester 1.5.1.0 or 1.5.1.1",
    "a late message still shows",
  );
}

/* The two Release rows never contradict each other: Rust's `note` (decided by `release_max`) replaces "Available". */
const CAP_ROWS = {
  release_cap: { label: "Release capped at 123", since: "1.6.6.0" },
  release_cap_voice: {
    label: "Release up to 127 with Tempester Voice",
    since: "1.6.7.0",
  },
};
const capEvent = (run, rows) => ({
  event: "installed",
  features: Object.fromEntries(
    Object.entries(CAP_ROWS).map(([id, base]) => [id, { ...base, ...rows[id] }]),
  ),
  running: run,
});
const LIFTED = { text: "Lifted (six Tempester Voice chips)", tone: "good" };
const NEEDS_ALL = {
  text: "Inactive: needs Tempester Voice on all six chips",
  tone: "warn",
};

function runCap(env, panel) {
  env.kit.updateSession({ connected: true, port: "Tempest", fake: false });
  env.receive(
    capEvent(running("1.6.7.0"), {
      release_cap: { on: true, note: LIFTED },
      release_cap_voice: { on: true },
    }),
  );
  assert.strictEqual(
    value(panel, "Release capped at 123"),
    "Lifted (six Tempester Voice chips)",
  );
  assert.strictEqual(tone(panel, "Release capped at 123"), "good");
  assert.strictEqual(
    value(panel, "Release up to 127 with Tempester Voice"),
    "Available",
  );

  env.receive(
    capEvent(running("1.6.7.0"), {
      release_cap: { on: true },
      release_cap_voice: { on: true, note: NEEDS_ALL },
    }),
  );
  assert.strictEqual(value(panel, "Release capped at 123"), "Available");
  assert.strictEqual(
    value(panel, "Release up to 127 with Tempester Voice"),
    "Inactive: needs Tempester Voice on all six chips",
  );
  assert.strictEqual(
    tone(panel, "Release up to 127 with Tempester Voice"),
    "warn",
  );
  assert.ok(
    all(panel).some((n) => n.tagName === "DD" && n.attrs.title === NEEDS_ALL.text),
    "the full text stays on hover when the column cuts it",
  );

  env.receive(
    capEvent(running("1.6.6.0"), {
      release_cap: { on: true },
      release_cap_voice: { on: false },
    }),
  );
  assert.strictEqual(value(panel, "Release capped at 123"), "Available");
  assert.strictEqual(
    value(panel, "Release up to 127 with Tempester Voice"),
    "Not in this version",
  );
}

async function run(dir) {
  const { env, panel, switched, chip } = setup(dir);
  env.receive(state("state", 0, null));
  runWord(env, panel, switched, chip);
  runReported(env, panel);
  runDsi(env, panel, chip);
  runVoices(env, panel);
  runCap(env, panel);
  await runVoiceWait(env, panel, 20);
}

const read = (file) => fs.readFileSync(path.join(uiDir, file), "utf8");
const PANEL = "identity-panel.js";
const SHARED = "os-verify.js";
const broken = [
  [PANEL, "const PROVEN_FROM = { 0x7f2:", "const PROVEN_FROM = { 0x7f9:"],
  [
    PANEL,
    'if (m && !m.tempester) return [S.featuresNo, "off"];',
    "",
  ],
  [PANEL, "clock(checked)", "S.none"],
  [PANEL, "unchecked ? S.none : port,", "S.none,"],
  [
    PANEL,
    'answer({ m: null, status: "unchecked", checked: null, port: "" });',
    "void 0;",
  ],
  [PANEL, 'm ? "0x" + identity.hex(m.version) : S.none', "S.none"],
  [PANEL, "m ? S.family : S.none", "S.none"],
  [
    PANEL,
    'return status === "none" ? identity.hint : S.notChecked;',
    "return S.notChecked;",
  ],
  [
    PANEL,
    'answer({ m, status: "answered", checked: new Date(), port: portName() }),',
    "void m,",
  ],
  // The reported version: shown, resolves each feature, replaces the hint, is forgotten.
  [PANEL, "running ? S.reported(running) : S.none,", "S.none,"],
  [PANEL, "if (running && running.known)", "if (false)"],
  [
    PANEL,
    'if (!feature.on) return [S.featuresNotHere, "off"];',
    "",
  ],
  // The Release cap rows (Rust's `note` replaces "Available").
  [
    PANEL,
    "return feature.note\n        ? [feature.note.text, feature.note.tone]\n        : [S.featuresYes, \"good\"];",
    'return [S.featuresYes, "good"];',
  ],
  [PANEL, "? [feature.note.text, feature.note.tone]", '? [feature.note.text, "good"]'],
  [PANEL, "? [feature.note.text, feature.note.tone]", '? [S.featuresYes, feature.note.tone]'],
  [PANEL, 'cell.value.setAttribute("title", text);', "void text;"],
  [
    PANEL,
    "if (running) return running.known ? S.reportedNote : S.unknownNote;",
    "",
  ],
  [
    PANEL,
    "running.known ? S.reportedNote : S.unknownNote",
    "S.reportedNote",
  ],
  [
    PANEL,
    "featureCells[id] = row(feature.label);",
    "featureCells[id] = row(id);",
  ],
  [
    SHARED,
    "features: m.features || view.features,",
    "features: view.features,",
  ],
  [
    SHARED,
    "if (!session.connected)\n      setView({ running: null, verified: null, voiceVersions: null });",
    "",
  ],
  // The Voice rows.
  [PANEL, "const label = v.label || S.none;", "const label = S.none;"],
  [
    PANEL,
    '/^Tempester/.test(label)\n          ? "good"',
    '/^Zzz/.test(label)\n          ? "good"',
  ],
  [PANEL, '/^unknown/.test(label)\n            ? "warn"', '/^zzz/.test(label)\n            ? "warn"'],
  [PANEL, "if (expected && !voiceWaitOver) {", "if (false) {"],
  [PANEL, "voiceWaitOver = true;\n          render(", "render("],
  [PANEL, "view.running ? S.voiceNotReported : S.none", "S.none"],
  [PANEL, "view.running && view.running.known && feature && feature.on", "view.running && view.running.known"],
  [SHARED, "voiceVersions: m.voice_versions || null,", "voiceVersions: view.voiceVersions,"],
  [
    SHARED,
    "const running = m.running || null;",
    "const running = m.running || view.running;",
  ],
];
(async () => {
  await run(uiDir);
  let refused = 0;
  for (const [file, from, to] of broken) {
    const source = read(file);
    assert.ok(source.includes(from), "counter-proof anchor missing: " + from);
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "identity-panel-"));
    for (const f of FILES)
      fs.copyFileSync(path.join(uiDir, f), path.join(dir, f));
    fs.writeFileSync(path.join(dir, file), source.replace(from, to));
    await assert.rejects(() => run(dir), undefined, "broken copy accepted: " + from);
    refused++;
  }
  console.log(
    `ok: identity shown in full, no popover, chip opens Machine, reported version resolves features, six Voice rows; ${refused} broken copies refused`,
  );
  // A broken copy may leave its wait timer re-arming itself: end the process rather than wait for it.
  process.exit(0);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
