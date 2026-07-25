# Security audit

Audit date: 2026-07-24

## Scope and threat model

This review covers the Rust CLI and sync server, SQLite persistence, Tailscale
discovery, the SwiftUI macOS client, the CLI process bridge, local credentials,
and macOS packaging. The primary threats considered were:

- a compromised or malicious peer that can reach the sync port;
- a hostile sync response or malformed local database;
- executable or environment hijacking at the macOS process boundary;
- memory, process, or SQLite denial of service;
- local disclosure of the database or application token;
- UI spoofing through synchronized text and overbroad macOS permissions.

An authenticated peer remains a fully trusted data peer. Tracker does not yet
provide per-device roles, signed records, or end-to-end encryption above
Tailscale.

## Findings corrected

### High: last-write-wins timestamp poisoning

A peer could submit records with timestamps arbitrarily far in the future,
causing its values to win normal merges indefinitely. Sync now rejects updates,
starts, and stops more than ten minutes ahead of the receiving clock. It also
rejects inverted timestamp relationships.

### High: unbounded response and helper-process output

The server limited request bodies, but the Rust client accepted unbounded
responses and the macOS app read unbounded helper output. A hostile peer or
damaged database could exhaust memory. Both network responses and macOS helper
streams are now capped. The helper also has command timeouts and is force-killed
if it does not terminate.

### High: executable and environment hijacking

The macOS app previously fell back to `/usr/bin/env tracker` and inherited the
entire parent environment. Tailscale discovery also tried an unqualified
`tailscale` executable from `PATH`. Release builds now use only the bundled
helper, strip dynamic-loader and unrelated environment variables, and use fixed
Tailscale executable locations. Fixed development helper locations and the
`TRACKER_CLI_PATH` override are available only in debug builds.

### Medium: malformed-record and resource-exhaustion sync

Sync accepted structurally valid JSON without semantic record validation. It now
enforces record-count, string, tag, UUID, timestamp, task-ID, duplicate-ID, and
reference-integrity constraints before writing. The server limits concurrent
sync operations and caps serialized responses.

### Medium: ambiguous peer URLs and redirects

Manual peer URLs could contain credentials, paths, queries, or non-HTTP schemes.
HTTP redirects could also move a request away from the intended origin. Tracker
now accepts only an `http` or `https` origin with a host, rejects embedded
credentials and ambiguous URL components, and disables redirects.

### Medium: local secret and database protection

The GUI had no secure way to configure the optional sync token, encouraging
shell-environment workarounds. Tokens are now stored with
`kSecAttrAccessibleWhenUnlockedThisDeviceOnly` in Keychain and passed only to
the helper process. Token length and control characters are validated. SQLite
database files are created and maintained with owner-only `0600` permissions on
Unix, with trusted schema disabled, secure deletion enabled, and a bounded busy
timeout.

### Low: synchronized control-character spoofing

Names, projects, and tags could contain terminal or display control characters.
Those values are now rejected for both local creation and incoming sync.

## macOS hardening and permissions

The distribution build signs both executables with Hardened Runtime. The
sandboxed configuration grants only:

- `com.apple.security.app-sandbox`;
- `com.apple.security.network.client` on the app;
- `com.apple.security.inherit` on the embedded helper.

Tracker does not request camera, microphone, location, contacts, calendar,
photos, Bluetooth, USB, Apple Events, incoming-network, or broad file access.
Automatic Tailscale CLI discovery is incompatible with the narrow sandbox, so a
sandboxed build should set a manual peer origin in Settings. An independently
distributed, nonsandboxed Developer ID build can retain automatic discovery
while still using Hardened Runtime.

## Residual risks

- Without `TRACKER_SYNC_TOKEN`, any tailnet identity allowed to reach port 7789
  can read and modify the complete dataset.
- The token is a shared bearer secret, not a per-device credential. A trusted
  peer can still make valid in-window edits and deletions.
- Sync uses HTTP inside Tailscale rather than application-layer TLS. Tailscale
  supplies transport encryption and identity policy.
- SQLite data is not application-encrypted. File permissions, macOS account
  isolation, FileVault, and device hygiene remain the at-rest controls.
- Full-dataset sync has a 16 MiB protocol ceiling. Incremental, signed sync is
  the recommended next protocol revision.
- Public distribution still requires an Apple Developer ID signing identity and
  notarization. The locally built app is ad-hoc signed for development.

## Verification

The review uses unit tests for malformed records, timestamp poisoning, task
references, token constraints, peer URL parsing, and literal CLI arguments.
Release acceptance should run:

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo audit
swift build --package-path macos/TrackerMac
macos/TrackerMac/scripts/build-app.sh
codesign -dvvv --entitlements - macos/TrackerMac/dist/Tracker.app
```
