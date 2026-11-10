# Security policy

For Tempester, a **security issue** is anything that could harm a machine or someone's data:

- a firmware patch or version that makes a Tempest freeze, lose data, corrupt its storage or fail to start;
- a way the app could send something to the machine without the user's confirmation, send the wrong OS file, or accept
  a file whose SHA-256 does not match;
- a way the app could overwrite or delete the user's files without saying so;
- DSI content or personal data found in the repository or a release.

## How to report

Use **GitHub's private vulnerability reporting**: the repository's *Security* tab, then *Report a vulnerability*. It is
private between you and the maintainers. Please do not open a public issue for these.

Include: the Tempester version (System > *Main OS Version*) and the app version, the Voice, Panel and SAM versions
(System > *Show System Information*), the cable (USB or DIN), what you did, what happened, and whether the machine
recovered. **Do not attach DSI files, firmware dumps or `.hex` backups.**

## What happens next

Tempester is delivered as is, without support: a private report is read when a maintainer has time, and no response
time is promised. When a report about a firmware version is confirmed, the documentation and the version list are
marked « do not install », and a fixed version or a withdrawal follows when possible. Credit in the changelog if you
wish (a nickname is fine).

## If your machine is in trouble now

See [docs/recovery.md](docs/recovery.md): an interrupted or failed update can be sent again over MIDI; the official OS
can be reinstalled the same way.
