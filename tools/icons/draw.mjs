// Drawing of the Tempester app icon (concept A "Panel", chosen by the author on 2026-11-07) and of its family
// (Pro 2, TB-303, Bass Station II), as SVG strings. Source of truth for the app icons; the exploration page
// Docs/mockups/tempester-icons.html keeps its own inline copy (an artifact page cannot import a module).
// usage: node tools/icons/draw.mjs <instrument> <big|small>   -> SVG on stdout (1024×1024 viewBox)

import { pathToFileURL } from "node:url";

const GLOW = "#9fd6ff",
  BLUE = "#3aa8ff",
  RED = "#e0322b";

export const INSTRUMENTS = {
  tempest: {
    name: "Tempester",
    sheet: ["#46505f", "#2c333e"],
    cheek: ["#63311a", "#a15b33", "#7a3f20"],
    grain: true,
    screen: { ground: "#06120a", pixel: "#7dff6a" },
    pad: "#e3e6e5",
    playing: "pads",
    side: "strips",
    knob: "#8e959e",
  },
  pro2: {
    name: "Pro 2",
    sheet: ["#2a2c30", "#141518"],
    cheek: ["#1b1c1f", "#34363b", "#1b1c1f"],
    grain: false,
    screen: { ground: "#050a0d", pixel: "#bfeeff" },
    pad: "#e3e6e5",
    playing: "keyboard",
    side: "wheels",
    knob: "#7f858d",
  },
  tb303: {
    name: "TB-303",
    sheet: ["#d9dcdf", "#a9adb2"],
    cheek: ["#8d9196", "#c4c8cc", "#8d9196"],
    grain: false,
    screen: { ground: "#1c0606", pixel: "#ff3b2f" },
    pad: "#f0f1f2",
    playing: "keys",
    side: "knobs",
    knob: "#c7cbd0",
  },
  bs2: {
    name: "Bass Station II",
    sheet: ["#2b2d31", "#17181b"],
    cheek: ["#b0231d", "#e0322b", "#b0231d"],
    grain: false,
    screen: { ground: "#1a0505", pixel: "#ff4436" },
    pad: "#e3e6e5",
    playing: "keyboard",
    side: "wheels",
    knob: "#9aa0a8",
  },
};

const u = "t";

function frame(pattern, I, blur) {
  const grain = I.grain
    ? [150, 190, 240, 300, 380, 470, 560, 650, 730, 800, 860]
        .map(
          (y, k) =>
            `<path d="M100 ${y} q24 ${k % 2 ? 9 : -9} 48 0 t48 0" stroke="#3d1c0c" stroke-opacity=".28" stroke-width="5" fill="none"/>`,
        )
        .join("")
    : "";
  return `<svg viewBox="0 0 1024 1024" width="1024" height="1024" xmlns="http://www.w3.org/2000/svg">
  <title>${I.name}</title>
  <defs>
    <linearGradient id="${u}t" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${I.sheet[0]}"/><stop offset="1" stop-color="${I.sheet[1]}"/></linearGradient>
    <linearGradient id="${u}b" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="${I.cheek[0]}"/><stop offset=".45" stop-color="${I.cheek[1]}"/><stop offset="1" stop-color="${I.cheek[2]}"/></linearGradient>
    <clipPath id="${u}c"><rect x="100" y="100" width="824" height="824" rx="185"/></clipPath>
    <filter id="${u}o" x="-20%" y="-20%" width="140%" height="140%"><feGaussianBlur stdDeviation="16"/></filter>
    <filter id="${u}g" x="-60%" y="-60%" width="220%" height="220%"><feGaussianBlur stdDeviation="${blur}" result="f"/><feMerge><feMergeNode in="f"/><feMergeNode in="SourceGraphic"/></feMerge></filter>
  </defs>
  <rect x="100" y="122" width="824" height="824" rx="185" fill="#000" opacity=".35" filter="url(#${u}o)"/>
  <g clip-path="url(#${u}c)">
    <rect x="100" y="100" width="824" height="824" fill="url(#${u}t)"/>
    <rect x="100" y="100" width="96" height="824" fill="url(#${u}b)"/>${grain}
    <rect x="828" y="100" width="96" height="824" fill="url(#${u}b)"/>
    <g transform="translate(728 0)">${grain}</g>
    <rect x="196" y="100" width="6" height="824" fill="#000" opacity=".3"/>
    <rect x="822" y="100" width="6" height="824" fill="#000" opacity=".3"/>
    <rect x="100" y="100" width="824" height="8" fill="#fff" opacity=".14"/>
    ${pattern(I)}
  </g>
  <rect x="101" y="101" width="822" height="822" rx="184" fill="none" stroke="#fff" stroke-opacity=".10" stroke-width="2"/>
</svg>
`;
}

const led = (cx, cy, r, c) =>
  `<circle cx="${cx}" cy="${cy}" r="${r}" fill="${c}" filter="url(#${u}g)"/>`;

function knob(cx, cy, r, I) {
  return (
    `<circle cx="${cx}" cy="${cy + 6}" r="${r}" fill="#000" opacity=".35"/><circle cx="${cx}" cy="${cy}" r="${r}" fill="#15171a"/>` +
    `<circle cx="${cx}" cy="${cy - 3}" r="${r * 0.62}" fill="${I.knob}"/><rect x="${cx - r * 0.09}" y="${cy - r * 0.62}" width="${r * 0.18}" height="${r * 0.5}" rx="${r * 0.09}" fill="#1b1d21"/>`
  );
}

function pad(x, y, s, lit, I) {
  return (
    `<rect x="${x}" y="${y}" width="${s}" height="${s}" rx="${s * 0.16}" fill="${lit ? "#f4fbff" : I.pad}" ${lit ? `filter="url(#${u}g)"` : ""}/>` +
    (lit
      ? `<rect x="${x}" y="${y}" width="${s}" height="${s}" rx="${s * 0.16}" fill="${GLOW}" opacity=".35"/>`
      : "") +
    `<rect x="${x + s * 0.06}" y="${y + s * 0.06}" width="${s * 0.88}" height="${s * 0.3}" rx="${s * 0.12}" fill="#fff" opacity="${lit ? 0.35 : 0.18}"/>`
  );
}

function miniScreen(x, y, w, h, I) {
  let s =
    `<rect x="${x}" y="${y}" width="${w}" height="${h}" rx="${h * 0.14}" fill="#0b0c0e"/>` +
    `<rect x="${x + 10}" y="${y + 10}" width="${w - 20}" height="${h - 20}" rx="${h * 0.08}" fill="${I.screen.ground}"/>`;
  [
    [0.18, 0.64],
    [0.18, 0.4],
    [0.18, 0.52],
  ].forEach(([dx, lw], k) => {
    s += `<rect x="${x + w * dx}" y="${y + h * (0.3 + k * 0.17)}" width="${w * lw}" height="${h * 0.09}" rx="4" fill="${I.screen.pixel}" opacity="${k ? 0.55 : 1}" ${k ? "" : `filter="url(#${u}g)"`}/>`;
  });
  return s;
}

function strip(x, y, w, h, lit_count) {
  let s = `<rect x="${x}" y="${y}" width="${w}" height="${h}" rx="${w * 0.22}" fill="#1b1e23"/><rect x="${x + 6}" y="${y + 6}" width="${w - 12}" height="${h - 12}" rx="${w * 0.16}" fill="#272b31"/>`;
  const count = 9,
    pitch = (h - 60) / (count - 1);
  for (let j = 0; j < count; j++) {
    const cy = y + h - 30 - j * pitch;
    s +=
      j < lit_count
        ? led(x + w / 2, cy, w * 0.17, BLUE)
        : `<circle cx="${x + w / 2}" cy="${cy}" r="${w * 0.17}" fill="#1a2c3f"/>`;
  }
  return s;
}

function wheel(x, y, w, h) {
  let s =
    `<rect x="${x}" y="${y}" width="${w}" height="${h}" rx="${w * 0.22}" fill="#0e0f11"/>` +
    `<rect x="${x + w * 0.16}" y="${y + 14}" width="${w * 0.68}" height="${h - 28}" rx="${w * 0.2}" fill="#4d545e"/>`;
  for (let k = 1; k < 9; k++)
    s += `<rect x="${x + w * 0.16}" y="${y + 14 + (k * (h - 28)) / 9}" width="${w * 0.68}" height="5" fill="#2a2f35"/>`;
  return s;
}

function knobColumn(x, y, w, h, I) {
  const r = Math.min(w * 0.42, h / 8);
  return [0, 1, 2]
    .map((k) => knob(x + w / 2, y + r + (k * (h - 2 * r)) / 2, r, I))
    .join("");
}

function padBlock(x, y, w, h, cols, rows, I) {
  const gap = w * 0.04,
    s = Math.min(
      (w - (cols - 1) * gap) / cols,
      (h - (rows - 1) * gap) / rows,
    );
  let out = "";
  for (let r = 0; r < rows; r++)
    for (let c = 0; c < cols; c++)
      out += pad(
        x + c * (s + gap),
        y + r * (s + gap),
        s,
        c === 1 && r === 0,
        I,
      );
  return out;
}

function keyboardBlock(x, y, w, h, I) {
  const lb = w / 7;
  let s = "";
  for (let i = 0; i < 7; i++) {
    const on = i === 4;
    s += `<rect x="${x + i * lb + 4}" y="${y}" width="${lb - 8}" height="${h}" rx="${lb * 0.14}" fill="${on ? "#f4fbff" : I.pad}" ${on ? `filter="url(#${u}g)"` : ""}/>`;
  }
  [0, 1, 3, 4, 5].forEach((i) => {
    s += `<rect x="${x + (i + 1) * lb - lb * 0.3}" y="${y}" width="${lb * 0.6}" height="${h * 0.58}" rx="${lb * 0.1}" fill="#15171a"/>`;
  });
  return s;
}

function keyBlock(x, y, w, h, I) {
  const count = 4,
    gap = w * 0.05,
    tw = (w - (count - 1) * gap) / count;
  let s = "";
  for (let i = 0; i < count; i++) {
    const tx = x + i * (tw + gap),
      r = Math.min(tw, h) * 0.08;
    s +=
      (i === 1
        ? led(tx + tw / 2, y + h * 0.14, r, RED)
        : `<circle cx="${tx + tw / 2}" cy="${y + h * 0.14}" r="${r}" fill="#5a1512"/>`) +
      `<rect x="${tx}" y="${y + h * 0.3}" width="${tw}" height="${h * 0.7}" rx="${tw * 0.12}" fill="${I.pad}"/>` +
      `<rect x="${tx + tw * 0.08}" y="${y + h * 0.34}" width="${tw * 0.84}" height="${h * 0.14}" rx="${tw * 0.08}" fill="#fff" opacity=".35"/>`;
  }
  return s;
}

const PLAYING = {
  pads: (I, x, y, w, h, c, r) => padBlock(x, y, w, h, c, r, I),
  keyboard: (I, x, y, w, h) => keyboardBlock(x, y, w, h * 0.9, I),
  keys: (I, x, y, w, h) => keyBlock(x, y, w, h, I),
};
const SIDE = {
  strips: (I, x, y, w, h, k) => strip(x, y, w, h, [7, 4][k] ?? 5),
  wheels: (I, x, y, w, h) => wheel(x, y, w, h),
  knobs: (I, x, y, w, h) => knobColumn(x, y, w, h, I),
};

/* Concept A: the panel, summarised (Dock and larger). */
const panel = (I) =>
  knob(282, 254, 40, I) +
  knob(390, 254, 40, I) +
  knob(498, 254, 40, I) +
  miniScreen(566, 206, 226, 104, I) +
  SIDE[I.side](I, 222, 370, 56, 430, 0) +
  SIDE[I.side](I, 298, 370, 56, 430, 1) +
  PLAYING[I.playing](I, 384, 604, 408, 196, 4, 2) +
  led(760, 520, 24, RED);

/* 16 and 32 px: the same panel with fewer, bigger elements (one strip, four pads, REC) to stay readable. */
const panelSmall = (I) =>
  SIDE[I.side](I, 236, 250, 120, 540, 0) +
  PLAYING[I.playing](I, 404, 330, 384, 384, 2, 2) +
  led(596, 230, 44, RED);

export function icon(instrument, size) {
  const I = INSTRUMENTS[instrument];
  if (!I) throw new Error(`unknown instrument: ${instrument}`);
  return size === "small"
    ? frame(panelSmall, I, 22)
    : frame(panel, I, 14);
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  process.stdout.write(
    icon(process.argv[2] ?? "tempest", process.argv[3] ?? "big"),
  );
}
