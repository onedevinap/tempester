#!/usr/bin/env python3
"""Check the Note FX fields of a Tempest beat dump (reply to F0 01 28 41 F7, i.e. F0 01 28 5F <packed beat> F7).

usage: python3 -I beat_fx_check.py <beat_dump.syx>

Stream layout (the firmware's beat sender): DSI 7-bit packing, then 16-bit words sent high byte first:
538 header words, then event chunks [N, (step, w0, w1, w2, w3) * (N-1)/5] repeated while N > 2000,
then 128-byte sounds (ignored here).
For every event it prints FX1-FX4 (7-bit signed) and flags:
  HALF  w2 bit 14 set (half-step). Legit only when recorded with 1/32 triplet quantize (param 5).
  W3B14 w3 bit 14 set (never written on purpose; only the FX4 overflow sets it).
Exit code 1 when an event has a negative FX2 together with HALF, or W3B14 set: the signature of the unmasked writers."""
import sys
from pathlib import Path

HEADER_WORDS = 538
CHUNK_LIMIT = 2000


def unpack(packed: bytes) -> bytes:
    out = bytearray()
    for i in range(0, len(packed), 8):
        group = packed[i:i + 8]
        msb = group[0]
        out.extend(c | (((msb >> j) & 1) << 7) for j, c in enumerate(group[1:]))
    return bytes(out)


def signed7(value: int) -> int:
    return value - 128 if value & 0x40 else value


def beat_words(syx: bytes) -> list[int]:
    start = syx.find(b'\xf0\x01\x28\x5f')
    if start < 0:
        sys.exit('no F0 01 28 5F beat message found')
    end = syx.find(b'\xf7', start)
    data = unpack(syx[start + 4:end])
    return [data[k] << 8 | data[k + 1] for k in range(0, len(data) - 1, 2)]


def events(words: list[int]) -> list[tuple[int, int, int, int, int]]:
    found: list[tuple[int, int, int, int, int]] = []
    pos = HEADER_WORDS
    while pos < len(words):
        count = words[pos]
        body = words[pos + 1:pos + count]
        found.extend((body[k], body[k + 1], body[k + 2], body[k + 3], body[k + 4]) for k in range(0, len(body) - 4, 5))
        pos += count
        if count <= CHUNK_LIMIT:
            break
    return found


def report(found: list[tuple[int, int, int, int, int]]) -> int:
    bad = 0
    for step, w0, w1, w2, w3 in found:
        fx = (signed7(w2 & 0x7f), signed7((w2 >> 7) & 0x7f), signed7(w3 & 0x7f), signed7((w3 >> 7) & 0x7f))
        half, w3b14 = bool(w2 & 0x4000), bool(w3 & 0x4000)
        suspect = (half and fx[1] < 0) or w3b14
        bad += suspect
        flags = ' '.join(f for f, on in (('HALF', half), ('W3B14', w3b14), ('SUSPECT', suspect)) if on)
        print(f'step {step:3d} pad {w0 & 0x1f:2d} vel {(w1 >> 8) & 0x7f:3d} dur {w1 & 0xff:3d} '
              f'FX {fx[0]:4d} {fx[1]:4d} {fx[2]:4d} {fx[3]:4d} {flags}')
    print(f'{len(found)} events, {bad} suspect')
    return 1 if bad else 0


if __name__ == '__main__':
    sys.exit(report(events(beat_words(Path(sys.argv[1]).read_bytes()))))
