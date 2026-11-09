#!/usr/bin/env python3
"""Prove that a combined release is exactly the union of its test builds, word for word.

usage: version_union.py <base.bin> <combined.bin> <version_patch.json> <test.bin…>

base       the previous release the test builds were made on (e.g. 1.6.2.0);
combined   the new release (e.g. 1.6.3.0 = base + the patches of every test build + its own version patch);
version_patch.json   the combined release's version-digit recipe: its words are the only ones allowed to differ from
           the base outside the test builds;
test.bin   each test build (base + some patches; its own version digits, if any, are left out: those words belong
           to the version patch).
Checks, exit 1 on the first failure:
  1. no word changed from the base by two test builds (each test build owns its words);
  2. every word a test build changed holds, in the combined image, that test build's value;
  3. every other word of the combined image equals the base, except words of the version patch;
  4. at least one word of the version patch differs from the base (the screens show a new number; the digits that
     stay the same, e.g. 6 and 0 from 1.6.2.0 to 1.6.3.0, are equal words).
Reads images only; writes nothing; sends nothing."""
import json
import sys
from pathlib import Path

BASE = 0x9D000000


def words(image: bytes) -> list[int]:
    return [int.from_bytes(image[i:i + 4], 'little') for i in range(0, len(image), 4)]


def changed(base: list[int], other: list[int]) -> set[int]:
    return {i for i, (a, b) in enumerate(zip(base, other)) if a != b}


def version_words(path: Path) -> set[int]:
    spec = json.loads(path.read_text())
    out = set()
    for edit in spec['edits']:
        first = (int(edit['at'], 16) - BASE) // 4
        out.update(range(first, first + edit.get('count', 1)))
    return out


def main() -> int:
    if len(sys.argv) < 5:
        sys.exit(__doc__)
    base, combined = (words(Path(p).read_bytes()) for p in sys.argv[1:3])
    version = version_words(Path(sys.argv[3]))
    tests = [(Path(p).name, words(Path(p).read_bytes())) for p in sys.argv[4:]]
    if len({len(base), len(combined), *(len(t) for _, t in tests)}) != 1:
        sys.exit('refused: images of different sizes')
    owner: dict[int, str] = {}
    for name, test in tests:
        for i in changed(base, test) - version:
            if i in owner:
                print(f'FAIL {BASE + 4 * i:#x} changed by {owner[i]} and {name}')
                return 1
            owner[i] = name
            if combined[i] != test[i]:
                print(f'FAIL {BASE + 4 * i:#x}: combined {combined[i]:08x} != {name} {test[i]:08x}')
                return 1
        print(f'{name}: {sum(1 for o in owner.values() if o == name)} words, all in the combined image')
    rest = changed(base, combined) - set(owner)
    if not rest <= version:
        print('FAIL words changed outside the test builds and the version patch:',
              [f'{BASE + 4 * i:#x}' for i in sorted(rest - version)])
        return 1
    if not rest:
        print('FAIL the version digits are those of the base')
        return 1
    print(f'union of {len(tests)} test builds: {len(owner)} words, disjoint; plus {len(rest)} version-digit words; '
          f'{len(owner) + len(rest)} words differ from the base, nothing else')
    return 0


if __name__ == '__main__':
    sys.exit(main())
