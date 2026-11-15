# Testing

**In short**

- Every firmware change is proved before any install: static checks, and for most changes a simulation of the
  machine's own code, with counter-proofs.
- Every Tempester OS and Tempester Voice release was installed on one real Tempest, over USB. It now runs Main 1.6.9.0
  and Voice 1.5.1.2.
- Not tested yet: Windows and Linux with a machine, the plugin in a real DAW, the DIN cable, and going back to the
  official OS. The full list is [below](#not-verified).

Dates are in 2026.

- [Automatic checks](#automatic-checks)
- [Firmware proofs](#firmware-proofs)
- [Verified on a real machine](#verified-on-a-real-machine)
- [Not verified](#not-verified)

## Automatic checks

### The app (`tools/test_app.sh`)

It runs on macOS, Linux and Windows (Git Bash), and in CI on the three systems. It checks:

- the generated Rust tables (`app/crates/tempest-proto/src/generated/`) are up to date with `patch/` and `versions/`;
  a hand-edited copy is refused;
- `cargo fmt --check`, `cargo clippy --all-targets -D warnings` and `cargo test`, with Rust 1.99.1 (pinned in
  `app/rust-toolchain.toml`);
- tests run with a **disposable home folder** (and `%APPDATA%` on Windows). The app's single access point to its data
  folder refuses anything else under test, so no test can touch your real library;
- the page scripts are parsed by Node, and the page logic runs on a fake DOM: the library manager, the project send
  and its version guard, the OS update hash flows, the panel's remote keys (a release is always sent), the Machine
  view, the identity panel, tooltips, tab order, the compact pages;
- each of those UI tests has a **counter-proof**: copies of the page with one behaviour deliberately broken (a command
  removed, a confirmation dropped, a release path cut...) must make the test fail, and they do;
- with Playwright installed (optional, not a dependency), a layout test drives every control in Chrome and WebKit. It
  fails if anything moves by more than 0.5 px, and has its own counter-proof.

Some Rust tests need data that cannot be in this repository: your DSI files, a captured project, the factory
projects. They are marked `#[ignore]` and run only when you give them the files
([building](building.md#running-the-verifier)). Otherwise they are skipped and the script says so.

### The whole repository (`tools/verify.sh <folder of your DSI files>`)

It needs the four files of the official OS 1.5.0.2 package. It stops red at the first failure. It checks:

1. decoding and re-encoding each official OS gives back the same file, byte for byte;
2. every version in `versions/` builds, with all checks: no overlap, update page untouched, the `.syx` decodes back,
   image and `.syx` SHA-256 equal to the recorded ones;
3. counter-proofs of the builder: overlapping patches, a patch touching the update page, an altered or missing
   recorded hash are all refused;
4. every Python tool compiles, the macOS MIDI helper compiles, the Python tests pass;
5. the app checks above, plus the `#[ignore]` tests on your DSI files. With `--long`, also the layout counter-proofs
   of the app's page.

The firmware simulations are not in this repository ([Firmware proofs](#firmware-proofs)). The verifier says so and
checks everything else.

### CI

`.github/workflows/ci.yml` runs `tools/test_app.sh` on Ubuntu, macOS and Windows for every push to `main` and every
pull request. No DSI file is available there, so the tests that need one are skipped.

`release.yml` builds the app packages only, never firmware.

## Firmware proofs

Each patch is described with its proof in [firmware](firmware.md). In short:

| Kind | What it shows | Tools |
|---|---|---|
| Static | changed words, no overlap, update page untouched, hashes | `build_version.py`, `apply_patch.py` |
| Re-import | the patched image changes only where intended | contributors' disassembler |
| Simulation | the firmware's own code behaves as specified | not published |
| Counter-proof | the checks fail on the unpatched image and on wrong variants | not published |
| Stack and registers | worst-case stack depth, saved registers preserved | not published |

- **Re-import**: the patched image is loaded again from scratch in a disassembler.
- **Simulation**: the firmware's own code runs instruction by instruction, on your image.

**Simulated patches**:

- n°8 (loop model), n°15, n°16;
- n°17 (two interrupts and the voice link, bit level);
- n°18-20 (MIDI input), n°21-23 (remote panel), n°25;
- n°24 and n°26 (project request, with the real screen routines);
- n°27 (packer and senders);
- n°28 (sender, app re-encoding and receiver on two simulated machines);
- n°29, n°30 (including a real OS update receive);
- n°31 and n°32 (ribbons and footswitch);
- n°33 to n°35 (on the real user-interface code), n°36;
- n°37 to n°46, n°48 to n°56, n°58, n°59, n°61;
- n°62 to n°65 (full backup against models of the voice chips, Release cap and lift, Voice versions);
- every release from 1.6.2.0 on, as combined images;
- Tempester Voice, on a simulator of the Voice processor: the chip's own code, the Main's reads, the release curve,
  the update.

The other patches rest on code reading and static checks.

**What is not published**: the simulators, the stack and register checks and the patch assembly sources. They work on
the addresses and routines of DSI's code, which this repository does not describe. Their results are summarised patch
by patch in [firmware](firmware.md).

What you can rerun here: the static part (every version rebuilds from the recipes to its recorded hashes) and the
app's tests.

## Verified on a real machine

One Tempest, over USB, each installation with the author's go. Each version replaced the previous one. The machine now
runs Tempester 1.6.9.0, with Tempester Voice 1.5.1.2 on its six voice chips.

How the installs were sent:

- Up to 1.6.2.0, and some later ones (Main 1.6.9.0 and Tempester Voice 1.5.1.2 among them): with a SysEx sender of
  this repository (`tools/tempest_midi.swift`).
- Main and Voice OS installs from the app's OS update page were also used, as were its full backup and identity card.

Release by release:

- **1.5.1.0** (7 Nov): installed.
- **1.5.2.0** (7 Nov): identity over USB (`0x7F2`). Sound request decoded. Beat request (7,165 bytes), the first use of n°2a.
- **1.5.3.0** (7 Nov): NRPN, 21 of 21 checks over USB (`tools/test_nrpn.py`). 740 NRPN received while turning knobs. A filter
  sweep by NRPN heard live.
- **1.5.4.0** (7 Nov): n°13, a beat sent with Voice Assign 9 came back with 0. Beat round trip and restore byte-identical.
- **1.5.5.0** (7 Nov): the screen shows 1.5.5.0. The Debug-menu Main send decodes to the 1.5.5.0 image byte for byte (last page
  aside). Voice and Panel sends carry the real chips (n°15). n°11 with notes recorded under an external clock: no FX
  leak.
- **1.5.6.0** (7 Nov): the screen shows 1.5.6.0. The n°14 message seen during the update. n°16: Undo restored a beat
  byte-identical after a 14 KB SysEx. n°19 (one path) and n°20.
- **1.6.0.0** (8 Nov): the screen shows 1.6.0.0. n°22 returned the 44 LED states. n°21 + n°22: a remote Click press lit the
  LED and was reported. n°23: a remote encoder step changed the sound by one step; the opposite step restored it byte
  for byte.
- **1.6.1.0** (8 Nov): the project request works (n°24). A project captured this way is read by the app (16 beats, Play
  Lists).
- **1.6.2.0** (8 Nov): the Debug-menu *Send Main OS* dump is byte-identical to the built image over the whole range an update
  writes (`0x9D000000`-`0x9D07EFFF`). The last page differs as expected (below).
- **1.6.3.0** (9 Nov): started normally, showing 1.6.3.0. The *Send Main OS* dump is byte-identical to the built image over the
  range an update writes; the last page is the machine's own. The behaviours of n°26 to n°36 were used on the machine
  and confirmed by the author.
- **1.6.4.0** (10 Nov): installed over USB, the machine power-cycled by hand after the send. Request 43 answers 1.6.4.0. The
  *Send Main OS* dump equals the built image on the written range. n°38, n°40, n°41, n°43-n°46 tried by hand the same
  day: as simulated.
- **1.6.5.0** (12 Nov): the machine answers 1.6.5.0 to the exact-version request. The *Send Main OS* dump equals the built image
  on the written range. A first capture was damaged in transit (bytes of 0x80 and above inside the SysEx); a second
  dump was clean. n°52 listened to by the author, who keeps it. The new behaviours (n°48 to n°61) were tried by hand on
  15 Nov.
- **1.6.6.0** (13 Nov): the first full backup of the machine (request `44`): the six voice chips with their calibration, the
  boot flash, the System settings.
- **Tempester Voice 1.5.1.0, then 1.6.7.0** (14 Nov): the Voice update on the six chips, with a full backup before and after
  (each chip's program pages equal to the built image). 1.6.7.0 reports six `0016`, and the Release knobs reach 127
  (heard). A second full backup in the same power-on stopped the voice chips: the official Voice stayed silent until
  power-off; Tempester Voice 1.5.1.0 restarted, shifted. This defect was fixed next.
- **Tempester Voice 1.5.1.1, then 1.6.8.0** (14 Nov): two full backups in a row, identical byte for byte, the voices still
  playing.
- **1.6.9.0, then Tempester Voice 1.5.1.2** (15 Nov): request `43` answers 1.6.9.0; `45` reports six `0017`. Two full backups in
  a row identical after each step. Each chip's program pages equal the built image; its calibration page is unchanged.

**Why the last flash page is left out of the comparison**

- The last 4 KB page (`0x9D07F000` and up) holds the routine that writes the flash during an update.
- No update rewrites it, official or Tempester. Every machine keeps the copy it left the factory with.
- That copy can differ from the one inside DSI's update file. On the test machine: 97 bytes, an older build of the
  same routine, seen identically in its 1.5.4.0, 1.5.5.0 and 1.6.2.0 dumps.
- So a dump of a correctly updated machine matches the built image everywhere except that page. The verification
  compares the range an update writes and ignores the page it never writes.

**Tools and app use**

- The checks above were made with small MIDI tools of this repository: `tools/tempest_midi.swift`,
  `tools/test_nrpn.py`, `tools/beat_voice_assign.py`, `tools/beat_fx_check.py`.
- The app was first used with the machine to receive a whole project (1.6.1.0, USB).
- By 9 Nov its pages (Panel, Library, Play, Machine) had been used with the machine on macOS over USB, and confirmed
  by the author, with the open points listed below.
- Later the OS update page (Main and Voice installs), the full backup and the identity card were used with the machine
  too.

## Not verified

Be aware of these before relying on them.

**App**

- **Windows and Linux on real hardware**: never run with a Tempest. They compile and pass the tests in CI only. The
  Windows web view (WebView2) and the Linux one (WebKitGTK) were not looked at by a person.
- **The app over a DIN cable**: sending an OS, *Verify the installed OS* and the transfers were used over USB only.
- **A project fetched right after power-on**, before any beat is loaded: whether its 16 beats are all present is not
  proven. The app saves such a project as partial when beats are missing.
- **The remote panel in the app**, beyond the checks listed above: direction and speed of an encoder step, scaling of
  absolute knobs, which of the two Shift keys is 41 or 47, Pad Function LEDs in 16 Beats / 16 Mutes.
- **Packages**: none is signed or notarised.
  - The Linux `.deb` packages and the Windows installer were built by CI scripts but never installed by a person.
  - The macOS `.pkg` was built and its content inspected, never installed.
  - No AppImage, no Intel macOS build.

**Plugin (Tempester Control)**

- **Never loaded in a real DAW.** It is built, unit-tested (with counter-proofs, including a check that the audio
  thread never allocates) and driven by a headless CLAP host.
- Whether a given DAW routes the MIDI output of a MIDI effect is not known yet. The VST3 build was never loaded by a
  host. Nothing was sent to a Tempest through it.
- No editor window: a learned controller does not move the DAW's own knob.

**Firmware**

- **n°26 to n°36**: proved by simulation, installed with 1.6.2.0 and 1.6.3.0 (images verified) and confirmed in use on
  the machine. Not every case was tested on purpose, for example the SysEx timeout on DIN, or the first sorted Load
  Sound on a very large folder.
- **n°25**: installed with 1.6.1.0; the LED behaviour not checked.
- **Tempester Voice**: installed on the six chips and read back. Not yet done on the machine:
  - a Calibrate run;
  - the Release times 101-127 measured with a stopwatch (60 s at 127);
  - one chip put back on the official Voice OS (the Main must then report it and keep the Release knobs at 123);
  - going back to the official Voice 1.5.
- **n°2b, 2c, 3, 4a, 4b, 5, 6, 7a-7d, 8, 10, 12**: installed since 1.5.1.0 / 1.5.4.0 and in daily use, but the
  original defect was never reproduced on purpose on the machine.
- **n°17** (muted voices): the cause is found with medium confidence. Confirming the fix needs long sessions with LFO
  Restart = Play.
- **n°18 and the other paths of n°19**: the DIN-keyboard-during-USB-import case and the DIN paths were not reproduced.
- **DIN cable**: almost everything was tested over USB. The DIN path of the requests, of the remote panel and of OS
  updates was not exercised on the machine.
- **Going back to the official 1.5.0.2**: not done on the machine yet (each installation replaced the previous
  Tempester version). It uses the same update path.
- **Exact screen text** of System > *Show System Information* for Voice, Panel and SAM: not recorded.
- **Programmer recovery**: never needed. The header pinout is not yet checked on the board ([recovery](recovery.md)).
