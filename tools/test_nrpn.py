#!/usr/bin/env python3
"""Run the dump-checkable cases of tests/nrpn_usb.txt against a Tempest running Tempester >= 1.5.3.0, over USB.

usage: test_nrpn.py <tempest_midi binary> <tests/nrpn_usb.txt> <work dir> T0 T1 … (sections to run)

For each section: SEND lines go through `tempest_midi request`, DUMP requests the current sound (F0 01 28 40 F7) and
decodes it with tempest_sysex.py, EXPECT lines are checked (v[i]=x pairs, "SAME AS ref", "empty" = no echo).
Lines starting with "#" or "swift" (manual steps) are skipped: run only sections that need no manual step.
At the end the current sound is restored by sending back the first dump (F0 01 28 60 …, accepted by the OS).
Exit code 1 if any expectation fails. Writes to the machine: NRPN messages and the final sound restore only."""
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import tempest_sysex  # noqa: E402

BINARY, CASES_FILE, FOLDER = sys.argv[1], Path(sys.argv[2]), Path(sys.argv[3])
SECTIONS = set(sys.argv[4:])
FOLDER.mkdir(parents=True, exist_ok=True)


def request(message_hex: str, output: Path, seconds: float) -> bytes:
    subprocess.run([BINARY, 'request', message_hex, str(output), str(seconds)], check=True, capture_output=True)
    return output.read_bytes()


def dump() -> tuple[bytes, list[int]]:
    raw = request('F0 01 28 40 F7', FOLDER / 'dump.syx', 3)
    message = next(tempest_sysex.messages(raw))
    return message, tempest_sysex.decode(message)['values']


def run() -> int:
    failures, base, refs, section = 0, None, {}, None
    last_dump, values, echo = b'', [], b''
    for line in CASES_FILE.read_text().splitlines():
        if line.startswith('## '):
            section = line.split()[1]
            if section in SECTIONS:
                print(line)
            continue
        if section not in SECTIONS or not line.strip() or line.startswith(('#', 'swift')):
            continue
        command, _, comment = line.partition(';')
        command = command.strip()
        if command.startswith('SEND'):
            echo = request(re.search(r'"(.+)"', command).group(1), FOLDER / 'echo.syx', 1)
        elif command.startswith('DUMP'):
            last_dump, values = dump()
            base = base or last_dump
            if m := re.search(r'save as (\S+)', comment):
                refs[m.group(1)] = last_dump
        elif command.startswith('EXPECT'):
            text = line[len('EXPECT'):]
            if 'SAME AS' in text:
                ref = re.search(r'SAME AS (\S+)', text).group(1)
                ok, detail = last_dump == refs[ref], f'dump identical to {ref}'
            elif 'empty' in text:
                ok, detail = echo == b'', f'no echo ({len(echo)} bytes received)'
            else:
                pairs = [(int(i), int(x)) for i, x in re.findall(r'v\[(\d+)\]=(\d+)', text)]
                ok = all(values[i] == x for i, x in pairs)
                detail = ', '.join(f'v[{i}]={values[i]} (expected {x})' for i, x in pairs)
            failures += not ok
            print(f'  {"✔" if ok else "✘"} {detail}')
    if base:
        request(' '.join(f'{o:02X}' for o in base), FOLDER / 'restored.syx', 1)
        _, after = dump()
        restored = after == tempest_sysex.decode(base)['values']
        failures += not restored
        print(f'{"✔" if restored else "✘"} starting sound restored')
    print('all green' if failures == 0 else f'{failures} failure(s)')
    return 1 if failures else 0


if __name__ == '__main__':
    sys.exit(run())
