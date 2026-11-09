#!/usr/bin/env python3
"""Model test of the four FX writers, before (andi 0xff) and after (andi 0x7f), plus a synthetic run of beat_fx_check.

usage: python3 -I test_fx_mask.py   (run from this folder; touches no file outside it)
Writers modelled from the disassembly: FX1/FX3 'and 0xff80 | v', FX2/FX4 'and 0xc07f | v << 7', result & 0xffff."""
import random
import subprocess
import sys
import tempfile
from pathlib import Path

LOW, HIGH = (0xFF80, 0, 0x007F), (0xC07F, 7, 0x3F80)


def write(word: int, value: int, field: tuple[int, int, int], mask: int) -> int:
    keep, shift, _ = field
    return (word & keep | ((value & mask) << shift)) & 0xFFFF


def check_writers() -> None:
    rng = random.Random(1)
    old_leaks = 0
    for value in range(-128, 128):
        for _ in range(64):
            word = rng.randrange(0x10000)
            for field in (LOW, HIGH):
                keep, shift, bits = field
                new = write(word, value, field, 0x7F)
                old = write(word, value, field, 0xFF)
                assert new & ~bits & 0xFFFF == word & ~bits & 0xFFFF, 'neighbour bits changed'
                assert (new & bits) >> shift == value & 0x7F, 'field value wrong'
                assert new & bits == old & bits, 'stored 7 bits differ from the original writer'
                old_leaks += old != new
                if value >= 0:
                    assert old == new, 'non-negative value must give the same word as before'
    assert old_leaks > 0, 'counter-test: the original writer must leak for negative values'
    print(f'writers: OK (original writer leaked in {old_leaks} cases, fixed writer in none)')


def pack(data: bytes) -> bytes:
    out = bytearray()
    for i in range(0, len(data), 7):
        group = data[i:i + 7]
        out.append(sum(((c >> 7) & 1) << j for j, c in enumerate(group)))
        out.extend(c & 0x7F for c in group)
    return bytes(out)


def dump(events: list[tuple[int, int, int, int, int]]) -> bytes:
    words = [0] * 538 + [1 + 5 * len(events)] + [w for e in events for w in e]
    raw = b''.join(bytes((w >> 8, w & 0xFF)) for w in words)
    return b'\xf0\x01\x28\x5f' + pack(raw) + b'\xf7'


def run_checker(events: list[tuple[int, int, int, int, int]]) -> int:
    with tempfile.TemporaryDirectory() as tmp:
        syx = Path(tmp) / 'beat.syx'
        syx.write_bytes(dump(events))
        return subprocess.run([sys.executable, '-I', str(Path(__file__).resolve().parent.parent / 'tools' / 'beat_fx_check.py'), str(syx)], capture_output=True).returncode


def check_checker() -> None:
    fx2_minus5_fixed = write(0, -5, HIGH, 0x7F)
    fx2_minus5_bug = write(0, -5, HIGH, 0xFF)
    assert fx2_minus5_bug & 0x4000 and not fx2_minus5_fixed & 0x4000
    good = [(10, 0x7003, 0x6410, fx2_minus5_fixed, 0)]
    bad = [(10, 0x7003, 0x6410, fx2_minus5_bug, 0)]
    assert run_checker(good) == 0, 'fixed event must pass'
    assert run_checker(bad) == 1, 'counter-test: buggy event must be flagged'
    print('beat_fx_check: OK (fixed event passes, buggy event flagged)')


if __name__ == '__main__':
    check_writers()
    check_checker()
