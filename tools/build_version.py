#!/usr/bin/env python3
"""Builds the SysEx of a Tempester version of the Main from its manifest (versions/*.json).

usage: build_version.py versions/<version>.json <stock Main_1.5.0.2.bin> <output folder> [--record]

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
_spec = importlib.util.spec_from_file_location('recipe', TOOLS / 'recipe.py')
recipe = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(recipe)


def changed_words(before: bytes, after: bytes) -> set[int]:
    return {BASE + (i & ~3) for i in range(len(before)) if before[i] != after[i]}


def repo_of_manifest(manifest: Path) -> Path:
    """The repository root: the parent of `versions/`, whether the manifest is in it or in `versions/archive/`."""
    folder = manifest.resolve().parent
    if folder.name == 'archive':
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


def fresh_hashes(image: Path, syx: Path) -> dict[str, str]:
    data = image.read_bytes()
    return {'sha256_image': hashlib.sha256(data).hexdigest(),
            # What an OS update writes (everything below the flasher page): the machine keeps its own flasher, so
            # only this range of a dump read back from the machine can be compared.
            'sha256_image_written': hashlib.sha256(data[:recipe.FLASHER_PAGE - BASE]).hexdigest(),
            'sha256_syx': hashlib.sha256(syx.read_bytes()).hexdigest()}


def check_recorded(manifest: Path, m: dict, fresh: dict[str, str]) -> None:
    """Refuses a manifest whose recorded hashes are missing or differ from the fresh build, naming file and field."""
    fix = f'python3 -I tools/build_version.py {manifest} <Main_1.5.0.2.bin> <folder> --record'
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
    root = repo_of_manifest(manifest)
    patches = [root / p for p in m['patches']]
    output.mkdir(parents=True, exist_ok=True)
    image = output / f'Main_{m["version"]}.bin'
    syx = output / f'Tempest_Main_{m["version"]}_Tempester.syx'
    read_back = output / f'.read_back_{m["version"]}.bin'
    before = source_image.read_bytes()
    after, declares = apply(patches, before)
    changes = changed_words(before, after)
    if changes != set(declares):
        sys.exit(f'refused: {len(changes - set(declares))} words outside the patches, {len(set(declares) - changes)} patches without effect')
    image.write_bytes(after)
    run(str(TOOLS / 'dsi_pack.py'), str(image), '71', str(syx))
    run(str(TOOLS / 'dsi_unpack.py'), str(syx), str(read_back))
    if read_back.read_bytes() != after:
        sys.exit('refused: the decoded .syx does not give back the image')
    read_back.unlink()
    fresh = fresh_hashes(image, syx)
    if '--record' in sys.argv[1:]:
        record(manifest, m, fresh)
    else:
        check_recorded(manifest, m, fresh)
    print(f'{m["name"]}: {len(patches)} patches, {len(changes)} words, flasher page intact')
    for f, key in ((image, 'sha256_image'), (image, 'sha256_image_written'), (syx, 'sha256_syx')):
        print(f'  {f.name}  {key} {fresh[key]}' + ('  (recorded)' if '--record' in sys.argv[1:] else ''))
