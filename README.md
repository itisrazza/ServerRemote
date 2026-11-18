<div align="center"><img src="logo.svg" alt="" width="64" height="64"></div>
<h1 align="center">Server Remote</h1>
<div align="center">
  <a href="https://github.com/itisrazza/ServerRemote/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/itisrazza/ServerRemote/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://github.com/itisrazza/ServerRemote/actions/workflows/build.yml"><img alt="Build" src="https://github.com/itisrazza/ServerRemote/actions/workflows/build.yml/badge.svg"></a>
</div>

A universal "server" remote — one client that aims to connect to as many remote
desktop and console protocols as possible, focused on the ones that lack a good
client on the platforms I actually use (macOS in particular).

> **Status: early, experimental.** VNC works end to end but isn't complete;
> Spice is in progress; iLO is starting soon. Interfaces will keep changing.

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
| VNC / RFB | 🚧 Works end to end (render, mouse, keyboard), not complete |
| Spice | 🚧 In progress |
| HPE iLO Remote Console | ⏳ Starting soon |

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

- `apps/`
  - `server-remote` — the [iced](https://iced.rs) GUI front end.
- `connectors/` — one crate per protocol, implementing the core `Connector`.
  - `vnc` (`server-remote-vnc`) — VNC/RFB, on top of [`vnc-rs`](https://crates.io/crates/vnc-rs).
  - `spice` (`server-remote-spice`) — Spice, on top of `lib/spice`.
  - `ilo` (`server-remote-ilo`) — placeholder; work starts soon.
- `lib/`
  - `core` (`server-remote-core`) — protocol-agnostic traits and types
    (UI-free), plus the dummy connector.
  - `spice` (`spice-rs`) — Spice protocol client library.
  - `icons` (`server-remote-icons`) — the app's icon set, embedded at build
    time, with optional `iced` integration.

`website/` is the landing page, published to GitHub Pages (see its
[README](website/README.md)).

See [docs/architecture/](docs/architecture/README.md) for how connectors, sessions,
and the capability-driven UI fit together.

## Development and releases

Work happens on branches merged into `main` by PR. Releases are cut as
`release/vX.Y` branches, whose builds are release candidates; a validated
candidate is tagged, and the macOS build is signed and notarised for the
release. See [docs/releasing.md](docs/releasing.md).

## Translations

The app's text lives in [Fluent](https://projectfluent.org/) files under
[`apps/server-remote/i18n/`](apps/server-remote/i18n/), one folder per
language: English (the fallback) and Romanian. To add a language, copy
`en/server-remote.ftl` into a new folder named after its language tag, add it
to `Language` in `persistence/settings.rs` and to `TRANSLATIONS` in
`i18n.rs`; the tests check that every translation has exactly the English
messages. Text supplied by connectors (toolbar menus, status details) isn't
translated yet.

## Credits

Icons are from IBM's [Carbon Design System](https://carbondesignsystem.com/elements/icons/library/)
([`carbon-design-system/carbon`](https://github.com/carbon-design-system/carbon)),
licensed under the [Apache License 2.0](LICENSE-APACHE).

The UI font is [Nunito Sans](https://github.com/googlefonts/NunitoSans) by the
Nunito Sans Project Authors, licensed under the
[SIL Open Font License 1.1](apps/server-remote/assets/fonts/OFL.txt). The
embedded static instances are generated from the variable font by
[`scripts/instance-fonts.py`](scripts/instance-fonts.py).

The About page's contributor list and avatars are fetched from GitHub by
[`scripts/fetch-contributors.py`](scripts/fetch-contributors.py); rerun it to
refresh them.

## Licence

Dual-licensed under either of

- MIT ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option.
