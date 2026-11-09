#!/usr/bin/env python3
"""Apply Tempester patch recipes (patch/*.json, tools/recipe.py) to a decoded Main image, with safeguards.

usage: apply_patch.py <original.bin> <patched.bin> <recipe1.json> [<recipe2.json> …]

Every recipe reads the ORIGINAL image (copies and fields come from it), whatever the order. Refuses an image whose
SHA-256 is not the recipe's input_sha256, two recipes (or two edits) writing the same word, a word in the flasher
page, an address outside the image, and a word that changes nothing. Writes nothing when it refuses. The .syx is built
afterwards with dsi_pack.py (or build_version.py for a whole version)."""
import importlib.util
import sys
from pathlib import Path

_spec = importlib.util.spec_from_file_location('recipe', Path(__file__).resolve().parent / 'recipe.py')
recipe = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(recipe)

if __name__ == '__main__':
    original = Path(sys.argv[1]).read_bytes()
    try:
        recipes = [(Path(p).name, recipe.load(Path(p))) for p in sys.argv[3:]]
        image, owners = recipe.apply(recipes, original)
    except recipe.RecipeError as e:
        sys.exit(f'refused: {e}')
    for name, r in recipes:
        print(f'{r["name"]}: {sum(1 for o in owners.values() if o == name)} words written')
    Path(sys.argv[2]).write_bytes(image)
    print(f'-> {sys.argv[2]}')
