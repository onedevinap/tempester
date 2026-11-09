# Changelog

All notable changes to Tempester. Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The app follows
[Semantic Versioning](https://semver.org/); firmware versions follow the Tempest's own four-part numbering (numbers DSI
never used). Details of each patch: [docs/firmware.md](docs/firmware.md).

## App

### [0.1.0] - unreleased

First public version.

#### Added

- **Panel page**: the 127 parameters of the selected sound, live both ways over NRPN (Full view); a Machine view that
  draws only the real panel's controls at their place and drives them remotely (49 keys, 14 encoders with step display,
  10 absolute knobs, 16 pads, the machine's LEDs); greying of the controls the firmware ignores in 16 Beats / 16 Mutes;
  zoom 60-200 %; view and zoom remembered.
- **Library page**: current sound, current beat and whole project (fetch, wait for the menu's Send Project, save, send
  back); library of sounds, beats and projects with search, filters, tags, favourites, details, history of older
  versions with compare and restore, rename (internal name and file together), trash; an index rebuilt from the files
  when lost; nothing overwritten or duplicated in silence; before a send, the machine's own item is fetched and backed
  up when it is unknown.
- **Whole projects**: received in any order of present beats, partial projects saved and named as such; saved as the
  exact capture and in the standard layout; sent back with a version guard (the Play List defect fixed by n°28), a
  progress bar, a cancel, and *Fetch it back and compare*.
- **Play page**: 16 pads, a MIDI clock master, and a Euclidean sequencer under each pad (steps, hits, rotation,
  probability), played on the app's clock; Note Offs always sent, also on stop and disconnect.
- **Kit builder** (Library): put library sounds on the 16 or 32 pads of the machine's current beat; only those sounds
  change in the beat, and the machine's beat is backed up first when the library does not hold it.
- **MIDI file export** (Library): a beat as a Standard MIDI File, one track per pad that plays, never over an existing
  file.
- **Tempester Control**: a CLAP and VST3 MIDI effect whose 146 parameters are the Tempest's sound parameters (NRPN), the
  beat-wide positions, Distort, Compress and Volume; MIDI learn, optional follow of the machine, a rate limit, no
  allocation in the audio thread. It opens no MIDI port: the DAW routes its output. No editor window yet.
- **Machine page**: channel, identity shown in full with the features the version word proves, installed version,
  data folder, live MIDI monitor (opt-in, readable descriptions, never slows the MIDI path).
- **Backup page**: guided capture of the Debug-menu Main, Voice and Panel OS sends, with constant images flagged.
- **OS update page**: builds a Tempester version from the user's own `Tempest_Main_1.5.0.2.syx` (SHA-256 checked), checks
  the result against the reference SHA-256 of the version, recognises any opened file by its hash (an unknown build
  needs an extra confirmation), sends at the cable rate with progress and cancel, remembers what was installed, reads
  the Main OS back from the machine and compares the range an update writes (the machine's own update routine page is
  reported apart); *Back to DSI 1.5.0.2*.
- Every file the app writes for sending and every message it sends uses the standard 7-bit layout; the app reads both
  the standard layout and the one older firmware writes at the end of a message.
- A simulated machine (`TEMPESTER_FAKE_MIDI=1`) for trying everything without hardware.
- Packages (unsigned): macOS `.dmg` (app) and `.pkg` (app and plugins), Linux `.deb` for the app and one for the
  plugins, Windows zip and installer (app and plugins as options); `tempester --version`.
- Tests on a disposable home folder, UI tests on a fake DOM with counter-proofs, a layout-stability test in Chrome and
  WebKit, CI on Linux, macOS and Windows; Rust pinned to 1.99.1.

## Firmware

Each version is built from the official Main OS 1.5.0.2 by the user; Voice, Panel and SAM are unchanged.

### [1.6.3.0] - 2026-11-09, installed and verified

- n°31: the two FX ribbons over MIDI, both ways (CC 108-113 in, CC 108-111 out).
- n°32: a footswitch can play a pad (System, Pedal 1/2 Assignment: « Pad 1 » to « Pad 16 »).
- n°33: the Load Sound knob remembers its position in each sound folder.
- n°34: Revert Snd asks for a second press.
- n°35: the Load Sound knob walks a folder in alphabetical order.
- n°36: a play probability for each pad of each beat, set with CC 14 and saved with the beat.
- Test versions 1.6.3.0-a, -b and -c hold n°31-n°32, n°33-n°35 and n°36, to prove each group alone.

### [1.6.2.0] - 2026-11-08, installed and verified

- n°26: the project request draws nothing on the screen.
- n°27: every packed SysEx message ends in the standard layout.
- n°28: a received project keeps its Play Lists.
- n°29: the 8 beat-wide positions reported and set as CC 20-27 + CC 52-59.
- n°30: a SysEx message that never ends is abandoned after about 3 s.
- Test versions 1.6.1.1 to 1.6.1.5 hold one of these patches each, to prove it alone.

### [1.6.1.0] - 2026-11-08, installed

- n°24: request `F0 01 28 42 F7` sends the whole project.
- n°25: the 4 Pole and AD Mode LEDs follow a change by NRPN (also alone in test version 1.6.0.1).

### [1.6.0.0] - 2026-11-08, installed

- n°21, n°22, n°23: the remote panel: keys (CC 102/103), LED states (CC 104/105, CC 107), encoders (NRPN 200-213) and
  absolute knobs (NRPN 220-229).

### [1.5.6.0] - 2026-11-07, installed (corrections only)

The fixes of 1.5.1.0 to 1.5.5.0 with the sound and beat requests and NRPN, plus these stability fixes; none of the
1.6 additions.

- n°16: SysEx buffers moved out of the undo copy of the beat (Compare/Undo corrupted by any received SysEx).
- n°17: LFO Restart = Play no longer cuts the voice transfers (voices muted until power-off).
- n°18: a USB SysEx no longer clears the DIN running status.
- n°19: an abandoned SysEx reception closes the Receiving screen.
- n°20: a project beat message for a beat above 15 is refused.

### [1.5.5.0] - 2026-11-07, installed

- n°15: the three Debug-menu OS senders merged into one routine; 1,584 bytes of flash freed; same behaviour.

### [1.5.4.1] - 2026-11-07, test version

- n°14: clearer OS update screen (« Writing... DO NOT POWER OFF. »).

### [1.5.4.0] - 2026-11-07, test version, installed

- n°2d: project export stack frame halved.
- n°7d: Play List Delete Step no longer writes into the next list.
- n°10: sequence copy looks for space in the target beat.
- n°11: Note FX values kept in their own 7 bits.
- n°12: deterministic note-off rule.
- n°13: Voice Assign bounded when a beat is loaded.

### [1.5.3.0] - 2026-11-07, installed

- n°9a, n°9b: NRPN received (parameters 0-126, samples 156/157) and sent when a knob turns.

### [1.5.2.0] - 2026-11-07, installed

- n°1: requests for the current sound (`40`) and beat (`41`); identity answer on the chosen cable, version word `0x7F2`.

### [1.5.1.0] - 2026-11-07, installed

- n°2a, n°2b, n°2c: stack frames of the beat send, beat receive and project header receive reduced below the stack size
  (freeze on « Exporting Project », memory overwritten by each beat reception).
- n°3: pressure of sliders 2-4 on the real-time FX.
- n°4a, n°4b: compressor Peak Hold saved; compressor envelope sent at each beat selection.
- n°5: note length editable up to 255.
- n°6: USB SysEx import no longer cut by DIN traffic.
- n°7a, n°7b, n°7c: Play List freezes (empty step, RT REC) and RT REC past 100 steps.
- n°8: Loop Screen freeze outside 4/4.
