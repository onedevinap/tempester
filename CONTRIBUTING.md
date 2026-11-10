# Contributing to Tempester

Tempester is delivered as is, without support: issues and pull requests may stay unanswered, and feature requests are
not taken (see [Support and contributions](README.md#support-and-contributions)). If you contribute anyway, thank you:
bug reports from real machines and tests on Windows and Linux are the most useful. The rules below apply.

## The one rule: no DSI content

Never add, attach or paste:

- DSI files: `.syx` OS files, decoded images, dumps from a machine, firmware `.hex` backups;
- decompiled or disassembled DSI code (no listings, no pseudo-C), or a disassembler's project or export;
- DSI's manuals, addendum, pictures or logos, factory sounds, beats or projects;
- photos you did not take yourself.

Short references in your own words are fine: the role of a routine, a register, a message format. Addresses of DSI's
routines or data and decompiler names (`FUN_…`, `DAT_…`) are not: the only flash addresses in the repository are the
positions a recipe edits, and the export refuses any other. Patches are **recipes**
(see [docs/patch-recipe-format.md](docs/patch-recipe-format.md)): the new words are yours, the original bytes are never
stored. The DSI-content guard must pass: `python3 -I tools/dsi_content_guard.py <folder of your official .syx files>` compares the repository with the official images (CI cannot, it has no DSI file); add `--strict-text` for prose you wrote. Do not commit assembly sources that re-express DSI routines. See [legal notes](docs/legal.md).

Also keep personal data out: no home-folder paths, e-mail addresses or machine names in code, logs or screenshots;
strip image metadata (EXIF).

## Before a pull request

- `tools/test_app.sh` passes (format, clippy, tests with a disposable home folder).
- If you touched `patch/`, `versions/` or `tools/`: `tools/verify.sh <folder of your official .syx files>` ends with
  exit code 0 and `all green`. Regenerate the Rust tables with `python3 -I tools/generate_rust_tables.py`.
- Every guarantee has a counter-proof: a test that fails on the defect built on purpose.
- Never weaken a test, a tolerance or a guard to make it pass.
- Tests never read or write the real home folder of the person running them.
- No new dependency without discussing it first: the project keeps to the standard library (Python) and four crates
  (Rust).
- Commits follow [Conventional Commits](https://www.conventionalcommits.org/) (`feat(app): …`, `fix(patch): …`), in
  English.
- Update `CHANGELOG.md` and the docs in the same pull request.

## Firmware patches

A new patch needs: the problem (how to reproduce it on the official OS), the cause with the evidence (reasoning in
your own words; keep addresses and decompiler names in your own notes), the fix as a recipe, a version manifest, and its proof (simulation of the image's own code
where possible, with the unpatched image and wrong variants failing). It must not touch the last flash page
(`0x9D07F000`); the build refuses it. Mark clearly what was and was not tried on a real machine. A patch is only
installed on a machine by its owner, after reading [Before you install](docs/user-manual.md#before-you-install-a-tempester-version).

## Licence and SPDX headers

By contributing you agree that your contribution is published under the GPL-3.0-or-later licence of the project.

New source files start with an SPDX line in the file's comment syntax:

```
// SPDX-License-Identifier: GPL-3.0-or-later
# SPDX-License-Identifier: GPL-3.0-or-later
```

Existing files without the line are covered by the root `LICENSE`; the header is added when a file is next edited. Do not
add personal copyright lines with real names or e-mail addresses: « Copyright © Tempester contributors » is used
throughout.

## Code of conduct

See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
