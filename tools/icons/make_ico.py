#!/usr/bin/env python3
"""Write a Windows .ico from PNG files (PNG-compressed entries, supported since Windows Vista), no dependency.

usage: python3 -I make_ico.py <out.ico> <icon_16.png> <icon_32.png> ...   (sizes read from each PNG header, ≤ 256)"""
import struct
import sys
from pathlib import Path

PNG_SIGNATURE = b'\x89PNG\r\n\x1a\n'


def png_size(data: bytes) -> tuple[int, int]:
    if data[:8] != PNG_SIGNATURE or data[12:16] != b'IHDR':
        raise ValueError('not a PNG file')
    return struct.unpack('>II', data[16:24])


def ico(pngs: list[bytes]) -> bytes:
    header = struct.pack('<HHH', 0, 1, len(pngs))
    offset = 6 + 16 * len(pngs)
    entries, payload = b'', b''
    for data in pngs:
        w, h = png_size(data)
        if w > 256 or h > 256:
            raise ValueError(f'{w}x{h}: an .ico entry is at most 256 px')
        entries += struct.pack('<BBBBHHII', w % 256, h % 256, 0, 0, 1, 32, len(data), offset + len(payload))
        payload += data
    return header + entries + payload


def main() -> None:
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    Path(sys.argv[1]).write_bytes(ico([Path(p).read_bytes() for p in sys.argv[2:]]))


if __name__ == '__main__':
    main()
