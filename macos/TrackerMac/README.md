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

Select the `TrackerMac` scheme and run it from Xcode. The app looks for the CLI
in this order:

1. `TRACKER_CLI_PATH`;
2. a `tracker` executable bundled in the application resources;
3. `/opt/homebrew/bin/tracker`;
4. `/usr/local/bin/tracker`;
5. the current shell `PATH`.

For a distributable `.app`, build a universal release version of the Rust binary
for Apple Silicon and Intel, combine it with `lipo`, and add it to the app
target's Copy Bundle Resources phase as `tracker`. End users then need only the
app and Tailscale; they do not need Rust or a separate CLI installation.

This Linux development workspace does not contain Swift or Xcode, so the SwiftUI
target must receive its final compile and signing validation on macOS.
