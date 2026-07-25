# Tracker for macOS

This directory contains the native SwiftUI companion for the Rust Tracker CLI.
It targets macOS 14 or newer and includes:

- a compact start/stop panel;
- a menu-bar timer;
- a Toggl-style weekly calendar;
- recent task shortcuts;
- manual, post-change, and periodic Tailscale sync;
- appearance, calendar, density, and sync customization.

The app uses `tracker snapshot` for structured reads and invokes the existing
`start`, `stop`, and `sync` commands for writes. It never parses human-readable
CLI output.

## Open on a Mac

First install the Rust CLI:

```sh
cd /path/to/tracker
cargo install --path .
```

Then open the Swift package:

```sh
open -a Xcode macos/TrackerMac/Package.swift
```

Select the `TrackerMac` scheme and run it from Xcode. Debug runs look for the CLI
in this order:

1. `TRACKER_CLI_PATH`;
2. the signed `tracker` helper bundled in `Contents/MacOS`;
3. `/opt/homebrew/bin/tracker`;
4. `/usr/local/bin/tracker`.

Release builds ignore `TRACKER_CLI_PATH` and never select a helper through the
current shell `PATH`.

## Build a distributable app

On a Mac with a matching full Xcode installation:

```sh
macos/TrackerMac/scripts/build-app.sh
open macos/TrackerMac/dist/Tracker.app
```

The default ad-hoc build enables App Sandbox, Hardened Runtime, outgoing network
access, and helper entitlement inheritance. It asks for no camera, microphone,
location, contacts, calendar, photo, Bluetooth, USB, Apple Events, incoming
network, or broad filesystem permissions.

The narrow sandbox cannot launch the external Tailscale CLI for automatic peer
discovery. Set a manual peer origin such as `http://100.64.0.2:7789` in
Tracker Settings > Sync. To retain automatic Tailscale CLI discovery for a
Developer ID release while keeping Hardened Runtime enabled, build with:

```sh
SANDBOXED=0 CODESIGN_IDENTITY="Developer ID Application: Example (TEAMID)" \
  macos/TrackerMac/scripts/build-app.sh
```

Use `CODESIGN_IDENTITY` for a real distribution signature. Notarize the finished
app with your Apple Developer credentials before public distribution.

The script currently builds the host architecture. For a universal release,
build on both Apple Silicon and Intel (or build both supported Rust and Swift
architectures in CI), combine matching executables with `lipo`, then sign the
combined bundle.

## Sync credentials

The optional application token can be managed in Settings > Sync. Tracker stores
it in this Mac's Keychain with a device-only accessibility class rather than
`UserDefaults`. A manual peer URL is stored as a nonsecret preference.

The complete review and remaining trust assumptions are documented in
[`../../docs/security-audit.md`](../../docs/security-audit.md).
