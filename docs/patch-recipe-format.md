# Patch recipe format (`tempester-recipe/1`)

A Tempester patch of the DSI Tempest Main OS is published as a **recipe**: a JSON file that holds no byte of DSI's
firmware. Our own words are literal; every word derived from DSI's code is an operation the user's own copy of the
DSI image resolves (copy a word, copy a word and change one instruction field, a jump computed from its target). The
image a recipe applies to is pinned by its SHA-256 (DSI Main 1.5.0.2). The user supplies the DSI file; the tools and
the app build the patched image from it and check the result against the hashes recorded in `versions/*.json`.

One source of truth: the files `patch/*.json` are the recipes themselves (the former `{adresse, origine, nouveau}`
files were converted in place, 2026-11-08). Readers: `tools/recipe.py` (Python, standard library) and
`app/crates/tempest-proto/src/recipe.rs` (Rust, through the tables `tools/generate_rust_tables.py` generates). Writer:
`tools/recipe_writer.py` (classification of every word, below), used by `tools/classify_patches.py`,
`tools/patch_version.py` and `tools/asm_generate_json.py` (the assembler chain, not in the public tree).

## File

```json
{
 "format": "tempester-recipe/1",
 "name": "Tempester n°20: SysEx 5C (beat n of a project) refused when n > 15",
 "image": "Main 1.5.0.2",
 "input_sha256": "86aaf7ca41be60140f11dd4fed84bffed2590746dcb267d4e9947128974336be",
 "explanation": "…",
 "edits": [
  {"at": "0x9d0600cc", "jal": "0x9d07ba00", "why": "…"},
  {"at": "0x9d07ba00", "word": "e8ffbd27", "why": "addiu sp,sp,-0x18"},
  {"at": "0x9d07ba14", "word": "1000482c", "why": "sltiu t0,v0,0x10"},
  {"at": "0x9d043620", "copy": "0x9d043610", "count": 2, "why": "checksum = 0 | byte parity = 0"},
  {"at": "0x9d042aa8", "copy": "0x9d042aa8", "set": {"imm": 63448}, "why": "frame 0x1028 -> 0x828"},
  {"at": "0x9d043890", "word": "ffffffff", "count": 396, "why": "freed: erased flash"}
 ]
}
```

| Key | Required | Meaning |
|---|---|---|
| `format` | yes | always `"tempester-recipe/1"`; any other value is refused |
| `name` | yes | patch name (shown by the tools and the app) |
| `image` | yes | human name of the input image |
| `input_sha256` | yes | SHA-256 (64 lowercase hex digits) of the decoded input image: DSI Main 1.5.0.2, `86aaf7ca…36be`. Every recipe of a build must give the same value |
| `explanation` | yes | prose: what the patch fixes or adds, and how |
| `edits` | yes | non-empty list, in patch order |
| any other key | no | prose metadata kept as is (e.g. `commands`, `excluded`, `announced_version` in `dump_request.json`); ignored by the builders |

The file is written one edit per line (`recipe.dump`), so that a diff shows one edit per line.

## Edits

Every edit has `at` (address of its first word, `"0x"` + 8 lowercase hex digits, word aligned) and exactly one of the
operations below. `why` (optional) is our comment. Unknown keys are refused.

| Operation | Words written | Meaning |
|---|---|---|
| `"word": "e8ffbd27"` | `count` (default 1) | our literal word, **in byte order** (the 4 bytes as stored in flash, little-endian), repeated `count` times from `at` on |
| `"copy": "0x9d043610"` | `count` (default 1) | the words of the **original input image** from that address on, written from `at` on |
| `"copy": A, "set": {"imm": 63448}` | 1 (`count` refused) | the original word at `A` with instruction fields replaced; fields: `rs` bits 21-25, `rt` bits 16-20, `rd` bits 11-15, `sa` bits 6-10, `imm` bits 0-15 (bit 0 = least significant); value = the raw unsigned field (an `imm` of -0x7e30 is written 33232); a value wider than its field is refused |
| `"j": T` / `"jal": T` | 1 (`count` refused) | MIPS `j` / `jal` to address `T`, encoded `opcode << 26 \| (T >> 2) & 0x03FFFFFF` (opcode 2 / 3); `T` must be word aligned and in the 256 MB region of `at + 4` |

Labels: the patch sources (`patch/*.s`, not in the public tree) use labels; `tools/asm_resolve.py` resolves them
before assembly, so a recipe holds addresses only. A jump is still written as its target, never as its encoding.

## Semantics (shared by every reader)

1. The input image must have the SHA-256 `input_sha256`; nothing is written otherwise.
2. **Every read is from the original input image**, never from an image already partly patched: copies and fields read
   DSI's words. Recipes are therefore independent of each other and of their order in a version.
3. A word may be written once only: by one edit of one recipe of a build (two recipes writing the same word, or a run
   covering another edit's word, are refused).
4. No word of the flasher page `0x9D07F000-0x9D07FFFF` may be written; every address written or read must be inside
   the image.
5. Every word written must differ from the original word at its address (an edit without effect is refused).
6. A version (`versions/*.json`; the test builds, kind `test`, in `versions/archive/`) lists recipes; the built image and its `.syx` must have the SHA-256 recorded in the
   manifest (`sha256_image`, `sha256_syx`), checked by `tools/build_version.py`, `tools/verify.sh` and the app
   (`versions::build`). These hashes are the per-version safety check: the same DSI input and the same recipes give
   the same bytes, or the build is refused.

No per-patch hash of the original ranges is stored. It is not needed: rule 1 pins all 512 KiB of the input, and every
tool and the app check it before writing anything, so a recipe can never be applied to a wrong base. It would also leak
DSI bytes: a SHA-256 over a few words (one word is 2^32 candidates) can be inverted by brute force.

## Classification (what goes literal, what becomes an operation)

`tools/recipe_writer.py` decides, for every word a patch writes, in this order (first match wins):

| # | Rule | Class | Written as |
|---|---|---|---|
| 1 | a `j` / `jal` | c (trivial) | `j` / `jal` |
| 2 | the previous word was copied from `x`, and this word is the original word at `x + 4` | b (derived) | `copy` (the run continues) |
| 3 | `nop`, erased flash `0xFFFFFFFF`, `jr ra` | c | `word` |
| 4 | the original word at the same address with exactly one instruction field changed (opcode and function kept) | b | `copy` + `set` |
| 5 | another trivial word: constant load (`li`, `ori` from zero, `lui`), register move (`move`, `clear`), stack-frame slot (`addiu sp,sp,N`, `lw`/`sw r,N(sp)`) | c | `word` |
| 6 | equal to an original word of the patch's rewritten span | b | `copy` of the nearest one |
| 7 | this word and the next equal a distinctive pair of consecutive original words anywhere | b | `copy` |
| 8 | a word of the rewritten span with exactly one field changed | b | `copy` + `set` |
| 9 | a conditional branch | c | `word` |
| 10 | otherwise | a (ours) | `word` |
| a= | two literal words next to each other in the recipe equal a distinctive DSI pair | a= (ours, stored by reference) | `copy` of that DSI location |

The rewritten span of a patch: the original DSI words it overwrites (not erased flash), each cluster of them (gaps of
less than 64 bytes) widened to its first and last address. Rules 4 and 8 make (b) an upper bound: a word one field away
from the DSI word it replaces is counted as derived even if we wrote it anew, so that no derived word stays literal.
A comment (`why`) of a copied word never spells out the copied DSI instruction (it is removed at conversion).

The counts per patch and every derived word: `Docs/analyses/patch-provenance.md`
(`tools/classify_patches.py <Main_1.5.0.2.bin> --report Docs/analyses/patch-provenance.md patch/*.json`).

## What the guard refuses

`tools/dsi_content_guard.py <DSI folder> [paths]` scans what would be published (default: `patch/`, `versions/`,
`tools/`, `app/`, `Docs/`, `refs/`, `tests/`, `profile/`, top-level `*.md`) against the four decoded DSI images (Main,
Voice, Panel, SAM). It fails on:

- any `.syx` or `.bin` file;
- any 8 bytes of a file, at any byte offset, equal to 8 bytes at a 4-byte-aligned offset of a DSI image, when that
  pair of words is **distinctive**;
- any quoted run of DSI words in a text file: hexadecimal written out as 8-digit words (with or without `0x`, read in
  byte order and as little-endian values), byte lists `[0x12, 0x34, …]`, spaced bytes `12 34 56 …`, gathered per
  label (the key or identifier before the value on its line, e.g. `origine`, `word`, `new`), per table column and per
  line, in file order. Two DSI words kept as two consecutive JSON fields are therefore caught. A blank line ends a
  run: the last word of one recipe and the first of the next in a generated table, or two paragraphs of a document,
  are checked each on its own (found on 10/11/2026: the tail `addiu sp,sp,0x20` of n°39 and the head
  `andi a0,a0,0xff` of n°5 follow each other in `versions.rs` and in a DSI function).

**Threshold.** The unit is a pair of consecutive aligned words (8 bytes): one instruction alone is a 32-bit encoding of
an opcode, registers and an immediate, a functional fact that any MIPS code may contain; two consecutive words are the
smallest unit that starts to carry DSI's own arrangement of its code. A pair is **commonplace** (allowed anywhere,
`recipe.commonplace`) when:

- it occurs at 16 places or more in the four images: a compiler idiom (prologues, epilogues, `nop` padding, erased
  flash). In the four images, 308 distinct pairs reach 16; the next ones are register variants of the same idioms;
- both words are trivial (rule 3 and 5 above): constants, register moves, stack-frame slots;
- it is one word repeated, or two addresses of the chip (program flash or RAM): they name places, as any analysis does;
- its 8 bytes hold at most 3 non-zero bytes, or at most 3 distinct byte values: at most about 24 bits of information,
  which small table entries, file headers and byte patterns (icons, PDF) match by chance.

Every other pair present in a DSI image is distinctive and refused. A sequence made only of printable characters is
text: 8 characters of ordinary words ("original", "referenc") match DSI's strings by chance, so it is not refused
(unless `--strict-text`); a quote of one DSI string of 24 characters or more (a menu label such as « Export Project over
MIDI ») is listed as a note. Not caught by design: a single isolated DSI word (below the threshold), and DSI's code
written as disassembly text.

Counter-proofs (`tools/dsi_content_guard_counter_proofs.py`, run by `tools/verify.sh`): one distinctive pair of the
Main planted in six forms (two `"word"` edits, two Rust `Op::Word`, a byte list, a spaced-byte listing, two words in a
sentence, raw bytes at an odd offset in a binary) is refused, and the same file with the second word altered is
accepted; a `.syx` is refused; the recipes of `patch/` are accepted; the same patches written back in the former format
(origin words) and a Rust table holding the origin bytes are refused.

## Making a recipe

- From assembly: `asm_resolve.py` → `Assembler.java` → `asm_generate_json.py <Main.bin> <assembler.out>
  <comments.json> <meta.json> patch/<name>.json [--source <assembler input>] [--fill <start> <end>]`.
- Version number patch: `patch_version.py <Main.bin> <a.b.c.d> <description> > patch/version_<a.b.c.d>.json`.
- A patch still in the former format (a branch older than the recipes): `classify_patches.py <Main.bin> --write
  patch/<name>.json` converts it in place (its origin words are checked against the image first).
- Then: `build_version.py versions/<v>.json <Main.bin> <folder> --record` for a new version,
  `generate_rust_tables.py`, `verify.sh`.
