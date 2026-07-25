# Security Policy

## Reporting a vulnerability

Please report vulnerabilities privately to the repository owner rather than
opening a public issue with exploit details. A dedicated security contact has not
yet been published; GitHub's private vulnerability reporting feature is preferred
when it is enabled for this repository.

## Deployment guidance

Tracker is an early personal-use project. The repository received an internal
code and threat-model review on 2026-07-24; it has not received an independent
third-party penetration test. See [`docs/security-audit.md`](docs/security-audit.md)
for findings, fixes, and residual risks.

- Use the automatic Tailscale binding rather than a manual `--bind`.
- Restrict TCP port 7789 on serving devices using Tailscale grants.
- On a multi-user or less-trusted tailnet, use a unique random
  `TRACKER_SYNC_TOKEN` of at least 32 bytes on every Tracker device.
- Protect the local database with OS permissions and full-disk encryption.
- Rotate the token if a device or shell environment may have been compromised.
- Keep device clocks synchronized. Sync rejects records more than ten minutes
  ahead to prevent last-write-wins timestamp poisoning.

Tracker trusts the tailnet boundary when no token is configured. The optional
token authenticates a peer but does not distinguish read access from write
access. Treat every device permitted by the tailnet policy—and every device
holding the token, when enabled—as fully trusted.
