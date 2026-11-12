# Roadmap

What will probably come next, and why. Tempester is a spare-time project delivered as is: there are **no dates**, the
order can change, an item listed here is a direction, not a promise, and nothing here is a commitment to anyone.
Feature requests are not taken (see [Support and contributions](../README.md#support-and-contributions)); the table
of what owners ask for below is a reading of the forum, kept as a guide for the authors' own choices.

- [Where things stand](#where-things-stand)
- [Firmware: making room](#firmware-making-room)
- [The plugin's editor window](#the-plugins-editor-window)
- [The features Tempest owners ask for most](#the-features-tempest-owners-ask-for-most)
- [The app](#the-app)

## Where things stand

- **Tempester OS 1.6.5.0** is the latest release (installed on the project's Tempest on 12 November 2026): a MIDI
  channel and a settings page for each pad, notes as long as a whole 8-bar beat, finer Quantize values, Copy Sound
  with its sequence, a beat roll that can follow the swing, six bug fixes, on top of 1.6.4.0 (room for new functions,
  the machine reporting its mixer, tempo, mode and exact version to an editor) and 1.6.3.0 (bug fixes, requests for
  sounds, beats and projects, NRPN in both directions, the front panel over MIDI, FX ribbons over MIDI, play
  probability per pad). The hand tests of the new functions are still to do.
  See [firmware](firmware.md).
- **The Tempester app** is used daily on macOS; the Windows build is checked to compile, and neither Windows nor
  Linux has been tried by a user yet.
- **Tempester Control** (CLAP and VST3) has its 146 parameters and MIDI learn, but no window of its own yet: the
  DAW shows its generic parameter list.

## Firmware: making room

Every new feature in the Main OS needs code space, and space is what the Tempest lacks. Its makers said in 2017 that
the instrument had reached the limits of its memory and processing power, and that each addition meant rewriting
something else to free room. Tempester has so far lived on the flash the official OS leaves erased.

| | Official 1.5.0.2 | Tempester 1.6.3.0 | Tempester 1.6.4.0 |
|---|---:|---:|---:|
| Erased flash (before the protected flasher page) | 22,420 bytes | 18,460 bytes | 28,336 bytes |
| Largest single erased block | 14,776 bytes | 2,536 bytes | 2,536 bytes |

The total is comfortable again, but it is spread in pieces: a large feature no longer fits in one block. The first
step, done in 1.6.4.0, was to **reclaim dead code**: routines of the official OS that nothing can ever call.

- **About 9.9 KB of the official code is proven unreachable** (about 2 % of it): an unused text formatter, unused
  floating-point helpers, file-system calls nothing makes, a USB helper, a flash-memory erase routine never called,
  and a few MIDI send helpers. It sits in about 60 zones; most of the bytes are in 35 zones of 64 bytes or more.
- **Done in 1.6.4.0** (patch n°37, [firmware](firmware.md#n37-room-for-new-functions-dead-code-erased)):
  the reachability check was run again on the exact 1.6.3.0 image (a patch can make dead code live again), and the
  38 proven zones of 64 bytes or more, 9,876 bytes, are erased by a recipe, like any other change. The image reaches
  exactly the same code as 1.6.3.0. A zone counted as « probably dead » is not touched until it is proved. The
  erased flash grows to 28,336 bytes, the new room being islands of 64 to 2,228 bytes inside the code.
- **RAM** (128 KB) is tight too. Tempester reuses bytes the official OS reserves and never uses, each one checked
  disjoint from every other patch.

This is about **program space**. The other memory owners ask for, room for more sounds (about 640 today), is a
different matter: see [below](#the-features-tempest-owners-ask-for-most).

## The plugin's editor window

Tempester Control will get the same face as the app. The plan:

- **The app's Panel page inside the plugin window**, with its two views: *Full* (every sound parameter of the
  selected pad, grouped as on the app) and *Machine* (the Tempest's front panel, knob for knob). One look, one set
  of controls, whether you use the app or the plugin.
- **A slim top bar**: the MIDI port and its state, MIDI learn, the view switch and the zoom, the same buttons as the
  app's Panel bar.
- **The host's knobs follow**: a learned controller or a knob turned on the Tempest moves the DAW parameter too, so
  automation records what you do on the machine.
- **Then**: Audio Unit on macOS (through a CLAP wrapper), and 14-bit controllers (CC pairs) for the parameters wider
  than 7 bits.

Before the window, the plugin needs testing in real DAWs: so far only a test host has driven it, and DAWs differ in
how they route a plugin's MIDI output.

## The features Tempest owners ask for most

From a reading of the Tempest section of the Sequential forum in November 2026. The forum stopped answering during the
reading, so part of the threads could not be read: the counts are **minimums** (distinct people per request). The
rows follow the number of people asking.

**Feasibility** says what the change touches. *Main* means the Main OS that Tempester patches; the Voice chips and the
sample chip run their own code, which Tempester does not change; a change to the sound, beat or project format must
keep every existing file and every other Tempest working.

| Request | People | Status | Feasibility |
|---|---:|---|---|
| **Polymeter**: a track length per sound, polyrhythms | 14+ | not yet | Hard but possible in the Main: a length per pad stored in the beat (format change) and a new playback rule in the sequencer. The largest candidate. |
| **More memory** for sounds and projects | 11+ | not planned | Adding memory is hardware. Re-splitting the internal flash between sounds and beats means a new file system: risky for existing data. |
| **MIDI control** of the FX and a fuller MIDI implementation | 10+ | **largely done** | Tempester OS: every sound parameter by NRPN, FX ribbons over MIDI, beat-wide controls in both directions, the whole front panel, sound, beat and project requests. |
| **New samples**, sample import | 9+ | not planned | The samples live in the sample chip, whose bank format is unknown; replacing them would change every existing sound. |
| **Multichannel MIDI**: one channel per pad, sequencing several synths | 8+ | **done** | Tempester OS 1.6.5.0: a channel per pad, set over MIDI (CC 15-17) or on the pad settings page. |
| **Digital effects** (reverb, delay) | 8+ | not possible | The audio path is analogue; there is no effects processor to program. |
| **Copy a sound's sequence to another beat** | 6+ | **done** | Tempester OS 1.6.5.0: Edit > Copy Sound obeys the Seq / NoteFX / Mixer switches of the Copy screen. |
| **Real-time automation** of knobs and sliders | 6+ | later | A new or denser kind of event in the beat: format change. |
| **Legato and glide modes** | 6+ | not planned | The envelopes run in the Voice chips. |
| **Load Sound knob** that stays where it was | 5+ | **done** | Tempester OS 1.6.3.0. |
| **Song mode**: mutes in the Play List, change project while playing | 5+ | later | Play List format; loading a project during playback is heavy for the processor. |
| **More modulation routes** and destinations | 5+ | not planned | Applied in the Voice chips, and the 128-byte sound is full. |
| **Swing on the beat roll**, swing per sound | 5+ | **partly done** | Beat roll: option « +Roll » of System > Swing Source, Tempester OS 1.6.5.0. Per sound: format change. |
| **Polyphonic recording** from the pads, MPE | 4+ | later | Voice allocation and the 8 events per step. |
| **Dotted values**, 1/64 quantize | 4+ | **partly done** | Dotted, 1/64, quintuplet and septuplet Quantize values for recording: Tempester OS 1.6.5.0. Dotted LFO rates: computed in the Voice chips. |
| **Note FX saved with the sound** | 4+ | not planned | The sound format is full. |
| **File management**: bulk delete, audition and delete | 4+ | candidate | Likely in the Main. |
| **Random steps, probability**, Euclidean rhythms | 3+ | **partly done** | Play probability per pad in 1.6.3.0. The app's step sequencer already makes Euclidean patterns. |
| **Alphabetical order** in live sound loading | 1+ | **done** | Tempester OS 1.6.3.0 (it had been removed in 1.5.0.2 for lack of room). |
| A pedal that plays a pad; a confirmation for Revert | 1+ each | **done** | Tempester OS 1.6.3.0. |

**What comes first**: the *candidates* above share three traits: they touch no file format, they live in the Main
OS, and each was asked for by several owners. Which one, and in what order, will be decided once dead code is
reclaimed, since each needs room, and by the authors alone.

## The app

- **Windows and Linux**: first real use, fixes from the first CI and release runs, then signed packages if possible.
- **Library**: the sound type and the oscillator and sample names shown in plain words.
- **Panel**: learn on the Machine view too, a channel filter for MIDI learn.
- **More languages**: the app is in English; other languages may come later.
