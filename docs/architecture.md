# Architecture

Tracker is local-first: CLI commands always read or write the local SQLite
database and never require a network connection.

```text
CLI ─────────────┐
                 ├── tracker Rust core ───── SQLite database
macOS SwiftUI ───┘             │
                               └── tailnet sync API
                                          │
                                  Tailscale tailnet
                                          │
                                  another Tracker peer
```

## Data model

A task contains a name, optional project, and default tags. A time entry snapshots
those fields when its timer starts, so later task changes do not rewrite historical
reports.

Tasks and time entries have stable IDs, UTC timestamps, a source-device ID, and a
soft-delete field reserved for future editing commands. Synchronization applies
the record with the greatest `(updated_at, source_device)` pair. The device ID is
the deterministic tie-breaker when two updates have identical timestamps.

Task IDs are derived from a normalized task name, allowing two offline devices to
create the same task and converge. Time-entry IDs are random UUIDs. If separate
offline devices start timers concurrently, both records are retained; the next
`tracker stop` closes the conflict set rather than losing either interval.

This last-write-wins strategy assumes device clocks are reasonably synchronized.
Tailscale peers normally also have ordinary network time available. A future
protocol revision may replace wall-clock ordering with hybrid logical clocks.

## Sync protocol

`POST /v1/sync` accepts and returns JSON containing all tasks and entries. The
operation is symmetric:

1. The caller sends its complete record set.
2. The receiver merges newer records in one SQLite transaction.
3. The receiver sends its resulting complete record set.
4. The caller performs the same merge locally.

Repeating a sync makes no further changes. Any device can act as the receiver;
there is no central cloud service.

Without `--peer`, Tracker runs `tailscale status --json`, takes the private IPv4
address of each online peer, and probes TCP port 7789 for the Tracker health
marker. Probes run concurrently and only identified Tracker servers receive sync
requests. No peer names, URLs, IPs, or Tailscale identifiers are stored in the
database.

The first sync pass gathers records from every reachable server. When more than
one server responds, a second pass sends the resulting union back to each one so
all servers converge during the same command.

The API is intentionally versioned. Incremental cursors can be added in a future
version without changing `/v1/sync`.

## Security boundary

- Automatic serving binds to the IPv4 address reported for the local device by
  the running Tailscale daemon, never to every interface.
- Tailnet encryption and access-control grants form the default network and
  authorization boundary.
- An optional `TRACKER_SYNC_TOKEN` adds bearer authentication. Tokens shorter
  than 32 bytes are rejected and compared in constant time.
- A manual non-loopback `--bind` is rejected unless that token is configured.
- Request bodies are limited to 16 MiB.
- The token is read only from the environment; it is not saved to the database
  or accepted as a command-line option.

The database itself is not encrypted. Its protection depends on operating-system
file permissions and full-disk encryption. Without an application token, any
tailnet identity permitted to reach port 7789 can read and alter all records.
Tailnet grants, the optional token, and device hygiene remain important.

## macOS application

The native SwiftUI app is a thin client over the Rust executable:

- `tracker snapshot` returns tasks, the current timer, and entries as versioned
  JSON for structured reads.
- Start, stop, and sync actions invoke the corresponding CLI commands.
- Both clients therefore use the same database path, merge rules, and Tailscale
  discovery without duplicating persistence logic in Swift.

The app owns only presentation preferences in `UserDefaults`: theme, density,
calendar range, week layout, and autosync cadence. Its root observable store
coordinates the CLI actor, while focused subviews render timer controls, the
week timeline, menu-bar content, and settings.

A production app bundle can include a universal Rust `tracker` executable in its
resources. During development the app also locates Homebrew and Cargo-installed
copies. A future direct C or UniFFI bridge could remove process boundaries, but
the JSON command boundary is deliberately small, inspectable, and compatible
with the existing standalone CLI.
