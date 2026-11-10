#!/usr/bin/env python3
"""Writes the patch that changes the number shown in "Main OS Version" (System and Debug screens).

usage: patch_version.py <stock Main_1.5.0.2.bin> <version a.b.c.d> <description> > patch/version_<version>.json

The four digits are "li a0,N" passed one by one to the display of each screen (DIGITS: their edit positions); each digit
goes from 0 to 16383 (0x3FFF): n°39 (patch/version_request.s) reads these words at run time and announces each digit
as two 7-bit bytes, so this file is the one source of the announced version too. Only the digits that change are
written; the stock digits are read back from the image and checked (1.5.0.2). Two version patches touch the same
instructions: a version holds only one.
Output: a recipe (tools/recipe_writer.py, Docs/patch-recipe-format.md), without any DSI byte."""
import importlib.util
import struct
import sys
from pathlib import Path

_spec = importlib.util.spec_from_file_location('recipe_writer', Path(__file__).resolve().parent / 'recipe_writer.py')
writer = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(writer)

BASE = 0x9D000000
STOCK = (1, 5, 0, 2)
DIGITS = {'system_screen': (0x9D007758, 0x9D007780, 0x9D0077A0, 0x9D0077C0),
            'debug_screen': (0x9D009DA0, 0x9D009DC8, 0x9D009DE8, 0x9D009E08)}
SCREEN_LABELS = {'system_screen': 'system screen', 'debug_screen': 'debug screen'}
DIGIT_MAX = 0x3FFF                  # two 7-bit data bytes in n°39's answer


def word_at(image: bytes, address: int) -> bytes:
    return image[address - BASE: address - BASE + 4]


def load_immediate(image: bytes, n: int) -> bytes:
    if n == 0:
        return word_at(image, DIGITS['system_screen'][2])      # "clear a0" as the compiler wrote it
    return struct.pack('<I', 0x24040000 | n)                  # li a0,n (addiu a0,zero,n)


def patch(image: bytes, version: str, description: str) -> str:
    digits = tuple(int(x) for x in version.split('.'))
    if len(digits) != 4 or not all(0 <= c <= DIGIT_MAX for c in digits):
        raise SystemExit(f'expected version: a.b.c.d, digits from 0 to {DIGIT_MAX}')
    words = []
    for ecran, addresses in DIGITS.items():
        label = SCREEN_LABELS[ecran]
        for i, (address, old, new) in enumerate(zip(addresses, STOCK, digits)):
            if word_at(image, address) != load_immediate(image, old):
                raise SystemExit(f'{hex(address)}: not the expected Main 1.5.0.2 image')
            if old != new:
                value = int.from_bytes(load_immediate(image, new), 'little')
                words.append(writer.Word(address, value, f'{label}, digit {i + 1}: {old} -> {new}'))
    head = {'name': f'Tempester: version number {version} ({description})', 'image': 'Main 1.5.0.2',
            'explanation': f'The « Main OS Version » screens (System and Debug) show '
                           f'{version} instead of 1.5.0.2. A number never used by DSI. Exclusive of the other '
                           f'patch/version_*.json (same instructions).'}
    recipe, _ = writer.encode(writer.Image(image), words, head)
    return writer.recipe.dump(recipe)


if __name__ == '__main__':
    image = Path(sys.argv[1]).read_bytes()
    sys.stdout.write(patch(image, sys.argv[2], sys.argv[3]))
