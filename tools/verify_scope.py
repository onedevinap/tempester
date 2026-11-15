#!/usr/bin/env python3
"""Says which steps of tools/verify.sh a change can reach: `ui`, `docs` or `full`, with the reason.

usage: verify_scope.py <repository> <base ref>      prints `ui`, `docs` or `full`, then one line per reason
       verify_scope.py <repository> --ui-tests       the tests/test_*.py that read the app's page files
       verify_scope.py <repository> --self-test      the classifier's own checks and counter-proofs

A change is `ui` when every changed path is a page file of the app (app/crates/tempester/ui/, ui-tests/) or
documentation (as below, Python tools excluded: the firmware steps do not run), with at least one page file, and no
documentation file the app step reads. A page file reaches only the page tests of tools/test_app.sh (node, the
browser layout tests), the Rust tests of the module that embeds the page (src/ui: include_str! of every ui/ file,
the asset list compared with the folder) and the Python tests that read the page (--ui-tests): verify.sh runs those,
the tools compile step, and nothing of the firmware (tools/verify_ui_scope.sh).

A change is `docs` only when every changed path (commits since the merge base with <base ref>, staged and unstaged
edits, untracked files outside work/ and out/) is something the app step never reads: documentation (under Docs/, refs/, or any
*.md outside app/ and public-src/) or a Python / Ghidra tool of tools/, and not a file the app step reads. The files it
reads are found in the sources, not listed by hand: every quoted string naming refs/, Docs/ or tools/ in app/ (Rust
include_str! / include_bytes!, test paths, node arguments), every `$P/...` path in tools/test_app.sh, and every
`<name>.py` a read Python tool loads, transitively. The other steps of the verifier (versions, recipes, guard, Python
tools, firmware simulations) still run. Anything else, or no change at all, is `full`.
The merged tree always gets the full verifier before a push; this only spares an agent's documentation commits the
Rust and page tests (several minutes per copy of the repository)."""
import re
import subprocess
import sys
from pathlib import Path

DOC_PREFIXES = ('Docs/', 'refs/', 'tools/ghidra_scripts/')
QUOTED = re.compile(r'''["'`]([^"'`\n]*?(?:refs|Docs|tools)/[^"'`\n]*?)["'`]''')
# Untracked local data (decoded images, captures, exports, often links to the main checkout's): never part of a change.
LOCAL_DATA = ('work/', 'out/')
PY_NAME = re.compile(r'''['"]([A-Za-z0-9_]+\.py)['"]''')
SHELL_PATH = re.compile(r'\$P/([A-Za-z0-9_./-]+)')
SOURCE_SUFFIXES = {'.rs', '.js', '.toml', '.html', '.css'}
UI_PREFIXES = ('app/crates/tempester/ui/', 'app/crates/tempester/ui-tests/')
# A path to the page folder, written as one string or as path parts ('tempester' / 'ui').
READS_UI = re.compile(r'''tempester['"]?\s*/\s*['"]?ui(?![-\w])''')
TOOL_NAME = re.compile(r'''tools/([A-Za-z0-9_]+\.py)''')


def app_sources(root: Path):
    for path in (root / 'app').rglob('*'):
        if path.suffix in SOURCE_SUFFIXES and 'target' not in path.relative_to(root).parts:
            yield path


def normalise(text: str, base: Path, root: Path) -> str | None:
    """Repository-relative path named by `text`, read from a file in `base`."""
    tail = text[min(text.index(d) for d in ('refs/', 'Docs/', 'tools/') if d in text):]
    candidate = (base / text).resolve() if text.startswith('.') else (root / tail).resolve()
    try:
        return candidate.relative_to(root.resolve()).as_posix()
    except ValueError:
        return tail


def files_read_by_app(root: Path) -> set[str]:
    read = set()
    for path in app_sources(root):
        for text in QUOTED.findall(path.read_text(encoding='utf-8', errors='replace')):
            read.add(normalise(text, path.parent, root))
    test_app = (root / 'tools' / 'test_app.sh').read_text(encoding='utf-8')
    read.update(SHELL_PATH.findall(test_app))
    return read | python_closure(root, {p for p in read if p.startswith('tools/') and p.endswith('.py')})


def python_closure(root: Path, tools: set[str]) -> set[str]:
    """The tools loaded by `tools`, by name, transitively (spec_from_file_location, subprocess)."""
    seen, todo = set(), list(tools)
    while todo:
        tool = todo.pop()
        path = root / tool
        if tool in seen or not path.is_file():
            continue
        seen.add(tool)
        todo += [f'tools/{name}' for name in PY_NAME.findall(path.read_text(encoding='utf-8'))]
    return seen


def outside_app_step(path: str) -> bool:
    markdown = path.endswith('.md') and not path.startswith(('app/', 'public-src/'))
    python_tool = path.startswith('tools/') and path.count('/') == 1 and path.endswith('.py')
    return markdown or python_tool or path.startswith(DOC_PREFIXES)


def is_page_file(path: str) -> bool:
    return path.startswith(UI_PREFIXES)


def is_documentation(path: str) -> bool:
    markdown = path.endswith('.md') and not path.startswith(('app/', 'public-src/'))
    return markdown or path.startswith(DOC_PREFIXES)


def classify(changed: list[str], read: set[str]) -> tuple[str, list[str]]:
    if not changed:
        return 'full', ['no change found: nothing proves the app step can be skipped']
    pages = [p for p in changed if is_page_file(p)]
    others = [p for p in changed if not is_page_file(p)]
    if pages and all(is_documentation(p) and p not in read for p in others):
        return 'ui', [f'{len(pages)} page file(s) of the app, {len(others)} documentation file(s), nothing else']
    reasons = [f'{p}: may change the app step' for p in changed if not outside_app_step(p)]
    reasons += [f'{p}: read by the app or its tests' for p in changed if p in read]
    return ('full', reasons) if reasons else ('docs', [f'{len(changed)} file(s), none read by the app step'])


def git_lines(root: Path, *args: str) -> list[str]:
    out = subprocess.run(['git', '-C', str(root), *args], check=True, capture_output=True, text=True).stdout
    return [line for line in out.splitlines() if line]


def changed_paths(root: Path, base: str) -> list[str]:
    paths = set(git_lines(root, 'diff', '--name-only', '--no-renames', f'{base}...HEAD'))
    paths.update(git_lines(root, 'diff', '--name-only', '--no-renames', 'HEAD'))
    untracked = git_lines(root, 'ls-files', '--others', '--exclude-standard')
    paths.update(p for p in untracked if not p.startswith(LOCAL_DATA))
    return sorted(paths)


def ui_tests(root: Path) -> list[str]:
    """The tests/test_*.py that read the page folder, themselves or through a tool they load."""
    found = []
    for test in sorted((root / 'tests').glob('test_*.py')):
        text = test.read_text(encoding='utf-8')
        tools = set(TOOL_NAME.findall(text)) | set(PY_NAME.findall(text))
        texts = [text] + [(root / 'tools' / t).read_text(encoding='utf-8') for t in sorted(tools)
                          if (root / 'tools' / t).is_file()]
        if any(READS_UI.search(t) for t in texts):
            found.append(test.relative_to(root).as_posix())
    return found


def self_test(root: Path) -> int:
    read = files_read_by_app(root)
    css = 'app/crates/tempester/ui/compact.css'
    rust = 'app/crates/tempester/src/ui/assets.rs'
    tests = ui_tests(root)
    cases = [
        ('scanner finds refs/panel_keys.tsv', 'refs/panel_keys.tsv' in read),
        ('scanner finds refs/panel_controls.tsv', 'refs/panel_controls.tsv' in read),
        ('docs only', classify(['Docs/a.md', 'TODO.md', 'refs/firmware-voice.md'], read)[0] == 'docs'),
        ('no change refused', classify([], read)[0] == 'full'),
        ('a table the app embeds refused', classify(['Docs/a.md', 'refs/panel_keys.tsv'], read)[0] == 'full'),
        ('scanner follows recipe.py from generate_rust_tables.py', 'tools/recipe.py' in read),
        ('scanner finds build_version.py in the Rust tests', 'tools/build_version.py' in read),
        ('a tool the app step runs refused', classify(['tools/recipe.py'], read)[0] == 'full'),
        ('a Voice tool accepted', classify(['tools/voice_compare.py', 'tools/README.md'], read)[0] == 'docs'),
        ('the verifier itself refused', classify(['tools/verify.sh'], read)[0] == 'full'),
        ('app code refused', classify(['app/crates/tempester/src/main.rs'], read)[0] == 'full'),
        ('a recipe refused', classify(['patch/beat_frame.json'], read)[0] == 'full'),
        ('public docs refused', classify(['public-src/README.md'], read)[0] == 'full'),
        ('markdown inside the app refused', classify(['app/crates/tempester/README.md'], read)[0] == 'full'),
        ('a planted read of a doc refused', classify(['Docs/x.md'], read | {'Docs/x.md'})[0] == 'full'),
        ('a page stylesheet alone: ui', classify([css], read)[0] == 'ui'),
        ('a page test with the changelog: ui', classify(['app/crates/tempester/ui-tests/os-cards.test.js',
                                                          'CHANGELOG.md', 'TODO.md'], read)[0] == 'ui'),
        ('a page file and a .rs file: full', classify([css, rust], read)[0] == 'full'),
        ('a page file and a Python tool: full', classify([css, 'tools/preview_ui.py'], read)[0] == 'full'),
        ('a page file and a recipe: full', classify([css, 'patch/beat_frame.json'], read)[0] == 'full'),
        ('a page file and a version: full', classify([css, 'versions/1.6.8.0.json'], read)[0] == 'full'),
        ('a page file and a table the page tests read: full',
         classify([css, 'refs/panel_controls.tsv'], read)[0] == 'full'),
        ('a page file and the app README: full', classify([css, 'app/crates/tempester/README.md'], read)[0] == 'full'),
        ('documentation alone stays docs', classify(['Docs/a.md'], read)[0] == 'docs'),
        ('the page tests found: test_preview_ui.py (through tools/preview_ui.py)', 'tests/test_preview_ui.py' in tests),
        ("the page tests found: test_ui_text_size.py (path in parts, 'tempester' / 'ui')",
         'tests/test_ui_text_size.py' in tests),
        ('a test that reads no page left out: test_manifests.py', 'tests/test_manifests.py' not in tests),
    ]
    failed = [name for name, passed in cases if not passed]
    for name in failed:
        print(f'  ✘ verify_scope: {name}')
    return 1 if failed else 0


def main() -> int:
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    root = Path(sys.argv[1]).resolve()
    if sys.argv[2] == '--self-test':
        return self_test(root)
    if sys.argv[2] == '--ui-tests':
        print('\n'.join(ui_tests(root)))
        return 0
    scope, reasons = classify(changed_paths(root, sys.argv[2]), files_read_by_app(root))
    print(scope)
    for reason in reasons:
        print(f'  {reason}')
    return 0


if __name__ == '__main__':
    sys.exit(main())
