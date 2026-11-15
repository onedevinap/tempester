#!/usr/bin/env python3
"""Builds the SysEx of a Tempester version of the Main (or of the Voice) from its manifest (versions/*.json).

usage: build_version.py versions/<version>.json <stock Main_1.5.0.2.bin> <output folder> [--record]
       build_version.py versions/voice/<version>.json <stock Voice_1.5.bin> <output folder> [--record]

The test builds (kind "test") are in versions/archive/; the tool builds them the same way.
Manifest: {"version": "1.5.1.0", "name": …, "description": …, "sha256_image": …, "sha256_syx": …,
"patches": ["patch/….json", …]}.
Applies the recipes in order to the stock image (tools/recipe.py: SHA-256 of the input image checked,
copies and fields read from the stock image, Docs/patch-recipe-format.md), then refuses the version if:
  - two recipes write the same word, or a word changes nothing;
  - a word changes outside the declared recipes;
  - the flasher page (0x9D07F000-0x9D07FFFF) is touched;
  - the rebuilt .syx does not give back exactly the image once decoded;
  - the manifest's hashes (sha256_image: the Main image;
    sha256_image_written: the same image without the flasher page (the range an update writes); sha256_syx: the .syx) are missing or differ from those of
    the fresh build (stale or edited manifest). The app compares these hashes before sending and
    after the installation: the DSI file is never in the repository, so the hashes come from the manifest.
--record: writes the fresh build's hashes into the manifest (new version, or intended change)
instead of comparing them; then run tools/generate_rust_tables.py again for the app's tables.
Outputs: <folder>/Tempest_Main_<version>_Tempester.syx and <folder>/Main_<version>.bin; prints their SHA-256.
A Voice manifest ("os": "voice", kept in versions/voice/, out of the app's tables) lists Voice recipes ("cpu":
"dspic33f"); the build is a Voice image (.syx id 72, Tempest_Voice_<version>_Tempester.syx, Voice_<version>.bin) and
is refused unless PC 0 still holds `goto 0x15F0`, PC 0x650E-0x651C (update entry) is DSI's and nothing at or above PC
0xA400 changed (boot page and calibration: an update never writes them); sha256_image_written is then the first
0xF600 bytes, the 41 pages an update sends.
This tool NEVER sends the .syx to the machine."""
import hashlib
import importlib.util
import json
import subprocess
import sys
from pathlib import Path

BASE = 0x9D000000
TOOLS = Path(__file__).resolve().parent
HASH_KEYS = ('sha256_image', 'sha256_image_written', 'sha256_syx')
VOICE_SENT = 0xF600          # bytes of a Voice image an update sends (41 pages of 768 words)
VOICE_RESET = (0x0415F0, 0x000000)
_spec = importlib.util.spec_from_file_location('recipe', TOOLS / 'recipe.py')
recipe = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(recipe)


def changed_words(before: bytes, after: bytes) -> set[int]:
    return {BASE + (i & ~3) for i in range(len(before)) if before[i] != after[i]}


def changed_voice_words(before: bytes, after: bytes) -> set[int]:
    """The PCs whose 3-byte instruction differs."""
    return {i // 3 * 2 for i in range(len(before)) if before[i] != after[i]}


def check_voice_image(before: bytes, after: bytes) -> None:
    """The Voice image rules, on the built image (independently of the recipes' own checks)."""
    if tuple(recipe.pic24_word_at(after, pc) for pc in (0, 2)) != VOICE_RESET:
        sys.exit('refused: PC 0 does not hold goto 0x15F0')
    protected = [pc for pc in changed_voice_words(before, after) if recipe.pic24_protected(pc)]
    if protected:
        sys.exit(f'refused: {recipe.pic24_protected(min(protected))}')


def repo_of_manifest(manifest: Path) -> Path:
    """The repository root: the parent of `versions/`, whether the manifest is in it or in `versions/archive/`
    (or `versions/voice/`)."""
    folder = manifest.resolve().parent
    if folder.name in ('archive', 'voice'):
        folder = folder.parent
    return folder.parent


def run(*args: str) -> None:
    r = subprocess.run([sys.executable, '-I', *args], capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f'refused: {Path(args[0]).name} failed\n{(r.stdout + r.stderr).strip()}')


def apply(patches: list[Path], before: bytes) -> tuple[bytes, dict[int, str]]:
    try:
        return recipe.apply([(str(p), recipe.load(p)) for p in patches], before)
    except recipe.RecipeError as e:
        sys.exit(f'refused: {e}')


def fresh_hashes(image: Path, syx: Path, voice: bool = False) -> dict[str, str]:
    data = image.read_bytes()
    # What an OS update writes (Main: everything below the flasher page; Voice: the 41 pages sent): the machine keeps
    # its own flasher / boot page, so only this range of a dump read back from the machine can be compared.
    written = data[:VOICE_SENT] if voice else data[:recipe.FLASHER_PAGE - BASE]
    return {'sha256_image': hashlib.sha256(data).hexdigest(),
            'sha256_image_written': hashlib.sha256(written).hexdigest(),
            'sha256_syx': hashlib.sha256(syx.read_bytes()).hexdigest()}


def check_recorded(manifest: Path, m: dict, fresh: dict[str, str], stock: str = 'Main_1.5.0.2.bin') -> None:
    """Refuses a manifest whose recorded hashes are missing or differ from the fresh build, naming file and field."""
    fix = f'python3 -I tools/build_version.py {manifest} <{stock}> <folder> --record'
    for key in HASH_KEYS:
        if key not in m:
            # The verifier's counter-proof looks for "<manifest> : <field> " in this message.
            sys.exit(f'refused: {manifest} : {key} is missing; fresh build value {fresh[key]}; '
                     f'to record it: {fix}')
        if m[key] != fresh[key]:
            sys.exit(f'refused: {manifest} : {key} records {m[key]} but the fresh build gives {fresh[key]} '
                     f'(stale or edited manifest); if the change is intended: {fix}')


def record(manifest: Path, m: dict, fresh: dict[str, str]) -> None:
    """Writes the hashes right after the description, keeping the manifest's formatting and final newline."""
    ordered: dict = {}
    for key, value in m.items():
        if key in HASH_KEYS:
            continue
        ordered[key] = value
        if key == 'description':
            ordered.update(fresh)
    if 'description' not in m:
        ordered.update(fresh)
    newline = '\n' if manifest.read_text().endswith('\n') else ''
    manifest.write_text(json.dumps(ordered, indent=2, ensure_ascii=False) + newline)


if __name__ == '__main__':
    arguments = [a for a in sys.argv[1:] if a != '--record']
    manifest, source_image, output = (Path(a) for a in arguments)
    m = json.loads(manifest.read_text())
    voice = m.get('os', 'main') == 'voice'
    os_name, os_id = ('Voice', '72') if voice else ('Main', '71')
    root = repo_of_manifest(manifest)
    patches = [root / p for p in m['patches']]
    output.mkdir(parents=True, exist_ok=True)
    image = output / f'{os_name}_{m["version"]}.bin'
    syx = output / f'Tempest_{os_name}_{m["version"]}_Tempester.syx'
    read_back = output / f'.read_back_{m["version"]}.bin'
    before = source_image.read_bytes()
    after, declares = apply(patches, before)
    changes = (changed_voice_words if voice else changed_words)(before, after)
    if changes != set(declares):
        sys.exit(f'refused: {len(changes - set(declares))} words outside the patches, {len(set(declares) - changes)} patches without effect')
    if voice:
        check_voice_image(before, after)
    image.write_bytes(after)
    run(str(TOOLS / 'dsi_pack.py'), str(image), os_id, str(syx))
    run(str(TOOLS / 'dsi_unpack.py'), str(syx), str(read_back))
    if read_back.read_bytes() != after:
        sys.exit('refused: the decoded .syx does not give back the image')
    read_back.unlink()
    fresh = fresh_hashes(image, syx, voice)
    if '--record' in sys.argv[1:]:
        record(manifest, m, fresh)
    else:
        check_recorded(manifest, m, fresh, 'Voice_1.5.bin' if voice else 'Main_1.5.0.2.bin')
    intact = 'reset goto, update entry, boot page and calibration intact' if voice else 'flasher page intact'
    print(f'{m["name"]}: {len(patches)} patches, {len(changes)} words, {intact}')
    for f, key in ((image, 'sha256_image'), (image, 'sha256_image_written'), (syx, 'sha256_syx')):
        print(f'  {f.name}  {key} {fresh[key]}' + ('  (recorded)' if '--record' in sys.argv[1:] else ''))
