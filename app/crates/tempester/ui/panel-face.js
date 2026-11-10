/* The geometry of the Machine view: the top face of the Tempest, element by element, in the arrangement of the machine.
   The HORIZONTAL numbers are positions MEASURED on the author's photo of the machine (2026-11-08, shown 2,000 px
   wide, origin at the top left of the photo); the builder (panel-machine.js) subtracts ORIGIN, so an x here can be
   checked against the photo as it is. The VERTICAL numbers keep the machine's order of rows (knob, its label, its
   shifted label, the line, the keys) but give every row the room its text needs: the photo prints a label on the
   knob's skirt and a shifted label on top of the main one, which a screen drawing cannot do without hiding text. Each
   band therefore starts at B1 / B2 / B3 and its rows are offsets from it, and ui-tests/layout-overlap.js proves that no
   text meets another text, a control (knob with its ring, key with its LED, pad, screen) or a printed line.
   Why a fixed canvas and not a flow layout: the machine is a fixed panel, the eye knows where each control is, and a
   control that never moves cannot shift when a caption or a value changes. The canvas is scaled to the window by
   panel-view.js (fit width, then the zoom).

   Tempester.panelFace.ORIGIN, SIZE           the face in photo pixels: wood sides included
   Tempester.panelFace.ZONES                  [x0, y0, x1, y1, kind]: the sections (kind "sunk" is the pad block)
   Tempester.panelFace.TITLES                 [text, cx, cy]: vertical section titles, read upward
   Tempester.panelFace.TEXTS                  [text, cx, cy, style]: silkscreen labels; style names a size / colour
   Tempester.panelFace.LINES                  [[x, y], ...]: printed lines and brackets (polylines)
   Tempester.panelFace.KNOBS                  [kind, n, cx, cy]: kind abs / enc as in panel-machine.js
   Tempester.panelFace.KEYS                   [name, x, y, w, h, led]: the key (rectangle) and its LED (see L)
   Tempester.panelFace.TRANSPORT, TRANSPORT_LEDS  the transport keys and the LEDs of REC and PLAY
   Tempester.panelFace.PAD, RIBBONS, SCREEN   the pads, the FX ribbons and the screen, measured the same way */
"use strict";
window.Tempester = window.Tempester || {};
window.Tempester.panelFace = (function () {
  const ORIGIN = { x: 35, y: 41 };

  /* Bands: top (the bar), B1 (oscillators, filters, amp), B2 (envelopes, LFO, mixer), B3 (function keys), PT (pads).
     Each band is 8 px under the one above, as the sections are 8 px apart sideways; B3 and PT are further apart, as on
     the machine. */
  const TOP = 98;
  const TOP_END = 230;
  const B1 = 254;
  const B1_END = B1 + 154;
  const B2 = B1_END + 8;
  const B2_END = B2 + 208;
  const B3 = B2_END + 18;
  const B3_END = B3 + 190;
  const PT = B3_END + 26;
  const PT_END = PT + 372;
  const SIZE = { w: 1932, h: PT_END + 22 - ORIGIN.y };

  /* Rows of each band, as offsets from its top. */
  const KNOB_TOP = 148; /* the knobs of the bar */
  const KNOB_B1 =
    B1 + 44; /* the knob rows: 64 px boxes, the labels start under them */
  const KNOB_B2 = B2 + 44;
  const R1 = B1 + 92; /* main labels under the knobs */
  const R2 = B1 + 116;
  const R3 = B1 + 137;
  const E1 = B2 + 92;
  const E2 = B2 + 113;
  const LINE2 = B2 + 127;
  const KL = B2 + 142; /* labels over the keys of band 2 */
  const KEY2 = B2 + 157;
  const SHOW2 = B2 + 194;
  const G0 = B3 + 14;
  const L1 = B3 + 37;
  const K1 = B3 + 51;
  const GT = B3 + 91;
  const L2 = B3 + 116;
  const K2 = B3 + 131;
  const L3 = B3 + 172;
  const SUB1 = B3 + 88; /* a shifted label under a key of the first row */
  const SUB2 = B3 + 167;

  /* The screen block (right). */
  const SCREEN = { x: 1433, y: 396, w: 392, h: 107 };
  const BAR1 = 370;
  const BAR2 = 520;
  const TEMPO = 538;
  const NAV1 = 567;
  const NAV1_KEY = 584;
  const NAV2 = 634;
  const NAV2_KEY = 651;
  const SEP = 698;
  const TLAB = 722;
  const TKEY = 740;
  const TSUB = 806;

  const ZONES = [
    [135, 122, 260, TOP_END, "sec"],
    [1393, TOP, 1493, TOP_END, "sec"],
    [1504, TOP, 1866, TOP_END, "sec"],
    [135, B1, 675, B1_END, "sec"],
    [683, B1, 1088, B1_END, "sec"],
    [1096, B1, 1231, B1_END, "sec"],
    [1239, B1, 1373, B1_END, "sec"],
    [135, B2, 675, B2_END, "sec"],
    [683, B2, 950, B2_END, "sec"],
    [966, B2, 1374, B2_END, "sec"],
    [1391, B1, 1866, B3_END, "sec"],
    [135, B3, 408, PT_END, "sec"],
    [433, PT, 1863, PT_END, "sunk"],
    [435, B3, 649, B3_END, "sec"],
    [657, B3, 852, B3_END, "sec"],
    [863, B3, 1062, B3_END, "sec"],
    [1069, B3, 1268, B3_END, "sec"],
    [1275, B3, 1372, B3_END, "sec"],
  ];

  const TITLES = [
    ["Oscillators", 148, B1 + 77],
    ["Lowpass", 699, B1 + 77],
    ["Highpass", 1115, B1 + 77],
    ["Amp", 1255, B1 + 77],
    ["Envelopes", 147.5, B2 + 104],
    ["LFO 1&2", 699, B2 + 104],
    ["Mixer", 980, B2 + 104],
    ["Main Out", 1518.5, (TOP + TOP_END) / 2],
    ["Real Time FX", 148, B3 + 90],
  ];

  /* Styles: m = main label, s = shifted function (smaller), sw = small main-colour label, g = group title in spaced
     letters, big = FX1 / FX2; rwmark = the « |« » print between the « and » keys. */
  const TEXTS = [
    ["Shift", 197, 160, "m"],
    ["BPM", 1442, 193, "m"],
    ["Swing", 1442, 214, "s"],
    ["Distort", 1567, 193, "m"],
    ["Compress", 1691, 193, "m"],
    ["Volume", 1816, 193, "m"],
    /* Oscillators */
    ["Select", 208, B1 + 22, "m"],
    ["Show", 208, B1 + 71, "s"],
    ["1", 198, R1, "m"],
    ["Ana", 223.7, R1, "m"],
    ["2", 246, R1, "m"],
    ["3", 198, R2, "m"],
    ["Digi", 222.6, R2, "m"],
    ["4", 246, R2, "m"],
    ["Shape", 341, R1, "m"],
    ["1/2 Mix", 484, R1, "m"],
    ["Pitch", 620, R1, "m"],
    ["Fine", 620, R2, "s"],
    ["Sample", 344, R2, "m"],
    ["Level", 486, R2, "m"],
    ["Pre/Post Filter", 483.7, R3, "s"],
    /* Lowpass, Highpass, Amp */
    ["Frequency", 762.6, R1, "m"],
    ["Resonance", 899, R1, "m"],
    ["Audio Mod", 1034, R1, "m"],
    ["4 Pole", 837, R2 - 0.5, "m"],
    ["Show", 837, R3 + 0.5, "s"],
    ["Frequency", 1172, R1, "m"],
    ["Feedback", 1310, R1, "m"],
    /* Envelopes */
    ["AD Mode", 208, B2 + 17, "m"],
    ["(Off=ADSR)", 208, B2 + 38, "sw"],
    ["Attack", 345.7, E1, "m"],
    ["Sustain Level", 345.7, E2, "s"],
    ["Decay", 483.6, E1, "m"],
    ["Release", 483.6, E2, "s"],
    ["Amount", 621.6, E1, "m"],
    ["Velocity Amt", 621.6, E2, "s"],
    ["Pitch", 208, KL, "m"],
    ["Lowpass", 312, KL, "m"],
    ["Amp", 416, KL, "m"],
    ["Aux1", 519.8, KL, "m"],
    ["Aux2", 623, KL, "m"],
    /* LFO 1&2 */
    ["Rate", 759, E1, "m"],
    ["Shape", 759, E2, "s"],
    ["Amount", 897, E1, "m"],
    ["Destination", 897, E2, "s"],
    ["LFO2", 759.6, KL, "m"],
    ["Show", 759.6, SHOW2, "s"],
    ["Mod Paths", 897.7, KL, "m"],
    /* Mixer */
    ["Level", 1034, E1, "m"],
    ["Pan", 1172, E1, "m"],
    ["Delay Repeats", 1171, E2, "s"],
    ["Delay Send", 1309, E1, "m"],
    ["Delay Time", 1308.5, E2, "s"],
    ["Mute", 1032.7, KL, "m"],
    ["Show", 1034, SHOW2, "s"],
    ["Solo", 1171.5, KL, "m"],
    ["Delay On", 1309, KL, "m"],
    /* The screen block */
    ["Tempo", 1581, TEMPO, "sw"],
    ["Location", 1675, TEMPO, "sw"],
    ["Screen Title", 1768, TEMPO, "sw"],
    ["Page Up", 1572, NAV1, "m"],
    ["▲", 1684, NAV1, "m"],
    ["Page Down", 1795, NAV1, "m"],
    ["Shift", 1460.5, NAV2, "m"],
    ["◄", 1572.5, NAV2, "m"],
    ["▼", 1684, NAV2, "m"],
    ["►", 1795, NAV2, "m"],
    ["«", 1439, TLAB, "m"],
    ["»", 1487, TLAB, "m"],
    ["REC", 1574, TLAB, "m"],
    ["STOP", 1684, TLAB, "m"],
    ["PLAY", 1794, TLAB, "m"],
    ["|«", 1463, TKEY + 9, "s rwmark"],
    ["Cont", 1739, TKEY + 9, "s"],
    ["<", 1439, TSUB, "s"],
    [">", 1487, TSUB, "s"],
    ["Tap Tempo", 1794, TSUB, "s"],
    /* Function keys */
    ["16 Tunings", 542, G0, "m"],
    ["16 Beats", 491, L1, "m"],
    ["16 Time Steps", 591, L1, "m"],
    ["Pad Function", 540, GT, "g"],
    ["16 Sounds", 491, L2, "m"],
    ["16 Mutes", 595, L2, "m"],
    ["16 Levels", 542, L3, "m"],
    ["Sound", 696, G0, "m"],
    ["Bank B", 696, L1, "m"],
    ["Reverse", 799, L1, "m"],
    ["Pad Options", 749, GT, "g"],
    ["Roll", 696, L2, "m"],
    ["Fixed Level", 799, L2, "m"],
    ["Play List", 903, L1, "m"],
    ["Events", 1005, L1, "m"],
    ["Screens", 959, GT, "g"],
    ["Pads", 902, L2, "m"],
    ["Sounds", 1005, L2, "m"],
    ["Mixer", 959, L3, "m"],
    ["Click", 1110, L1, "m"],
    ["Undo Rec", 1212, L1, "m"],
    ["Undo Snd", 1212, SUB1, "s"],
    ["Rec/Edit", 1120, GT, "g"],
    ["Erase", 1109, L2, "m"],
    ["Copy", 1213, L2, "m"],
    ["Edit", 1165, L3, "m"],
    ["System", 1314, L1, "m"],
    ["Save/Load", 1316, L2, "m"],
    /* Real Time FX */
    ["Playback", 207.5, L1, "m"],
    ["Playback", 332.5, L1, "m"],
    ["Assign", 207.5, SUB1, "s"],
    ["Assign", 332.5, SUB1, "s"],
    ["Latch On", 208, L2, "m"],
    ["Latch On", 333, L2, "m"],
    ["FX 3", 207.5, SUB2, "s"],
    ["FX 4", 332.5, SUB2, "s"],
    ["FX1", 208, B3 + 198, "big"],
    ["FX3", 207.5, B3 + 222, "s"],
    ["FX2", 332.5, B3 + 198, "big"],
    ["FX4", 332.5, B3 + 222, "s"],
  ];

  /* The pads: 8 columns 175.1 apart, two rows, the number of each pad printed above it. */
  const PAD = {
    x: 464,
    y: [PT + 36, PT + 36 + 138 + 38],
    w: 138.5,
    h: 138,
    pitch: 175.1,
  };

  /* A printed bracket round a label of the function keys: [x0, x1, y, side], the tick at each end pointing `side`
     (-1 up, 1 down); the label sits between the two halves. */
  const bracket = (xl, xl2, xr, xr2, y, side) => [
    [
      [xl, y + 6 * side],
      [xl, y],
      [xl2, y],
    ],
    [
      [xr, y],
      [xr2, y],
      [xr2, y + 6 * side],
    ],
  ];
  const slant = (x0, y0, x1, y1) => [
    [x0, y0],
    [x1, y1],
  ];
  const BARS = [
    [1448, 1529],
    [1542, 1622],
    [1635, 1715],
    [1728, 1807],
  ];
  const SOFT_X = [1448, 1572.5, 1699, 1826];
  const LINES = [
    /* Shape / Sample to 1/2 Mix / Level */
    slant(268, R1, 314, R1),
    slant(374, R1, 442, R1),
    slant(268, R2, 314, R2),
    slant(376, R2, 458, R2),
    /* Envelopes: the line over the keys; LFO: under Rate / Shape */
    slant(183.6, LINE2, 664.7, LINE2),
    slant(731.5, LINE2, 786.5, LINE2),
    /* The soft knobs down to the screen, then the labels under the screen */
    ...BARS.map(([a, b]) => slant(a, BAR1, b, BAR1)),
    ...BARS.map(([a, b], i) =>
      slant(
        SOFT_X[i] + (a + b - 2 * SOFT_X[i]) / 8,
        KNOB_B1 + 38,
        (a + b) / 2,
        BAR1,
      ),
    ),
    ...BARS.map(([a, b]) => slant(a, BAR2, b, BAR2)),
    slant(1460, NAV1_KEY - 2, 1490, BAR2 + 1),
    slant(1421, SEP, 1826, SEP),
    /* Between « and »: the « –+– » under the |« mark; between STOP and PLAY: the « Cont » link with its notch */
    slant(1454, TKEY + 22, 1459, TKEY + 22),
    slant(1461, TKEY + 22, 1465, TKEY + 22),
    slant(1463, TKEY + 19, 1463, TKEY + 25),
    slant(1467, TKEY + 22, 1472, TKEY + 22),
    [
      [1717, TKEY + 22],
      [1734, TKEY + 22],
      [1734, TKEY + 26],
      [1745, TKEY + 26],
      [1745, TKEY + 22],
      [1761, TKEY + 22],
    ],
    /* Brackets of the function keys */
    ...bracket(493, 498, 586, 593, G0, 1),
    ...bracket(493, 498, 586, 593, L3, -1),
    ...bracket(902, 928, 988, 1009, L3, -1),
    ...bracket(1108, 1140, 1190, 1215, L3, -1),
    /* FX1 / FX3 and FX2 / FX4 */
    [
      [227, B3 + 222],
      [245, B3 + 222],
      [245, SUB2 - 4],
    ],
    [
      [295, SUB2 - 4],
      [295, B3 + 222],
      [315, B3 + 222],
    ],
  ];

  /* The knobs: kind, number (panel-machine.js CONTROLS), centre of the cap. */
  const KNOBS = [
    ["enc", 8, 1448, KNOB_TOP],
    ["abs", 9, 1572.5, KNOB_TOP],
    ["abs", 2, 1699, KNOB_TOP],
    ["abs", 8, 1826, KNOB_TOP],
    ["enc", 3, 339, KNOB_B1],
    ["enc", 2, 478.5, KNOB_B1],
    ["enc", 1, 618, KNOB_B1],
    ["abs", 5, 757, KNOB_B1],
    ["abs", 4, 896, KNOB_B1],
    ["abs", 3, 1036, KNOB_B1],
    ["abs", 1, 1175.5, KNOB_B1],
    ["abs", 0, 1314, KNOB_B1],
    ["enc", 7, 339, KNOB_B2],
    ["enc", 6, 478.5, KNOB_B2],
    ["enc", 5, 618, KNOB_B2],
    ["enc", 0, 757, KNOB_B2],
    ["enc", 4, 896, KNOB_B2],
    ["abs", 7, 1036, KNOB_B2],
    ["abs", 6, 1175.5, KNOB_B2],
    ["enc", 9, 1314, KNOB_B2],
    ["enc", 13, SOFT_X[0], KNOB_B1],
    ["enc", 12, SOFT_X[1], KNOB_B1],
    ["enc", 11, SOFT_X[2], KNOB_B1],
    ["enc", 10, SOFT_X[3], KNOB_B1],
  ];
  /* A knob is drawn by the Full view's own drawing (panel.js, 40 x 40) scaled into this box: skirt, cap, the ring of
     the scale or of the dashes, all inside it, so that the box IS the knob for the overlap test. */
  const KNOB = { box: 64 };

  /* The LED of a key: beside it (`side` right or left, `shift` close to the key for the two Shift keys), centred on the key unless
     `dy` says otherwise. All LEDs are the blue of the Full view. */
  const L = (side = "right", dy = null) => ({ side, dy });
  const LED_SIZE = { w: 6, h: 14, gap: 6 };
  /* name (panel-keys.js), x, y, width, height, led: null | L(...) */
  const KEYS = [
    ["Shift 1", 173, 178, 45, 21, { side: "shift", dy: null }],
    ["Select", 183, B1 + 36.5, 49, 24, null],
    ["AD Mode", 183, B2 + 52, 49.5, 23, L()],
    ["Env Pitch", 183, KEY2, 49.5, 24.5, L()],
    ["Env Lowpass", 286, KEY2, 50, 24.5, L()],
    ["Env Amp", 390, KEY2, 50.5, 24.5, L()],
    ["Env Aux 1", 493, KEY2, 51, 24.5, L()],
    ["Env Aux 2", 597, KEY2, 49.5, 24.5, L()],
    ["4 Pole", 872.5, B1 + 112, 50, 24, L()],
    ["LFO2", 733.5, KEY2, 51, 24.5, L()],
    ["Mod Paths", 872.5, KEY2, 49.5, 24.5, null],
    ["Mute", 1009, KEY2, 51.5, 24.5, L()],
    ["Solo", 1147.5, KEY2, 50, 24.5, L()],
    ["Delay On", 1284.5, KEY2, 51, 24.5, L()],
    ["Soft Key", 1435, NAV1_KEY, 50, 26, null],
    ["Page Up", 1548, NAV1_KEY, 49, 26, null],
    ["Cursor Up", 1659, NAV1_KEY, 50, 26, null],
    ["Page Down", 1770, NAV1_KEY, 50, 26, null],
    ["Shift 2", 1437, NAV2_KEY, 47, 24.5, { side: "shift", dy: null }],
    ["Cursor Left", 1548, NAV2_KEY, 49, 24.5, null],
    ["Cursor Down", 1659, NAV2_KEY, 50, 24.5, null],
    ["Cursor Right", 1770, NAV2_KEY, 50, 24.5, null],
    ["Playback FX1", 183, K1, 51, 23.5, L()],
    ["Playback FX2", 309, K1, 50, 23.5, L("left")],
    ["Latch On FX1", 183, K2, 51, 23.5, L()],
    ["Latch On FX2", 309, K2, 50, 23.5, L("left")],
    ["16 Beats", 466, K1, 50.5, 23.5, L()],
    ["16 Time Steps", 568, K1, 50.5, 23.5, L()],
    ["16 Sounds", 466, K2, 50.5, 23.5, L()],
    ["16 Mutes", 568, K2, 50.5, 23.5, L()],
    ["Sound Bank B", 672, K1, 50.5, 23.5, L()],
    ["Reverse", 775, K1, 50.5, 23.5, L()],
    ["Roll", 672, K2, 50.5, 23.5, L()],
    ["Fixed Level", 775, K2, 50.5, 23.5, L()],
    ["Play List", 878, K1, 50.5, 23.5, L()],
    ["Events", 981.5, K1, 50.5, 23.5, L()],
    ["Pads", 878, K2, 50.5, 23.5, L()],
    ["Sounds", 981.5, K2, 50.5, 23.5, L()],
    ["Click", 1084, K1, 50.5, 23.5, L()],
    ["Undo Rec", 1187, K1, 50.5, 23.5, L()],
    ["Erase", 1084, K2, 50.5, 23.5, L()],
    ["Copy", 1187, K2, 50.5, 23.5, L()],
    ["System", 1291, K1, 50.5, 23.5, L()],
    ["Save/Load", 1291, K2, 50.5, 23.5, L()],
  ];

  /* The transport, as on the machine (photo, 2026-11-09): two narrow keys « and » close together, a « |« » mark and a
     « –+– » line printed between them; then REC, STOP and PLAY, larger, one size, evenly spaced under ◄ ▼ ►; all
     five on one top line. [name, x, y, w, h, kind] (kind: small, rec, stop, play). */
  const TRANSPORT_KEY = { w: 62, h: 45, pitch: 110, x: 1543 };
  const TRANSPORT_SMALL = { w: 24, h: 47, x: [1427, 1475] };
  const TRANSPORT = [
    ["Rew", TRANSPORT_SMALL.x[0], TKEY, TRANSPORT_SMALL.w, TRANSPORT_SMALL.h, "small"],
    ["FF", TRANSPORT_SMALL.x[1], TKEY, TRANSPORT_SMALL.w, TRANSPORT_SMALL.h, "small"],
    ...[
      ["Rec", "rec"],
      ["Stop", "stop"],
      ["Play", "play"],
    ].map(([name, kind], i) => [
      name,
      TRANSPORT_KEY.x + i * TRANSPORT_KEY.pitch,
      TKEY,
      TRANSPORT_KEY.w,
      TRANSPORT_KEY.h,
      kind,
    ]),
  ];
  /* The LEDs of REC (red, 0x23: flashes while armed, right of its title) and PLAY (blue, 0x24: lit while playing,
     left of its title, as on the machine: author, 10/11/2026), the size of the key LEDs. [led, x, y, colour] */
  const TRANSPORT_LEDS = [
    [0x23, 1598, TLAB - LED_SIZE.h / 2, "rec"],
    [0x24, 1760, TLAB - LED_SIZE.h / 2, "play"],
  ];

  /* The four LEDs of the oscillators, 0x27-0x2A: beside "1 Ana 2" and "3 Digi 4". [led, x, y, w, h] */
  const OSC_LEDS = [
    [0x27, 183, R1 - 7],
    [0x28, 256, R1 - 7],
    [0x29, 183, R2 - 7],
    [0x2a, 256, R2 - 7],
  ];

  const RIBBONS = [
    { x: 163, y: B3 + 240, w: 95, h: PT_END - 22 - (B3 + 240), name: "FX1" },
    {
      x: 288.4,
      y: B3 + 240,
      w: 94.6,
      h: PT_END - 22 - (B3 + 240),
      name: "FX2",
    },
  ];

  /* The printed lines and brackets as one drawing over the whole face. */
  function linesMarkup() {
    const shift = ([x, y]) =>
      `${(x - ORIGIN.x).toFixed(1)},${(y - ORIGIN.y).toFixed(1)}`;
    const lines = LINES.map(
      (points) => `<polyline points="${points.map(shift).join(" ")}"/>`,
    ).join("");
    return `<svg class="mvlines" width="${SIZE.w}" height="${SIZE.h}" viewBox="0 0 ${SIZE.w} ${SIZE.h}" aria-hidden="true">${lines}</svg>`;
  }

  /* The screen: the Full view's green screen, the sound's name at the top and the Pad Function mode at the bottom. */
  function screenMarkup() {
    return `<span class="nom"></span><span class="mode"></span>`;
  }

  /* The LEDs of an FX ribbon: a column of lights, as the Full view's ribbons (they stay dark: no MIDI path). */
  function ribbonMarkup() {
    return `<span class="leds"></span>`;
  }

  return {
    ORIGIN,
    SIZE,
    KNOB,
    ZONES,
    TITLES,
    TEXTS,
    LINES,
    KNOBS,
    KEYS,
    LED_SIZE,
    TRANSPORT,
    TRANSPORT_LEDS,
    OSC_LEDS,
    PAD,
    RIBBONS,
    SCREEN,
    TOP,
    TOP_END,
    PT_END,
    linesMarkup,
    screenMarkup,
    ribbonMarkup,
  };
})();
