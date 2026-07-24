# Security Policy

## Reporting a vulnerability

Please report vulnerabilities privately to the repository owner rather than
opening a public issue with exploit details. A dedicated security contact has not
yet been published; GitHub's private vulnerability reporting feature is preferred
when it is enabled for this repository.

## Deployment guidance

Tracker is an early personal-use project and has not received an independent
security audit.

- Prefer the default localhost binding together with Tailscale Serve.
- Never expose the sync service with Tailscale Funnel.
- Use a unique random `TRACKER_SYNC_TOKEN` of at least 32 bytes.
- Restrict the serving device and port using Tailscale grants.
- Protect the local database with OS permissions and full-disk encryption.
- Rotate the token if a device or shell environment may have been compromised.

The sync token authenticates a peer but does not distinguish read access from
write access. Treat every device holding it as fully trusted.
