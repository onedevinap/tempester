Tempester Control: CLAP and VST3 plugin for your DAW (64-bit, Windows)

The setup program (Tempester-<version>-windows-x64-setup.exe) installs these for you, with administrator rights.
To install by hand, copy:
  Tempester Control.clap   into   C:\Program Files\Common Files\CLAP
  Tempester Control.vst3   into   C:\Program Files\Common Files\VST3      (the whole folder)
Both folders need administrator rights. Without them, DAWs that read the per-user folders find the plugins in
  %LOCALAPPDATA%\Programs\Common\CLAP   and   %LOCALAPPDATA%\Programs\Common\VST3
(not every DAW scans those; the plugins have not been tried in a DAW yet).
To remove them, delete the same files.

Tempester Control is a MIDI effect: it opens no MIDI port. Route its MIDI output to the Tempest in your DAW. It needs a
Tempester Main OS that applies NRPN (1.5.3.0 or later). Nothing is sent to the machine unless you route it.
Not signed: Windows SmartScreen or your DAW may warn.

Licence: GNU General Public License, version 3 or (at your option) any later version (file LICENSE next to
tempester.exe).
