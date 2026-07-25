# Tracker

Tracker is a small, local-first time tracker written in Rust. It is designed for
people who want a fast CLI, their own data, and private device-to-device
synchronization through a Tailscale tailnet instead of a hosted account.

The current release is an MVP. It provides:

- reusable tasks with an optional project and tags;
- one-command start, stop, status, and reporting;
- local SQLite storage with no account or telemetry;
- peer-to-peer, idempotent record synchronization;
- automatic discovery of Tracker servers on the current Tailscale network;
- a native SwiftUI macOS app with a menu-bar timer and weekly calendar.

The macOS app lives in [`macos/TrackerMac`](macos/TrackerMac) and uses the same
database and sync behavior as the CLI through a structured JSON bridge.

## Install

Tracker currently builds from source and requires a recent stable Rust toolchain:

```sh
git clone https://github.com/EasternKentuckyDigital/tracker.git
cd tracker
cargo install --path .
```

The database is stored in the operating system's normal per-user application data
directory. Override it for testing or portable use with `--database PATH` or the
`TRACKER_DATABASE` environment variable.

## Use the CLI

Tasks can be saved first:

```sh
tracker task add "Implement sync" --project tracker --tag rust --tag backend
tracker task list
```

They can also be created implicitly when starting a timer:

```sh
tracker start "Chess Study" --chess
tracker stop

tracker start "Read Bass Number Paper" --cornell
tracker status
tracker stop
tracker report
tracker report --since 7d
```

For `tracker start`, any otherwise unknown `--name` flag is a tag shortcut.
Multiple shortcuts work together, such as `--chess --study`. The explicit
`--tag chess` form remains available, as does `--project tracker` when a separate
overarching project is useful.

`tracker report --since` accepts `today`, a number of days such as `30d`, or an
RFC 3339 timestamp. Run `tracker --help` or `tracker <command> --help` for the
full command reference.

## macOS app

TrackerMac targets macOS 14 and provides a compact native interface:

- start and stop controls in the main window and menu bar;
- a seven-day, time-of-day calendar with daily and project totals;
- recent task shortcuts and live elapsed time;
- automatic sync after timer changes and optional periodic sync;
- selectable accents, light/dark appearance, density, text size, visible hours,
  week start, weekend visibility, and calendar label preferences.

Install the Rust CLI, then open the Swift package in Xcode:

```sh
cargo install --path .
open -a Xcode macos/TrackerMac/Package.swift
```

See the [macOS app README](macos/TrackerMac/README.md) for development and
distribution details. A packaged release can bundle the Rust executable so
nontechnical users do not need to install the CLI separately.

## Private sync with Tailscale

Install Tracker and Tailscale on each device, then sign those devices into the
same tailnet. On an always-online device such as a homelab server, run:

```sh
tracker serve
```

Tracker asks the local Tailscale client for this device's private Tailscale
address and listens only on that address. Leave the process running. No IP,
hostname, peer list, or Tailscale Serve configuration is required.

On a desktop, MacBook, or another tailnet device:

```sh
tracker sync
tracker report
```

`tracker sync` asks Tailscale for the online peer list, probes those private
addresses for Tracker, and exchanges records with every reachable Tracker
server. If several servers are online, it performs a final pass so they all
receive the combined record set.

At least one other device must currently be running `tracker serve`. The client
performing a sync does not need to run a server. This makes an always-online
homelab a natural hub without turning it into a special cloud service.

The connection uses HTTP inside Tailscale's encrypted tunnel and is reachable
only according to the tailnet's access-control policy. It is not exposed to the
public internet or ordinary LAN interfaces.

### Optional application token

By default, Tracker trusts the tailnet boundary. On a tailnet containing users or
devices that should not access time records, restrict TCP port 7789 with
[Tailscale grants]. You can also add a shared application token. Generate it once
and set the same environment variable for both the server and every syncing
client:

```sh
export TRACKER_SYNC_TOKEN="$(openssl rand -hex 32)"
tracker serve
```

Tracker rejects tokens shorter than 32 bytes. Do not commit the token or put it
directly in a command argument. The advanced `--peer URL` and `--bind ADDRESS`
options remain available for debugging or nonstandard setups; a manual
non-loopback bind requires the application token.

Synchronization currently exchanges the complete dataset, which is appropriate
for a small personal time-tracking database. A later release can add incremental
sync cursors and an installable background service.

## Development

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

The public repository contains no tailnet names, IP addresses, device IDs, or
credentials. Device IDs are random values generated inside each local database.

## License

No software license has been selected yet. Until the repository owner adds one,
copyright law reserves the usual rights even though the source is publicly
visible. Contributors and users should not assume permission beyond viewing and
testing the code.

[Tailscale grants]: https://tailscale.com/docs/features/access-control/grants
