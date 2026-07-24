# Architecture

Tracker is local-first: CLI commands always read or write the local SQLite
database and never require a network connection.

```text
CLI now ─────────┐
                 ├── tracker Rust library ── SQLite database
macOS GUI later ─┘             │
                               └── authenticated sync API
                                          │
                                 Tailscale Serve/tailnet
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

Peer names and URLs are local configuration and are never sent to other devices.
`tracker stop` attempts each saved peer after it commits the stopped timer
locally. A failed network request therefore cannot lose tracked time; explicit
`tracker sync` can retry later.

The API is intentionally versioned. Incremental cursors can be added in a future
version without changing `/v1/sync`.

## Security boundary

- Tracker binds to localhost unless the operator explicitly chooses another
  address.
- Every endpoint requires `Authorization: Bearer ...`.
- Tokens shorter than 32 bytes are rejected and compared in constant time.
- Request bodies are limited to 16 MiB.
- Tailscale Serve is the recommended transport because it exposes the localhost
  service only within the tailnet and provides HTTPS.
- The sync token is read only from `TRACKER_SYNC_TOKEN`; it is not saved to the
  database or accepted as a command-line option.

The database itself is not encrypted. Its protection depends on operating-system
file permissions and full-disk encryption. Anyone with the token and network
access to a running server can read and alter all records, so token rotation,
tailnet grants, and device hygiene remain important.

## macOS GUI direction

The GUI should be a thin native client over the existing Rust library. A practical
next step is a menu-bar application with:

- the current timer and a start/stop action;
- recent and saved task selection;
- project/tag editing;
- sync status and an explicit sync action;
- a small report window.

The GUI should call library functions directly against the same database rather
than shelling out to the CLI. A C-compatible bridge or UniFFI can expose the Rust
core to Swift while keeping macOS presentation and Keychain token storage native.
