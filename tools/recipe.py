#!/usr/bin/env python3
"""Tempester patch recipes (patch/*.json, format "tempester-recipe/1"): load, resolve, apply, and write them.

A recipe never holds a byte of DSI's firmware. It holds our own words as literal bytes, and every word derived from
DSI's code as an operation on the user's own image (copy a word, copy a word with an instruction field changed, a
jump computed from its target). The image it applies to is pinned by its SHA-256 (DSI Main 1.5.0.2). The format is
specified in Docs/patch-recipe-format.md; the provenance of every word in Docs/analyses/patch-provenance.md.

Library, not a script: the tools load it by path (python3 -I does not put the script folder on sys.path):
    spec = importlib.util.spec_from_file_location('recipe', Path(__file__).resolve().parent / 'recipe.py')
Used by apply_patch.py, build_version.py, generate_rust_tables.py, classify_patches.py, patch_version.py,
asm_generate_json.py (through recipe_writer.py) and dsi_content_guard.py (the commonplace rule).

Semantics, shared with the app (app/crates/tempest-proto/src/versions.rs):
  - every read (copy, set) is from the ORIGINAL input image, never from a partly patched one: recipes are
    independent of each other and of their order;
  - a word may be written by one edit of one recipe only (overlap refused), never in the flasher page
    0x9D07F000-0x9D07FFFF, and must differ from the original word (an edit without effect is refused);
  - the input image must have the recipe's input_sha256 before anything is written."""
import hashlib
import json
import re
import struct
from dataclasses import dataclass
from pathlib import Path

FORMAT = 'tempester-recipe/1'
BASE = 0x9D000000
FLASHER_PAGE = 0x9D07F000
ERASED = 0xFFFFFFFF
NOP = 0x00000000
JR_RA = 0x03E00008
# Instruction fields a "set" may change: name -> (lowest bit, width). Bits are numbered from 0 (least significant).
FIELDS = {'rs': (21, 5), 'rt': (16, 5), 'rd': (11, 5), 'sa': (6, 5), 'imm': (0, 16)}
OPS = ('word', 'copy', 'j', 'jal')
EDIT_KEYS = {'at', 'count', 'set', 'why', *OPS}
# A pair of words that occurs this many times or more in DSI's images is a compiler idiom (prologue, epilogue,
# nop padding, erased flash), not an arrangement specific to DSI's code: see Docs/patch-recipe-format.md.
COMMONPLACE = 16
SP = 29
ADDRESS_RANGES = ((0x9D000000, 0x9D080000), (0xBD000000, 0xBD080000), (0x80000000, 0x80020000),
                  (0xA0000000, 0xA0020000))


class RecipeError(Exception):
    """A recipe that is malformed, or that does not apply to the given image."""


def is_trivial(w: int) -> bool:
    """A word that only encodes a constant, a register copy or a stack-frame slot, whoever wrote it: nop, erased
    flash, jr ra, li / ori from zero, lui, move / clear (addu, or with zero), addiu sp,sp,N, lw / sw r,N(sp)."""
    op, rs, rt = w >> 26, (w >> 21) & 0x1F, (w >> 16) & 0x1F
    if w in (NOP, ERASED, JR_RA):
        return True
    if op in (0x09, 0x0D, 0x0F) and rs == 0:
        return True
    if op == 0 and rt == 0 and (w >> 6) & 0x1F == 0 and w & 0x3F in (0x21, 0x25):
        return True
    return (op == 0x09 and rs == SP and rt == SP) or (op in (0x23, 0x2B) and rs == SP)


def is_address(w: int) -> bool:
    """An address of the PIC32MX695F512L: program flash (cached or not) or RAM (cached or not)."""
    return any(lo <= w < hi for lo, hi in ADDRESS_RANGES)


def commonplace(first: int, second: int, occurrences: int) -> bool:
    """Two consecutive words that may appear anywhere without quoting DSI's code:
      - a pair DSI's images hold at COMMONPLACE places or more (a compiler idiom);
      - two trivial words (is_trivial: constants, register moves, stack-frame slots);
      - one word repeated;
      - two addresses (they name places, as every analysis does; a table of them is not code);
      - 8 bytes of which at most 3 are not zero, or which take at most 3 distinct values (little information:
        small table entries, headers and byte patterns match any data by chance)."""
    data = struct.pack('<II', first, second)
    low_information = sum(1 for b in data if b) <= 3 or len(set(data)) <= 3
    return (occurrences >= COMMONPLACE or (is_trivial(first) and is_trivial(second)) or first == second
            or (is_address(first) and is_address(second)) or low_information)


def word_at(image: bytes, address: int) -> int:
    offset = address - BASE
    if address % 4 or not 0 <= offset <= len(image) - 4:
        raise RecipeError(f'{address:#x} is outside the image or not word aligned')
    return struct.unpack_from('<I', image, offset)[0]


def parse_address(text: str, where: str) -> int:
    if not isinstance(text, str) or not re.fullmatch(r'0x[0-9a-f]{8}', text):
        raise RecipeError(f'{where}: address {text!r} is not "0x" + 8 lowercase hex digits')
    return int(text, 16)


def jump_word(at: int, target: int, link: bool) -> int:
    """MIPS j / jal: the 256 MB region comes from the delay slot address, the target must be word aligned."""
    if target % 4 or (target & 0xF0000000) != ((at + 4) & 0xF0000000):
        raise RecipeError(f'{at:#x}: jump target {target:#x} not reachable by a j/jal')
    return (3 if link else 2) << 26 | (target >> 2) & 0x03FFFFFF


def check_fields(fields: object, where: str) -> dict[str, int]:
    if not isinstance(fields, dict) or not fields:
        raise RecipeError(f'{where}: "set" must be a non-empty object')
    for name, value in fields.items():
        if name not in FIELDS:
            raise RecipeError(f'{where}: unknown field {name!r} (known: {", ".join(FIELDS)})')
        width = FIELDS[name][1]
        if not isinstance(value, int) or isinstance(value, bool) or not 0 <= value < 1 << width:
            raise RecipeError(f'{where}: {name} = {value!r} does not fit {width} bits')
    return fields


def set_fields(word: int, fields: dict[str, int]) -> int:
    for name, value in fields.items():
        lo, width = FIELDS[name]
        word = word & ~(((1 << width) - 1) << lo) | value << lo
    return word


@dataclass(frozen=True, slots=True)
class Edit:
    """One edit, checked without any image: kind is 'word', 'copy', 'j' or 'jal'; value is the literal word (its
    little-endian value), the copy source or the jump target."""
    at: int
    kind: str
    value: int
    count: int
    fields: dict


def parse_edit(edit: dict, where: str) -> Edit:
    unknown = set(edit) - EDIT_KEYS
    if unknown:
        raise RecipeError(f'{where}: unknown keys {sorted(unknown)}')
    at = parse_address(edit.get('at'), where)
    ops = [k for k in OPS if k in edit]
    if len(ops) != 1:
        raise RecipeError(f'{where}: exactly one of {", ".join(OPS)} expected, found {ops or "none"}')
    kind = ops[0]
    count = edit.get('count', 1)
    if not isinstance(count, int) or isinstance(count, bool) or count < 1:
        raise RecipeError(f'{where}: count {count!r} is not a positive integer')
    if count != 1 and (kind in ('j', 'jal') or 'set' in edit):
        raise RecipeError(f'{where}: count is only allowed on a literal word or a plain copy')
    if 'set' in edit and kind != 'copy':
        raise RecipeError(f'{where}: "set" is only allowed on a copy')
    fields = check_fields(edit['set'], where) if 'set' in edit else {}
    if kind == 'word':
        if not isinstance(edit['word'], str) or not re.fullmatch(r'[0-9a-f]{8}', edit['word']):
            raise RecipeError(f'{where}: word {edit["word"]!r} is not 8 lowercase hex digits (byte order)')
        return Edit(at, kind, int.from_bytes(bytes.fromhex(edit['word']), 'little'), count, fields)
    value = parse_address(edit[kind], where)
    if kind in ('j', 'jal'):
        jump_word(at, value, kind == 'jal')
    return Edit(at, kind, value, count, fields)


def edit_words(edit: dict, image: bytes, where: str) -> list[tuple[int, int]]:
    """The (address, new word) pairs one edit writes, read from the original image where needed."""
    e = parse_edit(edit, where)
    if e.kind == 'word':
        return [(e.at + 4 * i, e.value) for i in range(e.count)]
    if e.kind in ('j', 'jal'):
        return [(e.at, jump_word(e.at, e.value, e.kind == 'jal'))]
    return [(e.at + 4 * i, set_fields(word_at(image, e.value + 4 * i), e.fields)) for i in range(e.count)]


def check_shape(recipe: dict, name: str) -> None:
    if recipe.get('format') != FORMAT:
        raise RecipeError(f'{name}: not a {FORMAT} file (format = {recipe.get("format")!r})')
    for key in ('name', 'image', 'input_sha256', 'explanation', 'edits'):
        if key not in recipe:
            raise RecipeError(f'{name}: "{key}" missing')
    if not re.fullmatch(r'[0-9a-f]{64}', recipe['input_sha256']):
        raise RecipeError(f'{name}: input_sha256 is not 64 lowercase hex digits')
    if not isinstance(recipe['edits'], list) or not recipe['edits']:
        raise RecipeError(f'{name}: "edits" must be a non-empty list')


def load(path: Path) -> dict:
    recipe = json.loads(Path(path).read_text())
    check_shape(recipe, str(path))
    return recipe


def resolve(recipe: dict, image: bytes, name: str = 'recipe') -> list[tuple[int, int]]:
    """Every (address, new word) of a recipe on the original image; refuses a wrong image, overlap inside the
    recipe, the flasher page, an address outside the image and a word that changes nothing."""
    check_shape(recipe, name)
    found = hashlib.sha256(image).hexdigest()
    if found != recipe['input_sha256']:
        raise RecipeError(f'{name}: the image is not {recipe["image"]} (SHA-256 {found})')
    words: list[tuple[int, int]] = []
    seen: set[int] = set()
    for i, edit in enumerate(recipe['edits']):
        where = f'{name} edit {i} ({edit.get("at")})'
        for address, value in edit_words(edit, image, where):
            if address >= FLASHER_PAGE:
                raise RecipeError(f'{where}: touches the flasher page ({address:#x})')
            original = word_at(image, address)
            if address in seen:
                raise RecipeError(f'{where}: {address:#x} written twice')
            if value == original:
                raise RecipeError(f'{where}: {address:#x} changes nothing')
            seen.add(address)
            words.append((address, value))
    return words


def apply(recipes: list[tuple[str, dict]], image: bytes) -> tuple[bytes, dict[int, str]]:
    """Applies named recipes to the original image; refuses two recipes writing the same word."""
    out = bytearray(image)
    owners: dict[int, str] = {}
    for name, recipe in recipes:
        for address, value in resolve(recipe, image, name):
            if address in owners:
                raise RecipeError(f'{address:#x} written by {owners[address]} and {name}')
            owners[address] = name
            struct.pack_into('<I', out, address - BASE, value)
    return bytes(out), owners


def dump(recipe: dict) -> str:
    """The canonical text of a recipe file: one edit per line, so that a diff shows one word per line."""
    head = {k: v for k, v in recipe.items() if k != 'edits'}
    text = json.dumps(head, ensure_ascii=False, indent=1)[:-2]
    edits = ',\n'.join('  ' + json.dumps(e, ensure_ascii=False) for e in recipe['edits'])
    return f'{text},\n "edits": [\n{edits}\n ]\n}}\n'
