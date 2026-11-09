#!/usr/bin/env python3
"""Decode DSI Tempest OS SysEx: F0 01 28 <id> then N blocks of 1024 bytes, each
7-bit packed independently (MSB byte first, bit j -> byte j), 1171 raw bytes/block,
then 2 trailing bytes (checksum) and F7."""
import sys
def unpack(b):
    out=bytearray()
    for i in range(0,len(b),8):
        g=b[i:i+8]; m=g[0]
        for j,c in enumerate(g[1:]): out.append(c|(((m>>j)&1)<<7))
    return bytes(out)
src,dst=sys.argv[1],sys.argv[2]
d=open(src,'rb').read()
assert d[0]==0xF0 and d[-1]==0xF7
body=d[4:-1]
RAW=1171
n=len(body)//RAW
out=b''.join(unpack(body[i*RAW:(i+1)*RAW]) for i in range(n))
tail=body[n*RAW:]
open(dst,'wb').write(out)
print(f"{src}: id={d[3]:#04x} blocks={n} out={len(out)} tail={tail.hex()}")
