#!/usr/bin/env python3
"""Test of tools/preview_ui.py: the preview inlines every asset of the app's page, with the parameter table.

usage: python3 -I test_preview_ui.py   (writes only in a temporary folder)
Counter-proofs: a page whose CSP meta is missing, or that names an asset the tool cannot inline, is refused."""
import importlib.util
import re
import shutil
import sys
import tempfile
from pathlib import Path

sys.dont_write_bytecode = True  # no __pycache__ left in tools/
ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location("preview_ui", ROOT / "tools/preview_ui.py")
preview = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(preview)


def check_build() -> None:
    html = preview.build(preview.DEFAULT_UI, preview.DEFAULT_PARAMS)
    assert "<script src=" not in html and 'rel="stylesheet"' not in html, "an asset was left as a link"
    assert "Content-Security-Policy" not in html
    rows = re.search(r"window\.TEMPEST_PARAMS = \[(.*?)\];", html).group(1)
    assert rows.startswith("[0,120,36],[1,100,50]") and "[34,164,164]" in rows, rows[:80]
    assert rows.count("[") == 127
    for name in ("panel.js", "page-play.js", "app.js"):
        first_line = (preview.DEFAULT_UI / name).read_text(encoding="utf-8").splitlines()[0]
        assert first_line in html, f"{name} not inlined"
    assert "EXAMPLE" in html and "PREVIEW" in html
    print("preview_ui: OK (all assets inlined, 127 parameters, example values marked)")


def refused(ui_dir: Path) -> bool:
    try:
        preview.build(ui_dir, preview.DEFAULT_PARAMS)
    except (ValueError, FileNotFoundError):
        return True
    return False


def check_counter_proofs() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        ui = Path(tmp) / "ui"
        shutil.copytree(preview.DEFAULT_UI, ui)
        index = ui / "index.html"
        original = index.read_text(encoding="utf-8")
        index.write_text(re.sub(r'<meta http-equiv="Content-Security-Policy"[^>]*>', "", original))
        assert refused(ui), "counter-proof: a page without its CSP meta must be refused"
        index.write_text(original.replace('<script src="app.js">', '<script src="missing.js">'))
        assert refused(ui), "counter-proof: a missing script must be refused"
    print("preview_ui: OK (page without CSP and missing script refused)")


if __name__ == "__main__":
    check_build()
    check_counter_proofs()
