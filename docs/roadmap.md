# Roadmap

What will probably come next, and why.

- Tempester is a spare-time project, delivered as is. There are **no dates**, and the order can change.
- An item here is a direction, not a promise to anyone.
- Feature requests are not taken ([Support and contributions](../README.md#support-and-contributions)). The table of
  what owners ask for is a reading of the forum, kept as a guide for the authors' own choices.

Contents:

- [Where things stand](#where-things-stand)
- [Firmware: making room](#firmware-making-room)
- [The plugin's editor window](#the-plugins-editor-window)
- [The features Tempest owners ask for most](#the-features-tempest-owners-ask-for-most)
- [The app](#the-app)

## Where things stand

- **Tempester OS 1.7.0.0** is the latest release: 1.6.9.1 with the Sound of any beat and pad read or stored over
  SysEx (requests `46` and `47`, public issue #6), installed on the project's Tempest from the app on 21 November 2026
  and tried there over USB, also with the sequencer playing. The 1.6.5.0 functions were tried by hand on the machine.
  What each release brings:
  [Everything Tempester OS changes](firmware.md#everything-tempester-os-changes).
- **Tempester Voice 1.5.1.2**, the first Tempester build of the voice chips' own OS, runs on the project's six voice
  chips (15 November 2026). See [firmware](firmware.md#tempester-voice).
- **The Tempester app** is used daily on macOS. On Windows 10 one user reported that the installed app 0.1.4 starts
  and that an OS update over USB worked (issues #1 and #2); nothing else has been tried on Windows. Linux has not been
  tried by a user yet.
- **Tempester Control** (CLAP and VST3) has its 146 parameters and MIDI learn, but no window of its own yet. The DAW
  shows its generic parameter list. From 0.1.6 it has a stereo audio input and output that pass the audio through
  untouched, because Ableton Live refused the 0.1.4 VST3 without them (issue #5); the corrected plugin has not been
  tried in any DAW yet.

## Firmware: making room

Every new feature in the Main OS needs code space, and the Tempest lacks space. Its makers said in 2017 that the
instrument had reached the limits of its memory and processing power: each addition meant rewriting something else.

Tempester has so far lived on the flash the official OS leaves erased.

| | Official 1.5.0.2 | Tempester 1.6.3.0 | Tempester 1.6.4.0 |
|---|---:|---:|---:|
| Erased flash (before the protected flasher page) | 22,420 bytes | 18,460 bytes | 28,336 bytes |
| Largest single erased block | 14,776 bytes | 2,536 bytes | 2,536 bytes |

The total is comfortable again, but it is spread in pieces: a large feature no longer fits in one block.

**Reclaiming dead code** (done in 1.6.4.0, patch
[n°37](firmware.md#n37-room-for-new-functions-dead-code-erased)):

- About 9.9 KB of the official code is proven unreachable, about 2 % of it: an unused text formatter, unused
  floating-point helpers, file-system calls nothing makes, a USB helper, a flash-memory erase routine never called,
  and a few MIDI send helpers.
- It sits in about 60 zones. Most of the bytes are in 35 zones of 64 bytes or more.
- The reachability check was run again on the exact 1.6.3.0 image, since a patch can make dead code live again.
- The 38 proven zones of 64 bytes or more, 9,876 bytes, are erased by a recipe, like any other change. The image
  reaches exactly the same code as 1.6.3.0.
- A zone counted as « probably dead » is not touched until it is proved.
- The erased flash grows to 28,336 bytes. The new room is in islands of 64 to 2,228 bytes inside the code.

**RAM** (128 KB) is tight too. Tempester reuses bytes the official OS reserves and never uses, each one checked
disjoint from every other patch.

This is about **program space**. Room for more sounds (about 640 today) is a different matter: see
[below](#the-features-tempest-owners-ask-for-most).

## The plugin's editor window

Tempester Control will get the same face as the app:

- **The app's Panel page inside the plugin window**, with its two views: *Full* (every sound parameter of the selected
  pad) and *Machine* (the Tempest's front panel, knob for knob).
- **A slim top bar**: the MIDI port and its state, MIDI learn, the view switch and the zoom, as on the app's Panel bar.
- **The host's knobs follow**: a learned controller, or a knob turned on the Tempest, moves the DAW parameter too. So
  automation records what you do on the machine.
- **Then**: Audio Unit on macOS (through a CLAP wrapper), and 14-bit controllers (CC pairs) for the parameters wider
  than 7 bits.

Before the window, the plugin needs testing in real DAWs. So far only a test host has driven it, and DAWs differ in
how they route a plugin's MIDI output.

## The features Tempest owners ask for most

From a reading of the Tempest section of the Sequential forum in November 2026.

- The forum stopped answering during the reading, so part of the threads could not be read. The counts are
  **minimums** (distinct people per request).
- *Main* means the Main OS that Tempester patches. The Voice chips run their own code, which only the optional
  Tempester Voice changes. The sample chip's code is not changed.
- A change to the sound, beat or project format must keep every existing file and every other Tempest working.

| Request | People | Status | Feasibility |
|---|---:|---|---|
| **Polymeter**: a track length per sound | 14+ | not yet | hard, but possible in the Main |
| **More memory** for sounds and projects | 11+ | not planned | hardware, or a risky new file system |
| **MIDI control** of the FX, fuller MIDI | 10+ | **largely done** | Tempester OS |
| **New samples**, sample import | 9+ | not planned | sample bank format unknown |
| **Multichannel MIDI**: one channel per pad | 8+ | **done** | Tempester OS 1.6.5.0 |
| **Digital effects** (reverb, delay) | 8+ | not possible | the audio path is analogue |
| **Copy a sound's sequence to another beat** | 6+ | **done** | Tempester OS 1.6.5.0 |
| **Real-time automation** of knobs and sliders | 6+ | later | beat format change |
| **Legato and glide modes** | 6+ | not planned | both chips, and the sound is full |
| **Load Sound knob** that stays where it was | 5+ | **done** | Tempester OS 1.6.3.0 |
| **Song mode** | 5+ | later | Play List format, processor load |
| **More modulation routes** | 5+ | not planned | Voice chips, and the sound is full |
| **Swing on the beat roll**, swing per sound | 5+ | **partly done** | roll: 1.6.5.0; per sound: format change |
| **Polyphonic recording** from the pads, MPE | 4+ | later | voice allocation, 8 events per step |
| **Dotted values**, 1/64 quantize | 4+ | **partly done** | Quantize: 1.6.5.0; LFO: Voice chips |
| **Note FX saved with the sound** | 4+ | not planned | the sound format is full |
| **File management**: bulk delete, audition and delete | 4+ | candidate | likely in the Main |
| **Random steps, probability**, Euclidean rhythms | 3+ | **partly done** | probability: 1.6.3.0; Euclidean: the app |
| **Alphabetical order** in live sound loading | 1+ | **done** | Tempester OS 1.6.3.0 |
| A pedal that plays a pad; a Revert confirmation | 1+ each | **done** | Tempester OS 1.6.3.0 |

Notes on the table:

- **Polymeter** is the largest candidate: a length per pad stored in the beat (format change), and a new playback rule
  in the sequencer.
- **More memory**: adding memory is hardware. Re-splitting the internal flash between sounds and beats means a new
  file system, which is risky for existing data.
- **MIDI control**: every sound parameter by NRPN, FX ribbons over MIDI, beat-wide controls both ways, the whole front
  panel, and sound, beat and project requests.
- **New samples** live in the sample chip, whose bank format is unknown. Replacing them would change every existing
  sound.
- **Multichannel MIDI**: a channel per pad, set over MIDI (CC 15-17) or on the pad settings page.
- **Copy a sequence**: Edit > Copy Sound obeys the Seq / NoteFX / Mixer switches of the Copy screen.
- **Real-time automation** needs a new or denser kind of event in the beat.
- **Legato and glide** need both chips: the Main decides which voice a note takes and whether it restarts; the Voice
  OS runs the envelopes (Tempester Voice makes that half possible). The 128-byte sound has no room left for the mode.
- **Song mode** means mutes in the Play List and changing project while playing. Loading a project during playback is
  heavy for the processor.
- **More modulation routes** are applied in the Voice chips, and the 128-byte sound is full.
- **Swing on the roll**: option « +Roll » of System > Swing Source.
- **Dotted values**: dotted, 1/64, quintuplet and septuplet Quantize values for recording. Dotted LFO rates are
  computed in the Voice chips.
- **Alphabetical order** had been removed in 1.5.0.2 for lack of room.

**What comes first**: the *candidates* touch no file format, live in the Main OS, and were asked for by several owners.
Which one, and in what order, will be decided once dead code is reclaimed (each needs room), and by the authors alone.

## The app

- **Windows and Linux**: more real use (on Windows only the app's start and an OS update over USB have been reported
  working), then signed packages if possible.
- **Library**: the sound type and the oscillator and sample names shown in plain words.
- **Panel**: learn on the Machine view too, and a channel filter for MIDI learn.
- **More languages**: the app is in English; other languages may come later.
