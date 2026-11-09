#!/usr/bin/env python3
"""Decode a PNG (8-bit RGB or RGBA, not interlaced) to raw RGBA bytes, no dependency (stdlib zlib only).
Used to embed the window icon in the app (tao `Icon::from_rgba`).

usage: python3 -I png_to_rgba.py <in.png> <out.rgba>   (prints the size)"""
import struct
import sys
import zlib
from pathlib import Path

SIGNATURE = b'\x89PNG\r\n\x1a\n'


def chunks(data: bytes):
    pos = 8
    while pos < len(data):
        length, kind = struct.unpack('>I4s', data[pos:pos + 8])
        yield kind, data[pos + 8:pos + 8 + length]
        pos += 12 + length


def paeth(a: int, b: int, c: int) -> int:
    p = a + b - c
    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
    return a if pa <= pb and pa <= pc else (b if pb <= pc else c)


def unfilter(raw: bytes, width: int, height: int, bpp: int) -> bytearray:
    stride, out, prev = width * bpp, bytearray(), bytearray(width * bpp)
    for y in range(height):
        kind, line = raw[y * (stride + 1)], bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        for i in range(stride):
            a = line[i - bpp] if i >= bpp else 0
            b, c = prev[i], prev[i - bpp] if i >= bpp else 0
            line[i] = (line[i] + (0, a, b, (a + b) // 2, paeth(a, b, c))[kind]) & 0xFF
        out += line
        prev = line
    return out


def decode(data: bytes) -> tuple[int, int, bytes]:
    if data[:8] != SIGNATURE:
        raise ValueError('not a PNG file')
    header, idat = None, b''
    for kind, body in chunks(data):
        if kind == b'IHDR':
            header = struct.unpack('>IIBBBBB', body)
        elif kind == b'IDAT':
            idat += body
    if header is None:
        raise ValueError('no IHDR')
    width, height, depth, color, _, _, interlace = header
    if depth != 8 or color not in (2, 6) or interlace:
        raise ValueError(f'unsupported PNG: depth {depth}, color type {color}, interlace {interlace}')
    bpp = 4 if color == 6 else 3
    pixels = unfilter(zlib.decompress(idat), width, height, bpp)
    if bpp == 3:
        pixels = b''.join(pixels[i:i + 3] + b'\xff' for i in range(0, len(pixels), 3))
    return width, height, bytes(pixels)


def main() -> None:
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    width, height, rgba = decode(Path(sys.argv[1]).read_bytes())
    Path(sys.argv[2]).write_bytes(rgba)
    print(f'{width}x{height}, {len(rgba)} bytes')


if __name__ == '__main__':
    main()
