# Security policy

Server Remote handles credentials and connects to management interfaces such
as iLO, so security reports are welcome.

## Reporting a vulnerability

Report it privately through
[GitHub's vulnerability reporting](https://github.com/itisrazza/ServerRemote/security/advisories/new),
not in a public issue. Include what's affected, how to reproduce it, and the
version or commit you tested.

This is a one-person hobby project, so there's no fixed response time, but
reports are acknowledged and fixed as quickly as practical.

## Supported versions

Only the latest release and `main` receive fixes.

## Known limitations

These are known, deliberate for now, and don't need a report:

- Saved passwords are stored in plain text in the app's configuration
  directory.
- VNC and Spice connections aren't encrypted yet; use them on a trusted
  network or through a tunnel.
- Self-signed and expired certificates, common on management interfaces,
  can be trusted on first use: once you approve a certificate's fingerprint,
  that certificate is accepted.

The [protocol notes](../docs/protocols/README.md) and the
[TLS design](../docs/architecture/tls.md) have the details.
