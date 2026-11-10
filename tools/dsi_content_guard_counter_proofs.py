#!/usr/bin/env python3
"""Counter-proofs of tools/dsi_content_guard.py: a planted DSI fragment, in every form the guard claims to see, must
turn it red, and the same file with the fragment broken must leave it green; the two words of a pair on either side
of a blank line (two recipes of a generated table) must leave it green, the same two lines without the blank line red;
the patches written back in the former {adresse, origine, nouveau} format (DSI's original words) and a Rust table
holding them must be red, the recipes green.

usage: dsi_content_guard_counter_proofs.py <folder of the DSI .syx files> <scratch folder>
Writes only in <scratch folder> (planted files are never left in the repository). Exit 1 at the first proof that
fails, with its name."""
import importlib.util
import json
import struct
import sys
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
ROOT = TOOLS.parent
GUARD = TOOLS / 'dsi_content_guard.py'
_spec = importlib.util.spec_from_file_location('dsi_content_guard', GUARD)
guard_tool = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(guard_tool)
recipe = guard_tool.recipe


def distinctive_pair(image: bytes) -> tuple[int, int, int]:
    """The first pair of consecutive words of the Main code that the guard must refuse."""
    words = struct.unpack(f'<{len(image) // 4}I', image)
    counts: dict[tuple[int, int], int] = {}
    for pair in zip(words, words[1:]):
        counts[pair] = counts.get(pair, 0) + 1
    for i in range(0x1000 // 4, len(words) - 1):
        pair = (words[i], words[i + 1])
        if not recipe.commonplace(*pair, counts[pair]):
            return 4 * i, pair[0], pair[1]
    raise SystemExit('no distinctive pair in the Main image')


def planted(first: int, second: int) -> dict[str, bytes]:
    """One file per form the guard claims to see, each holding the two DSI words."""
    as_bytes = struct.pack('<II', first, second)
    return {
        'recipe_literals.json': (f'{{"edits": [\n  {{"at": "0x9d07e000", "word": "{as_bytes[:4].hex()}"}},\n'
                                 f'  {{"at": "0x9d07e004", "word": "{as_bytes[4:].hex()}"}}\n]}}\n').encode(),
        'table.rs': (f'Edit {{ address: 0x9d07e000, count: 1, op: Op::Word(0x{first:08x}) }},\n'
                     f'Edit {{ address: 0x9d07e004, count: 1, op: Op::Word(0x{second:08x}) }},\n').encode(),
        'byte_list.rs': ('new: [' + ', '.join(f'0x{b:02x}' for b in as_bytes) + ']\n').encode(),
        'listing.md': ('| addr | bytes |\n| x | ' + ' '.join(f'{b:02x}' for b in as_bytes) + ' |\n').encode(),
        'words_in_prose.md': f'The routine starts with 0x{first:08X} then 0x{second:08X}.\n'.encode(),
        'blob.dat': b'\x13\x37\x42' + as_bytes + b'\x99' * 5,
    }


def guard(dsi: 'guard_tool.Dsi', *paths: Path) -> bool:
    """True when the guard accepts the paths (the guard's own scan, run in this process on the decoded images)."""
    return not guard_tool.scan(dsi, list(paths))[1]


def former_format(path: Path, image: bytes) -> dict:
    """A recipe written back the old way, origin words included (what the repository held before the recipes)."""
    spec = json.loads(path.read_text())
    words = recipe.resolve(spec, image, path.name)
    return {'nom': spec['name'], 'sha256_origine': spec['input_sha256'], 'modifications': [
        {'adresse': f'{a:#x}', 'origine': image[a - recipe.BASE:a - recipe.BASE + 4].hex(),
         'nouveau': v.to_bytes(4, 'little').hex()} for a, v in words]}


def check(name: str, ok: bool) -> None:
    if not ok:
        sys.exit(f'counter-proof failed: {name}')
    print(f'  ok  {name}')


def main() -> None:
    folder, scratch = Path(sys.argv[1]), Path(sys.argv[2])
    scratch.mkdir(parents=True, exist_ok=True)
    dsi = guard_tool.Dsi(folder)
    image = next(data for name, data in dsi.images.items() if name.startswith('Tempest_Main_'))
    offset, first, second = distinctive_pair(image)
    for name, data in planted(first, second).items():
        red, green = scratch / 'red' / name, scratch / 'green' / name
        red.parent.mkdir(exist_ok=True)
        green.parent.mkdir(exist_ok=True)
        red.write_bytes(data)
        broken = planted(first, second ^ 0x5A5A5A5A)[name]
        green.write_bytes(broken)
        check(f'{name}: DSI words at {offset:#x} refused', not guard(dsi, red))
        check(f'{name}: same file with the second word broken accepted', guard(dsi, green))
    # A blank line ends a run: the pair split over two paragraphs (the tail of one recipe, the head of the next in
    # app/.../generated/versions.rs) is not a quoted run of DSI words; the same two lines joined are.
    rows = planted(first, second)['table.rs'].decode().splitlines()
    split, joined = scratch / 'green' / 'two_recipes.rs', scratch / 'red' / 'two_recipes.rs'
    split.write_text(f'{rows[0]}\n    ],\n}};\n\nstatic P_NEXT: Patch = Patch {{\n{rows[1]}\n')
    joined.write_text(f'{rows[0]}\n    ],\n}};\nstatic P_NEXT: Patch = Patch {{\n{rows[1]}\n')
    check('the pair split by a blank line (two recipes of a generated table) accepted', guard(dsi, split))
    check('the same two lines without the blank line refused', not guard(dsi, joined))
    firmware = scratch / 'firmware' / 'harmless.syx'
    firmware.parent.mkdir(exist_ok=True)
    firmware.write_bytes(b'\xf0\x7e\x7f\x06\x01\xf7')
    check('a .syx file refused whatever it holds', not guard(dsi, firmware))
    recipes = sorted((ROOT / 'patch').glob('*.json'))
    check(f'the {len(recipes)} recipes of patch/ accepted', guard(dsi, *recipes))
    old = scratch / 'former'
    old.mkdir(exist_ok=True)
    refused = 0
    for path in recipes:
        target = old / path.name
        target.write_text(json.dumps(former_format(path, image), indent=1))
        refused += not guard(dsi, target)
    check(f'the same patches in the former format refused ({refused} of {len(recipes)} files)',
          not guard(dsi, old) and refused > 0)
    table = old / 'versions_former.rs'
    rows = [f'    Modification {{ address: {m["adresse"]}, origin: [{", ".join(f"0x{b:02x}" for b in bytes.fromhex(m["origine"]))}], '
            f'new: [{", ".join(f"0x{b:02x}" for b in bytes.fromhex(m["nouveau"]))}] }},'
            for p in recipes for m in former_format(p, image)['modifications']]
    table.write_text('\n'.join(rows) + '\n')
    check('a Rust table with the origin words refused', not guard(dsi, table))


if __name__ == '__main__':
    main()
