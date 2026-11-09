; Per-machine installer for Tempester and the DAW plugin Tempester Control (NSIS). Asks for administrator rights once.
; Built by tools/package_windows.ps1 -Installer:
;   makensis /DVERSION=<x.y.z> /DSOURCE=<folder with tempester.exe and tempest.ico> /DPLUGINS=<folder with the bundles>
;            /DLICENSE=<LICENSE file> /DOUTFILE=<installer.exe> tempester.nsi
;
; Why per-machine (the first version of this script was per-user): the plugins must sit in the machine-wide
; "Common Files" folders (CLAP: %COMMONPROGRAMFILES%\CLAP, VST3: %COMMONPROGRAMFILES%\VST3) that every DAW scans, and
; writing there needs administrator rights, which an installer cannot get half way. The app then goes under Program Files
; for all users. The portable zip stays the way to run Tempester without administrator rights (no plugin installed).
; $COMMONFILES64 and $PROGRAMFILES64, not $COMMONFILES: this installer is a 32-bit program, where $COMMONFILES would be
; "Program Files (x86)\Common Files", a folder no DAW scans for these 64-bit plugins.
; The data folder (%APPDATA%\Tempester: sounds, beats, projects, backups) is never removed by the uninstaller.

Unicode true
!ifndef VERSION
  !error "pass /DVERSION=<x.y.z>"
!endif
!ifndef SOURCE
  !error "pass /DSOURCE=<folder>"
!endif
!ifndef PLUGINS
  !error "pass /DPLUGINS=<folder with Tempester Control.clap and .vst3>"
!endif
!ifndef LICENSE
  !error "pass /DLICENSE=<LICENSE file>"
!endif
!ifndef OUTFILE
  !error "pass /DOUTFILE=<installer.exe>"
!endif

!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\Tempester"
!define PLUGIN "Tempester Control"

Name "Tempester ${VERSION}"
OutFile "${OUTFILE}"
InstallDir "$PROGRAMFILES64\Tempester"
RequestExecutionLevel admin
SetCompressor /SOLID lzma

LicenseData "${LICENSE}"
Page license
Page components
Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

Function .onInit
  SetRegView 64
  SetShellVarContext all
FunctionEnd

Function un.onInit
  SetRegView 64
  SetShellVarContext all
FunctionEnd

Section "Tempester (app)" SecApp
  SectionIn RO
  SetOutPath "$INSTDIR"
  File "${SOURCE}\tempester.exe"
  File "${SOURCE}\tempest.ico"
  File "/oname=LICENSE.txt" "${LICENSE}"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateShortcut "$SMPROGRAMS\Tempester.lnk" "$INSTDIR\tempester.exe" "" "$INSTDIR\tempest.ico"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayName" "Tempester"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\tempest.ico"
  WriteRegStr HKLM "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD HKLM "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

Section "${PLUGIN}, CLAP plugin" SecClap
  SetOutPath "$COMMONFILES64\CLAP"
  File "${PLUGINS}\${PLUGIN}.clap"
SectionEnd

Section "${PLUGIN}, VST3 plugin" SecVst3
  SetOutPath "$COMMONFILES64\VST3"
  File /r "${PLUGINS}\${PLUGIN}.vst3"
SectionEnd

; The plugins are removed whether or not they were selected at install time: both paths are ours alone. The CLAP and
; VST3 folders themselves stay (other plugins live there).
Section "Uninstall"
  Delete "$COMMONFILES64\CLAP\${PLUGIN}.clap"
  RMDir /r "$COMMONFILES64\VST3\${PLUGIN}.vst3"
  Delete "$SMPROGRAMS\Tempester.lnk"
  Delete "$INSTDIR\tempester.exe"
  Delete "$INSTDIR\tempest.ico"
  Delete "$INSTDIR\LICENSE.txt"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  DeleteRegKey HKLM "${UNINSTALL_KEY}"
SectionEnd
