#!/usr/bin/env python3
"""Guard: refuse any file that would publish DSI firmware content.

usage: dsi_content_guard.py <folder of the DSI .syx files> [<file or folder>…] [--quiet]

The DSI folder holds Tempest_Main_*.syx, Tempest_Voice_*.syx, Tempest_Panel_*.syx, Tempest_Sam_*.syx (the four
images are decoded in memory, never written). Without paths, scans what would be published from this repository:
patch/, versions/, tools/, app/ (build folders skipped), Docs/, refs/, tests/, profile/ and the top-level *.md files.
Fails (exit 1) and names file, line and DSI location on:
  1. a .syx or .bin file;
  2. any 8-byte sequence of the file, at any byte offset, equal to an 8-byte sequence at a 4-byte-aligned offset of
     a DSI image, unless that pair of words is commonplace (tools/recipe.py: it occurs at 16 places or more in the
     four images, or both words are trivial: nop, erased flash, jr ra, constant load, register move, stack-frame
     slot, one word repeated, two addresses, at most 3 non-zero bytes or 3 distinct byte values). A sequence made only of printable
     characters is text: 8 characters of ordinary words match DSI's strings by chance, so it is not refused
     (unless --strict-text); a quote of one DSI string of 24 characters or more (a menu label, a message quoted by
     the documentation) is listed as a note;
  3. a quoted run of DSI words in a text file: hexadecimal written out as words (8 digits, with or without 0x, read
     both in byte order and as a little-endian value), byte lists ([0x12, 0x34, …]) or spaced bytes (12 34 56 …),
     gathered per label (the identifier before the value on its line, e.g. "origine", "word", "new", or the column)
     in file order, and per line, then checked as in 2. So two DSI words kept as two consecutive JSON fields, or
     written one after the other in a sentence, are caught. A blank line ends a run: values on either side of an
     empty line are never joined (two recipes of a generated table, two paragraphs of a document), each side being
     checked on its own.
The threshold and its justification: Docs/patch-recipe-format.md ("What the guard refuses")."""
import importlib.util
import re
import struct
import subprocess
import sys
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
_spec = importlib.util.spec_from_file_location('recipe', Path(__file__).resolve().parent / 'recipe.py')
recipe = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(recipe)

DEFAULT_PATHS = ('patch', 'versions', 'tools', 'app', 'Docs', 'refs', 'tests', 'profile',
                 'README.md', 'CHANGELOG.md', 'CONTRIBUTING.md', 'SECURITY.md', 'docs')
SKIPPED_DIRS = {'.git', 'target', 'node_modules', '__pycache__', 'dist'}
FIRMWARE_SUFFIXES = {'.syx', '.bin'}
OS_FILES = ('Main', 'Voice', 'Panel', 'Sam')
RAW_BLOCK = 1171
BYTE_LIST = re.compile(r'\[\s*(0x[0-9a-fA-F]{2}(?:\s*,\s*0x[0-9a-fA-F]{2})+)\s*,?\s*\]')
HEX_RUN = re.compile(r'(?<![0-9A-Za-z_])(0x)?([0-9a-fA-F]{8,})(?![0-9A-Za-z_])')
SPACED_BYTES = re.compile(r'(?<![0-9A-Za-z_])((?:[0-9a-fA-F]{2}[ ,]){7,}[0-9a-fA-F]{2})(?![0-9A-Za-z_])')
IDENTIFIER = re.compile(r'(?<![A-Za-z_0-9])[A-Za-z_][A-Za-z_0-9]*')
HEX_WORD = re.compile(r'[0-9a-fA-F]{8,}')
PRINTABLE = set(range(0x20, 0x7F)) | {0x09, 0x0A, 0x0D}
TEXT_QUOTE = 24  # characters: a note for a quote of a DSI string at least this long


def unpack_os(syx: bytes) -> bytes:
    """A DSI OS file (F0 01 28 <id>, blocks of 1024 bytes each packed 7-bit on its own) decoded to its image."""
    if syx[:3] != b'\xf0\x01\x28' or syx[-1] != 0xF7:
        raise SystemExit('not a DSI OS file')
    body = syx[4:-1]
    out = bytearray()
    for start in range(0, len(body) - len(body) % RAW_BLOCK, RAW_BLOCK):
        block = body[start:start + RAW_BLOCK]
        for i in range(0, len(block), 8):
            group = block[i:i + 8]
            out += bytes(c | ((group[0] >> j) & 1) << 7 for j, c in enumerate(group[1:]))
    return bytes(out)


class Dsi:
    """The distinctive 8-byte sequences of the DSI images, keyed as native 64-bit integers."""

    def __init__(self, folder: Path) -> None:
        self.keys: dict[int, tuple[str, int]] = {}
        self.images: dict[str, bytes] = {}
        counts: Counter = Counter()
        firsts: dict[tuple[int, int], tuple[str, int]] = {}
        for name in OS_FILES:
            for path in sorted(folder.glob(f'Tempest_{name}_*.syx')):
                image = unpack_os(path.read_bytes())
                self.images[path.name] = image
                words = struct.unpack(f'<{len(image) // 4}I', image)
                for i, pair in enumerate(zip(words, words[1:])):
                    counts[pair] += 1
                    firsts.setdefault(pair, (path.name, 4 * i))
        if not any(n.startswith('Tempest_Main_') for n in self.images):
            raise SystemExit(f'refused: no Tempest_Main_*.syx in {folder}')
        for (first, second), count in counts.items():
            if not recipe.commonplace(first, second, count):
                key = int.from_bytes(struct.pack('<II', first, second), sys.byteorder)
                self.keys[key] = firsts[(first, second)]

    def quote_length(self, data: bytes, offset: int, where: tuple[str, int]) -> int:
        """How many bytes of data, from offset, equal the DSI image from the matched location on."""
        image, start = self.images[where[0]], where[1]
        length = 0
        while offset + length < len(data) and start + length < len(image) \
                and data[offset + length] == image[start + length]:
            length += 1
        return length

    def hits(self, data: bytes) -> list[tuple[int, tuple[str, int]]]:
        """(byte offset, DSI location) of every 8-byte window of data equal to a distinctive DSI sequence."""
        out = []
        view = memoryview(data)
        for shift in range(8):
            length = (len(data) - shift) // 8 * 8
            if length <= 0:
                continue
            values = view[shift:shift + length].cast('Q')
            if self.keys.keys().isdisjoint(values):
                continue
            out += [(shift + 8 * i, self.keys[v]) for i, v in enumerate(values) if v in self.keys]
        return sorted(out)


def label(prefix: str, column: int) -> str:
    """The stream a value belongs to: its table column, or the last identifier before it on its line (a key such
    as "origine" or "word"), or its rank among the values of the line."""
    if prefix.lstrip().startswith('|'):
        return f'table column {prefix.count("|")}'
    names = [n for n in IDENTIFIER.findall(prefix) if not HEX_WORD.fullmatch(n)]
    return names[-1] if names else f'column {column}'


def text_streams(text: str) -> dict[str, list[tuple[bytes, int, bool, int]]]:
    """Per label, the hexadecimal quoted in the text as (chunk of bytes, line, made of words, paragraph) in file
    order; words written out give two streams (as written, and as little-endian values). The paragraph number
    changes at every blank line: check_text never joins two chunks of different paragraphs."""
    streams: dict[str, list[tuple[bytes, int, bool, int]]] = defaultdict(list)
    paragraph = 0
    for number, line in enumerate(text.splitlines(), 1):
        if not line.strip():
            paragraph += 1
            continue
        taken: list[tuple[int, int]] = []
        found: list[tuple[int, str, bytes, bool]] = []
        for match in BYTE_LIST.finditer(line):
            data = bytes(int(t, 16) for t in re.findall(r'0x([0-9a-fA-F]{2})', match.group(1)))
            found.append((match.start(), line[:match.start()], data, False))
            taken.append(match.span())
        for match in SPACED_BYTES.finditer(line):
            if not any(a <= match.start() < b for a, b in taken):
                found.append((match.start(), line[:match.start()], bytes.fromhex(re.sub('[ ,]', '', match.group(1))), False))
                taken.append(match.span())
        for match in HEX_RUN.finditer(line):
            digits = match.group(2)
            if any(a <= match.start() < b for a, b in taken) or len(digits) % 2:
                continue
            found.append((match.start(), line[:match.start()], bytes.fromhex(digits), len(digits) % 8 == 0))
        for column, (_, prefix, data, words) in enumerate(sorted(found, key=lambda f: f[0])):
            name = label(prefix, column)
            swapped = b''.join(data[i:i + 4][::-1] for i in range(0, len(data), 4)) if words else data
            for stream in (name, f'line {number}'):
                streams[stream + ' (as written)'].append((data, number, words, paragraph))
                streams[stream + ' (as values)'].append((swapped, number, words, paragraph))
    return streams


def check_text(path: Path, text: str, dsi: Dsi) -> list[str]:
    problems = []
    for label, chunks in text_streams(text).items():
        for paragraph in sorted({c[3] for c in chunks}):
            run = [c for c in chunks if c[3] == paragraph]
            joined = b''.join(c for c, _, _, _ in run)
            lines = [number for chunk, number, _, _ in run for _ in chunk]
            # Words written out are compared word against word; byte lists and spaced bytes at every offset.
            step = 4 if all(words for _, _, words, _ in run) else 1
            for offset, where in dsi.hits(joined):
                if offset % step:
                    continue
                problems.append(f'{path}:{lines[offset]}: quoted DSI words ({label}) {joined[offset:offset + 8].hex()} '
                                f'= {where[0]} offset {where[1]:#x}')
    return problems


def check_file(path: Path, dsi: Dsi, strict_text: bool) -> tuple[list[str], list[str]]:
    if path.suffix.lower() in FIRMWARE_SUFFIXES:
        return [f'{path}: firmware file ({path.suffix})'], []
    data = path.read_bytes()
    try:
        text = data.decode('utf-8')
    except UnicodeDecodeError:
        text = None
    problems, notes = [], []
    quoted_until = -1
    for offset, where in dsi.hits(data):
        window = data[offset:offset + 8]
        line = data.count(b'\n', 0, offset) + 1
        message = f'{path}:{line}: bytes {window.hex()} = {where[0]} offset {where[1]:#x}'
        if not set(window) <= PRINTABLE or strict_text:
            problems.append(message)
            continue
        # Printable text (in a text file, or the text of a PDF or an icon): 8 characters of ordinary words match
        # DSI's strings by chance ("original", "referenc"); only a longer quote of one DSI string is worth a note.
        length = dsi.quote_length(data, offset, where)
        if length >= TEXT_QUOTE and offset > quoted_until:
            notes.append(f'{path}:{line}: {length} characters of DSI text '
                         f'{data[offset:offset + length].decode(errors="replace")!r}')
            quoted_until = offset + length
    if text is not None:
        problems += check_text(path, text, dsi)
    return problems, notes


def files_under(paths: list[Path]) -> list[Path]:
    out = []
    for path in paths:
        if path.is_file():
            out.append(path)
        elif path.is_dir():
            for child in sorted(path.rglob('*')):
                if child.is_file() and not SKIPPED_DIRS.intersection(child.relative_to(path).parts):
                    out.append(child)
    return out


def tracked_files() -> list[Path]:
    """The default scope: what the repository versions (and so could publish) under DEFAULT_PATHS. Files git ignores
    (local builds, captures) are never published, so they are not this guard's business; an exported tree given as an
    explicit path is walked on disk instead."""
    listed = subprocess.run(['git', '-C', str(ROOT), 'ls-files', '-z', '--', *DEFAULT_PATHS],
                            capture_output=True, check=True).stdout.split(b'\0')
    return [ROOT / name.decode() for name in listed if name]


def scan(dsi: Dsi, paths: list[Path] | None, strict_text: bool = False) -> tuple[list[Path], list[str], list[str]]:
    files = files_under(paths) if paths else tracked_files()
    problems, notes = [], []
    for path in files:
        p, n = check_file(path, dsi, strict_text)
        problems += p
        notes += n
    return files, problems, notes


def main() -> None:
    args = sys.argv[1:]
    quiet = '--quiet' in args
    strict_text = '--strict-text' in args
    args = [a for a in args if a not in ('--quiet', '--strict-text')]
    if not args:
        sys.exit(__doc__)
    dsi = Dsi(Path(args[0]))
    paths = [Path(a) for a in args[1:]] or None
    files, problems, notes = scan(dsi, paths, strict_text)
    if notes and not quiet:
        print(f'{len(notes)} notes (DSI text quoted, at least {TEXT_QUOTE} characters; refused with --strict-text):')
        print('\n'.join(f'  {n}' for n in notes))
    if problems:
        print('\n'.join(problems))
        sys.exit(f'refused: {len(problems)} DSI fragments in {len(files)} files (images: {", ".join(dsi.images)})')
    print(f'no DSI content in {len(files)} files (images: {", ".join(dsi.images)}; {len(notes)} notes)')


if __name__ == '__main__':
    main()
