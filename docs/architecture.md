# Architecture

> Status: early but working. Dummy and VNC connect end to end (render, mouse,
> keyboard); Spice and iLO are not started. Expect the shape to keep shifting as
> those land.

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
| `server-remote-spice` | Spice — not started. |
| `server-remote-ilo` | HPE iLO Remote Console — not started. |
| `server-remote-icons` (`crates/icons`) | The app's icon set — [Carbon](https://github.com/carbon-design-system/carbon) icons (Apache-2.0) plus custom ones, exported from the Figma design into `assets/` and embedded with `include_bytes!`. Each icon is an `Icon` constant; the `iced` feature adds `Icon::element()`, which builds an `iced::widget::svg::Svg`. |

Workspace: edition 2024, resolver 3, iced 0.14 (`advanced` feature) on the front
end.

## Core abstractions

Everything hangs off `server-remote-core`.

### `Connector`

A protocol is a `Connector`: a cheap, reusable descriptor (zero-sized, or just
holding credentials) that opens sessions. The live connection does **not** live
in the connector — it lives behind the handle it returns.

```rust
pub trait Connector {
    fn name(&self) -> &'static str;

    fn connect(
        &self,
        target: &Target,
        policy: &TlsPolicy,
    ) -> impl Future<Output = Result<SessionHandle, ConnectError>> + Send;
}
```

`connect` is `async` (desugared to `impl Future<…> + Send` so the future can be
driven on a multi-threaded runtime). TLS policy is passed in explicitly;
protocols without TLS ignore it.

### `SessionHandle`

A running session is represented by a set of channels, not a `dyn Session`
object. The protocol implementation owns the connection on a background task and
holds the far end of every channel; the UI holds this handle.

```rust
pub struct SessionHandle {
    pub events:   mpsc::Receiver<SessionEvent>,  // session -> UI
    pub input:    mpsc::Sender<InputEvent>,      // UI -> session (hot path)
    pub commands: mpsc::Sender<SessionCommand>,  // UI -> session (structured)
}
```

Input and commands are separate channels on purpose: input is a high-frequency
hot path, commands are rare and structured. Dropping every sender (and the
handle's `events`) signals shutdown. This gives type erasure for free (all
sessions are the same concrete type) and keeps `core` free of any UI dependency.

- **`SessionEvent`** — `Connected { size, capabilities }`, `Frame(Frame)`,
  `Resized { .. }`, `Capabilities(..)` (mid-session change), `Disconnected {
  reason }`.
- **`InputEvent`** — `PointerMoved` (absolute), `PointerMotion` (relative
  deltas), `PointerButton`, `Key { keysym, .. }` (X11 keysyms, matching what RFB
  and Spice expect).
- **`SessionCommand`** — non-input requests, currently `Action { id }` to invoke
  a declarative capability.
- **`Frame`** — a decoded framebuffer, RGBA8, row-major, `width * height * 4`
  bytes. Full-frame for now; regional/damage updates are a later optimisation.

### Capabilities

The connection toolbar is **capability-driven**: a session reports what it
supports and the UI renders from that, rather than hardcoding per-protocol
buttons.

```rust
pub struct Capabilities {
    pub pointer: PointerMode,   // Absolute | Relative
    pub actions: Vec<Action>,   // protocol-specific, declarative
}
```

Common capabilities are typed fields (e.g. `pointer`, which decides whether the
UI sends absolute positions or grabs the pointer and sends deltas).
Protocol-specific ones are **declarative `Action`s** — an `id`, a `label`, and a
`Control` from a small vocabulary (`Button` today; `File`/`Toggle`/`Choose` when
Spice/iLO media mounting lands). The UI renders them generically and reports the
invoked one back as a `SessionCommand::Action { id }`; the plugin recognises its
own id. This lets a protocol contribute toolbar UI **as data**, so `core` and
every protocol crate stay UI-free.

If the declarative vocabulary can't express something, the plan is an **opt-in**
trait for a plugin to provide its own iced UI — kept opt-in so protocols that
don't need it pull no UI dependency. Not built yet.

### TLS trust

Some targets (iLO especially) ship self-signed or expired certificates, so
connecting insecurely must be possible — but never as a blanket "skip
verification" switch. Instead it is trust-on-first-use:

```rust
pub enum TlsPolicy {
    Secure,
    AllowInsecure { accepted: Option<CertFingerprint> },
}
```

When `accepted` is `None`, a connector that hits an invalid certificate returns
`ConnectError::UntrustedCertificate(fingerprint)`. The UI surfaces that
fingerprint, the user approves it, and the approved fingerprint is what gets
trusted on the next attempt — a specific certificate, not "anything goes".

## Threading model

- Each session runs on its own background async task (tokio). It owns the socket
  and the protocol state machine.
- Communication with the UI is exclusively through the `SessionHandle` channels.
  No shared mutable state, no locks across the boundary.
- The GUI runs the connector on an iced `Subscription` whose worker owns the
  `events` receiver and hands the UI the session's own `input`/`commands`
  senders, then pumps events out as messages. The UI sends input and commands
  straight down those senders from its `update` loop.

## Roadmap

Done:

- ~~Wire `DummyConnector` into the GUI end to end~~ — a connection tab renders
  the streamed framebuffer and drives a capability-driven toolbar (`Disconnect`
  plus the dummy's `Reset gradient` action).
- ~~First real protocol: VNC~~ — connects, renders (Raw + CopyRect composited
  into a full framebuffer), and takes mouse (position + buttons, coordinates
  scaled to the framebuffer) and keyboard (X11 keysyms) input.

Next:

1. Real TLS transport (`rustls` / `tokio-rustls`) with the trust-on-first-use
   flow, driven from a UI prompt.
2. VNC polish for real networks: richer encodings (ZRLE/Tight) and damage
   updates instead of full-frame snapshots; nearest-neighbour image filtering.
3. Pointer capture for `PointerMode::Relative`, and grow the `Control` vocabulary
   (`File`/`Toggle`/`Choose`) alongside Spice/iLO media mounting.
4. Spice, then iLO — effectively from-scratch protocol work.

[`Target`]: ../crates/server-remote-core/src/transport.rs
