# Building

**In short**: most people only need [Installing a release package](#installing-a-release-package). The rest of this
page is for building the app, the plugin or a firmware release yourself.

- [Requirements](#requirements)
- [Building and running the app](#building-and-running-the-app)
- [The plugin](#the-plugin)
- [Packages](#packages)
- [Installing a release package](#installing-a-release-package)
- [Rebuilding a firmware version from your own DSI file](#rebuilding-a-firmware-version-from-your-own-dsi-file)
- [Running the verifier](#running-the-verifier)
- [Previewing the app's page in a browser](#previewing-the-apps-page-in-a-browser)

## Requirements

- **Rust 1.99.1**, pinned in `app/rust-toolchain.toml`. `rustup` installs it on first build. Changing the version is a
  deliberate step: newer Rust brings new lints.
- **Linux**: `libwebkit2gtk-4.1-dev libgtk-3-dev libasound2-dev` (Debian/Ubuntu names).
- **Windows**: the WebView2 runtime (present on Windows 11), and Git Bash to run the shell scripts.
- **Python 3.10+** for the tools (standard library only). Run them with `python3 -I`.
- Optional: **Node.js** for the page tests, and **Playwright** (`playwright-core` with Chrome and WebKit) for the
  layout and screenshot scripts. Neither is a dependency; tests that need them are skipped without them.

The app depends on four crates only: `tao` and `wry` (window and web view), `midir` (MIDI), and the project's own
`tempest-proto` and `tempest-midi`. Its JSON, SHA-256 and SysEx code are written in the project.

## Building and running the app

```bash
cd app
cargo run -p tempester                          # the real machine (port named "Tempest")
TEMPESTER_FAKE_MIDI=1 cargo run -p tempester    # a simulated machine in memory: nothing is sent to a MIDI port
cd .. && tools/test_app.sh                    # tables, fmt, clippy, tests with a disposable home folder
```

- `tempester --version` prints the version.
- After any change in `patch/` or `versions/`, regenerate the Rust tables with
  `python3 -I tools/generate_rust_tables.py`. The tests fail otherwise.

## The plugin

Tempester Control lives in `app/crates/tempester-plugin`. It is a separate Cargo workspace with its own lock file,
because it depends on `nih-plug`, which the app does not use.

```bash
cd app/crates/tempester-plugin
cargo xtask bundle tempester-plugin --release     # target/bundled/Tempester Control.clap and .vst3
cargo run --release --example host_clap -- "target/bundled/Tempester Control.clap"   # headless check (macOS, Linux)
```

To install by hand, copy the two bundles to your plug-in folders:

- macOS: `~/Library/Audio/Plug-Ins/CLAP/` and `.../VST3/`
- Linux: `~/.clap` and `~/.vst3`
- Windows: `%COMMONPROGRAMFILES%\CLAP` and `...\VST3`

They are not signed, so a host may ask you to approve them.

## Packages

| System | Command | Output in `dist/` |
|---|---|---|
| macOS | `tools/package_mac.sh` | `.dmg` and `.pkg` |
| Linux | `tools/package_linux.sh` | two `.deb` (app, plugins) |
| Windows | `tools/package_windows.ps1 -Installer` | portable zip and setup |

- **macOS**: `Tempester-<version>-macos-<arch>.dmg` holds the app (checked by mounting it). The `.pkg` holds the app
  and the plugins as options.
- **Linux**: `tempester_<version>_<arch>.deb` (the app) and `tempester-plugins_<version>_<arch>.deb`. With
  `TEMPESTER_DISTRO=ubuntu24.04` (the system's own release, checked) the names end with `_ubuntu24.04`.
- **Windows**: a portable zip (plugins in `plugins/`) and an NSIS setup with the plugins as options.

The release workflow (`.github/workflows/release.yml`) builds the same packages on a `v*` tag and attaches them to a
**draft** release.

- It builds the Linux pair twice: on Ubuntu 24.04 and on Ubuntu 26.04 (`_ubuntu24.04.deb`, `_ubuntu26.04.deb`).
- The dependencies of each pair are computed on its own build system (`dpkg-shlibdeps`), so each pair names the
  library packages of its release, with minimum versions.
- It packages the app only: no firmware, no DSI file.
- Run by hand (*Run workflow*) on a branch, it only builds the packages as workflow artifacts. On a `v*` tag, it also
  creates the draft release.

## Installing a release package

None of the packages is signed or notarized: Tempester has no Apple or Microsoft developer account. Each system
therefore blocks the first launch. Here is the way around it.

Check the file first. The Releases page lists the SHA-256 of each package:

- macOS and Linux: `shasum -a 256 <file>`
- Windows: `certutil -hashfile <file> SHA256`

### macOS

The packages are not signed with an Apple Developer ID, so macOS blocks them the first time.

**The app (`.dmg`)**

1. Open the `.dmg` and drag `Tempester.app` to Applications.
2. Open the app once. macOS refuses it.
3. Open **System Settings > Privacy & Security**, scroll to « Tempester was blocked » and click **Open Anyway**.

**The installer (`.pkg`)**

It installs the app and, if you keep them selected, the CLAP and VST3 plugins in `/Library/Audio/Plug-Ins`
(administrator password).

1. Double-click the `.pkg` once. macOS refuses it.
2. Click **Open Anyway** in **System Settings > Privacy & Security**, then run the installer.

#### macOS blocks the app or says it is damaged

Remove the quarantine mark macOS puts on downloaded files. In Terminal:

- the app:

  ```bash
  xattr -dr com.apple.quarantine /Applications/Tempester.app
  ```

- the installer, before opening it:

  ```bash
  xattr -d com.apple.quarantine ~/Downloads/Tempester-*-macos-arm64.pkg
  ```

- the plugins, if a DAW refuses them:

  ```bash
  sudo xattr -dr com.apple.quarantine "/Library/Audio/Plug-Ins/CLAP/Tempester Control.clap" \
    "/Library/Audio/Plug-Ins/VST3/Tempester Control.vst3"
  ```

« Is damaged » came from the 0.1.2 packages, whose signature was invalid; 0.1.3 fixes it. Right-click > *Open* no
longer works for an unsigned app on macOS 15 (Sequoia) and later.

### Windows

- **Setup**: run `Tempester-<version>-windows-x64-setup.exe`. It needs administrator rights, because the optional
  plugins go to the system plug-in folders. The uninstaller removes them.
- **Portable zip**: unzip it. No administrator rights; the plugins are in `plugins/`.

SmartScreen warns about an unknown publisher: click **More info**, then **Run anyway**. If the file is blocked without
that choice, unblock it first in PowerShell:

```powershell
Unblock-File .\Tempester-<version>-windows-x64-setup.exe
```

Or right-click the file > Properties > tick *Unblock*. For the zip, unblock the zip before extracting it.

### Linux (Debian/Ubuntu)

Nothing blocks an unsigned package, but `apt` warns that it is not from a repository:

```bash
sudo apt install ./tempester_<version>_amd64_ubuntu24.04.deb            # the app
sudo apt install ./tempester-plugins_<version>_amd64_ubuntu24.04.deb    # the plugins, in /usr/lib/clap and /usr/lib/vst3
```

Each release has two pairs of packages. Pick the one built for your system:

| Your system | Packages to take |
|---|---|
| Ubuntu 24.04 LTS and systems built on it (Linux Mint 22, Pop!_OS 24.04...) | `*_ubuntu24.04.deb` |
| Ubuntu 26.04 LTS and systems built on it | `*_ubuntu26.04.deb` |
| Another Debian-based system | see below |

For another Debian-based system, take the pair of the newest Ubuntu release that is not newer than your system. If it
does not fit, `apt` refuses it and names the missing or too old library.

About the two pairs:

- They hold the same app and plugins, built on each release.
- Their dependencies are the library packages of that release, with minimum versions: WebKitGTK 4.1, GTK 3 and ALSA
  for the app, the C library for both (`libasound2t64`, for example).
- So `apt` installs what is missing, and refuses a system older than the build system (for example the 26.04 pair on
  Ubuntu 24.04 or Debian 12) instead of installing a program that would not start.
- Both pairs carry the same package names, so installing one replaces the other.

## Rebuilding a firmware version from your own DSI file

The app does this for you (OS update page). To do it by hand, or to check the app:

1. Download the official OS package from <https://sequential.com/support/download/tempest-operating-system/> and unzip
   it. Keep it outside this repository: DSI files are never committed (`.gitignore` refuses `*.syx` and `*.bin`).
2. Decode the Main OS into an image:

   ```bash
   python3 -I tools/dsi_unpack.py Tempest_Main_1.5.0.2.syx work/Main_1.5.0.2.bin
   ```

   The next step checks that this image is exactly the official Main 1.5.0.2 (SHA-256) and refuses anything else.
3. Build a version from its manifest:

   ```bash
   python3 -I tools/build_version.py versions/1.6.1.0.json work/Main_1.5.0.2.bin build/
   ```

   It applies the version's patches (each is a recipe: see the [recipe format](patch-recipe-format.md)). It refuses
   overlaps and any change to the update page. It re-encodes the `.syx`, decodes it back, and compares the image and
   the `.syx` with the SHA-256 recorded in the manifest. It prints both hashes; a difference is an error.
4. Send the resulting `.syx` with the app (*Or any OS file*: it recognises a Tempester version by its hash) or with
   Sequential's method. Read [Before you install](user-manual.md#before-you-install-a-tempester-version) first.

**Tempester Voice** builds the same way from the official Voice file:

1. Decode `Tempest_Voice_1.5.syx` to `work/Voice_1.5.bin`.
2. Run `python3 -I tools/build_version.py versions/voice/1.5.1.2.json work/Voice_1.5.bin build/`.

It also refuses any change to the chip's boot and calibration pages, and to its update entry.

**The OS SysEx format**

- `tools/dsi_pack.py <image> 71 <out.syx>` re-encodes an image (71 Main, 72 Voice, 73 Panel, 74 SAM).
- Layout: `F0 01 28 <id>`, then blocks of 1,024 bytes, each packed in 7 bits on its own (1,171 bytes per block), then a
  14-bit checksum in two bytes, and `F7`.
- The checksum is the sum of the image's 16-bit little-endian words.

The tools' internal names and messages are partly in French (the project's first language). They are being converted.

## Running the verifier

```bash
tools/verify.sh <folder with the four official .syx files>          # about 10 s plus the app tests
tools/verify.sh <folder> --long                                     # adds the layout counter-proofs of the app's page
```

- The folder must hold `Tempest_Main_1.5.0.2.syx`, `Tempest_Voice_1.5.syx`, `Tempest_Panel_1.3.syx` and
  `Tempest_Sam_1.1.syx`.
- The verifier writes only into a temporary folder and `work/` (ignored).
- It ends with « all green » and exit code 0. Anything else is a failure.
- The firmware simulations are not published ([testing](testing.md#firmware-proofs)). The verifier says so and checks
  everything else.
- Some tests also look for optional data next to that folder (the factory projects, a captured project). Without it
  they are skipped and say so.

Run it before every pull request that touches `patch/`, `versions/` or `tools/`.

## Previewing the app's page in a browser

```bash
python3 -I tools/preview_ui.py /tmp/tempester.html --library-demo
```

This writes one HTML file with the app's real page and example data (marked EXAMPLE). Open it in a browser; nothing
is sent anywhere. `tools/preview_capture.js` and its siblings take screenshots from it with Playwright.
