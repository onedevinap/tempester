# Recovery

- [Going back and re-sending: no programmer needed](#going-back-and-re-sending-no-programmer-needed)
- [Why this is safe in practice](#why-this-is-safe-in-practice)
- [Optional: full chip backup and recovery with a PICkit](#optional-full-chip-backup-and-recovery-with-a-pickit)

## Going back and re-sending: no programmer needed

Installing a Tempester version, moving to another one and going back to the official OS all happen **over MIDI (USB or
DIN), without any programmer**:

- **Another Tempester version**: build it and send it from the OS update page ([manual](user-manual.md#installing-a-version-with-the-app)).
- **Back to the official OS**: *Back to DSI 1.5.0.2* on the same page, or send the official
  `Tempest_Main_1.5.0.2.syx` with Sequential's own method (<https://sequential.com/updating-tempest-os/>).
- **An update that was interrupted or failed** (cable pulled, computer asleep, checksum error): power the Tempest off
  and on and send the file again.

On the project's test machine, Tempester 1.5.1.0, 1.5.2.0, 1.5.3.0, 1.5.4.0, 1.5.5.0, 1.5.6.0, 1.6.0.0, 1.6.1.0,
1.6.2.0 and 1.6.3.0 were installed this way over USB, one after the other, each replacing the previous one, with no
programmer. Each time the machine started and showed the new version under System > *Main OS Version*; for 1.5.5.0,
1.6.2.0 and 1.6.3.0 the Main OS read back through the Debug menu was compared with the built image and matched over the
whole range an update writes (the last flash page aside, see below). Going back to the official 1.5.0.2 has not been
done on that machine yet; it uses exactly the same update path. Details:
[testing](testing.md#verified-on-a-real-machine).

## Why this is safe in practice

- The Tempest checks the 14-bit checksum and the end of the message of a Main OS update **before** it erases anything.
  A truncated or corrupted file is refused with an error on the screen.
- The routine that erases and writes the Main flash lives in the **last 4 KB page** of the flash (`0x9D07F000`). It
  rewrites the 127 pages below it and never its own page. **No Tempester version modifies that page**: the build tools
  refuse any patch that touches it, and every version is checked for it.
- So, after a failed or interrupted write, the machine still has the routine it needs to receive the next update, as
  long as the rest of the Main OS starts far enough to accept one (its update reception lives in the normal OS). Send
  the file again.
- The one case MIDI cannot fix: a Main OS that no longer starts at all (for example a write interrupted at the worst
  moment). That has never happened with Tempester, but it is possible in principle with any firmware update, official or
  not. Then the last resort is a programmer on the Main board, below.

Note: the copy of the update routine in a machine comes from the factory and may differ slightly from the one in DSI's
file (97 bytes differ on the test machine: an older build of the same routine, same control flow). Tempester never
touches it either way.

## Optional: full chip backup and recovery with a PICkit

> [!IMPORTANT]
> **This is optional.** You do not need a programmer to install, change or remove Tempester. It is a last-resort safety
> net if the Main OS ever stops starting. It means opening the instrument, which may void a warranty, and connecting
> to a powered board. Do it only if you are comfortable with electronics. **Not verified** below means not yet done on
> the project's own hardware.

### What you need

- A Microchip programmer: **MPLAB PICkit 5** (PG164150) or PICkit 4, with **MPLAB IPE** (installed with MPLAB X IDE).
  - PICkit 5: <https://www.microchip.com/en-us/development-tool/PG164150>
  - PICkit 5 user's guide: <https://ww1.microchip.com/downloads/aemDocuments/documents/DEV/ProductDocuments/UserGuides/MPLAB-PICkit-5-In-Circuit-Debugger-Users-Guide-50003525.pdf>
  - MPLAB IPE: <https://www.microchip.com/en-us/tools-resources/production/mplab-integrated-programming-environment>
  - MPLAB IPE user's guide: <https://www.microchip.com/content/dam/mchp/documents/DEV/ProductDocuments/UserGuides/MPLAB_IPE_USER_GUIDE.pdf>
- Target device: **PIC32MX695F512L**, the Main processor of the Tempest (identified on the Main board).

### The programming header (not verified)

The Main board has a 6-pin header next to the Main processor, marked **P7** in this project's notes, with pin 1
marked. The usual Microchip ICSP order is 1 MCLR, 2 VDD, 3 VSS (ground), 4 PGEDx (data), 5 PGECx (clock), 6 not
connected.

- **Check the silkscreen and the pinout on your board before connecting**: with the machine off, use an ohmmeter to
  find VSS (ground) and VDD, and follow PGEDx / PGECx / MCLR to the processor's pins with the device datasheet. This
  check has **not** been done yet on the project's machine.
- **Power the board from the Tempest itself.** In MPLAB IPE, set the tool **not** to power the target, unless you are
  sure of what you do: the programmer cannot supply a whole instrument.

### Back up the chip first (not verified)

Before any experiment, read the whole chip and keep the file:

1. Tempest powered on, programmer connected to P7 and to the computer.
2. In MPLAB IPE (Advanced Mode): device PIC32MX695F512L, tool PICkit 5 (or 4), power from the target.
3. Select every memory region: **program flash, boot flash and configuration words**.
4. **Read**, then **Export** the result as a `.hex` file.
5. **Read a second time**, export again, and compare the two files: they must be identical. If they differ, do not
   rely on them.
6. If the read comes back blank (all `FF`) while the machine works, the chip is probably **code-protected**: then a
   backup is impossible this way, and erasing it would destroy the factory boot code. Whether the Tempest's chip is
   protected is **unknown** (not observed).

Keep the `.hex` **private**: it contains DSI's firmware. Never share it or attach it to an issue.

### Restore (not verified)

If the Main OS no longer starts: connect as above, import your `.hex` in MPLAB IPE, **Program**, then **Verify**.
The machine should start with the OS it had when you made the backup; from there, MIDI updates work again.

The Main board's other memories (external RAM, the 4 MB flash with your projects) and the other chips (Panel, Voice,
SAM) are not touched by this procedure. The Voice and Panel chips' own OS can be captured over MIDI with the app's
[Backup](user-manual.md#backup) page.
