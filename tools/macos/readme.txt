Tempester: controller, librarian and updater for the DSI Tempest, plus the DAW plugin Tempester Control.

What this installer puts on your Mac (untick what you do not want):
  - Tempester (app)                    /Applications/Tempester.app
  - Tempester Control, CLAP plugin     /Library/Audio/Plug-Ins/CLAP/Tempester Control.clap
  - Tempester Control, VST3 plugin     /Library/Audio/Plug-Ins/VST3/Tempester Control.vst3
The plugins go to the system folders, so every DAW and every user account finds them, and the installer asks for an
administrator password. Plugins you installed by hand under ~/Library/Audio/Plug-Ins are not touched.

NOT SIGNED. This installer and its contents are not signed with an Apple Developer ID and not notarised.
  - Double-click the .pkg once (macOS refuses it), then click Open Anyway in System Settings > Privacy & Security.
  - The first launch of the app needs the same: open it once, then Open Anyway in Privacy & Security. Control-click >
    Open no longer works for unsigned software on macOS 15 (Sequoia) and later. A DAW may ask you to approve the
    plugin the same way.

Tempester Control sends MIDI and opens no MIDI port: route its MIDI output to the Tempest in your DAW. Its stereo
audio input passes to its output untouched, so a DAW that takes it only as an audio effect can load it. It needs a
Tempester Main OS that applies NRPN (1.5.3.0 or later); the original DSI OS ignores them. Nothing is sent to the machine
unless you route it.

Remove: drag /Applications/Tempester.app to the Bin and delete the two plugin folders named above. Your sounds, beats,
projects and backups (~/Library/Application Support/Tempester) are never touched.

Licence: GNU General Public License, version 3 or (at your option) any later version (shown on the previous page).
Not affiliated with Dave Smith Instruments, Sequential or Focusrite.
