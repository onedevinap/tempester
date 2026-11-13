# Changelog

All notable changes to Tempester. Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The app follows
[Semantic Versioning](https://semver.org/); firmware versions follow the Tempest's own four-part numbering (numbers DSI
never used). Details of each patch: [docs/firmware.md](docs/firmware.md).

## App

### [Unreleased]

### [0.1.1] - 2026-11-13

#### Added

- **Support for Tempester OS 1.6.5.0**: notes up to 768 steps long are read, written and exported; the fine Quantize
  value (project word 18) is kept, and the project send says whether the machine's OS keeps it; the pad notes panel
  reads, sets and saves each pad's MIDI channel (a read that times out says « check Remote Pad OUT »). The app edits no
  System file, so the Swing Source « +Roll » option is set on the machine only.
- **The app shows and verifies the OS the machine reports**: the exact version a Tempester 1.6.5.0 answers is used
  everywhere (status line, OS page, read-back of the Main OS compared with that version's image).
- **Machine identity card**: it shows the exact version the machine reports and says, for each of the 12 Tempester
  features, whether this version has it (« Available » / « Not in this version »). Without a reported version it keeps
  the old guesses.
- **MIDI monitor**: names for the play probability, the pad channel, the ribbons and the mixer reports (CC 14 to 17,
  107 to 113, NRPN 230 to 239).

#### Changed

- **Play Lists guard**: a project is sent plainly when the version the machine reports has the Play List fix, even
  without an OS installed from the app; when nothing is known, the app warns.
- **Controls that need a newer Tempester** say « x.y.z.w or later », or « OS unknown: check the machine identity »
  when the app cannot tell which OS runs; a machine that answered as the original DSI OS has none of them.

#### Fixed

- The Machine page no longer tells you to read the version on the machine when the machine reports it.
- Protocol notes: the mixer reports, the exact-version request and the Pad Function report are in release 1.6.4.0.
- The OS chip of the top bar was cut at both ends at 1320 px; it now reads « OS: 1.6.5.0 » (the whole release is its tooltip).
- The Panel page said « Not connected » once connected; it says « Not synced » until a sound is synced (« Offline » when the link is down). The Play page
  no longer asks you to connect when you are connected.
- The Machine page names the release the machine reports (« Tempester 1.6.5.0 ») instead of the version word.
- The OS page no longer calls patch n°52 (compressor Amount down to -127) optional in release 1.6.5.0: it is kept.
- The MIDI channel of each pad was hard to find: on the Machine page, the « Pads » button of the top strip opens the
  table of notes, and the channels are under it.
- The MIDI channel section of the Pads panel no longer leaves an empty band between its title and the pad grid.

#### Security

- **Tempester Control no longer contains `atty`** (RUSTSEC-2021-0145, unaligned read on Windows, unmaintained with no
  fixed release). nih-plug's logger only asks whether stderr is a terminal; a small stand-in with the same API, built on
  the standard library, replaces it (`app/crates/tempester-plugin/atty-std`).
- **`glib` 0.18 (RUSTSEC-2024-0429) stays, not reachable**: the advisory is about `glib::VariantStrIter`, which neither
  the app nor its GTK and WebKit libraries call. The Linux app gets glib through wry's GTK 3 bindings, which stop at
  0.18 even in the latest wry; it goes when wry moves on.

### [0.1.0] - 2026-11-10

First public version.

#### Added

- **The machine's own moves follow in the app** (Tempester OS 1.6.4.0): Level, Pan, Delay Send, Delay Time, Delay
  Repeats, BPM, Swing, Volume, Distort and Compress turned on the machine move the Machine view's knobs (and the Full
  view's Volume, Distort and Compress); the Pad Function mode and the exact Tempester version are taken from the
  machine's own reports.
- **Machine view as the machine shows it**: LED colours by key (Shift and Sounds orange; 16 Sounds, 16 Mutes, Pads and
  Events green; 16 Beats red; Play blue, left of its title; the others blue); a lit pad keeps its colour with a round
  blue halo in the centre.
- **OS update page**: three choices build (the latest release, the corrections-only one, back to DSI 1.5.0.2); the
  History lists every release, to read.
- **Panel page**: a Machine view that draws only the real panel's controls at their place and drives them remotely
  (49 keys, 14 encoders with step display, 10 absolute knobs, 16 pads, the FX ribbons, the machine's LEDs); a Full view
  with every control of the Machine view (Mixer, BPM / Swing, Select, Mod Paths included) plus the 127 parameters of the
  selected sound, live both ways over NRPN; knobs and encoders drawn as on the machine in both views; greying of the
  controls the firmware ignores in 16 Beats / 16 Mutes; zoom 60-200 %; view and zoom remembered.
- **Pad notes** taken only on the machine's Remote Pad OUT channel (the channel of its LED messages), so a Sequencer or
  Synth Sound playing its pitch on another channel is never mistaken for a pad.
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

### [1.6.5.0] - 2026-11-12, installed and verified

For a player:

- n°48: after a beat change, the Reverse LED shows whether the pads play reversed now (off in 16 Beats / 16 Mutes);
  one press of Reverse reverses again.
- n°49: Pads + Sounds (« Mixer ») opens the Mixer after the compressor envelope page or the Compression / Distortion
  page, instead of reopening that page.
- n°50: in 16 Time Steps a pad lights when any note of the selected sound starts inside its step; pressing a lit pad
  removes those notes.
- n°51: with the sequencer stopped, Roll and arpeggio repeats fall on the Quantize grid and no longer drift at each
  loop; the Momentary Roll footswitch turns Roll on while pressed and off when released.
- n°52: the compressor envelope Amount reaches −127 as shown (the knob stopped at −64). Shift + Compression knob opens
  the envelope page; Amount is the 4th encoder. Listened to by the author, who keeps it.
- n°53: notes up to 768 steps long, a whole 8-bar beat (255 before); a beat with long notes played on an older OS
  plays them short.
- n°54: option System > Swing Source « Project+Roll », « Beat+Roll » or « System+Roll »: the beat roll follows the
  swing (1/16 and shorter rolls alternate long-short). Without it, as before.
- n°61: a playing beat roll no longer loses notes recorded late in its window.
- n°55: eight more Quantize values for recording: 8 Dot, 16 Dot, 32 Dot, 64th, 64T, 8 Quint, 16 Quint, 16 Sept.
  Saved with the project; the official OS and older Tempester versions load such a project with Quantize Off.
- n°56: Edit > Copy Sound obeys the Seq / NoteFX / Mixer switches of the Copy screen, so a sound's sequence can be
  copied to another beat, also while playing. No undo.
- n°58: a MIDI output channel for each pad (Off, 1-16 with the pad's played note, 17-32 = channel minus 16 with the
  pad's fixed « MIDI Note: Pad »), for the sequencer and the pads played by hand. Set over MIDI on Remote Pad IN:
  CC 15 pad (0-31), CC 16 value (0-32), CC 17 = 127 saves the System settings (stopped only).
- n°59: Shift + pad only selects the pad, in every mode, with no sound, MIDI or recording; a second press of the same
  pad opens the pad settings page: MIDI note, MIDI channel, play probability and Level, on the four encoders.
- Test versions 1.6.5.0-f, -g, -h, -i and -j hold n°48-n°51, n°52-n°53, n°54 and n°61, n°55-n°56, and n°58-n°59, to
  prove each group alone.
- Installed with the author's go on 12 November 2026: the machine answers 1.6.5.0 to the exact-version request, and the
  Main OS read back equals the built image on the written range. Hand tests of each new function are still to do
  (n°52 listened to).

### [1.6.4.0] - 2026-11-10, installed and verified

- n°37: 9,876 bytes of the official code that nothing can run are erased, room for new functions; the machine runs
  the same instructions.
- n°38: Level, Pan, Delay Send, Delay Time, Delay Repeats, BPM, Swing, Volume, Distort and Compress are reported on
  Remote Pad OUT (NRPN 230-239) when they change on the machine, so an editor follows.
- n°39: the request `F0 01 28 43 F7` answers the exact Tempester version.
- n°40: the Pad Function mode is reported as CC 106 on Remote Pad OUT.
- n°41: in Pad to Note Mode « Mutes », a note mutes its own pad's sound, bank A or B.
- n°43: the first Save after power-on no longer offers the Factory folder.
- n°44: Undo All Snds and Undo Beat restore sound types, play states, Voice Assign and pad LEDs.
- n°45: Reverse Off clears the reverse of every beat and the live reverse.
- n°46: the Save/Load browser lists a folder in one pass, much faster.
- Test versions 1.6.4.0-a, -b, -c and -d hold n°37, n°38, n°39-n°40 and n°41-n°46, to prove each group alone.
- Not made: n°42 (slave Start: the suspected cause was disproved) and n°47 (Copy Beat onto the playing beat).
- Installed over USB on 10 November 2026 (the machine had to be power-cycled by hand after the send): the exact version
  is answered and the Main OS dump equals the built image; hand tests of the new behaviours are still to do.

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
