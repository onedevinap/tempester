# Testing

What is tested, how, what was checked on a real Tempest, and what was **not**. Dates are in 2026.

- [Automatic checks](#automatic-checks)
- [Firmware proofs](#firmware-proofs)
- [Verified on a real machine](#verified-on-a-real-machine)
- [Not verified](#not-verified)

## Automatic checks

### The app (`tools/test_app.sh`)

Runs on macOS, Linux and Windows (Git Bash), and in CI on the three systems:

- the generated Rust tables (`app/crates/tempest-proto/src/generated/`) are up to date with `patch/` and `versions/`,
  and a hand-edited copy is refused;
- `cargo fmt --check`, `cargo clippy --all-targets -D warnings`, `cargo test` with Rust 1.99.1 (pinned in
  `app/rust-toolchain.toml`);
- tests run with a **disposable home folder** (and `%APPDATA%` on Windows); the app's single access point to its data
  folder refuses anything else under test, so no test can touch your real library;
- the page scripts are parsed by Node, and the page logic runs on a fake DOM: the library manager, the project send and
  its version guard, the OS update hash flows, the panel's remote keys (a release is always sent), the Machine view,
  the identity panel, tooltips, tab order, the compact pages;
- each of those UI tests has a **counter-proof**: copies of the page with one behaviour deliberately broken (a command
  removed, a confirmation dropped, a release path cut...) must make the test fail, and they do;
- with Playwright installed (optional, not a dependency), a layout test drives every control in Chrome and WebKit and
  fails if anything moves by more than 0.5 px, with its own counter-proof.

Some Rust tests need data that cannot be in this repository (your DSI files, a captured project, the factory projects).
They are marked `#[ignore]` and run only when you give them the files (see [building](building.md#running-the-verifier));
otherwise they are skipped and the script says so.

### The whole repository (`tools/verify.sh <folder of your DSI files>`)

Needs the four files of the official OS 1.5.0.2 package. It checks, and stops red at the first failure:

1. decoding and re-encoding each official OS gives back the same file, byte for byte;
2. every version in `versions/` builds, with all checks (no overlap, update page untouched, the `.syx` decodes back,
   image and `.syx` SHA-256 equal to the recorded ones);
3. counter-proofs of the builder: overlapping patches, a patch touching the update page, an altered or missing
   recorded hash are all refused;
4. every Python tool compiles, the macOS MIDI helper compiles, the Python tests pass;
5. the app checks above, plus the `#[ignore]` tests on your DSI files; with `--long`, also the layout counter-proofs
   of the app's page.

The firmware simulations are not in this repository (see [Firmware proofs](#firmware-proofs)): the verifier says so
and checks everything else.

### CI

`.github/workflows/ci.yml` runs `tools/test_app.sh` on Ubuntu, macOS and Windows runners for every push to `main` and
every pull request. No DSI file is available there, so the tests that need one are skipped. `release.yml` builds the
app packages only (never firmware).

## Firmware proofs

Each patch is described with its proof in [firmware](firmware.md). In short:

| Kind | What it shows | Tools |
|---|---|---|
| Static | every changed word, no overlap, update page untouched, hashes | `build_version.py`, `apply_patch.py` |
| Re-import | the patched image, loaded again from scratch in a disassembler, changes only where intended | (contributors' disassembler) |
| Simulation | the firmware's own code, run instruction by instruction on your image, behaves as specified | not published (below) |
| Counter-proof | the same checks fail on the unpatched image and on wrong variants of each patch | not published (below) |
| Stack and registers | worst-case stack depth, saved registers preserved | not published (below) |

Simulated: n°8 (loop model), n°15, n°16, n°17 (two interrupts and the voice link, bit level), n°18-20 (MIDI input),
n°21-23 (remote panel), n°24 and n°26 (project request, with the real screen routines), n°25, n°27 (packer and senders),
n°28 (sender, app re-encoding and receiver on two simulated machines), n°29, n°30 (including a real OS update
receive), n°31 and n°32 (ribbons and footswitch), n°33 to n°35 (on the real user-interface code), n°36, and 1.6.2.0 and
1.6.3.0 as combined images. The other patches rest on code reading and static checks.

The simulators, the stack and register checks and the patch assembly sources are not published: they work on the
addresses and routines of DSI's code, which this repository does not describe. Their results are summarised patch by
patch in [firmware](firmware.md); what you can rerun here is the static part (every version rebuilds from the
recipes to its recorded hashes) and the app's tests.

## Verified on a real machine

One Tempest, over USB, each installation with the author's go. Each version replaced the previous one; the machine
now runs 1.6.3.0. The installations up to 1.6.2.0 were made with a SysEx sender of this repository
(`tools/tempest_midi.swift`), not with the app's OS update page.

| Date | Version | What was checked on the machine |
|---|---|---|
| 7 Nov | 1.5.1.0 | installed |
| 7 Nov | 1.5.2.0 | identity over USB (`0x7F2`); sound request (decoded); beat request (7,165 bytes), first use of n°2a |
| 7 Nov | 1.5.3.0 | NRPN: 21 of 21 checks over USB (`tools/test_nrpn.py`); 740 NRPN received while turning knobs; a filter sweep by NRPN heard live |
| 7 Nov | 1.5.4.0 | n°13: a beat sent with Voice Assign 9 came back with 0; beat round trip and restore byte-identical |
| 7 Nov | 1.5.5.0 | screen shows 1.5.5.0; the Debug-menu Main send decodes to the 1.5.5.0 image byte for byte (last page aside); Voice and Panel sends carry the real chips (n°15); n°11 with notes recorded under an external clock: no FX leak |
| 7 Nov | 1.5.6.0 | screen shows 1.5.6.0; n°14 message seen during the update; n°16: Undo restored a beat byte-identical after a 14 KB SysEx; n°19 (one path) and n°20 |
| 8 Nov | 1.6.0.0 | screen shows 1.6.0.0; n°22 returned the 44 LED states; n°21 + n°22: a remote Click press lit the LED and was reported; n°23: a remote encoder step changed the sound by one step, the opposite step restored it byte for byte |
| 8 Nov | 1.6.1.0 | the project request works (n°24); a project captured this way is read by the app (16 beats, Play Lists) |
| 8 Nov | 1.6.2.0 | the Debug-menu *Send Main OS* dump is byte-identical to the built 1.6.2.0 image over the whole range an update writes (`0x9D000000`-`0x9D07EFFF`); the last page differs as expected (below) |
| 9 Nov | 1.6.3.0 | started normally showing 1.6.3.0; the *Send Main OS* dump is byte-identical to the built image over the range an update writes, the last page is the machine's own; the behaviours of n°26 to n°36 used on the machine and confirmed by the author |

**Why the last flash page is left out of the comparison.** The last 4 KB page (`0x9D07F000` and up) holds the routine
that writes the flash during an update. No update rewrites it, official or Tempester: every machine keeps the copy it
left the factory with, which can differ from the copy inside DSI's update file (on the test machine: 97 bytes, an older
build of the same routine; seen identically in its 1.5.4.0, 1.5.5.0 and 1.6.2.0 dumps). A dump of a correctly updated
machine therefore matches the built image everywhere except that page, so the verification compares the range an
update writes and ignores the page it never writes.

The checks above were made with small MIDI tools of this repository (`tools/tempest_midi.swift`, `tools/test_nrpn.py`,
`tools/beat_voice_assign.py`, `tools/beat_fx_check.py`). The app was first used with the machine to receive a whole
project (1.6.1.0, USB); by 9 Nov its pages (Panel, Library, Play, Machine) had been used with the machine on
macOS over USB and confirmed by the author, with the open points listed below.

## Not verified

Be aware of these before relying on them.

**App**

- **Windows and Linux on real hardware**: never run with a Tempest. They compile and pass the tests in CI only; the
  Windows web view (WebView2) and the Linux one (WebKitGTK) were not looked at by a person.
- **The app over a DIN cable**: sending an OS, *Verify the installed OS* and the transfers were used over USB only.
- **A project fetched right after power-on**, before any beat is loaded: whether its 16 beats are all present is not
  proven; the app saves such a project as partial when beats are missing.
- **The remote panel in the app** against the real machine beyond the checks listed above: direction and speed of an
  encoder step, scaling of absolute knobs, which of the two Shift keys is 41 or 47, Pad Function LEDs in 16 Beats /
  16 Mutes.
- **Packages**: none is signed or notarised. The Linux `.deb` packages and the Windows installer have been built by CI
  scripts but never installed by a person; the macOS `.pkg` was built and its content inspected, never installed; no
  AppImage, no Intel macOS build.

**Plugin (Tempester Control)**

- **Never loaded in a real DAW.** It is built, unit-tested (with counter-proofs, including a check that the audio
  thread never allocates) and driven by a headless CLAP host. Whether a given DAW routes the MIDI output of a MIDI
  effect is not known yet; the VST3 build was never loaded by a host; nothing was sent to a Tempest through it.
- No editor window: a learned controller does not move the DAW's own knob.

**Firmware**

- **n°26 to n°36**: proven by simulation, installed with 1.6.2.0 and 1.6.3.0 (images verified) and confirmed in use on
  the machine, but not every case was tested on purpose (for example the SysEx timeout on DIN, or the first sorted Load
  Sound on a very large folder).
- **n°25**: installed with 1.6.1.0, the LED behaviour not checked.
- **n°2b, 2c, 3, 4a, 4b, 5, 6, 7a-7d, 8, 10, 12**: installed since 1.5.1.0 / 1.5.4.0 and in daily use, but no targeted
  reproduction of the original defect on the machine.
- **n°17** (muted voices): the cause is found with medium confidence; confirming the fix needs long sessions with LFO
  Restart = Play.
- **n°18 and the other paths of n°19**: the DIN-keyboard-during-USB-import case and the DIN paths were not reproduced.
- **DIN cable**: almost everything was tested over USB. The DIN path of the requests, of the remote panel and of
  OS updates was not exercised on the machine.
- **Going back to the official 1.5.0.2**: not done on the machine yet (each installation replaced the previous
  Tempester version); it uses the same update path.
- **Exact screen text** of System > *Show System Information* for Voice, Panel and SAM: not recorded.
- **Programmer recovery**: never needed; the header pinout is not yet checked on the board (see [recovery](recovery.md)).
