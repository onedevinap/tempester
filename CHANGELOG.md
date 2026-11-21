# Changelog

All notable changes to Tempester. Format: [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The app follows
[Semantic Versioning](https://semver.org/); firmware versions follow the Tempest's own four-part numbering (numbers DSI
never used). Details of each patch: [docs/firmware.md](docs/firmware.md).

## App

### [Unreleased]

### [0.1.7] - 2026-11-21

#### Added
- **Tempester OS 1.7.0.0**, the release the app now offers: 1.6.9.1 with the Sound of any beat and pad read or stored
  over SysEx (requests `46` and `47`, issue #6). A machine that reports 1.7.0.0 gets every feature of 1.6.9.1 in the
  app; the app itself does not use `46` / `47` yet.

### [0.1.6] - 2026-11-21

#### Fixed
- **Tempester Control: Ableton Live refused the VST3** (« plugin has an effect category, but no valid audio input
  bus », issue #5). The plugin had no audio input or output. It now has a stereo input and output and passes the audio
  through untouched; its MIDI output is unchanged. The CLAP build gets the same audio ports and is also listed as an
  audio effect. **Not tried in Live, on Windows or elsewhere**: the project has no Windows machine. The buses, the
  stereo arrangement and the untouched audio were checked through the plugin framework's VST3 code and a test CLAP
  host, without a DAW.
- **Tempester Control announced version 0.1.0** to the DAW whatever the app's version. It now announces the app's
  version, also written in the macOS plugin bundles (they said 1.0.0).
- **Windows and Linux: during an OS update the app looked frozen and could not be closed.** The file leaves in one
  piece, and the MIDI driver reports nothing until it ends (at least about 3 minutes for the Main). The OS update page
  now counts the elapsed time against the least the send takes, says so when it runs longer, and never calls the
  send failed on its own: only the end of the send decides. Closing the window during such a send now asks whether
  to quit now, which interrupts the update (power the Tempest off and on, then send the file again), or to keep
  sending. macOS is unchanged: its send has a progress bar and a Cancel button. **Not tried on Windows or Linux**: the
  project has no such machine.
#### Changed
- **Full backup: an unusual calibration shape is a warning, no longer a refusal** (issue #4). Four Voice chips of
  another machine were refused with « table A does not increase », so no full backup could be saved there. The shape
  rules came from one machine only, and the chip uses such a page as it is: the row now says « warning », the backup
  can be saved, and the manifest and the report name the warning. A page that was not read where the chip keeps it
  (blank, a word without the calibration's marker byte, an erased word: what a dump shifted by one to three bytes
  gives) is still refused.

#### Added
- **Full backup: the 35 calibration words of each Voice chip on the page**, under its row, accepted, warned or
  refused, and in the report, so they can be quoted in a bug report (they are the machine's own measurements; the
  Voice `.syx` files hold DSI's Voice OS and must not be posted). A refused or warned chip's row also says whether its
  pages before the calibration are identical to another chip's.

### [0.1.5] - 2026-11-21

#### Added
- **Tempester OS 1.6.9.1**, the release the app now offers: 1.6.9.0 with the pad settings page's knobs in the screen's
  order (issue #3). A machine that reports 1.6.9.1 gets every feature of 1.6.9.0 in the app.

### [0.1.4] - 2026-11-20

Three fixes for Windows, from the first reports of a Windows user (issues #1 and #2). **None of them could be tried
on Windows**: the project has no Windows machine. macOS sends exactly what it sent before.

*Note added later*: on 20 November 2026 the Windows user confirmed, on Windows 10, that the installed app 0.1.4 starts
and that an OS update over USB works (issues #1 and #2).

#### Fixed
- **Windows: the installed app showed its window for an instant and quit** (the portable zip worked). WebView2 kept
  its data beside the executable, under Program Files, where it cannot write. The data now lives in
  `%APPDATA%\Tempester\WebView2`. A failure at start-up is no longer silent: its message is written to
  `startup-error.txt` in the data folder (`%APPDATA%\Tempester`), or in the temporary folder when the data folder
  itself could not be opened.
- **Windows: an OS update stopped after 128 bytes** with « non-sysex message must not be longer than 3 bytes ». The
  app sent the file in 128-byte pieces, which the MIDI layer refuses on Windows. An OS file is one SysEx message, so
  on Windows and Linux it now leaves in one piece: the OS update page shows no progress until the end and offers no
  cancel (the countdown on the Tempest's screen shows the progress). Not confirmed: that the Windows MIDI driver
  takes the whole file (about 600 KB) in one go, and an update through a MIDI interface on the DIN input. After a
  failed attempt, power the Tempest off and on before sending again.
- **Windows and Linux: parameter edits and the remote panel** sent the four Control Changes of an NRPN in one packet.
  Windows refused it, and Linux sent only the first of the four. Each message now goes out on its own.

### [0.1.3] - 2026-11-16

#### Fixed
- **macOS: the downloaded app was reported « damaged » and would not open.** The packaging signed only the binary
  (the linker's ad-hoc signature), so the bundle's Info.plist and resources were not sealed and the signature was
  invalid; with the download quarantine, Gatekeeper refused it with no way past. The app and the CLAP / VST3 bundles
  are now ad-hoc signed as a whole (`tools/app_mac.sh`, `tools/package_mac.sh`): the first launch asks for **Open
  Anyway** in System Settings > Privacy & Security, as documented. The packages are still not signed with an Apple
  Developer ID. If 0.1.2 is already installed: `xattr -dr com.apple.quarantine /Applications/Tempester.app`.

### [0.1.2] - 2026-11-15

#### Added

- **Support for Tempester OS 1.6.6.0 to 1.6.9.0** (1.6.9.0 is the release the app offers).
- **Full machine backup** (Backup page, Tempester 1.6.6.0 or later): the app asks the machine for its full backup (the
  six voice chips with their calibration, the Main boot flash and its configuration words, the System settings with
  the MIDI channel of each pad), checks every message and its order, and saves a dated folder with the messages as
  received, the System settings file rebuilt from them, the pad channels, a manifest and a report. Each Voice row
  names the chip's Voice OS (Tempester Voice when its program pages equal the built image, the official Voice 1.5, or
  an unknown build), says whether those pages equal the Voice update last sent, and whether the calibration page is
  unchanged since the previous full backup. The files are for a programmer restore and are never sent back. Before
  Tempester 1.6.8.0 the page allows one full backup per power-on (a second one stops the voice chips) until you
  confirm the machine was powered off and on; a Voice message the machine could not read (1.6.8.0's empty message)
  is refused with the same advice.
- **Tempester Voice on the OS update page**: a Voice card builds Tempester Voice 1.5.1.2 from your own
  `Tempest_Voice_1.5.syx` (its SHA-256 checked, the result checked against the version's three hashes) and sends it to
  the six voice chips at once, with the Main update's confirmations and progress, a warning that one update goes to
  all six chips and what a power cut means, a full backup asked before and after, and a warning when the machine
  reported a Main older than 1.6.9.0 (or none). Tempester Voice 1.5.1.0 and 1.5.1.1 are in the Voice history,
  marked *Superseded*, and are no longer built or sent.
- **Machine identity**: the Voice OS of each of the six chips (Tempester 1.6.7.0 and later report it); the rows
  « Full machine backup », « Release up to 127 with Tempester Voice », « Voice versions reported » and « Repeated full
  backups »; the Release rows say whether the lift to 127 is active, and why not.
- **Release maxima follow the machine**: the Panel page, the NRPN commands and the edit history stop Pitch, Filter,
  Aux 1 and Aux 2 Release at 123 on 1.6.6.0, and let the five Releases reach 127 when the machine reports six
  Tempester Voice chips (1.6.7.0 and 1.6.8.0: Voice 1.5.1.0 / 1.5.1.1; 1.6.9.0: any Tempester Voice).
- **Library**: a project's card shows the Quantize it records, fine values included (Tempester 1.6.5.0).

#### Changed

- **OS update page**: the choices are grouped by OS (Main OS, Voice OS), each with its recommended cards and its
  read-only History; a filter shows every OS or one; every group, card and history folds; a card's Build button sits
  in its header, so a folded card still builds; the Voice group says on how many chips the machine reports Tempester
  Voice.
- **Back to DSI 1.5.0.2** says that it changes the Main only: voice chips on Tempester Voice keep it until the
  official Voice file is sent.

#### Fixed

- The Machine identity card showed « Release capped at 123 » and « Release up to 127 with Tempester Voice » both as
  available.
- At the window's minimum width (900 px) the top bar ran past the window and every page scrolled sideways.
- The Machine, Backup and OS update pages use 17 px text (15 px before).
- A long OS card title ran over the card's summary; card headers now wrap.

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
- **Ubuntu 26.04 packages**: the Linux app and plugins packages are built on Ubuntu 24.04 and on Ubuntu 26.04, and
  their file names say which: take `*_ubuntu24.04.deb` on Ubuntu 24.04 (and Linux Mint 22), `*_ubuntu26.04.deb` on
  Ubuntu 26.04 ([Installing a release package](docs/building.md#installing-a-release-package)). Each pair depends on
  the library packages of its release with minimum versions, so `apt` refuses a pair on a system older than the one
  it was built on. CI checks the app and the plugin on both.

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

Each version is built from the official Main OS 1.5.0.2 by the user; Panel and SAM are unchanged, and the Voice chips
keep the official Voice 1.5 unless Tempester Voice (below) is installed.

### [1.7.0.0] - 2026-11-21, installed

- n°74: SysEx request `46` returns the Sound of any beat and pad (`F0 01 28 46 bb pp F7` → `F0 01 28 47 bb pp <152
  bytes> F7`, the same Sound bytes as a Sound dump), without changing the selection or the Sound read (issue #6).
- n°75: SysEx message `47` stores a Sound into any beat and pad of the project in memory, answered by a status message
  (`F0 01 28 4F 47 00 F7`, or `01` when the beat or pad is out of range). Save the beat or the project on the machine
  to keep it, and keep the previous Sound yourself (read it with `46` first): on the project's Tempest one try of
  Revert Snd after a store did not bring it back (not investigated).
- Details and byte layouts: [MIDI protocol](docs/midi-protocol.md#one-sound-by-beat-and-pad).
- Proved in simulation. Installed on the project's Tempest from the app on 21 November 2026 and tried there over USB:
  reads, out-of-range answers, stores read back and restored, also with the sequencer playing (the playback was not
  disturbed, by the author's ear); a Sound stored into the pad on screen showed and played as the new Sound; listened
  to on the project's Tempest. Not tried on the machine: the LEDs after a store into the pad on screen, over DIN.

### [1.6.9.1] - 2026-11-21, installed

- n°69: on the pad settings page (Shift + the same pad twice) the four knobs under the screen now change the value
  above them: the left knob the MIDI note, the second the MIDI channel, the third the play probability, the right one
  the Level. From 1.6.5.0 to 1.6.9.0 they were mirrored: the left knob changed the Level, the right one the MIDI note
  (issue #3). Nothing else changes.
- Proved in simulation; installed on the project's Tempest from the app on 21 November 2026 and tried by hand there.

### [1.6.9.0] - 2026-11-15, installed

- n°65: every Tempester Voice counts as Tempester Voice (a version word `0016`-`0079`; 1.6.7.0 and 1.6.8.0 took
  exactly `0016`): Tempester Voice 1.5.1.2 keeps the Release knobs up to 127 and its place in a second full backup.
  Nothing else changes. **Install it before Tempester Voice 1.5.1.2.**
- Test version 1.6.9.0-a holds n°65 alone.
- Installed with the author's go on 15 November 2026: it answers 1.6.9.0 to the exact-version request; two full backups
  in a row identical; then Tempester Voice 1.5.1.2, reported as six `0017`, two full backups in a row identical again.

### [1.6.8.0] - 2026-11-14, installed

- n°62 (1.6.8.0 form): a second full backup, or Debug-menu Voice dump, in the same power-on no longer reads a voice
  chip past the end of its flash, which stopped the voices until power-off: a Tempester Voice chip is brought back to
  its beginning, a chip on the official Voice OS is read once per power-on (a second request gets an empty message for
  it: power the machine off and on).
- Test version 1.6.8.0-a holds it alone.
- Installed on 14 November 2026 with Tempester Voice 1.5.1.1: two full backups in a row, identical, the voices still
  playing.

### [1.6.7.0] - 2026-11-13, installed

- n°63 (1.6.7.0 form): the five Release parameters go up to 127 again when the six voice chips answered at power-on as
  Tempester Voice, whose releases always end (up to 60 s); otherwise they stay at 123.
- n°64: the exact-version request is followed by `F0 01 28 45` with the Voice OS version of each of the six chips.
- Test version 1.6.7.0-a holds both.
- Installed on 14 November 2026 with Tempester Voice 1.5.1.0: six chips reported, the Release knobs reach 127 (heard).
  Its full backup stops the voices on a second request in the same power-on: one per power-on, fixed in 1.6.8.0.

### [1.6.6.0] - 2026-11-13, installed

- n°62: full backup over MIDI, request `F0 01 28 44 F7`: the six voice chips with their calibration, the Main boot
  flash and its configuration words, the System settings with the pad channels, as eight messages on the SysEx cable;
  every Debug-menu Voice dump starts at the beginning of the chip. One per power-on (see 1.6.8.0).
- n°63: Pitch, Filter, Aux 1 and Aux 2 Release stop at 123 like Amp Release, on the knob, over NRPN, on the screen and
  when a sound is loaded: at 124-127 the official voices never end the release.
- Test versions 1.6.6.0-a (n°62) and -b (n°62 + n°63).
- Installed on 13 November 2026: the first full backup of the machine.

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
  Main OS read back equals the built image on the written range. n°52 listened to then; the new functions were tried by
  hand on 15 November 2026.

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
  is answered and the Main OS dump equals the built image; n°38, n°40, n°41 and n°43-n°46 tried by hand the same day,
  as simulated.

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

## Tempester Voice

Each version is built from the official Voice OS 1.5 by the user and sent to the six voice chips at once. The boot page
and each chip's calibration page are never written. Details: [docs/firmware.md](docs/firmware.md#tempester-voice).

### [1.5.1.2] - 2026-11-15, installed

- n°66: a version word of its own, `0017` (« 1.7 » on the System screen), so the machine and the app can tell it from
  1.5.1.0 / 1.5.1.1 (`0016`). Nothing else changes.
- **Needs Main Tempester 1.6.9.0 first**: on 1.6.7.0 or 1.6.8.0 a 1.5.1.2 chip keeps the Release knobs at 123 and is
  read once per power-on by the full backup.
- Installed on the six chips on 15 November 2026 after Main 1.6.9.0: six `0017` reported, each chip's program pages
  equal to the built image, its calibration unchanged, two full backups in a row identical.

### [1.5.1.1] - 2026-11-14, installed, superseded

- The dump stream goes back to the beginning at the end of the flash: no read, whatever the Main (a second full
  backup on 1.6.6.0 or 1.6.7.0, the Debug-menu Voice dump twice), can stop the chip. Version word `0016`, like 1.5.1.0.
- Installed on the six chips on 14 November 2026 with Main 1.6.8.0. Superseded by 1.5.1.2 on 15 November 2026: no
  longer built or sent by the app.

### [1.5.1.0] - 2026-11-13, installed, superseded

The first Tempester Voice:

- a cut sound message no longer silences a voice for good: the chip finds its place again within a few milliseconds;
- a stray calibration command can no longer mute a voice: the chip calibrates only when the Main asks for it;
- a full message queue no longer looks empty and throws away a whole sound;
- a chip that hits an internal error restarts with its voice number instead of staying stuck;
- a half-written calibration page is not used (the default tuning instead, as when no page was saved);
- a note after a cut-off Note FX note no longer plays with the previous sound's settings;
- the dump stream starts again at the Main's start, so a full backup stays in step;
- Release 101-127 rise smoothly up to 60 s at 127 (0-100 as before; 124-127 no longer hold forever);
- version word `0016` (« 1.6 » on the System screen), so Tempester 1.6.7.0 and later can lift the Release limit.
- Installed on the six chips on 14 November 2026 (with Main 1.6.6.0, then 1.6.7.0), a full backup before and after.
  Superseded by 1.5.1.1 on 15 November 2026.
