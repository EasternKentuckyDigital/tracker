# Tracker

Tracker is a small, local-first time tracker written in Rust. It is designed for
people who want a fast CLI, their own data, and private device-to-device
synchronization through a Tailscale tailnet instead of a hosted account.

The current release is an MVP. It provides:

- reusable tasks with an optional project and tags;
- one-command start, stop, status, and reporting;
- local SQLite storage with no account or telemetry;
- peer-to-peer, idempotent record synchronization;
- a localhost-only sync server with bearer-token authentication;
- a Rust library that a future native macOS menu-bar app can reuse.

The macOS GUI is not implemented yet. See [Architecture](docs/architecture.md)
for the intended boundary between the GUI and the shared Rust core.

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
tracker start "Implement sync" --project tracker --tag rust
tracker status
tracker stop
tracker report
tracker report --since 7d
```

`tracker report --since` accepts `today`, a number of days such as `30d`, or an
RFC 3339 timestamp. Run `tracker --help` or `tracker <command> --help` for the
full command reference.

## Private sync with Tailscale

Every trusted device needs Tracker, Tailscale, and the same strong sync token.
Generate the token once and transfer it to your other devices through a secure
channel:

```sh
openssl rand -hex 32
export TRACKER_SYNC_TOKEN='paste-the-generated-value-here'
```

Do not commit the token, put it directly in a shell command argument, or share it
with anyone who should not be able to read and update your time records.

On the device that will receive sync requests, start Tracker:

```sh
tracker serve
```

It listens on `127.0.0.1:7789` by default. In another terminal, publish that local
port only inside your tailnet with [Tailscale Serve]:

```sh
tailscale serve 7789
```

Tailscale prints a private HTTPS URL. On another trusted device, using the same
token, save it as a peer and synchronize:

```sh
tracker peer add home https://the-private-name-from-tailscale
tracker sync
```

Saved peers synchronize automatically after `tracker stop`. Use
`tracker stop --no-sync` when deliberately offline, and run `tracker sync` later
to retry. `tracker sync --peer URL` performs a one-off sync without saving the
address.

This approach keeps Tracker bound to localhost, gives the connection Tailscale's
HTTPS endpoint and encrypted tailnet transport, and still requires the
application token. Do **not** use Tailscale Funnel: Funnel is intended for public
internet access. Tailscale access-control [grants] can further restrict which
users or tagged devices may reach the serving device.

For systems where Tailscale Serve is unavailable, Tracker can bind directly to a
Tailscale address:

```sh
tracker serve --bind TAILSCALE_ADDRESS:7789
tracker peer add home http://TAILSCALE_ADDRESS:7789
```

Verify that the address belongs only to the Tailscale interface and restrict TCP
port 7789 with your tailnet policy. Never use `0.0.0.0:7789` on an internet-facing
host.

Synchronization exchanges the complete local dataset, which is appropriate for
a small personal time-tracking database. A later release can add incremental
sync cursors and background retry.

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

[Tailscale Serve]: https://tailscale.com/docs/features/tailscale-serve
[grants]: https://tailscale.com/docs/features/access-control/grants
