# Architecture

> Status: early but working. Dummy and VNC work end to end (render, mouse,
> keyboard); Spice connects and takes input but is still in progress; iLO is
> not started. Expect the shape to keep shifting as it lands.

## Goal

ServerRemote is a universal "server" remote — one client that connects to as
many remote-desktop / console protocols as possible. The focus is protocols the
author actually uses that lack a good client on the platforms they use (macOS in
particular):

- **Dummy** and **VNC** — known-good, kept as a baseline and reference.
- **Spice** — no good macOS client exists.
- **iLO** — the .NET Remote Console is Windows-only (Linux via Wine), and the
  target server predates the HTML5 console, so the web UI isn't an option.

A previous Java/Swing attempt was abandoned over TLS trouble with iLO; Rust was
chosen for tighter control of the TLS stack.

## Two axes

The design keeps two concerns separate:

- **Transport** — how a byte stream reaches the endpoint: a direct TCP
  connection, TLS over that, or (later) a tunnel over SSH.
- **Protocol** — what is spoken over that byte stream once connected: VNC,
  Spice, iLO, ...

Keeping these orthogonal means SSH support becomes "wrap the transport" rather
than a change to every protocol implementation. Today only a direct connection
to a [`Target`] exists; the seam lives in `server-remote-core::transport`.

## Crate layout

| Crate | Role |
|---|---|
| `server-remote` | iced GUI front end. |
| `server-remote-core` | Protocol-agnostic traits and types, plus the Dummy implementation. Does **not** depend on iced. |
| `server-remote-vnc` | VNC / RFB — implemented, on top of the `vnc-rs` crate. |
| `server-remote-spice` | Spice — connects and renders, on top of `spice-rs`. |
| `spice-rs` (`lib/spice`) | Spice protocol client library: wire format, channels, framebuffer. |
| `server-remote-ilo` | HPE iLO Remote Console — not started. |
| `server-remote-icons` (`lib/icons`) | The app's icon set — [Carbon](https://github.com/carbon-design-system/carbon) icons (Apache-2.0) plus custom ones, exported from the Figma design into `assets/` and embedded with `include_bytes!`. Each icon is an `Icon` constant; the `iced` feature adds `Icon::element()`, which builds an `iced::widget::svg::Svg`. |

Workspace: edition 2024, resolver 3, iced 0.14 (`advanced` feature) on the front
end.

## In this folder

- [Sessions](sessions.md) — how a protocol opens a session, the channels
  between it and the UI, and the threading model.
- [Capabilities and status](capabilities.md) — how a session describes what it
  offers and what's going on, and the UI connectors contribute through that.
- [TLS trust](tls.md) — connecting to targets with self-signed or expired
  certificates without switching verification off.
- [Roadmap](roadmap.md) — what's done and what's next.

[`Target`]: ../../lib/core/src/transport.rs
