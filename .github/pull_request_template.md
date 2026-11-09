## What and why

<!-- What this changes, and the problem it solves. Link the issue. -->

## Proof

<!-- Tests added or run, counter-proofs (a test that fails on the defect), and for firmware: simulation results. -->

## Checklist

- [ ] **No DSI content**: no `.syx`, `.bin`, `.hex`, dumps, decompiled or disassembled DSI code, manual pages,
      factory sounds or third-party pictures; patches are recipes; `tools/dsi_content_guard.py <your DSI folder>` passes
- [ ] No personal data: no home-folder paths, e-mail addresses or machine names; image metadata stripped
- [ ] `tools/test_app.sh` passes
- [ ] If `patch/`, `versions/` or `tools/` changed: `tools/verify.sh <your DSI folder>` ends with exit code 0 and
      `all green`, and the Rust tables are regenerated
- [ ] No test, tolerance or guard weakened; no new dependency (or discussed first)
- [ ] `CHANGELOG.md` and the docs updated; what was **not** verified on a real machine is written down
- [ ] Conventional Commits, in English; new files carry `SPDX-License-Identifier: GPL-3.0-or-later`
