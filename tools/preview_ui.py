#!/usr/bin/env python3
"""Standalone preview of the Tempester app's page, for looking at it in a browser without the app.

usage: python3 -I tools/preview_ui.py <out.html> [--ui <ui dir>] [--params <sound_params.json>] [--start <page>] [--library-demo]

Reads the app's real page files (app/crates/tempester/ui/: index.html and every stylesheet and script it names)
and writes ONE html file with all of them inlined, so that it opens from disk (file://) with no server:
  - `params.js` is built from the parameter table like `params_js()` in app/crates/tempester/src/ui/assets.rs
    (`window.TEMPEST_PARAMS = [[index, max, default], ...]`);
  - the Rust side is replaced by a stub `window.ipc` that answers a few commands with EXAMPLE data (a port, a
    data folder, a sound) and ignores the others: nothing is sent anywhere, no MIDI port exists in a browser;
  - the page's Content-Security-Policy (`script-src 'self'`) is removed, since every script is inline here;
  - a banner at the top says it is a preview with example values.
Every page of the app is there, through its tabs; it opens on the Panel page (`--start`), or on the page named in
the address (`<out.html>#machine`, `#play`...).
Limits: the stub answers only what the pages need to draw; buttons that would talk to the machine do nothing.
No dependency (standard library only).
"""

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_UI = ROOT / "app/crates/tempester/ui"
DEFAULT_PARAMS = ROOT / "profile/dsi-tempest/sound_params.json"

BANNER = (
    '<div style="background:#3b1d4a;color:#f3e6ff;padding:6px 16px;font:600 14px/1.4 -apple-system,Arial,'
    'sans-serif;border-bottom:1px solid #6b3d82">PREVIEW of the Tempester page: example values only '
    "(port, folder, sound), no machine, nothing is sent.</div>"
)

# Stands for the Rust side (src/ui/script.rs). Answers are asynchronous, as from the real bridge.
STUB = r"""
window.ipc = { postMessage(line) {
  const [name, ...args] = line.split("\t");
  const answer = window.__previewAnswers[name];
  if (answer) setTimeout(() => window.Tempester.bridge.receive(answer(args)), 0);
} };
window.__previewAnswers = {
  state: () => ({ event: "state", channel: 1, port: null,
                  data_dir: "/Users/example/Library/Application Support/Tempester (EXAMPLE)" }),
  ports: () => ({ event: "ports", ports: ["Tempest (EXAMPLE port)", "IAC Driver Bus 1 (EXAMPLE)"] }),
  connect: (a) => ({ event: "conn", state: "connected", port: a[0], fake: false }),
  disconnect: () => ({ event: "conn", state: "disconnected" }),
  fetch_sound: () => window.__previewSound(),
  library: (a) => {
    const one = a[0].replace(/s$/, "");
    const files = ["EXAMPLE " + one + " A.syx", "EXAMPLE " + one + " B.syx",
                   "EXAMPLE Project/01 " + one + ".syx", "EXAMPLE Project/02 " + one + ".syx"]
      .map((name) => ({ name, size: 300 }));
    return { event: "library", kind: a[0], files };
  },
  os_versions: () => ({ event: "os_versions", versions: window.__previewVersions || [] }),
  edit_state: () => ({ event: "edit", synced: true, undo: "n34", redo: null, depth: 1, a: true, b: true, active: "A" }),
  learn_ports: () => ({ event: "learn_ports", ports: ["EXAMPLE Controller"] }),
  pad_notes: () => window.__previewPadNotes(),
};
"""


# With --library-demo: the stub answers the library commands with EXAMPLE data (320 sounds, 64 beats, 3 projects, a
# history, a review of a project, a look at the machine before a send), so that search, filters, the details panel and
# the dialogs can be looked at. Everything here is made up; nothing is read from the user's library.
LIBRARY_DEMO = r"""
(function () {
  const NAMES = ["Kick", "Snare", "HiHat", "Tom", "Clap", "Bass", "Lead", "Noise", "Rim", "Cymbal"];
  const TYPES = ["Kicks", "Snares", "HiHats", "Toms", "Percussion", "Bass", "Lead", "Noise", "Misc", "Cymbals"];
  const SOURCES = ["machine", "file import", "project", "found in folder"];
  const TAGS = ["808", "punchy", "dark", "analog", "layered", "favourite-kit"];
  const BASE = 1790000000;
  function item(kind, i) {
    const n = NAMES[i % NAMES.length];
    const folder = i % 5 === 0 ? "Project 2026-11-08_100000Z" : i % 7 === 0 ? "Pack A" : "";
    const display = kind === "sounds" ? n + " " + String(100 + (i * 7) % 300) + " v" + (1 + i % 4)
                  : kind === "beats" ? "Groove " + String(i + 1).padStart(2, "0") + " " + ["funk", "house", "break", "dub"][i % 4]
                  : "Project 2026-11-0" + (1 + i);
    return {
      id: kind[0] + (i + 1), kind, name: (folder ? folder + "/" : "") + display + ".syx", display, folder,
      size: kind === "sounds" ? 177 : 5200, hash: ("0123456789abcdef" + i).repeat(4).slice(0, 64),
      favourite: i % 9 === 0, tags: i % 3 === 0 ? [TAGS[i % TAGS.length]] : i % 8 === 0 ? [TAGS[1], TAGS[2], TAGS[3]] : [],
      note: i % 11 === 0 ? "from the 2026 session" : "", source: SOURCES[i % SOURCES.length],
      created: BASE + i * 7000, modified: BASE + i * 9000 + 3600, fetched: i % 4 === 0 ? BASE + i * 9000 : null,
      history: i % 6 === 0 ? 2 : 0, capture: kind === "projects" ? display + " (as received).syx" : null,
      beats: kind === "projects" ? 16 - i : null, sound_type: kind === "sounds" ? TYPES[i % TYPES.length] : "",
      osc: kind === "sounds" ? ["Shape " + (i % 40), "Shape " + ((i * 3) % 40), "Sample/Noise " + (i % 12), "Sample/Noise 0"] : [],
      slot: false,
    };
  }
  function items(kind) {
    const count = { sounds: 320, beats: 64, projects: 3 }[kind];
    const list = Array.from({ length: count }, (_, i) => item(kind, i));
    if (kind !== "projects") {
      const slot = item(kind, 0);
      Object.assign(slot, { id: "machine-" + kind, name: "", display: kind === "sounds" ? "Machine: current sound" : "Machine: current beat",
        folder: "", hash: "", size: 0, favourite: false, tags: [], note: "Kept automatically before something was sent to the machine.",
        source: "auto backup", history: 3, capture: null, beats: null, sound_type: "", osc: [], slot: true });
      list.unshift(slot);
    }
    return list;
  }
  const A = window.__previewAnswers;
  A.library = (a) => ({ event: "library", kind: a[0], files: items(a[0]), trash: 2 });
  A.lib_trash = () => ({ event: "lib_trash", items: [
    { id: "s900", kind: "sounds", name: "Old Kick", path: "Old Kick.syx", trashed: BASE + 500000 },
    { id: "b901", kind: "beats", name: "Groove 99", path: "Groove 99.syx", trashed: BASE + 600000 } ] });
  A.lib_history = (a) => ({ event: "lib_history", id: a[0], kind: "sounds", name: "Kick 100 v1", slot: a[0].startsWith("machine"), modified: BASE + 90000,
    versions: [ { index: 0, date: BASE + 20000, label: "replaced by an update on 2026-11-02 09:12 UTC", same_as_current: false },
                { index: 1, date: BASE + 50000, label: "renamed to \"Kick 100 v1\" on 2026-11-05 18:40 UTC", same_as_current: false },
                { index: 2, date: BASE + 70000, label: "auto backup 2026-11-07_201500Z", same_as_current: true } ] });
  A.lib_compare = (a) => ({ event: "lib_compare", id: a[0], version: Number(a[1]), date: BASE + 20000, label: "x",
    lines: ["Name: \"Kick 100\" -> \"Kick 100 v1\"", "Osc 1 Frequency: 36 -> 40", "LP Freq: 120 -> 96", "Env Amp Decay: 70 -> 59"] });
  A.fetch_project = () => ({ event: "lib_plan", title: "Project 2026-11-08_100000Z", source: "project", new: 3, identical: 261, modified: 3,
    entries: [
      { label: "01 Groove 01 funk", kind: "beats", verdict: "modified", name: "Groove 01 funk", place: "Groove 01 funk.syx", in_batch: false, existing_date: BASE + 50000, candidate_date: BASE + 400000, newer: true },
      { label: "02 Groove 02 house", kind: "beats", verdict: "modified", name: "Groove 02 house", place: "Groove 02 house.syx", in_batch: false, existing_date: BASE + 900000, candidate_date: BASE + 400000, newer: false },
      { label: "Project 2026-11-08_100000Z", kind: "projects", verdict: "modified", name: "Project 2026-11-01", place: "Project 2026-11-01.syx", in_batch: false, existing_date: BASE + 30000, candidate_date: BASE + 400000, newer: true },
      { label: "03 Groove 03 break", kind: "beats", verdict: "new", name: null, place: null, in_batch: false, existing_date: null, candidate_date: BASE + 400000, newer: true },
      { label: "04 Groove 04 dub", kind: "beats", verdict: "new", name: null, place: null, in_batch: false, existing_date: null, candidate_date: BASE + 400000, newer: true },
      { label: "05 Groove 05", kind: "beats", verdict: "new", name: null, place: null, in_batch: false, existing_date: null, candidate_date: BASE + 400000, newer: true },
      { label: "06 Groove 06 funk", kind: "beats", verdict: "identical", name: "Groove 06 funk", place: "Groove 06 funk.syx", in_batch: false, existing_date: BASE + 20000, candidate_date: BASE + 400000, newer: true },
      { label: "07 Groove 07 house", kind: "beats", verdict: "identical", name: "Groove 07 house", place: "Groove 07 house.syx", in_batch: false, existing_date: BASE + 20000, candidate_date: BASE + 400000, newer: true } ] });
  A.lib_send_check = (a) => ({ event: "lib_send_plan", kind: a[0], id: a[1], status: "unknown", changed: false, item: "Kick 100 v1", from: a[0] + "/Kick 100 v1.syx",
    machine: "Fake Kick", token: "a".repeat(64),
    lines: ["Sending replaces the machine's current sound \"Fake Kick\", which is NOT in your library. It is first saved as \"auto backup <date>\" in the history of the machine's current sound (Library > Auto backups), then \"Kick 100 v1\" is sent."] });
  // Sending a whole project: p1 may be misread by the machine's OS (warning), p2 uses no list, p3 is on an OS with the fix.
  const PROJECT_GUARD = { p1: "at_risk", p2: "no_risk", p3: "fixed" };
  A.lib_project_check = (a) => {
    const guard = PROJECT_GUARD[a[0]] || "at_risk";
    const partial = a[0] === "p3";
    return { event: "lib_project_plan", kind: "projects", id: a[0], version: a[1] === "-" ? null : Number(a[1]),
      status: "unknown", changed: false, item: "Project 2026-11-0" + a[0].slice(1), from: "projects/Project 2026-11-0" + a[0].slice(1) + ".syx",
      token: "b".repeat(64), beats: partial ? 12 : 16, total: 16, lists_used: [guard !== "no_risk", guard === "at_risk", false, false],
      guard, installed: guard === "fixed" ? "Tempester 1.6.2.0" : null,
      lines: ["Sending replaces the machine's whole project in RAM: its beats, their sounds and its 4 Play Lists. It is not saved to the machine's flash until you save it there.",
        ...(partial ? ["The file holds 12 of 16 beats: beat(s) 5, 6, 7, 8 are not sent and keep their current content on the machine."] : []),
        "The machine's current project is NOT in your library. It is first saved as \"auto backup <date>\" (Library > Projects > Auto backups), then the project is sent."],
      warning: guard === "at_risk" ? ["The machine's OS may misread Play Lists: lists after a used one can come back wrong; beats and sounds are not affected.",
        "The app has no record of the OS installed on the machine (the OS update page records it after a send).", "This project uses Play List(s) 1, 2."] : [],
      without_lists: ["Send without Play Lists: the project goes with its 4 Play Lists empty. The receiver reads empty lists correctly, so nothing can shift, but it also writes them: the machine's own 4 Play Lists become empty (the machine then fills an empty Play List 1 with the selected beat as step 1)."] };
  };
  A.lib_project_send = () => {
    const total = 124000, beats = 16;
    let message = 0;
    const emit = (m) => window.Tempester.bridge.receive(Object.assign({ event: "project_send", beats, total }, m));
    emit({ state: "started", message: 0, sent: 0 });
    window.__sendTimer = setInterval(() => {
      message += 1;
      if (message > beats) {
        clearInterval(window.__sendTimer);
        emit({ state: "done", sent: total });
        return;
      }
      emit({ state: "sending", message, sent: Math.round((total * message) / (beats + 1)) });
    }, 250);
    return { event: "lib_done", title: "Send project to the machine", lines: ["Sent (EXAMPLE): nothing was really sent."] };
  };
  A.lib_project_cancel = () => {
    clearInterval(window.__sendTimer);
    return { event: "project_send", state: "cancelled", beats: 16, sent: 41000, total: 124000 };
  };
  A.lib_project_verify = () => ({ event: "project_verify", state: "differ", beats_compared: 16,
    lines: ["The machine does NOT hold exactly what was sent: 3 difference(s)."],
    differences: ["Play List 2 differs: its End word, step(s) 1, 2, 3", "Play List 3 differs: its name", "Play List 4: empty in the sent project, used on the machine"],
    notes: [] });
  A.lib_rename = (a) => ({ event: "lib_done", title: "Rename", lines: ["Renamed sound \"Kick 100 v1\" to \"" + a[1] + "\": the name inside the data (20 characters, padded with spaces) and the file changed together. The previous version is kept in its history. To undo: rename it back, or restore the previous version from its History."] });
})();
"""

# Run after app.js: connect to the example port and load an example sound, so the panel is not all defaults.
BOOT = r"""
window.__previewSound = () => {
  const values = window.TEMPEST_PARAMS.map(([index, max, def]) => def);
  [[34, 96], [35, 70], [37, 180], [43, 90], [46, 110], [0, 40], [2, 30], [61, 20], [72, 60]]
    .forEach(([i, x]) => { values[i] = x; });
  values[92] = 6; values[93] = 170; values[94] = 13;
  return { event: "sound", name: "EXAMPLE Kick 1", values };
};
// The pad note table as Rust sends it at start (the default of tempest_proto::pad_notes, two-digit notes).
window.__previewPadNotes = () => ({ event: "pad_notes", notes: [36, 38, 40, 41, 43, 45, 47, 48, 50, 52, 53, 55, 57, 59,
  60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77] });
window.Tempester.bridge.receive(window.__previewPadNotes());
window.Tempester.bridge.receive({ event: "conn", state: "connected", port: "Tempest (EXAMPLE port)", fake: false });
window.Tempester.bridge.receive(window.__previewSound());
window.Tempester.bridge.receive({ event: "beat", name: "EXAMPLE Beat", events: 412, assigned: 9, pads: 32,
  size: 5200, voice_assign: Array.from({ length: 32 }, (_, i) => (i < 9 ? (i % 6) + 1 : 0)) });
"""


def params_js(table_path: Path) -> str:
    """`window.TEMPEST_PARAMS` as the Rust `params_js()` writes it."""
    table = json.loads(Path(table_path).read_text(encoding="utf-8"))
    rows = [f"[{p['index']},{p['max']},{p['default']}]" for p in table["params"]]
    if len(rows) != 127:
        raise ValueError(f"{table_path}: {len(rows)} parameters, 127 expected")
    return f"window.TEMPEST_PARAMS = [{','.join(rows)}];\n"


def versions_js(root: Path = ROOT) -> str:
    """`window.__previewVersions`: the versions the app offers (kinds release and corrections, newest first), read from
    the manifests like the generated Rust table, for the OS page's `os_versions` answer."""
    def key(spec: dict) -> list[int]:
        return [int(part) for part in spec["version"].split(".")]

    specs = [json.loads(path.read_text(encoding="utf-8")) for path in sorted((root / "versions").glob("*.json"))]
    shown = [
        {k: spec[k] for k in ("version", "name", "kind", "date", "summary", "changes", "description", "sha256_image", "sha256_syx")}
        for spec in sorted(specs, key=key, reverse=True)
        if spec.get("kind") in ("release", "corrections")
    ]
    return f"window.__previewVersions = {json.dumps(shown, ensure_ascii=False)};\n"


def inline_script(code: str) -> str:
    """A `<script>` element; `</script` inside the code would close it early."""
    return "<script>\n" + code.replace("</script", "<\\/script") + "\n</script>"


def inline_assets(html: str, ui_dir: Path, params_code: str) -> str:
    """Replaces every `<link rel=stylesheet>` and `<script src>` by the file's content."""

    def style(match: re.Match[str]) -> str:
        css = (ui_dir / match.group(1)).read_text(encoding="utf-8")
        return "<style>\n" + css + "\n</style>"

    def script(match: re.Match[str]) -> str:
        name = match.group(1)
        if name == "params.js":
            return inline_script(params_code)
        return inline_script((ui_dir / name).read_text(encoding="utf-8"))

    html = re.sub(r'<link rel="stylesheet" href="([\w.-]+)">', style, html)
    return re.sub(r'<script src="([\w.-]+)"></script>', script, html)


def start_page_js(page: str) -> str:
    """Opens the preview on `page` when the address names none (app.js reads the page from the #hash)."""
    if not re.fullmatch(r"[a-z]+", page):
        raise ValueError(f"page id expected, got {page!r}")
    return f'if (!location.hash) history.replaceState(null, "", "#{page}");\n'


def build(ui_dir: Path, table_path: Path, start: str = "panel", library_demo: bool = False) -> str:
    """The standalone preview page."""
    html = (ui_dir / "index.html").read_text(encoding="utf-8")
    html, csp = re.subn(r'<meta http-equiv="Content-Security-Policy"[^>]*>\n?', "", html)
    if csp != 1:
        raise ValueError("index.html: Content-Security-Policy meta not found")
    html = inline_assets(
        html, ui_dir, params_js(table_path) + versions_js() + STUB + (LIBRARY_DEMO if library_demo else "") + start_page_js(start)
    )
    if re.search(r'<script src=|<link rel="stylesheet"', html):
        raise ValueError("index.html: an asset was not inlined")
    html = html.replace("<body>", "<body>\n" + BANNER, 1)
    return html.replace("</body>", inline_script(BOOT) + "\n</body>", 1)


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description="Standalone preview of the Tempester page (example values).")
    parser.add_argument("out", type=Path)
    parser.add_argument("--ui", type=Path, default=DEFAULT_UI)
    parser.add_argument("--params", type=Path, default=DEFAULT_PARAMS)
    parser.add_argument("--start", default="panel", help="page shown first when the address has no #page")
    parser.add_argument("--library-demo", action="store_true", help="example library (320 sounds...) for the Library page")
    args = parser.parse_args(argv)
    args.out.write_text(build(args.ui, args.params, args.start, args.library_demo), encoding="utf-8")
    print(f"{args.out} ({args.out.stat().st_size} bytes): opens on #{args.start}; #machine, #play... for another")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
