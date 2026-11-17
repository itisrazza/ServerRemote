# Server Remote

[![CI](https://github.com/itisrazza/ServerRemote/actions/workflows/ci.yml/badge.svg)](https://github.com/itisrazza/ServerRemote/actions/workflows/ci.yml)

A universal "server" remote — one client that aims to connect to as many remote
desktop and console protocols as possible, focused on the ones that lack a good
client on the platforms I actually use (macOS in particular).

> **Status: early, experimental.** Dummy and VNC work end to end; Spice and iLO
> are planned but not started. Interfaces will keep changing.

## Why

Most protocols already have decent clients somewhere. This project targets the
gaps:

- **Spice** — no good native client on macOS.
- **iLO Remote Console** — the vendor client is Windows-only (Linux via Wine),
  and older iLO hardware predates the HTML5 console.
- **VNC** and a **dummy** source round it out as a known-good baseline and a
  server-free way to exercise the UI.

## Protocol support

| Protocol | Status |
|---|---|
| Dummy (test pattern) | ✅ Works |
| VNC / RFB | ✅ Works — render, mouse, keyboard |
| Spice | ⏳ Planned |
| HPE iLO Remote Console | ⏳ Planned |

VNC currently negotiates None / VNC-password auth over plain TCP, using Raw and
CopyRect encodings. Richer encodings, TLS, and damage updates are on the roadmap.

## Build and run

Requires a recent Rust toolchain (edition 2024, Rust ≥ 1.85).

```sh
cargo run -p server-remote
```

Pick a protocol, enter `host` or `host:port` and a password if needed, then
connect. VNC's default port is 5900.

## Layout

It's a Cargo workspace:

- `server-remote` — the [iced](https://iced.rs) GUI front end.
- `server-remote-core` — protocol-agnostic traits and types (UI-free), plus the
  dummy connector.
- `server-remote-vnc` — VNC/RFB, on top of [`vnc-rs`](https://crates.io/crates/vnc-rs).
- `server-remote-spice`, `server-remote-ilo` — placeholders for now.
- `server-remote-icons` — the app's icon set, embedded at build time, with
  optional `iced` integration.

See [docs/architecture.md](docs/architecture.md) for how connectors, sessions,
and the capability-driven UI fit together.

## Credits

Icons are from IBM's [Carbon Design System](https://carbondesignsystem.com/elements/icons/library/)
([`carbon-design-system/carbon`](https://github.com/carbon-design-system/carbon)),
licensed under the [Apache License 2.0](LICENSE-APACHE).

## Licence

Dual-licensed under either of

- MIT ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option.
