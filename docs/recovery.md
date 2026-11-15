# Recovery

**In short**: you can go back to the official OS, or send a failed update again, over MIDI. No programmer is needed.
A programmer is only a last resort if the Main OS ever stops starting.

- [Going back and re-sending: no programmer needed](#going-back-and-re-sending-no-programmer-needed)
- [Why this is safe in practice](#why-this-is-safe-in-practice)
- [Tempester Voice: going back, and its one risk](#tempester-voice-going-back-and-its-one-risk)
- [Optional: full chip backup and recovery with a PICkit](#optional-full-chip-backup-and-recovery-with-a-pickit)

## Going back and re-sending: no programmer needed

Everything happens **over MIDI (USB or DIN)**:

- **Another Tempester version**: build it and send it from the OS update page
  ([manual](user-manual.md#installing-a-version-with-the-app)).
- **Back to the official OS**: *Back to DSI 1.5.0.2* on the same page. Or send the official `Tempest_Main_1.5.0.2.syx`
  with [Sequential's own method](https://sequential.com/updating-tempest-os/).
- **An interrupted or failed update** (cable pulled, computer asleep, checksum error): power the Tempest off and on,
  and send the file again.

On the project's test machine, every Tempester release from 1.5.1.0 to 1.6.9.0 was installed this way over USB, each
replacing the previous one, with no programmer.

- Each time, the machine started and showed the new version under System > *Main OS Version*.
- For 1.5.5.0, 1.6.2.0, 1.6.3.0, 1.6.4.0 and 1.6.5.0, the Main OS read back through the Debug menu matched the built
  image over the whole range an update writes (the last flash page aside, see below).
- Going back to the official 1.5.0.2 has not been done on that machine yet. It uses exactly the same update path.

Details: [testing](testing.md#verified-on-a-real-machine).

## Why this is safe in practice

- The Tempest checks the 14-bit checksum and the end of the message of a Main OS update **before** it erases anything.
  A truncated or corrupted file is refused with an error on the screen.
- The routine that erases and writes the Main flash lives in the **last 4 KB page** of the flash (`0x9D07F000`). It
  rewrites the 127 pages below it, never its own page.
- **No Tempester version modifies that page.** The build tools refuse any patch that touches it, and every version is
  checked for it.
- So after a failed or interrupted write, the machine still has the routine it needs for the next update. The rest of
  the Main OS must still start far enough to accept one (its update reception lives in the normal OS). Send the file
  again.

The one case MIDI cannot fix: a Main OS that no longer starts at all, for example after a write interrupted at the
worst moment. It has never happened with Tempester, but it is possible in principle with any firmware update,
official or not. The last resort is then a programmer on the Main board ([below](#optional-full-chip-backup-and-recovery-with-a-pickit)).

The copy of the update routine in a machine comes from the factory. It may differ slightly from the one in DSI's file:
97 bytes differ on the test machine (an older build of the same routine, same control flow). Tempester never touches
it either way.

## Tempester Voice: going back, and its one risk

[Tempester Voice](firmware.md#tempester-voice) replaces the program of the six voice chips, with one Voice update that
goes to all six at once.

**Going back**

- Send the official `Tempest_Voice_1.5.syx` like any Voice update: Sequential's method, or the app's OS update page
  (« Choose an OS .syx file to check », then send).
- *Back to DSI 1.5.0.2* changes the Main only. The voice chips keep the Voice OS they run.
- Main and Voice can go back in either order. The official Main only checks that a chip answers with a version in
  range. Every Tempester OS works with the official Voice OS (from 1.6.6.0 with the Release capped at 123).

**Never written**: each chip's boot page (which holds the routine that writes the chip) and its own calibration page.

**The risk**

- A Voice update interrupted while the first 29 of its 41 pages are written (about two thirds of the transfer) can
  leave chips that no longer start, and so no longer accept an update over MIDI.
- Then only a programmer on that chip's own ICSP header brings it back. A full backup over MIDI does not hold
  everything such a restore needs.
- This is the risk of an official Voice update too.

To keep it small: make a full backup (Backup page) right before and right after, stop the sequencer, and unplug
nothing during the update. It has not happened on the project's machine (three Voice installs, 14 and 15 November
2026).

## Optional: full chip backup and recovery with a PICkit

> [!IMPORTANT]
> **This is optional.** You do not need a programmer to install, change or remove Tempester. It is a last-resort
> safety net if the Main OS ever stops starting.
>
> It means opening the instrument, which may void a warranty, and connecting to a powered board. Do it only if you
> are comfortable with electronics. **Not verified** below means not yet done on the project's own hardware.

### What you need

- A Microchip programmer: **MPLAB PICkit 5** (PG164150) or PICkit 4, with **MPLAB IPE** (installed with MPLAB X IDE).
  - PICkit 5: <https://www.microchip.com/en-us/development-tool/PG164150>
  - PICkit 5 user's guide: <https://ww1.microchip.com/downloads/aemDocuments/documents/DEV/ProductDocuments/UserGuides/MPLAB-PICkit-5-In-Circuit-Debugger-Users-Guide-50003525.pdf>
  - MPLAB IPE: <https://www.microchip.com/en-us/tools-resources/production/mplab-integrated-programming-environment>
  - MPLAB IPE user's guide: <https://www.microchip.com/content/dam/mchp/documents/DEV/ProductDocuments/UserGuides/MPLAB_IPE_USER_GUIDE.pdf>
- Target device: **PIC32MX695F512L**, the Main processor of the Tempest (identified on the Main board).

### The programming header (not verified)

The Main board has a 6-pin header next to the Main processor, marked **P7** in this project's notes, with pin 1
marked. The usual Microchip ICSP order is:

1. MCLR
2. VDD
3. VSS (ground)
4. PGEDx (data)
5. PGECx (clock)
6. not connected

Before connecting:

- **Check the silkscreen and the pinout on your board.** With the machine off, use an ohmmeter to find VSS (ground)
  and VDD. Follow PGEDx / PGECx / MCLR to the processor's pins with the device datasheet. This check has **not** been
  done yet on the project's machine.
- **Power the board from the Tempest itself.** In MPLAB IPE, set the tool **not** to power the target, unless you are
  sure of what you do: the programmer cannot supply a whole instrument.

### Back up the chip first (not verified)

Before any experiment, read the whole chip and keep the file:

1. Tempest powered on, programmer connected to P7 and to the computer.
2. In MPLAB IPE (Advanced Mode): device PIC32MX695F512L, tool PICkit 5 (or 4), power from the target.
3. Select every memory region: **program flash, boot flash and configuration words**.
4. **Read**, then **Export** the result as a `.hex` file.
5. **Read a second time**, export again, and compare the two files. They must be identical; if they differ, do not
   rely on them.
6. If the read comes back blank (all `FF`) while the machine works, the chip is probably **code-protected**. A backup
   is then impossible this way, and erasing it would destroy the factory boot code. Whether the Tempest's chip is
   protected is **unknown** (not observed).

Keep the `.hex` **private**: it contains DSI's firmware. Never share it or attach it to an issue.

### Restore (not verified)

If the Main OS no longer starts:

1. Connect as above.
2. Import your `.hex` in MPLAB IPE.
3. **Program**, then **Verify**.

The machine should start with the OS it had when you made the backup. From there, MIDI updates work again.

This procedure does not touch the Main board's other memories (external RAM, the 4 MB flash with your projects) or
the other chips (Panel, Voice, SAM). The Voice and Panel chips' own OS can be captured over MIDI with the app's
[Backup](user-manual.md#backup) page.
