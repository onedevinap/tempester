#!/usr/bin/env python3
"""Read or set the per-pad Voice Assign byte of a Tempest beat dump (F0 01 28 5F <packed beat> F7).

usage: python3 -I beat_voice_assign.py read <beat_dump.syx>
       python3 -I beat_voice_assign.py set <beat_dump.syx> <out.syx> <pad 1-32>=<value> [...]
       python3 -I beat_voice_assign.py diff <a.syx> <b.syx>

Layout (as the firmware's beat header reader takes it): after the format word, header word 25 starts 32 pad blocks of
15 words; Voice Assign is the low byte of word 1 of each block (0 = None, 1-6 = voice).
Used to prove patch n°13 on the machine: send a beat with an out-of-range value (> 6), dump it back (F0 01 28 41 F7),
the Tempester OS answers 0 where the stock OS keeps the value. 'set' only writes 7-bit values (MSB cleared)."""
import sys

PAD0_WORD = 26
PAD_WORDS = 15
PADS = 32


def message(syx: bytes) -> bytes:
    start = syx.find(b'\xf0\x01\x28\x5f')
    if start < 0:
        sys.exit('no F0 01 28 5F beat message found')
    return syx[start:syx.index(b'\xf7', start) + 1]


def unpack(packed: bytes) -> bytes:
    out = bytearray()
    for i in range(0, len(packed), 8):
        group = packed[i:i + 8]
        out.extend(c | (((group[0] >> j) & 1) << 7) for j, c in enumerate(group[1:]))
    return bytes(out)


def byte_index(pad: int) -> int:
    return 2 * (PAD0_WORD + PAD_WORDS * pad + 1) + 1


def voice_assigns(msg: bytes) -> list[int]:
    data = unpack(msg[4:-1])
    return [data[byte_index(p)] for p in range(PADS)]


def set_voice_assign(msg: bytes, pad: int, value: int) -> bytes:
    group, pos = divmod(byte_index(pad), 7)
    out = bytearray(msg)
    out[4 + 8 * group + 1 + pos] = value & 0x7f
    out[4 + 8 * group] &= ~(1 << pos) & 0x7f
    return bytes(out)


def main() -> None:
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    cmd, msg = sys.argv[1], message(open(sys.argv[2], 'rb').read())
    if cmd == 'read':
        print('Voice Assign, pads 1-32:', voice_assigns(msg))
    elif cmd == 'set' and len(sys.argv) >= 5:
        for spec in sys.argv[4:]:
            pad, value = (int(x) for x in spec.split('='))
            if not (1 <= pad <= PADS and 0 <= value <= 0x7f):
                sys.exit(f'bad pad=value: {spec}')
            msg = set_voice_assign(msg, pad - 1, value)
        open(sys.argv[3], 'wb').write(msg)
    elif cmd == 'diff':
        a, b = unpack(msg[4:-1]), unpack(message(open(sys.argv[3], 'rb').read())[4:-1])
        changed = [(i // 2, a[i], b[i]) for i in range(min(len(a), len(b))) if a[i] != b[i]]
        print(f'sizes {len(a)} {len(b)}; differing bytes: {len(changed)} (word, a, b): {changed[:12]}')
        sys.exit(0 if not changed and len(a) == len(b) else 1)
    else:
        sys.exit(__doc__)


if __name__ == '__main__':
    main()
