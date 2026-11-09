#!/usr/bin/env python3
"""Decoder of the Tempest "data" SysEx messages (Main 1.5.0.2).
Sending: continuous DSI packing (bit7 of byte j -> bit j of the head byte), the last group filled with zeros,
NO checksum.
  F0 01 28 63 <lname> <pack(name[lname] + 128 sound bytes)> F7     sound file
  F0 01 28 60 <pack(128 sound bytes)> F7                         current sound (request 40, Main Tempester)
  F0 01 28 5F <pack(beat)> F7                                    beat (request 41): 16-bit words, not decoded here
The sound: see profile/dsi-tempest/sound_params.json.
usage: tempest_sysex.py file.syx [--json output.json]"""
import sys, json
def unpack(b):
    out=bytearray()
    for i in range(0,len(b),8):
        g=b[i:i+8]; m=g[0]
        for j,c in enumerate(g[1:]): out.append(c|(((m>>j)&1)<<7))
    return bytes(out)
def messages(d):
    i=0
    while (s:=d.find(b'\xf0',i))>=0:
        e=d.find(b'\xf7',s); yield d[s:e+1]; i=e+1
TABLE=None
def table():
    global TABLE
    if TABLE is None:
        import os
        TABLE=json.load(open(os.path.join(os.path.dirname(__file__),'..','profile','dsi-tempest','sound_params.json')))
    return TABLE
def sound_param_values(sound_bytes):
    """128 bytes of a sound -> {internal name, values by parameter index} (profile/dsi-tempest/sound_params.json)."""
    t=table(); v=int.from_bytes(sound_bytes,'little')
    vals=[(v>>p['bit'])&((1<<p['bits'])-1) for p in t['params']]
    n=t['name']; name=''.join(chr((v>>(n['byte']*8+7*k))&0x7f) for k in range(n['chars']))
    return {'internal_name':name.rstrip(),'values':vals}
def decode(m):
    assert m[:3]==b'\xf0\x01\x28', m[:4].hex()
    cmd=m[3]
    if cmd==0x63:
        ln=m[4]; u=unpack(m[5:-1])
        name=u[:ln].split(b'\0')[0].decode('latin-1')
        words=[u[ln+2*k]<<8|u[ln+2*k+1] for k in range(64)]
        data=bytes(u[ln:ln+128])
        return {'type':'sound_file','name':name,**sound_param_values(data)}
    if cmd==0x60:
        return {'type':'current_sound',**sound_param_values(bytes(unpack(m[4:-1])[:128]))}
    if cmd==0x5f:
        u=unpack(m[4:-1])
        return {'type':'beat','length':len(u),'words':[u[2*k]<<8|u[2*k+1] for k in range(min(12,len(u)//2))]}
    return {'type':f'cmd_{cmd:02x}','size':len(m)}
if __name__=='__main__':
    d=open(sys.argv[1],'rb').read()
    res=[decode(m) for m in messages(d)]
    if '--json' in sys.argv: json.dump(res,open(sys.argv[sys.argv.index('--json')+1],'w'),ensure_ascii=False,indent=0)
    for r in res[:5]: print(r)
    print(len(res),'messages')
