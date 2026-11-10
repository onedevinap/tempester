# Tempester MIDI protocol

This is the interface specification of the messages that **Tempester firmware versions add** to the DSI Tempest, plus
the few existing messages they rely on. For the messages of the official OS, see the DSI manual and its addendum
(<https://sequential.com/support/download/tempest-documentation/>), which this page does not copy.

All values are hexadecimal unless written in words. `n` = the *Remote Pad IN Channel* − 1 (the channel the machine
listens on), `m` = the *Remote Pad OUT* channel − 1 (the channel it reports on). Messages to the machine are accepted on
DIN and USB unless stated; replies and reports leave on the cable or channel stated.

- [Versions](#versions)
- [SysEx requests](#sysex-requests)
- [Identity](#identity)
- [Sound parameters (NRPN)](#sound-parameters-nrpn)
- [Remote panel](#remote-panel)
- [Beat-wide positions](#beat-wide-positions)
- [FX ribbons](#fx-ribbons)
- [Mixer, beat and main values](#mixer-beat-and-main-values)
- [Play probability](#play-probability)
- [7-bit packing](#7-bit-packing)
- [Sound parameter numbers](#sound-parameter-numbers)

## Versions

| Feature | From Tempester | Patch |
|---|---|---|
| Requests `40` (sound), `41` (beat); identity on the chosen cable | 1.5.2.0 | n°1 |
| NRPN received and sent | 1.5.3.0 | n°9a, n°9b |
| Remote panel: keys, LEDs, encoders, absolute knobs | 1.6.0.0 | n°21, n°22, n°23 |
| Request `42` (whole project); 4 Pole / AD Mode LEDs follow NRPN | 1.6.1.0 | n°24, n°25 |
| Request `42` draws nothing on the screen; standard last packed group; project receive fixed; beat-wide CC 52-59 and reports; SysEx timeout | 1.6.2.0 | n°26 to n°30 |
| FX ribbons over MIDI (CC 108-113); play probability per pad (CC 14) | 1.6.3.0 | n°31, n°36 |
| Mixer, beat and main values reported (NRPN 230-239) | test build 1.6.4.0-b | n°38 |
| Request `43` (exact Tempester version); Pad Function mode reported (CC 106) | not in a release yet (test build 1.6.4.0-c) | n°39, n°40 |

## SysEx requests

The official OS answers no request. Tempester adds four. Each reply leaves on the cable set in System > *MIDI: SysEx
IN+OUT Cable* (USB or DIN), and is the same message the machine's menu would send.

| Request | Reply | Content |
|---|---|---|
| `F0 01 28 40 F7` | `F0 01 28 60 <data> F7` | the sound of the pad selected on the machine: 128 bytes, 7-bit packed |
| `F0 01 28 41 F7` | `F0 01 28 5F <data> F7` | the current beat (events, settings and its 32 sounds) |
| `F0 01 28 42 F7` | `F0 01 28 5E <data> F7`, then `F0 01 28 5C <k> <data> F7` for each beat k (0-15) present in the project | the whole project: header and 4 Play Lists, then its beats; byte for byte what the menu sends when it exports the project |
| `F0 01 28 43 F7` | `F0 01 28 43 a1 a0 b1 b0 c1 c0 d1 d0 F7` | the exact Tempester version `a.b.c.d` (n°39), each digit `N` as two bytes `N >> 7`, `N & 7F` (0-16383): the number System > *Main OS Version* shows |

Notes:

- A project may have fewer than 16 beats; the beats present come in increasing order.
- A request is served from the machine's main loop. A 16-beat project takes tens of seconds on DIN.
- The machine receives these same messages (`60`, `5F`, `5E` + `5C`) to load a sound, a beat or a project; that is
  stock behaviour. Receiving a project into Play Lists correctly needs n°28 (1.6.2.0).
- Unknown commands are ignored as before: a machine without n°39 does not answer `43`, so a program that gets no
  answer within a second falls back to what it knows (the identity says only « Tempester »).
- The version is read from the same place the System screen reads it: every release announces its own number. A test
  build keeps the number of the release it is built on.

## Identity

Universal identity request `F0 7E 7F 06 01 F7`. Reply:
`F0 7E <device> 06 02 01 28 01 00 00 15 <v1> <v2> F7`, version word `v = v1 | v2 << 4`.

- Every Tempester version answers `v = 0x7F2` (`v1 = 02`, `v2 = 7F`); the official OS answers its own word. The word says
  « Tempester », not which release: ask with the request `43` ([above](#sysex-requests)) from n°39 on, or read System >
  *Main OS Version*. The identity reply itself is unchanged, so existing identity readers keep working.
- The official OS sends this reply on the DIN port only. From 1.5.2.0 it follows the SysEx cable setting.

## Sound parameters (NRPN)

From 1.5.3.0. Standard NRPN: `Bn 63 <number MSB>`, `Bn 62 <number LSB>`, `Bn 06 <value MSB>`, `Bn 26 <value LSB>`;
number and value are 14-bit (`MSB × 128 + LSB`).

**To the machine**

- Numbers 0-126: the sound parameter with that index ([table below](#sound-parameter-numbers)); 156 and 157: Osc 3 and
  Osc 4 sample (0-463). Other numbers are ignored (127-146 included).
- The value is applied when CC 38 arrives, clamped to the parameter's range, through the same path as the machine's
  knob (voices updated, dependent values recomputed).
- Target: on the Remote Pad IN channel (or any channel when it is *All*), the pad selected on the machine; else, on the
  *MIDI: Synth IN* channel, the *MIDI: Synth Sound*; else ignored.
- `Bn 63 7F Bn 62 7F` (null) deselects; CC 6 alone stores the MSB; CC 96/97 are absorbed. Every other CC behaves as in the
  official OS.

**From the machine**

- When a parameter changes on the panel, the machine sends the same four CCs on Remote Pad OUT (DIN and USB). *Off*
  sends nothing. A change that came from MIDI is not echoed.

## Remote panel

From 1.6.0.0. Every message to the machine is on the Remote Pad IN channel; reports come on Remote Pad OUT. Nothing is
applied while the Debug menu is shown, and the Debug menu cannot be opened remotely.

### Keys (n°21)

| Message | Meaning |
|---|---|
| `Bn 66 kk` (CC 102) | key `kk` pressed |
| `Bn 67 kk` (CC 103) | key `kk` released |

The key codes are listed in [`refs/panel_keys.tsv`](../refs/panel_keys.tsv) (0-48; for example 16 = 16 Beats,
22 = Click, 32 = System, 38 = REC, 39 = STOP, 46 = PLAY, 41 and 47 = the two Shift keys). Combinations work as on the
machine (16 Tunings = 16 Beats + 16 Time Steps, Mixer = Pads + Sounds, Edit = Erase + Copy). **Always send the
release**, also when your program closes: a key left pressed stays pressed. Values above 48 are ignored.

### LEDs (n°22, needs n°9b)

| Message | Meaning |
|---|---|
| `Bm 68 ii` (CC 104) | LED `ii` on |
| `Bm 69 ii` (CC 105) | LED `ii` off |
| `Bn 6B 00` (CC 107) to the machine | send every LED state, without the pad LEDs |
| `Bn 6B 01` | the same, with the 16 pad LEDs |

**Pad notes.** Which note plays which pad is the machine's own table, System > *MIDI Note : Pad A1-B16* (one note per
pad of banks A and B, each note on one pad only), not 36 + pad. On the author's machine A1-A15 are the white keys from
C2 (36, 38, 40, 41, 43, 45, 47, 48, 50, 52, 53, 55, 57, 59, 60), A16 is 61 and B1-B16 are 62-77. A pad tapped on
the machine sends its note on Remote Pad OUT, then the pad LED of the selected pad: the app pairs the two to learn the
table (Machine page, *Pad notes*).

LED indexes are in [`refs/panel_leds.tsv`](../refs/panel_leds.tsv) (0x00-0x2B: keys and indicators; 0x2C-0x3B:
pads 1-16, sent only after a `CC 107` with value bit 0 set; 0x12 is Sound Bank B, lit when the pads are B1-B16). Only on/off is reported, not brightness or flashing.
Changes are sent at most 4 per main-loop pass and only when the DIN output buffer is nearly empty; a LED that toggles
back before it is sent produces nothing. Remote Pad OUT must not be *Off*.

### Pad Function mode (n°40, needs n°9b)

| Message | Meaning |
|---|---|
| `Bm 6A pp` (CC 106) from the machine | the Pad Function mode is `pp`: 0 16 Beats, 1 16 Time Steps, 2 16 Sounds, 3 16 Tunings, 4 16 Mutes, 5 16 Levels |

Sent when the mode changes (a panel key or a remote key), and again after a `CC 107`, after the LEDs. Same rules as the
LEDs: Remote Pad OUT must not be *Off* (the change is sent once it is on), and only when the DIN output buffer is
nearly empty. Nothing sets the mode over MIDI: send the Pad Function keys (16 = 16 Beats, 17 = 16 Time Steps,
24 = 16 Sounds, 25 = 16 Mutes; 16 Tunings and 16 Levels are two keys held). The first report after power-on gives the
start mode (16 Sounds). Without n°40, read the mode from the four mode LEDs (0x10, 0x11, 0x18, 0x19) once they have
stopped changing.

### Encoders and absolute knobs (n°23, needs n°9a)

| NRPN | Control | Value |
|---|---|---|
| 200-213 | encoder `e = number − 200` | relative: `value − 8192`, clamped to −16..+15 steps (0 = nothing) |
| 220-229 | absolute knob `k = number − 220` | 0-255 (the machine uses `value >> 1` for most knobs) |

Bytes: encoder `e` one step up = `Bn 63 01 Bn 62 <48+e> Bn 06 40 Bn 26 01`, one step down = `... Bn 06 3F Bn 26 7F`
(while the number and MSB are unchanged, `Bn 26 xx` alone is enough). Absolute knob `k` = `Bn 63 01 Bn 62 <5C+k> Bn 06
<v >> 7> Bn 26 <v & 7F>`.

| Encoder | Control (with Shift) | | Absolute | Control |
|---|---|---|---|---|
| 0 | LFO Rate (Shape) | | 0 | Feedback |
| 1 | Pitch (Fine) | | 1 | Highpass Frequency |
| 2 | 1/2 Mix (Level, Pre/Post Filter) | | 2 | Compress |
| 3 | Shape (Sample) | | 3 | Audio Mod |
| 4 | LFO Amount (Destination) | | 4 | Resonance |
| 5 | Env Amount (Velocity Amt) | | 5 | Lowpass Frequency |
| 6 | Env Decay (Release) | | 6 | Pan (Delay Repeats) |
| 7 | Env Attack (Sustain Level) | | 7 | Level |
| 8 | BPM (Swing) | | 8 | Volume |
| 9 | Delay Send (Delay Time) | | 9 | Distort |
| 10-13 | soft knobs 4, 3, 2, 1 (under the screen) | | | |

The machine decides what a control does, exactly as for its own panel (screen, Shift, Pad Function mode). Full table
with evidence: [`refs/panel_controls.tsv`](../refs/panel_controls.tsv).

## Beat-wide positions

In 16 Beats and 16 Mutes, eight knobs act on the whole beat. The official OS already **receives** CC 20-27 (position =
2 × value). From 1.6.2.0 (n°29) the machine also **reports** them, and a second CC carries the low bit:

| CC | LSB CC | Position |
|---|---|---|
| 20 | 52 | All Osc Freq (Pitch) |
| 21 | 53 | Feedback |
| 22 | 54 | LP Freq |
| 23 | 55 | Resonance |
| 24 | 56 | Audio Mod |
| 25 | 57 | HP Freq |
| 26 | 58 | Attack |
| 27 | 59 | Decay |

- Position `p` is 0-254, neutral 127.
- From the machine, on Remote Pad OUT: `Bm <14+i> <p >> 1>`, then `Bm <34+i> 40` when `p` is odd. Sent after any panel
  action that changes position `i` (also remote ones), after a beat change, and on `Bn 6B xx`. Coalesced per main-loop
  pass, only when the DIN output buffer is nearly empty.
- To the machine, on Remote Pad IN: the same bytes. CC 20-27 set `p = 2 × value` (stock), CC 52-59 then set the low bit.
  Never echoed back.
- CC 19 with a value other than 0 reverts the beat-wide positions (stock behaviour).

## FX ribbons

From 1.6.3.0 (n°31, needs n°9b). `r` = 0 for ribbon 1, 1 for ribbon 2.

| Message | Direction | Meaning |
|---|---|---|
| `Bn <6C+r> p` (CC 108 / 109) | to the machine, Remote Pad IN | ribbon touched at position `p` (0-127) |
| `Bn <6E+r> xx` (CC 110 / 111) | to the machine | ribbon released (value ignored) |
| `Bn <70+r> v` (CC 112 / 113) | to the machine | ribbon pressure `v` (0-127) |
| `Bm <6C+r> p` | from the machine, Remote Pad OUT | the ribbon was moved by hand to `p` |
| `Bm <6E+r> 00` | from the machine | the ribbon was released by hand |

- A remote touch acts exactly as a finger: Note FX, Beat FX in 16 Beats, Latch, and the FX3 / FX4 bank chosen with
  Shift + Latch. The last message wins when a finger and MIDI act at once.
- Reports are coalesced (the latest state of a ribbon), sent only when the DIN output buffer is nearly empty, and never
  echo a remote move. Pressure is not reported, and `CC 107` does not resend the ribbons. Remote Pad OUT must not be
  *Off*.

The footswitch pads of the same release (n°32) are a setting on the machine (System, *Pedal 1/2 Assignment*: « Pad 1 »
... « Pad 16 »); they add no MIDI message: the pad plays as when it is pressed by hand.

## Mixer, beat and main values

From the test build 1.6.4.0-b (n°38, needs n°9b). The machine **reports** these values on Remote Pad OUT as NRPN
`230 + item`: `Bm 63 01`, `Bm 62 <66 + item>`, `Bm 06 <v >> 7>`, `Bm 26 <v & 7F>` (full status every time). Example:
Level 72 on channel 3 = `B2 63 01 B2 62 66 B2 06 00 B2 26 48`.

| NRPN | Item | Value | Machine control |
|---|---|---|---|
| 230 | Level of the selected pad | 0-127 | knob Level (absolute 7): knob byte >> 1 |
| 231 | Pan of the selected pad | 0-127 | knob Pan (absolute 6): knob byte >> 1 |
| 232 | Delay Send of the selected pad | 0-127 | encoder Delay Send (9) |
| 233 | Delay Time of the beat | 0-11 | Shift + encoder Delay Send |
| 234 | Delay Repeats of the beat | 0-126 | Shift + knob Pan: (knob byte >> 1) × 126 / 127 |
| 235 | Tempo × 10 (rounded; 0 = unknown) | 300-2500 (30.0-250.0 BPM) | encoder BPM (8), 0.5 BPM per step |
| 236 | Swing | 0-12 | Shift + encoder BPM |
| 237 | Volume | 0-127 | knob Volume (absolute 8): knob byte >> 1 |
| 238 | Distort | 0-255 | knob Distort (absolute 9): the knob byte, 0 below 4 |
| 239 | Compress | 0-255 | knob Compress (absolute 2): the knob byte |

- Sent on the main-loop pass after the value changes, whatever changed it (a knob, the remote panel, a beat or pad
  change, a beat received), the latest value only, at most 4 reports per pass and only when the DIN output buffer is
  nearly empty; DIN then USB. *Off*: nothing, the changes go out when Remote Pad OUT is set again. Nothing at power on.
- Not sent: a value set by `CC 12` / `CC 13` (the sender knows it), and the tempo while the machine follows MIDI clock.
- `Bn 6B xx` (CC 107) also sends the 10 values.
- The machine ignores NRPN 230-239: sending a report back changes nothing.
- `CC 12` / `CC 13` (official OS) set Distort / Compress to the value 0-127, the same as the knob at that byte: the
  upper half of the knob's range needs the remote panel (NRPN 229 / 222).
- Mute, Solo and Delay On of the selected pad are the LEDs 0x0E, 0x07, 0x0F ([remote panel](#leds-n22-needs-n9b)).

## Play probability

From 1.6.3.0 (n°36). `Bn 0E p` (CC 14) on Remote Pad IN sets the play probability of the pad selected on the machine,
in the beat being edited: `p` = 0-100 %, 101-127 count as 100 %. At 100 % (the default) the pad plays as before; below,
each note of that pad is skipped with probability 100 - `p`. The value is saved with the beat (`5F`, and `5C n` in a
project) in a field the official OS writes as 0 and never reads. The machine does not report it; read it back from a
beat dump.

## 7-bit packing

Data in DSI SysEx messages is packed in groups: one byte holding the high bits of up to 7 data bytes (bit j = high bit
of byte j), then those 7 bytes with their high bit cleared. The official OS ends a message whose data length is not a
multiple of 7 with a non-standard last group (shifted high bits, stale padding); its own receivers expect the standard
layout. From 1.6.2.0 (n°27) the machine sends the standard layout. A tolerant reader should accept both; a writer should
always send the standard layout (high bit j for byte j, zero padding). The app's `tempest-proto` crate implements both.

## Sound parameter numbers

NRPN number = parameter index. Names and ranges come from the parameter table inside the firmware (`profile/dsi-tempest/sound_params.json`); a few entries carry the same name there (for example 116-120, 121-122) and are not yet told apart individually. The DSI manual describes what each parameter does.

| NRPN | Group | Parameter | Range |
|---|---|---|---|
| 0 | Osc 1 | Frequency | 0-120 |
| 1 | Osc 1 | Fine Freq | 0-100 |
| 2 | Osc 1 | Shape | 0-103 |
| 3 | Osc 1 | Glide | 0-127 |
| 4 | Osc 1 | Key Follow | 0-1 |
| 5 | Osc 1 | Wave Reset | 0-1 |
| 6 | Osc 2 | Frequency | 0-120 |
| 7 | Osc 2 | Fine Freq | 0-100 |
| 8 | Osc 2 | Shape | 0-103 |
| 9 | Osc 2 | Glide | 0-127 |
| 10 | Osc 2 | Key Follow | 0-1 |
| 11 | Osc 2 | Wave Reset | 0-1 |
| 12 | Osc 3 | Frequency | 0-120 |
| 13 | Osc 3 | Fine Freq | 0-100 |
| 14 | Osc 3 | Sample/Noise | 0-127 |
| 15 | Osc 3 | Sample/Noise | 0-4 |
| 16 | Osc 3 | Glide | 0-127 |
| 17 | Osc 3 | Key Follow | 0-1 |
| 18 | Osc 4 | Frequency | 0-120 |
| 19 | Osc 4 | Fine Freq | 0-100 |
| 20 | Osc 4 | Sample/Noise | 0-127 |
| 21 | Osc 4 | Sample/Noise | 0-4 |
| 22 | Osc 4 | Glide | 0-127 |
| 23 | Osc 4 | Key Follow | 0-1 |
| 24 | Oscillators | Sync 2->1 | 0-1 |
| 25 | Oscillators | Glide Mode | 0-3 |
| 26 | Oscillators | Osc Slop | 0-5 |
| 27 | Oscillators | Pitchbend Range | 0-12 |
| 28 | Oscillators | Osc 1/2 Mix | 0-127 |
| 29 | Oscillators | Sub Osc | 0-127 |
| 30 | Mixer | Level | 0-127 |
| 31 | Mixer | Level | 0-127 |
| 32 | Mixer | Pre/Post Filter | 0-127 |
| 33 | Mixer | Feedback | 0-127 |
| 34 | Lowpass | LP Freq | 0-164 |
| 35 | Lowpass | Resonance | 0-127 |
| 36 | Lowpass | LP Key>Freq | 0-127 |
| 37 | Lowpass | Amount | 0-254 |
| 38 | Lowpass | Vel Amt | 0-127 |
| 39 | Lowpass | Audio Mod | 0-127 |
| 40 | Lowpass | 4 Pole | 0-1 |
| 41 | Highpass | HP Freq | 0-127 |
| 42 | Highpass | HP Key>Freq | 0-127 |
| 43 | VCA | VCA Level | 0-127 |
| 44 | VCA | Amount | 0-127 |
| 45 | VCA | Vel Amt | 0-127 |
| 46 | VCA | Volume | 0-127 |
| 47 | LFO 1 | Rate | 0-178 |
| 48 | LFO 1 | Shape | 0-4 |
| 49 | LFO 1 | Amount | 0-127 |
| 50 | LFO 1 | Destination | 0-57 |
| 51 | LFO 1 | Sync | 0-1 |
| 52 | LFO 2 | Rate | 0-178 |
| 53 | LFO 2 | Shape | 0-4 |
| 54 | LFO 2 | Amount | 0-127 |
| 55 | LFO 2 | Destination | 0-57 |
| 56 | LFO 2 | Sync | 0-1 |
| 57 | Envelopes | Env Sustain | 0-1 |
| 58 | Env Pitch | Destination | 0-57 |
| 59 | Env Pitch | Amount | 0-254 |
| 60 | Env Pitch | Vel Amt | 0-127 |
| 61 | Env Pitch | Delay | 0-127 |
| 62 | Env Pitch | Attack | 0-127 |
| 63 | Env Pitch | Decay | 0-127 |
| 64 | Env Pitch | Sustain | 0-127 |
| 65 | Env Pitch | Release | 0-127 |
| 66 | Env Filter | Delay | 0-127 |
| 67 | Env Filter | Attack | 0-127 |
| 68 | Env Filter | Decay | 0-127 |
| 69 | Env Filter | Sustain | 0-127 |
| 70 | Env Filter | Release | 0-127 |
| 71 | Env Amp | Delay | 0-127 |
| 72 | Env Amp | Attack | 0-127 |
| 73 | Env Amp | Decay | 0-127 |
| 74 | Env Amp | Sustain | 0-127 |
| 75 | Env Amp | Release | 0-123 |
| 76 | Env Aux 1 | Destination | 0-57 |
| 77 | Env Aux 1 | Amount | 0-254 |
| 78 | Env Aux 1 | Vel Amt | 0-127 |
| 79 | Env Aux 1 | Delay | 0-127 |
| 80 | Env Aux 1 | Attack | 0-127 |
| 81 | Env Aux 1 | Decay | 0-127 |
| 82 | Env Aux 1 | Sustain | 0-127 |
| 83 | Env Aux 1 | Release | 0-127 |
| 84 | Env Aux 2 | Destination | 0-57 |
| 85 | Env Aux 2 | Amount | 0-254 |
| 86 | Env Aux 2 | Vel Amt | 0-127 |
| 87 | Env Aux 2 | Delay | 0-127 |
| 88 | Env Aux 2 | Attack | 0-127 |
| 89 | Env Aux 2 | Decay | 0-127 |
| 90 | Env Aux 2 | Sustain | 0-127 |
| 91 | Env Aux 2 | Release | 0-127 |
| 92 | Mod matrix | Mod 1 source | 0-22 |
| 93 | Mod matrix | Mod 1 amount | 0-254 |
| 94 | Mod matrix | Mod 1 destination | 0-57 |
| 95 | Mod matrix | Mod 2 source | 0-22 |
| 96 | Mod matrix | Mod 2 amount | 0-254 |
| 97 | Mod matrix | Mod 2 destination | 0-57 |
| 98 | Mod matrix | Mod 3 source | 0-22 |
| 99 | Mod matrix | Mod 3 amount | 0-254 |
| 100 | Mod matrix | Mod 3 destination | 0-57 |
| 101 | Mod matrix | Mod 4 source | 0-22 |
| 102 | Mod matrix | Mod 4 amount | 0-254 |
| 103 | Mod matrix | Mod 4 destination | 0-57 |
| 104 | Mod matrix | Mod 5 source | 0-22 |
| 105 | Mod matrix | Mod 5 amount | 0-254 |
| 106 | Mod matrix | Mod 5 destination | 0-57 |
| 107 | Mod matrix | Mod 6 source | 0-22 |
| 108 | Mod matrix | Mod 6 amount | 0-254 |
| 109 | Mod matrix | Mod 6 destination | 0-57 |
| 110 | Mod matrix | Mod 7 source | 0-22 |
| 111 | Mod matrix | Mod 7 amount | 0-254 |
| 112 | Mod matrix | Mod 7 destination | 0-57 |
| 113 | Mod matrix | Mod 8 source | 0-22 |
| 114 | Mod matrix | Mod 8 amount | 0-254 |
| 115 | Mod matrix | Mod 8 destination | 0-57 |
| 116 | Peak Hold | Peak Hold | 0-127 |
| 117 | Peak Hold | Peak Hold | 0-127 |
| 118 | Peak Hold | Peak Hold | 0-127 |
| 119 | Peak Hold | Peak Hold | 0-127 |
| 120 | Peak Hold | Peak Hold | 0-127 |
| 121 | Misc | Reverse | 0-1 |
| 122 | Misc | Reverse | 0-1 |
| 123 | Misc | Key Assign | 0-1 |
| 124 | Misc |  | 0-31 |
| 125 | Misc | Restart | 0-3 |
| 126 | Misc | Restart | 0-3 |
| 156 | Osc 3 | Sample | 0-463 |
| 157 | Osc 4 | Sample | 0-463 |
