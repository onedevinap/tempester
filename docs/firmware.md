# Tempester firmware

Tempester firmware versions are the official **Main OS 1.5.0.2** of the DSI Tempest with numbered patches applied.
This page explains each version and each patch: the problem, what changes for you, how it was proved, whether it was
checked on a real machine, and its limits. The new MIDI messages are specified in [MIDI protocol](midi-protocol.md).

- [How a version is made](#how-a-version-is-made)
- [Prerequisites](#prerequisites)
- [Versions](#versions)
- [Patches](#patches)
- [How the patches are proved](#how-the-patches-are-proved)
- [Known defects not fixed](#known-defects-not-fixed)

## How a version is made

- The repository holds **no DSI software**. Each patch is a recipe: the flash addresses it changes and the new words,
  written by Tempester contributors. The input file is identified by its SHA-256 and every result by the SHA-256
  recorded in the version's manifest (`versions/<version>.json`); see [building](building.md#rebuilding-a-firmware-version-from-your-own-dsi-file)
  and the [recipe format](patch-recipe-format.md).
- Patches only touch the **Main** chip. Voice, Panel and SAM stay those of the official 1.5.0.2 package.
- New code lives in flash that the official image leaves empty (erased, `0xFF`). A version is refused at build time if
  two patches overlap, if anything touches the last flash page (`0x9D07F000`, the routine that writes the flash during
  an update), or if the rebuilt `.syx` does not decode back to the image.
- Each version shows its own number in System > *Main OS Version* (numbers DSI never used). The identity answer of
  every Tempester version carries the version word `0x7F2`.

## Prerequisites

Tempester expects the complete official OS 1.5.0.2 set on the machine: Main 1.5.0.2 and the Voice (1.5), Panel (1.3)
and SAM (1.1) files of the same package. Some patches rely on the other chips: the remote panel (n°21, n°22, n°25) uses
the Panel chip's key codes and LED commands, n°4b and n°17 send words to the Voice chips, n°15 reads the Voice and Panel
chips. Step-by-step: [user manual](user-manual.md#before-you-install-a-tempester-version).

## Versions

Each version contains everything of the one it builds on. « Installed » means installed and used on a real Tempest
over USB; « verified » adds that the Main OS read back from the machine matched the built image. See
[testing](testing.md#verified-on-a-real-machine) for what was checked each time. The app offers the releases; the
test versions are built by the verifier only, to prove a patch on its own.

**Which one?** 1.6.4.0 is the latest release, with everything; it is proved in simulation but not yet installed on a machine, so 1.6.3.0 is the latest one verified on a real Tempest. 1.5.6.0 is the corrections-only release: the bug fixes
with the sound and beat requests and NRPN, without the 1.6 additions.

| Version | Builds on | Adds | Status |
|---|---|---|---|
| **1.5.1.0** fixes | official 1.5.0.2 | n°2a, 2b, 2c, 3, 4a, 4b, 5, 6, 7a, 7b, 7c, 8: bug fixes only, no new function | installed |
| **1.5.2.0** fixes + requests | 1.5.1.0 | n°1: requests for the current sound and beat, identity answer on the chosen cable | installed |
| **1.5.3.0** NRPN | 1.5.2.0 | n°9a, 9b: sound parameters received and sent as NRPN | installed |
| 1.5.4.0 fixes 2 (test) | 1.5.3.0 | n°2d, 7d, 10, 11, 12, 13 | installed |
| 1.5.4.1 update message (test) | 1.5.4.0 | n°14: clearer update screen | not installed alone (included from 1.5.5.0 on) |
| **1.5.5.0** optimisation | 1.5.4.1 | n°15: the three Debug-menu OS senders merged, 1,584 bytes freed | installed |
| **1.5.6.0** stability, corrections only | 1.5.5.0 | n°16, 17, 18, 19, 20 | installed |
| **1.6.0.0** remote panel | 1.5.6.0 | n°21, 22, 23: keys, LEDs, encoders and knobs over MIDI | installed |
| 1.6.0.1 (test) | 1.6.0.0 | n°25 alone | not installed |
| **1.6.1.0** project request | 1.6.0.0 | n°24, 25: whole-project request, 4 Pole / AD Mode LEDs follow NRPN | installed |
| 1.6.1.1 to 1.6.1.5 (test) | 1.6.1.0 | one patch each, n°26 to n°30, to prove it alone | not installed |
| **1.6.2.0** MIDI transfers | 1.6.1.0 | n°26, 27, 28, 29, 30 together | installed and verified (8 Nov) |
| 1.6.3.0-a, -b, -c (test) | 1.6.2.0 | n°31-32, n°33-35 and n°36, each group alone | not installed |
| **1.6.3.0** performance and browsing | 1.6.2.0 | n°31, 32, 33, 34, 35, 36 together | **installed and verified** (9 Nov) |
| 1.6.4.0-a (test) | 1.6.3.0 | n°37 alone: dead code erased, room for new functions | not installed |
| 1.6.4.0-b (test) | 1.6.3.0 | n°38 alone | not installed |
| 1.6.4.0-c (test) | 1.6.3.0 | n°39, 40: the exact version and the Pad Function mode announced | not installed |
| 1.6.4.0-d (test) | 1.6.3.0 | n°41, 43, 44, 45, 46: five fixes of reported bugs | not installed |
| **1.6.4.0** room, reports and fixes | 1.6.3.0 | n°37, 38, 39, 40, 41, 43, 44, 45, 46 together | built and proved in simulation (10 Nov), **not installed yet** |

The test versions are kept so that each patch can be checked on its own. 1.6.2.0 and 1.6.3.0 were installed on a real
machine and verified by reading the Main OS back (see [testing](testing.md#verified-on-a-real-machine)); the behaviours
of n°26 to n°36, proven by simulation first, were then used on the machine with 1.6.3.0.

## Patches

Each entry: **Problem** in the official OS, **Effect** for you, **Proof**, **On a machine**, **Limits**. « Simulated »
means the firmware's own instructions were run in an instruction-level simulator of the Main processor on the user's
own image (the simulators are not published, see [testing](testing.md#firmware-proofs)), with counter-proofs: the
same checks run on the unpatched image and on
deliberately wrong variants of the patch, which must fail. See [How the patches are proved](#how-the-patches-are-proved).

### n°1: sound and beat requests

- **Problem**: the official OS sends a sound or a beat only from its menus; it answers no request. Its identity answer
  always leaves by the DIN port, even when SysEx is set to USB.
- **Effect**: `F0 01 28 40 F7` returns the current sound, `F0 01 28 41 F7` the current beat, on the cable chosen in
  System; the identity answer follows that cable too and carries the version word `0x7F2`. This is what lets the app
  fetch sounds and beats.
- **Proof**: static checks of every replaced word, independent re-import of the patched image in a disassembler.
- **On a machine**: verified on 1.5.2.0 (identity over USB, a sound and a 7,165-byte beat fetched and decoded).
- **Limits**: requests are served from the main loop, like the menu sends.

### n°2a, n°2b, n°2c, n°2d: stack frames of the beat and project transfers

- **Problem**: the routines that send a beat, receive a beat, receive a project header and export a project reserved
  stack frames larger than the whole stack (up to 16.7 KB for about 9.9 KB of stack). Their buffers overlapped global
  variables: exporting a project could freeze on « Exporting Project », and every beat reception overwrote memory.
- **Effect**: frames reduced to 4.2 KB, 4.4 KB, 2.1 KB and 2.1 KB; the largest stack use measured during a project
  export is about 6.9 KB.
- **Proof**: the routines decompile identically apart from the buffer sizes; a received piece is bounded by the largest
  piece the sender produces; stack depth measured statically.
- **On a machine**: n°2a was first exercised by a beat request on 1.5.2.0; project exports and requests worked on
  1.6.1.0. No targeted reproduction of the original freeze was made.
- **Limits**: none known.

### n°3: slider pressure on the real-time FX

- **Problem**: in Realtime mode with the sequencer running, the pressure of sliders 2 to 4 was compared with the wrong
  source number; only slider 1 worked.
- **Effect**: the pressure of all four sliders acts on the FX. One instruction.
- **Proof**: code reading of the panel message decoder (proved cause).
- **On a machine**: not tested on its own.

### n°4a, n°4b: compressor envelope

- **Problem**: (a) when a beat was saved, the compressor's Decay was written where Peak Hold belongs; (b) the compressor
  envelope reached the voice chips only once you touched it, so a loaded beat played with the previous settings.
- **Effect**: Peak Hold is saved; the envelope is sent to the voices each time a beat is selected.
- **Proof**: code reading (proved causes); static checks.
- **On a machine**: not tested on its own.
- **Limits**: the Amount is still limited to -64 by a deliberate clamp in the official code; whether the voice chip
  needs it is not known, so it is left alone.

### n°5: note length up to 255

- **Problem**: the note length could be edited up to 254 only.
- **Effect**: up to 255. Longer notes would need a format change and are not done.
- **On a machine**: not tested on its own.

### n°6: USB SysEx import protected from the DIN port

- **Problem**: a SysEx import over USB was cut by a status byte or an end-of-SysEx arriving on the DIN input (for
  example a keyboard playing).
- **Effect**: DIN traffic no longer breaks a USB import.
- **On a machine**: not tested on its own.

### n°7a, n°7b, n°7c, n°7d: Play Lists

- **Problem**: the sequencer divides by the length of the current Play List step; an empty step has length 0, the
  division traps, and the trap handler loops forever (hard freeze). (a) Init Project emptied the lists while Play List
  mode stayed active; (b) RT REC advanced to the next step before filling it, freezing every time; (c) RT REC could
  write a 101st step past the table; (d) Delete Step wrote into the header of the next list.
- **Effect**: no division by zero on an empty step, RT REC fills the step before advancing and stops at 100 steps,
  Delete Step shifts the steps of its own list only.
- **Proof**: code reading of all 14 division sites; n°7d rewrites the Delete Step routine in place.
- **On a machine**: not tested on their own.

### n°8: Loop Screen freeze outside 4/4

- **Problem**: the sequencer advanced its loop counter with a formula that is correct only when the loop start is a
  multiple of the loop length. In odd meters (5/8, 7/8...) the counter could never reach its target and the sequencer
  interrupt looped forever.
- **Effect**: no freeze; 4/4 behaviour unchanged.
- **Proof**: model of the loop: 51 freezing cases before, 0 after, 44,532 sequences identical in
  4/4.
- **On a machine**: not tested on its own.
- **Limits**: projections in such loops are still shifted by a few ticks; a possible freeze in external sync on beat
  changes is not proved to be the same cause.

### n°9a, n°9b: NRPN

- **Problem**: the official OS parses NRPN on the DIN port but never applies them, and ignores them on USB; it never
  sends them.
- **Effect**: (a) NRPN received on DIN and USB change sound parameters 0-126 and the Osc 3/4 samples (156, 157), on the
  pad selected on the machine (Remote Pad IN channel) or on the MIDI Synth sound (Synth IN channel), exactly as the knob
  would; (b) turning a knob sends the NRPN on Remote Pad OUT, without echoing what came from MIDI.
- **Proof**: re-import and comparison; 21 test vectors.
- **On a machine**: verified on 1.5.3.0: 21 of 21 checks over USB, 740 NRPN received while turning knobs, a filter sweep
  heard live.
- **Limits**: the screen is not redrawn after a MIDI change; DIN output has no overflow check when knobs move very fast
  (not observed).

### n°10: sequence copy target

- **Problem**: a sequence copy looked for free space in the source beat instead of the target beat.
- **Effect**: none visible today (source and target are the same in the official menus); latent defect removed.

### n°11: Note FX masked to 7 bits

- **Problem**: a negative FX value recorded into a note overflowed into the neighbouring field: FX2 shifted the note by
  a half step, FX1/FX3 changed the next FX.
- **Effect**: the recorded FX stay in their own field.
- **Proof**: model of the four writers (`tests/test_fx_mask.py`) and a checker for beat dumps (`tools/beat_fx_check.py`).
- **On a machine**: verified on 1.5.5.0 (recorded notes with FX at -53 to -60, no leak).

### n°12: note-off rule

- **Problem**: the rule that decides which voices a note-off frees read an unrelated table entry, so the result depended
  on chance.
- **Effect**: deterministic: releasing a pad also frees the MIDI and sequencer voices of the same pad.
- **On a machine**: not tested on its own.

### n°13: Voice Assign bounded when a beat is loaded

- **Problem**: a Voice Assign value above 6 in a loaded beat indexed past the voice table.
- **Effect**: such a value becomes 0 (none).
- **On a machine**: verified on 1.5.4.0 (a beat sent with 9 came back with 0; the other pad kept its value).

### n°14: clearer update screen

- **Effect**: during an OS update the screen says « Writing... DO NOT POWER OFF. » and « When the FX1 LEDs stop: done,
  power off and on. ». It is shown from the next update made while a version containing it is installed (the receiving
  OS draws it).
- **On a machine**: seen during the 1.5.6.0 update.

### n°15: one routine for the three Debug-menu OS senders

- **Problem**: the Main, Voice and Panel senders of the Debug menu were three identical copies.
- **Effect**: same behaviour, 1,584 bytes of flash freed.
- **Proof**: simulated: the bytes sent for Panel and Voice equal DSI's own `.syx` files, the same calls in the same order,
  a wrong expected file refused.
- **On a machine**: verified on 1.5.5.0: the Main send decodes to the installed image (last page aside), the Voice and
  Panel sends carry the real chips.

### n°16: SysEx rings out of the undo beat

- **Problem**: the buffer that holds a received SysEx shared external memory with the undo copy of the beat. Any
  received SysEx, even an identity request, could corrupt Compare and Undo.
- **Effect**: the buffers moved (4 constants); Undo survives SysEx traffic.
- **Proof**: simulated, the unpatched image refused.
- **On a machine**: verified on 1.5.6.0 (a 14 KB SysEx received during recording, then Undo restored the beat
  byte-identical).

### n°17: muted voices

- **Problem** (cause found with medium confidence): with LFO Restart = Play, the sequencer tick interrupt talked to the
  voices while the voice engine was in the middle of a transfer, cutting it; the voice chip then read sound data as
  commands, and one value makes it start a self-calibration that never returns: the voice stays silent until power-off.
- **Effect**: the tick only sets a flag; the restart words are sent by the voice engine's own timer.
- **Proof**: bit-level simulation of the two interrupts and the voice link: 2,287 of 2,375 injection points fail
  without the patch, 0 with it.
- **On a machine**: installed with 1.5.6.0; confirming it needs long playing sessions, not done yet.

### n°18, n°19, n°20: MIDI input robustness

- **Problem**: (18) a SysEx received over USB cleared the DIN running status, so notes from a DIN keyboard were lost or
  stuck; (19) an abandoned SysEx reception left the « Receiving » screen up, the machine looked frozen; (20) a project
  beat message with a beat number above 15 wrote past the beat tables.
- **Effect**: DIN running status kept; the screen closes; the message is refused.
- **Proof**: simulation of the MIDI input (33 checks, the defects shown on the previous version), 6 wrong variants
  refused.
- **On a machine**: n°19 (one path) and n°20 verified on 1.5.6.0. n°18 and the other n°19 paths not tested.

### n°21, n°22, n°23: remote panel

- **Effect**: the whole panel over MIDI. (21) every key: CC 102 press, CC 103 release, value = key code; the Debug menu
  cannot be opened remotely. (22) the panel LEDs are reported: CC 104 on, CC 105 off; CC 107 asks for all of them.
  (23) the 14 encoders (NRPN 200-213, relative steps) and 10 absolute knobs (NRPN 220-229).
- **Proof**: simulated with the firmware's own panel handlers: 69, 30 and 51 checks, 29 wrong variants refused.
- **On a machine**: verified on 1.6.0.0: the 44 LED states returned on request, a remote Click press lit the Click LED
  and was reported, a remote encoder step changed the sound by one step and the opposite step restored it byte for
  byte. The absolute knobs were not checked one by one.
- **Limits**: brightness, flashing and the slider LED bars are not reported. Requires n°9a/9b.

### n°24: whole-project request

- **Effect**: `F0 01 28 42 F7` makes the machine send the whole project (`5E`, then `5C n` for each beat of the project),
  the same bytes as the project export of the menu, on the cable set in System.
- **Proof**: simulated: the bytes equal the menu export of the same project, byte for byte (24,077 bytes); the stack
  peak (6,656 bytes) stays below the menu's own.
- **On a machine**: works on 1.6.1.0; a project captured this way is read by the app.
- **Limits**: on 1.6.1.0 the progress lines are drawn over the current screen (fixed by n°26).

### n°25: 4 Pole and AD Mode LEDs follow NRPN

- **Problem**: NRPN 40 (4 Pole) and 57 (AD Mode) changed the sound but not the key's LED.
- **Effect**: the LED follows, as with the key.
- **Proof**: simulated against the real key path, 7 wrong variants refused.
- **On a machine**: installed with 1.6.1.0; the LED behaviour was not checked specifically.

### n°26: project request without screen

- **Effect**: the request of n°24 sends the same bytes without drawing anything; the menu export keeps its screen.
- **Proof**: simulated with the real screen routines: 0 bytes written to the screen for a request, the menu export
  identical store for store.
- **On a machine**: installed with 1.6.2.0 (image verified); confirmed in use with 1.6.3.0 (9 Nov), without a targeted test of each case.

### n°27: standard last group in packed messages

- **Problem**: the machine packs SysEx data in groups of 7 bytes plus a byte of high bits. Its final flush, for a
  message whose length is not a multiple of 7, shifted the high bits and left stale bytes (and wrote one byte past its
  buffer for 6 bytes). The machine's own receivers read the standard layout, so it misread the end of its own exports.
- **Effect**: every packed message it sends ends in the standard layout. One word.
- **Proof**: simulated packer and real senders, every length 0-6; only the last groups differ from 1.6.1.0.
- **On a machine**: installed with 1.6.2.0 (image verified); confirmed in use with 1.6.3.0 (9 Nov), without a targeted test of each case. The app reads both layouts.

### n°28: project receive

- **Problem**: when receiving a project, the machine read each used Play List one word too long; every later list was
  shifted, and the last one was filled from stale buffer bytes.
- **Effect**: a project sent back to the machine keeps its 4 Play Lists. One word.
- **Proof**: simulated end to end: the firmware's sender on one machine, the app's re-encoding, the firmware's receiver
  on another, with 0 to 4 lists used and with a real captured project; the unpatched version fails.
- **On a machine**: installed with 1.6.2.0 (image verified); confirmed in use with 1.6.3.0 (9 Nov), without a targeted test of each case. The app warns before sending a project to a machine without it.
- **Limits**: by DSI's own rule, an empty Play List 1 is filled with the selected beat after a reception.

### n°29: beat-wide positions over MIDI

- **Problem**: in 16 Beats / 16 Mutes the knobs act on the whole beat but nothing was sent, so an editor could not
  follow them.
- **Effect**: after any panel action that moves one of the 8 beat-wide positions, after a beat change and on CC 107, the
  machine sends CC 20-27 (position / 2) and CC 52-59 (low bit); the same CCs received set the position, never echoed.
- **Proof**: simulated: 576 control x mode x Shift runs, round trips, no echo, output rate limit; 13 wrong variants
  refused.
- **On a machine**: installed with 1.6.2.0 (image verified); confirmed in use with 1.6.3.0 (9 Nov), without a targeted test of each case.

### n°30: SysEx timeout

- **Problem**: a sender that stopped before the end of a SysEx left the machine waiting forever: the Receiving screen
  stayed, the panel and MIDI notes were swallowed.
- **Effect**: after about 3 s without a byte, the message is abandoned (screen closed, file closed). OS updates are
  never abandoned.
- **Proof**: simulated: cuts in every state on DIN and USB, slow valid messages kept, a real OS update received; 10 wrong
  variants refused.
- **On a machine**: installed with 1.6.2.0 (image verified); confirmed in use with 1.6.3.0 (9 Nov), without a targeted test of each case.

### n°31: the FX ribbons over MIDI

- **Problem**: the two Real Time FX ribbons could only be played by hand; nothing was sent or received over MIDI.
- **Effect**: on Remote Pad IN, CC 108 / 109 touch ribbon 1 / 2 at a position (0-127), CC 110 / 111 release it and
  CC 112 / 113 set its pressure, with the same effect as a finger (Note FX, Beat FX in 16 Beats, Latch, the FX3 / FX4
  bank). Moving a ribbon by hand sends CC 108 / 109 (position) and CC 110 / 111 (release) on Remote Pad OUT. A remote
  move is never echoed. Bytes: [MIDI protocol](midi-protocol.md#fx-ribbons).
- **Proof**: simulated with the firmware's own panel handlers and MIDI input; wrong variants refused.
- **On a machine**: installed with 1.6.3.0 (image verified) and used on the machine.
- **Limits**: pressure is not reported; CC 107 does not resend the ribbons. Requires n°9b.

### n°32: a footswitch plays a pad

- **Effect**: System, *Pedal 1/2 Assignment*, in a switch mode, goes on past the existing choices to « Pad 1 » ...
  « Pad 16 ». The footswitch then presses and releases that pad like a finger, at the Fixed Level Velocity.
- **Proof**: simulated: every mode, both inputs, Norm Open and Norm Closed, the same calls as the panel pad.
- **On a machine**: installed with 1.6.3.0 (image verified) and used on the machine.

### n°33: Load Sound remembers its place

- **Problem**: the Load Sound knob jumped back to the first sound of the folder at every change of pad or folder.
- **Effect**: the position in each of the 14 sound folders is kept (until power-off) and bounded by the folder size.
- **Proof**: simulated on the real user-interface code, with the official behaviour shown on the previous version.
- **On a machine**: installed with 1.6.3.0 (image verified) and used on the machine.

### n°34: Revert Snd asks for a second press

- **Problem**: one accidental press of Revert Snd threw away the edits of the sound.
- **Effect**: the first press arms it (the soft key shows « Confirm? »); a second press on the same beat and pad
  reverts. Any other panel action disarms it. Shift + Undo Snd is unchanged.
- **Proof**: simulated on the 8 screens that show the key; wrong variants refused.
- **On a machine**: installed with 1.6.3.0 (image verified) and used on the machine.

### n°35: Load Sound in alphabetical order

- **Problem**: the Load Sound knob walked a folder in directory order, not in the order of the Save/Load browser.
- **Effect**: it walks the folder in the browser's alphabetical order, built in one pass over the directory. Above 256
  files in a folder, it keeps the directory order.
- **Proof**: simulated: the list equals the browser's, name for name; far fewer reads of the storage than the
  browser's own sort.
- **On a machine**: installed with 1.6.3.0 (image verified) and used on the machine.
- **Limits**: the time of a storage read is not modelled; a very large folder may take a moment the first time.

### n°36: play probability per pad

- **Effect**: each pad of each beat has a play probability from 0 to 100 %. When the sequencer reaches a note of that
  pad, it plays with that probability, otherwise it is skipped (no voice, no MIDI note, nothing to release). Set with
  CC 14 = the percentage, on Remote Pad IN, for the selected pad of the beat being edited. Saved with the beat, in beat
  and project files and SysEx; reset to 100 % by Init Beat. 100 % (the default, and every existing beat) plays exactly
  as before. Bytes: [MIDI protocol](midi-protocol.md#play-probability).
- **Proof**: simulated: draws, skipped notes, the beat format both ways, the CC on both ports; wrong variants refused.
- **On a machine**: installed with 1.6.3.0 (image verified) and used on the machine.
- **Limits**: no screen of the machine shows the value. The official OS ignores it: such a beat plays every note there.

### n°37: room for new functions (dead code erased)

- **Problem**: the official OS carries code that nothing can ever run (an unused text formatter of the C library,
  floating-point helpers, file-system calls nobody makes, USB, flash and MIDI helpers without caller), and two earlier
  Tempester patches (n°9a, n°17) left the code they replaced in place. The free flash left on 1.6.3.0 was too small
  for the next functions.
- **Effect**: 9,876 bytes in 38 pieces of 64 to 2,228 bytes are erased (0xFF) and can hold new code. Nothing changes
  for you: the machine runs exactly the same instructions.
- **Proof**: static: a reachability map of the whole image (from the reset and every interrupt vector, through every
  jump, call, jump table and code pointer, including pointers built by instruction pairs) finds no path and no pointer
  to any erased piece, a disassembler's own references agree, and the patched image reaches exactly the same code as
  1.6.3.0. Counter-proofs: erasing a live routine next to an erased piece, code kept alive only by one pointer, or a
  piece the map calls only « probably » dead is refused. Simulated: every simulation of 1.6.3.0 passes on it.
- **On a machine**: not installed yet (test build 1.6.4.0-a).
- **Limits**: requires n°9a and n°17. A code address computed at run time by arithmetic would not be seen by the map
  (no sign of one). The Main OS read back from the machine shows 0xFF in those pieces.

### n°38: the machine reports its mixer, beat and main values

- **Problem**: Level, Pan, Delay Send, Delay Time, BPM... could be turned from an editor (remote panel), but turning
  them on the machine sent nothing, so the editor could not follow.
- **Effect**: on Remote Pad OUT, the machine sends NRPN 230-239 when one of these values changes: Level, Pan and Delay
  Send of the selected pad, Delay Time and Delay Repeats of the beat, the tempo (BPM × 10), Swing, Volume, Distort
  and Compress. Coalesced, at most 4 per main-loop pass, only when the DIN output buffer is nearly empty; all 10 on
  CC 107. CC 12 / CC 13 are not echoed. Bytes: [MIDI protocol](midi-protocol.md#mixer-beat-and-main-values).
- **Proof**: simulated with the firmware's own knob handlers and MIDI input: every control x mode x Shift, the other
  messages unchanged, no echo, output rate limit; 18 wrong variants refused.
- **On a machine**: not installed yet (test build 1.6.4.0-b).
- **Limits**: only the selected pad's mixer values; the tempo is not reported while the machine follows MIDI clock
  (except on CC 107). Requires n°9b.
- **Found on the way**: Shift + Pan sets Delay Repeats (the remote-panel table said Pan had no Shift function).

### n°39: the exact version on request

- **Problem**: the identity answer of every Tempester gives the same version word, so a program could not tell one
  Tempester release from another.
- **Effect**: the request `F0 01 28 43 F7` answers the exact version `a.b.c.d` on the SysEx cable set in System. The
  number is read where the System screen reads it, so every release announces its own number without a new patch (a
  test build keeps the number of its base release). The identity answer is unchanged. Bytes:
  [MIDI protocol](midi-protocol.md#sysex-requests).
- **Proof**: simulated: the answer on DIN and USB, the number following the screens for other versions, every other
  command and the identity unchanged, the previous version silent; wrong variants refused.
- **On a machine**: not installed yet (test build 1.6.4.0-c).
- **Limits**: digits up to 16383. Requires n°1.

### n°40: the Pad Function mode reported

- **Problem**: a program could only guess the Pad Function mode from the four mode LEDs, after they settled.
- **Effect**: the mode (16 Beats, 16 Time Steps, 16 Sounds, 16 Tunings, 16 Mutes, 16 Levels) is sent as CC 106 on
  Remote Pad OUT when it changes and after a full-state request (CC 107), with the rules of the LED reports. Bytes:
  [MIDI protocol](midi-protocol.md#pad-function-mode-n40-needs-n9b).
- **Proof**: simulated with the firmware's own panel keys and remote keys: each mode reported once, Off, the output
  buffer guard, CC 107 on the right channel only, every other MIDI input unchanged; wrong variants refused.
- **On a machine**: not installed yet (test build 1.6.4.0-c).
- **Limits**: the mode cannot be set over MIDI (send the Pad Function keys). Requires n°9b.

### n°41: MIDI notes in Pad to Note Mode « Mutes », banks A and B

- **Problem**: with System › Pad to Note Mode = Mutes, a note muted the sound shown on the matching panel pad, so with
  bank B shown the note of A3 muted B3, and the notes of bank B muted a wrong sound (usually A1); a note assigned to no
  pad could also toggle a stray sound.
- **Effect**: the note of a pad (System › MIDI Note: Pad A1-B16) mutes or unmutes that pad's sound, whatever bank is
  shown; a note assigned to no pad does nothing. The « MIDI Note: Mute » items are still not used.
- **Proof**: simulated through the MIDI input and the real note handler, both banks; the other modes identical to
  1.6.3.0; wrong variants refused.
- **On a machine**: not installed yet (test build 1.6.4.0-d).

### n°43: the first Save no longer offers the Factory folder

- **Problem**: after power-on, the first Save Beat or Save Project proposed the « Factory » folder.
- **Effect**: it proposes « Beats 1 » and « Projects 1 »; after that the folder you choose or load from is kept, as
  before, and Factory can still be chosen by hand.
- **Proof**: simulated from the boot call to the Save dialog.
- **On a machine**: not installed yet (test build 1.6.4.0-d).
- **Limits**: the folder of the project loaded at startup is not used.

### n°44: « Undo All Snds » restores everything a single undo does

- **Problem**: Undo All Snds (and Undo Beat, which runs the same code) copied the sounds back but left the sound types,
  the play states (a sound whose volume crossed zero stayed silent or audible wrongly), the Voice Assign reservations
  and the pad's LEDs as they were.
- **Effect**: the same restore as Undo Sound, for every pad.
- **Proof**: simulated on the real Undo screen with a model of the sound memory; the pad after Undo All equals the pad
  after a single Undo Sound; wrong variants refused.
- **On a machine**: not installed yet (test build 1.6.4.0-d).

### n°45: Reverse Off turns every reverse off

- **Problem**: Reverse on, then a change of beat or of pad mode, then Reverse off: the LED went off but a beat kept
  recording reversed notes, or the playback stayed reversed.
- **Effect**: Reverse off clears the reverse of every beat and the live reverse of 16 Beats / 16 Mutes.
- **Proof**: simulated through the real Reverse key; Reverse on identical to 1.6.3.0; wrong variants refused.
- **On a machine**: not installed yet (test build 1.6.4.0-d).
- **Limits**: after a beat change during playback in 16 Beats / 16 Mutes, the LED can still stay lit while the
  playback is forward (unchanged).

### n°46: the Save/Load browser opens a folder much faster

- **Problem**: the browser read the folder's directory again for every file name: 9,161 directory reads for 76 files,
  growing with the square of the folder size.
- **Effect**: one pass over the directory; the same list, in the same order. When that pass cannot be sure of the
  result (an empty folder, more than 256 files, a folder that changed while it was read), the official listing runs.
- **Proof**: simulated for sound, beat and project folders: the list equal to 1.6.3.0's, file for file; wrong
  variants refused.
- **On a machine**: not installed yet (test build 1.6.4.0-d).
- **Limits**: the time of a storage read is not modelled.

## How the patches are proved

- **Static checks**: every replaced word is checked against the input image (by SHA-256 of the whole file and of the
  result), no two patches overlap, the update page is never touched, and every version rebuilds to its recorded hashes.
- **Independent re-import**: the patched image is loaded again in a disassembler from scratch, and only the intended
  functions change.
- **Simulation**: an instruction-level MIPS simulator runs the firmware's own code from the user's image (MIDI input,
  panel handlers, SysEx senders and receivers, screen routines, interrupts and the voice link where needed). The
  checks run on the patched image, on the unpatched one (which must fail) and on wrong variants (which must fail).
- **On a real machine**: after the author's go, over USB, one version at a time; see [testing](testing.md).

None of this is a guarantee. Read [Before you install](user-manual.md#before-you-install-a-tempester-version).

## Known defects not fixed

Among others: crossed note-offs lost in rare cases; the projection shift in odd-meter loops (n°8 limits); the
compressor Amount clamp; a latent defect in the factory update routine (a failed read-back of one row erases the whole
4 KB page and rewrites only that row; it never showed and the page is never modified by Tempester); several forum
reports studied but not fixed (copy of a beat onto the playing beat, arpeggiator sync, roll swing, and more); the
« start offset as slave » report is not explained by the code: the first step plays a tenth of a millisecond after the
first clock, and only the last quarter of each clock is late.
