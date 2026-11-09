# Legal notes

> [!WARNING]
> **This is not legal advice.** It explains how the project is organised and which rules it relies on, as its
> contributors understand them. The law differs between countries and its application to a given case is for courts
> and lawyers. If in doubt, ask a lawyer where you live.

## Whose is what

- **The Tempest firmware** (Main, Voice, Panel and SAM operating systems), its manuals and its sounds are © Sequential
  LLC (Dave Smith Instruments). Sequential offers the operating system as a free download for Tempest owners. The
  download page states no licence terms (checked on 8 November 2026).
- **Tempester** (the app, the tools, the patch recipes, the documentation, the screenshots of the app) is the work of
  the Tempester contributors, published under the [GNU GPL v3.0 or later](../LICENSE).
- The app's dependencies keep their own licences (`midir`: MIT; `tao`, `wry`: Apache-2.0 or MIT), compatible with the
  GPL v3. The plugin depends on `nih-plug` (ISC) and, for its VST3 format, on `vst3-sys` (GPL v3), so a plugin binary is
  distributed under the GPL v3 with its source.

## What this repository does not contain

None of the following is in this repository, its history or its releases, and contributions that add them are refused:

- DSI binaries: the `.syx` OS files, decoded images (`.bin`), dumps read from a machine;
- decompiled or disassembled DSI code, or exports of a disassembler;
- DSI's manuals, addendum, pictures or logos;
- factory sounds, beats or projects;
- third-party photos.

Links point to Sequential's own pages instead:

- operating system: <https://sequential.com/support/download/tempest-operating-system/>
- update guide: <https://sequential.com/updating-tempest-os/>
- manuals: <https://sequential.com/support/download/tempest-documentation/>

An automatic guard checks every published tree for these (file types, DSI byte sequences, personal data,
decompiler-style code, image metadata), and the pull request template asks for it.

## Patches as recipes

A Tempester firmware version is never distributed as a file. The repository holds **recipes**: for each patch, the flash
addresses and the new words written by the contributors, and the SHA-256 of the official input file and of the
expected result. Each owner downloads the official Main OS from Sequential and applies the recipe **to their own copy**,
on their own computer (the app or `tools/build_version.py`). The input is checked by SHA-256 before anything is
done, and the result is checked against the recorded SHA-256. See the [recipe format](patch-recipe-format.md).

This keeps the publication to the contributors' own work: the fixes, the added code, the interface descriptions and
the tools. Short technical references (the role of a routine, a register, a message format) are used where needed to
explain or apply a fix; DSI's code itself is not reproduced, and neither the addresses of DSI's routines nor a
decompiler's names are published: the only flash addresses in the repository are the positions a recipe edits, the
bounds of the update page and the free area where the added code goes.

## The rules relied on

**European Union, Directive 2009/24/EC on the legal protection of computer programs**
(<https://eur-lex.europa.eu/eli/dir/2009/24/oj>):

- Article 5(1): the person having a right to use a program may, without authorisation, do the acts necessary to use it
  in accordance with its intended purpose, **including for error correction**, unless the contract says otherwise.
- Article 5(3): that person may observe, study or test the functioning of the program to determine the ideas and
  principles behind any of its elements, while loading, running or storing it.
- Article 6: decompilation is allowed without authorisation when indispensable to obtain the information necessary for
  the **interoperability** of an independently created program, under conditions: done by a person with a right to use
  the program, the information not readily available otherwise, limited to the parts needed. The information obtained
  may not be used for other goals, **nor given to others except when necessary for the interoperability** of the
  independently created program, nor used to make a substantially similar program.

**France, Code de la propriété intellectuelle, article L122-6-1** (Légifrance, <https://www.legifrance.gouv.fr/>):
transposes articles 5 and 6 of the directive (use including error correction, study, decompilation for
interoperability under the same conditions).

**Switzerland, Federal Act on Copyright (LDA / URG, RS 231.1), article 21**
(<https://www.fedlex.admin.ch/eli/cc/1993/1798_1798_1798/fr>), and **Copyright Ordinance (ODAu / URV, RS 231.11),
article 17**: a person entitled to use a program may decode it to obtain information on interfaces with independently
developed programs; the information may be used only to develop, maintain and use interoperable programs, without
prejudicing the normal exploitation of the program or unreasonably harming the rights holder.

How the project applies them:

- the app and the MIDI additions are independently created programs that need the Tempest's interfaces (SysEx
  formats, NRPN, panel codes): the [MIDI protocol](midi-protocol.md) documents those interfaces;
- the fixes correct errors of a program its owners have the right to use, applied by each owner to their own copy;
- nothing of DSI's program is copied into the repository or the releases.

Whether every part of this reasoning holds in a given country is exactly the kind of question this page cannot answer.

## Names and trademarks

« Tempest », « DSI », « Dave Smith Instruments » and « Sequential » are used only to say which instrument Tempester is
for (descriptive use). Tempester is **not affiliated with, endorsed or supported by** Sequential LLC, Dave Smith
Instruments, Focusrite plc or Roger Linn Design. No logo or trade dress of those companies is used; the app's
« Machine » view draws the layout of the controls with its own neutral lettering and no maker's logo. VST is a
trademark of Steinberg Media Technologies GmbH; the plugin is built with an independent VST3 binding, not with
Steinberg's SDK.

## Warranty

None. The GPL's sections 15 and 16 apply: you use Tempester, and install any firmware version, at your own risk.

## Contact

A rights holder who considers something in this repository infringing can open a private report through the
repository's security advisories (see [SECURITY.md](../SECURITY.md)); it will be looked at promptly.
