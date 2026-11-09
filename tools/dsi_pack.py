#!/usr/bin/env python3
"""Rebuild a DSI Tempest OS SysEx from a decoded image (inverse of dsi_unpack.py).
usage: dsi_pack.py <image.bin> <id hex: 71 Main,72 Voice,73 Panel,74 SAM> <out.syx>
Format: F0 01 28 <id> | per 1024-byte block: 7-bit packing (MSB byte then up to 7
data bytes, bit j = MSB of byte j), 1171 raw bytes | checksum 14 bits LSB7, MSB7 | F7
Checksum = sum of little-endian 16-bit words of the image, & 0x3FFF
(as the Main 1.5.0.2 firmware computes it)."""
import sys
def pack(b):
    out=bytearray()
    for i in range(0,len(b),7):
        g=b[i:i+7]; m=0
        for j,c in enumerate(g): m|=(c>>7)<<j
        out.append(m); out+=bytes(c&0x7f for c in g)
    return bytes(out)
img=open(sys.argv[1],'rb').read(); dev=int(sys.argv[2],16)
assert len(img)%1024==0, "image size must be a multiple of 1024"
ck=sum(img[i]|img[i+1]<<8 for i in range(0,len(img),2))&0x3fff
syx=bytes([0xF0,0x01,0x28,dev])+b''.join(pack(img[i:i+1024]) for i in range(0,len(img),1024))+bytes([ck&0x7f,ck>>7,0xF7])
open(sys.argv[3],'wb').write(syx)
print(f"{sys.argv[3]}: {len(syx)} bytes, checksum {ck:#06x}")
