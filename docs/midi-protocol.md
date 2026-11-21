# Tempester MIDI protocol

**In short**

- **SysEx requests** `40`-`44`: fetch the current sound, the current beat, the whole project, the exact version (with
  each voice chip's version in a `45` message after it), or a full backup.
- **One Sound by beat and pad** (`46`, answered `47`; `47` to store; status message `4F`, from 1.7.0.0): read or
  store the Sound of any pad of any beat, without changing the selection.
- **NRPN** for every sound parameter, both ways.
- **Remote panel**: keys, LEDs, encoders and knobs as CCs and NRPN.
- **Reports** from the machine: beat-wide positions, FX ribbons, mixer and tempo values, the Pad Function mode.
- **Per pad**: play probability (CC 14) and MIDI channel (CC 15-17).

This is the interface specification of the messages that **Tempester firmware versions add** to the DSI Tempest, plus
the few existing messages they rely on. For the messages of the official OS, see the DSI manual and its addendum
(<https://sequential.com/support/download/tempest-documentation/>), which this page does not copy.

Conventions:

- All values are hexadecimal unless written in words.
- `n` = the *Remote Pad IN Channel* − 1: the channel the machine listens on.
- `m` = the *Remote Pad OUT* channel − 1: the channel it reports on.
- Messages to the machine are accepted on DIN and USB unless stated. Replies and reports leave on the cable or channel
  stated.

Contents:

- [Versions](#versions)
- [SysEx requests](#sysex-requests)
- [One Sound by beat and pad](#one-sound-by-beat-and-pad)
- [Full backup](#full-backup)
- [Identity](#identity)
- [Sound parameters (NRPN)](#sound-parameters-nrpn)
- [Remote panel](#remote-panel)
- [Beat-wide positions](#beat-wide-positions)
- [FX ribbons](#fx-ribbons)
- [Mixer, beat and main values](#mixer-beat-and-main-values)
- [Play probability](#play-probability)
- [Channel per pad](#channel-per-pad)
- [Fine Quantize value](#fine-quantize-value)
- [7-bit packing](#7-bit-packing)
- [Sound parameter numbers](#sound-parameter-numbers)

## Versions

| Feature | From Tempester | Patch |
|---|---|---|
| Requests `40` (sound), `41` (beat); identity on the chosen cable | 1.5.2.0 | n°1 |
| NRPN received and sent | 1.5.3.0 | n°9a, n°9b |
| Remote panel: keys, LEDs, encoders, absolute knobs | 1.6.0.0 | n°21, n°22, n°23 |
| Request `42` (whole project); 4 Pole / AD Mode LEDs follow NRPN | 1.6.1.0 | n°24, n°25 |
| Request `42` silent on screen; standard last packed group; project receive fixed | 1.6.2.0 | n°26, n°27, n°28 |
| Beat-wide CC 52-59 and reports; SysEx timeout | 1.6.2.0 | n°29, n°30 |
| FX ribbons over MIDI (CC 108-113); play probability per pad (CC 14) | 1.6.3.0 | n°31, n°36 |
| Mixer, beat and main values reported (NRPN 230-239) | 1.6.4.0 | n°38 |
| Request `43` (exact Tempester version); Pad Function mode reported (CC 106) | 1.6.4.0 | n°39, n°40 |
| MIDI channel per pad (CC 15-17); fine Quantize value in the project header (word 18) | 1.6.5.0 | n°58, n°55 |
| Request `44` (full backup: six Voice chips, Main boot flash, System settings); every Voice OS dump aligned | 1.6.6.0 | n°62 |
| Pitch, Filter, Aux 1 and Aux 2 Release capped at 123, as Amp Release | 1.6.6.0 | n°63 |
| The five Release parameters up to 127 with six Tempester Voice 1.5.1.0 chips (123 otherwise) | 1.6.7.0 | n°63 |
| Request `43` also answers `45`: the Voice OS version of each chip | 1.6.7.0 | n°64 |
| A second request `44` (or Debug Voice dump) never reads a Voice chip past its flash | 1.6.8.0 | n°62 |
| Every Tempester Voice word (`0016`-`0079`) counts for the Release lift and the re-alignment | 1.6.9.0 | n°65 |
| Request `46` (the Sound of a beat and pad) answered `47`; status message `4F` | 1.7.0.0 | n°74 |
| Message `47` stores a Sound into a beat and pad | 1.7.0.0 | n°75 |

From 1.6.8.0, at a second dump in the same power-on, Tempester Voice chips are re-aligned, and a DSI Voice chip
already read answers an empty message ([Full backup](#full-backup)).

## SysEx requests

The official OS answers no request. Tempester adds five here, and two more for one Sound by beat and pad
([below](#one-sound-by-beat-and-pad)). Each reply leaves on the cable set in System > *MIDI: SysEx IN+OUT Cable* (USB
or DIN). The replies of `40`-`42` are the same messages the machine's menu would send.

| Request | Reply | Content |
|---|---|---|
| `F0 01 28 40 F7` | `F0 01 28 60 <data> F7` | the sound of the pad selected on the machine: 128 bytes, 7-bit packed |
| `F0 01 28 41 F7` | `F0 01 28 5F <data> F7` | the current beat (events, settings and its 32 sounds) |
| `F0 01 28 42 F7` | `F0 01 28 5E <data> F7`, then `F0 01 28 5C <k> <data> F7` per beat | the whole project |
| `F0 01 28 43 F7` | `F0 01 28 43 a1 a0 b1 b0 c1 c0 d1 d0 F7` | the exact Tempester version `a.b.c.d` (n°39) |
| `F0 01 28 44 F7` | 8 messages: `78`-`7D`, `75`, `76` ([Full backup](#full-backup)) | the six Voice chips, the Main boot flash and the System settings (n°62) |

Notes:

- **Request `42`**: the reply is the header and the 4 Play Lists (`5E`), then one `5C` message for each beat k (0-15)
  present in the project. It is byte for byte what the menu sends when it exports the project.
- **Request `43`**: each digit `N` is sent as two bytes, `N >> 7` and `N & 7F` (0-16383). It is the number System >
  *Main OS Version* shows. From 1.6.7.0 the reply is followed by `F0 01 28 45 <18 bytes> F7`
  ([Voice versions](#voice-versions)).
- A project may have fewer than 16 beats; the beats present come in increasing order.
- A request is served from the machine's main loop. A 16-beat project takes tens of seconds on DIN.
- The machine receives these same messages (`60`, `5F`, `5E` + `5C`) to load a sound, a beat or a project; that is
  stock behaviour. Receiving a project into Play Lists correctly needs n°28 (1.6.2.0).
- Unknown commands are ignored as before: a machine without n°39 does not answer `43`, so a program that gets no
  answer within a second falls back to what it knows (the identity says only « Tempester »).
- The version is read from the same place the System screen reads it: every release announces its own number. A test
  build keeps the number of the release it is built on.

### Voice versions

From 1.6.7.0 (n°64), right after the `43` reply and on the same cable, a second message (23 bytes):

`F0 01 28 45 h0 m0 l0 h1 m1 l1 h2 m2 l2 h3 m3 l3 h4 m4 l4 h5 m5 l5 F7`

- For Voice chip k (voice k, 0-5), `v = hk << 14 | mk << 7 | lk` is a 16-bit word.
- It is the word the chip answered when the Main asked its Voice OS version, once, at power-on.
- System > *Voice OS Version* shows it as `(v & 7F) >> 4`.`v & F`.

| `v` | Voice OS |
|---|---|
| `0015` | DSI Voice 1.5 |
| `0016` | Tempester Voice 1.5.1.0 or 1.5.1.1 (the same word: what the Main does with it is the same) |
| `0017` | Tempester Voice 1.5.1.2 |
| any other | not a known Voice OS, or the chip did not answer |

The Main itself takes a chip as Tempester Voice (Release up to 127, re-aligned full backups) when its word is exactly
`0016` on 1.6.7.0 and 1.6.8.0, and any word `0016`-`0079` from 1.6.9.0: install Main 1.6.9.0 before Tempester Voice
1.5.1.2.

- The `43` reply is unchanged: a program that reads only `43` gets the same message as before, then a `45` it can
  ignore. `45` is not a command of the machine; nothing is sent for it.
- A machine before 1.6.7.0 sends no `45`: wait a short time after the `43` reply, then treat the versions as unknown.
- The Release maximum of Pitch, Filter, Amp, Aux 1 and Aux 2 Release (65, 70, 75, 83, 91) follows from it:
  - 1.6.6.0: always 123;
  - 1.6.7.0 and 1.6.8.0: 127 when the six words are `0016`, 123 otherwise;
  - from 1.6.9.0: 127 when the six words are `0016`-`0079`.
- The words are read at power-on only: a Voice OS installed since then counts after the next power-on.

## One Sound by beat and pad

From 1.7.0.0 (n°74, n°75). Read or store the Sound of any pad of any beat of the project in memory, without changing
the selection on the machine. Accepted on the cable set in System > *MIDI: SysEx IN+OUT Cable*, like every request;
every answer leaves on that cable.

| Message to the machine | Answer | Meaning |
|---|---|---|
| `F0 01 28 46 bb pp F7` | `F0 01 28 47 bb pp <152 bytes> F7` | the Sound of beat `bb`, pad `pp` |
| `F0 01 28 46 bb pp F7`, `bb` or `pp` out of range | `F0 01 28 4F 46 01 F7` | nothing read |
| `F0 01 28 47 bb pp <152 bytes> F7` | `F0 01 28 4F 47 00 F7` | the Sound stored into beat `bb`, pad `pp` |
| `F0 01 28 47 bb pp <152 bytes> F7`, `bb` or `pp` out of range | `F0 01 28 4F 47 01 F7` | nothing written |

**Numbers.**

- `bb` = beat − 1: `00`-`0F` for beats 1-16. `10` and above are out of range.
- `pp` = pad: `00`-`0F` for A1-A16, `10`-`1F` for B1-B16 (0-31 = A1-B16, as CC 15). `20` and above are out of range.

**The 152 bytes** are exactly the data of a Sound dump `F0 01 28 60 <152 bytes> F7` (request `40`, the menu's
*Send Sound*), for the same Sound:

- They are the 128 bytes of the Sound, 7-bit packed ([7-bit packing](#7-bit-packing)): 18 groups of 8 bytes (a head
  byte with the high bit of the next 7 bytes, bit j for byte j, then those 7 bytes with their high bit cleared), then a
  19th group: its head byte, Sound bytes 126 and 127, and five padding bytes. 18 × 8 + 8 = 152.
- The machine sends zeros as padding. When you send `47`, send five zeros too; the machine does not read them.
- The 128 bytes hold the 127 parameters packed bit by bit, lowest bit first, each at the bit position and width of
  [Sound parameter numbers](#sound-parameter-numbers) (the table in `profile/dsi-tempest/sound_params.json`, fields
  `bit` and `bits`), then the name: 20 characters of 7 bits from byte 110.
- So a `60` message becomes a `47` message by replacing the byte `60` with `47 bb pp`, and the answer to `46` becomes a
  `60` message by replacing `47 bb pp` with `60`. A Sound read with `46` can be sent back with `47` unchanged.
- Parameter 124 (an internal format number, no knob) always reads 3 in an answer, as in a `60` from the machine. The
  stored Sound is not changed by a read.

**Status message** `F0 01 28 4F cc ss F7` (7 bytes):

| `cc` | `ss` | Meaning |
|---|---|---|
| `46` | `01` | request `46` with a beat or pad out of range: nothing read (a valid `46` is answered by `47`, not by `4F`) |
| `47` | `00` | the Sound is stored |
| `47` | `01` | beat or pad out of range: nothing written |

Other values of `ss` are not used by 1.7.0.0. `4F` is not a command of the machine: sent back to it, it is ignored.

**Examples.**

- Beat 1, pad A1: `F0 01 28 46 00 00 F7` → `F0 01 28 47 00 00 <152 bytes> F7` (159 bytes).
- Beat 3, pad B5: `F0 01 28 46 02 14 F7` → `F0 01 28 47 02 14 <152 bytes> F7`.
- Store into beat 16, pad B16: `F0 01 28 47 0F 1F <152 bytes> F7` → `F0 01 28 4F 47 00 F7`.
- Beat 17 does not exist: `F0 01 28 46 10 00 F7` → `F0 01 28 4F 46 01 F7`.

**What a store does.**

- The 152 bytes go through the same checks as a received `60`: a value above a parameter's maximum becomes that
  parameter's default; a name character below space becomes a space.
- Only that pad of that beat changes, **in the project in memory**, as after an edit on the machine. It is kept only
  when the beat or the project is saved on the machine; switching off without saving loses it.
- The selection does not change. When the stored pad is the one on screen, its knob values and LEDs follow; the play
  state of the pads of that beat is updated as after a received `60`.
- A pad that is sounding plays the new Sound from its next note.
- Sending `60` still loads a Sound into the selected pad of the current beat, as before (an « edit buffer »).
- Keep the previous Sound yourself: read it with `46` before a store, to put it back with `47`. On the project's
  Tempest one try of Revert Snd after a store did not bring the previous Sound back (not investigated; it is not known
  whether Revert was carried out to its end).

**Rules.**

- Send one message and wait for its answer before the next. The answer to `46` is 159 bytes, about 51 ms on DIN; the
  machine queues what it sends on DIN, and many unanswered requests in a row can overrun that queue.
- A `47` whose 152 bytes are not complete before `F7` is not executed and gets no answer. Bytes after the 152nd are
  ignored (one store, one answer).
- A message cut by another MIDI status byte, or left without `F7`, is abandoned (after about 3 seconds for the latter)
  and gets no answer.
- A machine before 1.7.0.0, or with the official OS, ignores `46` and `47` and does not answer. Ask for the version
  with `43` first.
- Requests are served from the machine's main loop; the sequencer keeps playing.

## Full backup

From 1.6.6.0 (n°62). Request `F0 01 28 44 F7`, accepted on the SysEx cable set in System like the other requests. The
machine answers on that cable with eight messages, back to back, in this order, and draws nothing on its screen:

| Order | Message | Content | Length |
|---|---|---|---|
| 1-6 | `F0 01 28 78+k <64 blocks> <c0> <c1> F7`, k = 0-5 | Voice chip k, whole flash | 74,951 bytes |
| 7 | `F0 01 28 75 <12 blocks> <c0> <c1> F7` | Main boot flash | 14,059 bytes |
| 8 | `F0 01 28 76 <packed data> F7` | System settings and pad channels | 277 bytes |

- **Messages 1-6**: the whole flash image of Voice chip k (voice k), 65,536 bytes, in the layout of a Voice OS file.
  The boot page and the chip's own calibration page are included.
- **Message 7**: the Main processor's boot flash, 12,288 bytes. Its last 16 bytes are the configuration words
  (DEVCFG3, 2, 1, 0).
- **Message 8**: the System settings and the pad channels, 235 bytes in the standard 7-bit packing
  ([7-bit packing](#7-bit-packing)).

**Layout of messages 1-7.** This is the OS-file layout, the one the Debug menu's *Send ... OS over MIDI* uses:

- the four header bytes;
- blocks of 1,024 data bytes, **each block packed on its own**: 146 groups of 8 bytes (1 + 7), then a group of 3 bytes
  (1 + the last 2 data bytes), so 1,171 bytes a block;
- two checksum bytes, then `F7`.

Checksum: the sum of the data read as 16-bit little-endian words, modulo 2^14, sent low 7 bits first
(`c0 = sum & 7F`, `c1 = sum >> 7 & 7F`).

Unpacking all the blocks of a message in one stream shifts every block by 3 bytes: unpack each 1,171-byte block alone.

Message 8, unpacked (235 bytes; the last packed group is padded with zeros, which are not data):

| Bytes | Content |
|---|---|
| 0 | 7, the version of the System settings file |
| 1-202 | the 202 System settings, in the order of the System settings file |
| 203-234 | the MIDI output channel of pads 1-32 (n°58): 0 stock route, 1-16 the channel with the played note, 17-32 channel − 16 with the pad's *MIDI Note: Pad* |

The System settings file the machine saves (406 bytes) is rebuilt from it as 203 little-endian 16-bit words:

- word 0 = byte 0;
- word 1 + i (i = 0-201): setting i in the low byte. The high byte is:
  - the channel of pad i − 48, for i = 48-79 (the *MIDI Note: Pad* entries);
  - `FF` for i = 138-201 when setting i is 80 or above (the machine stores those 64 settings sign-extended);
  - 0 otherwise.

### Alignment

A Voice chip answers each byte request with the next byte of a stream. DSI's Voice OS never resets that stream, and it
does not stop at the end of the flash. So before 1.6.6.0, only the first Voice dump after power-on started at the
beginning of the chip.

**1.6.6.0 and 1.6.7.0** count the bytes they read from each chip, and skip forward to the beginning before every dump.
Reading forward goes past the end of the chip's flash, which stops the chip.

- **With 1.6.6.0 and 1.6.7.0, only make one full backup (or one Debug Voice dump) per power-on.**
- A second one silences the voices of a DSI Voice OS until power-off (its messages read all `FF`).
- It restarts the chips of Tempester Voice 1.5.1.0, often for good (its messages come back shifted).

**From 1.6.8.0** the Main never reads forward:

| Chip, at a second dump in the same power-on | 1.6.8.0 does | Message `78+k` |
|---|---|---|
| Tempester Voice: answered `0016` at power-on (from 1.6.9.0: `0016`-`0079`) | sends it its own number again (`F5 0k`) | the whole chip, from its first byte |
| any other answer (DSI Voice 1.5, no answer) | does not read it | `F0 01 28 78+k F7` (5 bytes) |

- Sending `F5 0k` brings a Tempester Voice chip's stream back to the beginning. The words are listed under
  [Voice versions](#voice-versions).
- The 5-byte message has no data and no checksum. Power the machine off and on, then ask again.
- The Debug menu's *Send Voice OS over MIDI* (`72`, voice 0) follows the same rule. When voice 0 cannot be read from
  its beginning, it sends nothing: the screen shows the dump's title and « Sending MIDI » for a moment, then the menu
  again. No reason is shown.
- Tempester Voice 1.5.1.1 also brings its stream back to the beginning by itself at the end of its flash, so no Main
  can stop it this way. An older Main then gets a shifted message; the voice keeps playing.

**If the Main restarts alone.** The counting holds while the Main and the Voice chips started together (power-on). If
the Main restarts alone (a crash, a reset without switching the machine off) after its chips were read, it counts
from zero again while the chips do not.

- A Tempester Voice chip is brought back to its beginning by the Main's own start-up, which sends `F5 0k` to every
  chip.
- **A chip on DSI's Voice OS is then read from the middle of its stream, past the end of its flash: it stops and stays
  silent until the machine is switched off.** This is the same on 1.6.6.0, 1.6.7.0 and 1.6.8.0.
- After any restart of the Main alone, switch the machine off and on before a full backup.

**A message not read from the chip**: its first bytes are not the chip's reset jump (a `goto`: third byte `04`, then
three zero bytes), or its data is one byte repeated. Power the machine off and on and ask again.

### Timing and rules

- The machine reads the chips in at least 16 µs a byte (CPU simulated at 80 MHz, one cycle per instruction, flash
  wait states not counted).
  - USB: about 6.4 s for the whole backup, plus the USB transfer. 1.6.6.0 and 1.6.7.0 take up to 1.6 s more per chip
    to skip forward to its beginning.
  - DIN: the wire sets the pace. 464,042 bytes at 31,250 baud: about 2 min 35 s in all, about 25 s per Voice message.
- While it answers, the machine's main loop does nothing else: no screen, no panel, no MIDI input is read (DIN bytes
  beyond its 4 KB input ring are lost). Send nothing during a backup; stop the sequencer first (a playing voice may
  lose a few frames).
- Nothing in a backup is an update: the ids `75`, `76`, `78`-`7D` are no command of the machine, so a backup sent back
  to it is ignored. Never send the Debug menu's `72` / `73` dumps back to a machine: those ids are update commands.
- A machine without n°62 does not answer `44`. A program asks for the exact version first (request `43`).

## Identity

Universal identity request `F0 7E 7F 06 01 F7`. Reply:
`F0 7E <device> 06 02 01 28 01 00 00 15 <v1> <v2> F7`, version word `v = v1 | v2 << 4`.

- Every Tempester version answers `v = 0x7F2` (`v1 = 02`, `v2 = 7F`); the official OS answers its own word.
- The word says « Tempester », not which release. Ask with the request `43` ([above](#sysex-requests)) from n°39 on,
  or read System > *Main OS Version*.
- The identity reply itself is unchanged, so existing identity readers keep working.
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

**LED indexes** are in [`refs/panel_leds.tsv`](../refs/panel_leds.tsv):

- 0x00-0x2B: keys and indicators. 0x12 is Sound Bank B, lit when the pads are B1-B16.
- 0x2C-0x3B: pads 1-16, sent only after a `CC 107` with value bit 0 set.

Rules:

- Only on/off is reported, not brightness or flashing.
- Changes are sent at most 4 per main-loop pass, and only when the DIN output buffer is nearly empty. A LED that
  toggles back before it is sent produces nothing.
- Remote Pad OUT must not be *Off*.

**Pad notes.** Which note plays which pad is the machine's own table, System > *MIDI Note : Pad A1-B16*, not 36 + pad.
It holds one note per pad of banks A and B, each note on one pad only.

- On the author's machine, A1-A15 are the white keys from C2 (36, 38, 40, 41, 43, 45, 47, 48, 50, 52, 53, 55, 57, 59,
  60), A16 is 61 and B1-B16 are 62-77.
- A pad tapped on the machine sends its note on Remote Pad OUT, then the pad LED of the selected pad. The app pairs the
  two to learn the table (Machine page, *Pads* button).

### Pad Function mode (n°40, needs n°9b)

| Message | Meaning |
|---|---|
| `Bm 6A pp` (CC 106) from the machine | the Pad Function mode is `pp`: 0 16 Beats, 1 16 Time Steps, 2 16 Sounds, 3 16 Tunings, 4 16 Mutes, 5 16 Levels |

- Sent when the mode changes (a panel key or a remote key), and again after a `CC 107`, after the LEDs.
- Same rules as the LEDs: Remote Pad OUT must not be *Off* (the change is sent once it is on), and only when the DIN
  output buffer is nearly empty.
- The first report after power-on gives the start mode (16 Sounds).
- Nothing sets the mode over MIDI. Send the Pad Function keys: 16 = 16 Beats, 17 = 16 Time Steps, 24 = 16 Sounds,
  25 = 16 Mutes. 16 Tunings and 16 Levels are two keys held.
- Without n°40, read the mode from the four mode LEDs (0x10, 0x11, 0x18, 0x19) once they have stopped changing.

### Encoders and absolute knobs (n°23, needs n°9a)

| NRPN | Control | Value |
|---|---|---|
| 200-213 | encoder `e = number − 200` | relative: `value − 8192`, clamped to −16..+15 steps (0 = nothing) |
| 220-229 | absolute knob `k = number − 220` | 0-255 (the machine uses `value >> 1` for most knobs) |

Bytes:

- encoder `e`, one step up: `Bn 63 01 Bn 62 <48+e> Bn 06 40 Bn 26 01`;
- one step down: `... Bn 06 3F Bn 26 7F`;
- while the number and MSB are unchanged, `Bn 26 xx` alone is enough;
- absolute knob `k`: `Bn 63 01 Bn 62 <5C+k> Bn 06 <v >> 7> Bn 26 <v & 7F>`.

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

From 1.6.4.0 (n°38, needs n°9b). The machine **reports** these values on Remote Pad OUT as NRPN
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
in the beat being edited.

- `p` = 0-100 %; 101-127 count as 100 %.
- At 100 % (the default) the pad plays as before. Below, each note of that pad is skipped with probability 100 - `p`.
- The value is saved with the beat (`5F`, and `5C n` in a project), in a field the official OS writes as 0 and never
  reads.
- The machine does not report it. Read it back from a beat dump.

## Channel per pad

### Channel per pad (n°58, needs n°9b)

| Message | Meaning |
|---|---|
| `Bn 0F pp` (CC 15) to the machine | address pad `pp` (0-31 = A1-B16); 32-127: no pad addressed |
| `Bm 10 vv` (CC 16) from the machine | answer to CC 15: the addressed pad's value `vv` (0-32) |
| `Bn 10 vv` (CC 16) to the machine | the addressed pad's value: 0 = usual route, 1-16 = channel 1-16 with the played note, 17-32 = channel (`vv` - 16) with the pad's « MIDI Note: Pad »; 33-127 ignored |
| `Bn 11 7F` (CC 17) to the machine | save the System settings (with the table) if a value changed; ignored while playing or recording (the save stops the transport): send it again once stopped |

- `n` = Remote Pad IN (any channel when *All*), DIN or USB. `m` = Remote Pad OUT (no answer when *Off*).
- A pad with a value sends its note-on / note-off there (DIN then USB), from the sequencer and from the pads played by
  hand. This replaces its usual route (Sequencer Out, Synth IN or Remote Pad OUT).
- Values take effect at once. They are lost at power-off until CC 17 saves them.
- To read the whole table: CC 15 = 0 to 31, one answer each.

## Fine Quantize value

From 1.6.5.0 (n°55).

- The project header (`5E`, and the project file) holds the project parameters as 44 16-bit words (version 5 in
  word 0).
- Word 4's high byte is the Quantize, 0-7 (1/4, 1/8, 1/8T, 1/16, 1/16T, 1/32, 1/32T, Off), as on the official OS.
- When the Quantize is one of the eight Tempester values, word 4 says Off (7), and word 18 (byte offset 36 of the
  record) holds `0x5100 | v`:

| v | value | grid |
|---|---|---|
| 1 | 8 Dot | 72 ticks (96 per quarter note) |
| 2 | 16 Dot | 36 |
| 3 | 32 Dot | 18 |
| 4 | 64th | 6 |
| 5 | 64T | 4 |
| 6 | 8 Quint | 5 per half note |
| 7 | 16 Quint | 5 per quarter note |
| 8 | 16 Sept | 7 per quarter note |

- Otherwise word 18 is what the official OS writes there (`0x0000` or `0xFFFF`).
- Words 18-33 are never read by the official OS nor by earlier Tempester versions, which load such a project with
  Quantize Off.
- Tempester 1.6.5.0 reads word 18 only with the high byte `0x51`, `v` 1-8 and word 4 at Off. Anything else loads as
  plain Off.
- The knob order on the machine, longest to shortest: 1/4, 8 Dot, 1/8, 8 Quint, 16 Dot, 1/8T, 1/16, 16 Quint, 32 Dot,
  1/16T, 16 Sept, 1/32, 1/32T, 64th, 64T, Off.

## 7-bit packing

Data in DSI SysEx messages is packed in groups: one byte holding the high bits of up to 7 data bytes (bit j = high bit
of byte j), then those 7 bytes with their high bit cleared.

- The official OS ends a message whose data length is not a multiple of 7 with a non-standard last group (shifted high
  bits, stale padding). Its own receivers expect the standard layout.
- From 1.6.2.0 (n°27) the machine sends the standard layout.
- A tolerant reader should accept both. A writer should always send the standard layout (high bit j for byte j, zero
  padding).
- The app's `tempest-proto` crate implements both.

## Sound parameter numbers

NRPN number = parameter index.

- From 1.6.6.0 (n°63) the five Release parameters (65, 70, 75, 83, 91) stop at 123: at 124-127 the voices never end
  the release.
- A stored sound with 124-127 loads, shows and plays as 123. Its file keeps the value until it is saved again. An NRPN
  of 124-127 is clamped to 123.
- Names and ranges come from the parameter table inside the firmware (`profile/dsi-tempest/sound_params.json`). A few
  entries carry the same name there (for example 116-120, 121-122) and are not yet told apart individually.
- The DSI manual describes what each parameter does.

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
| 65 | Env Pitch | Release | 0-127 (0-123 from 1.6.6.0) |
| 66 | Env Filter | Delay | 0-127 |
| 67 | Env Filter | Attack | 0-127 |
| 68 | Env Filter | Decay | 0-127 |
| 69 | Env Filter | Sustain | 0-127 |
| 70 | Env Filter | Release | 0-127 (0-123 from 1.6.6.0) |
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
| 83 | Env Aux 1 | Release | 0-127 (0-123 from 1.6.6.0) |
| 84 | Env Aux 2 | Destination | 0-57 |
| 85 | Env Aux 2 | Amount | 0-254 |
| 86 | Env Aux 2 | Vel Amt | 0-127 |
| 87 | Env Aux 2 | Delay | 0-127 |
| 88 | Env Aux 2 | Attack | 0-127 |
| 89 | Env Aux 2 | Decay | 0-127 |
| 90 | Env Aux 2 | Sustain | 0-127 |
| 91 | Env Aux 2 | Release | 0-127 (0-123 from 1.6.6.0) |
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
