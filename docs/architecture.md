# Architecture

> Status: early but working. Dummy, VNC and Spice connect end to end (render,
> mouse, keyboard); iLO is not started. Expect the shape to keep shifting as it
> lands.

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
  `Resized { .. }`, `Capabilities(..)` (mid-session change), `Status(..)` (a
  fresh status snapshot), `Disconnected { reason }`.
- **`InputEvent`** — `PointerMoved` (absolute), `PointerMotion` (relative
  deltas), `PointerButton`, `Key { keysym, .. }` (X11 keysyms, matching what RFB
  and Spice expect).
- **`SessionCommand`** — non-input requests, currently `Action { id, value }`
  to invoke a declarative action, with the `ActionValue` its control collected
  (e.g. a picked file).
- **`Frame`** — a decoded framebuffer, RGBA8, row-major, `width * height * 4`
  bytes. Full-frame for now; regional/damage updates are a later optimisation.

### Capabilities and status

The connection's toolbar and status bar are **capability-driven**: a session
describes what it offers and what's going on, and the UI renders from that
rather than hardcoding per-protocol UI. Everything is **data**, so `core` and
every protocol crate stay UI-free.

Two snapshots, each sent whole whenever it changes:

- **`Capabilities`** — what the session can do. `pointer` (absolute vs
  relative) decides how the UI sends pointer input; `input_actions` (e.g.
  Ctrl+Alt+Del) go in the Input menu; `toolbar` holds the connector's own
  toolbar entries: `Menu`s of actions (Power, Virtual media) and `Link`s the UI
  opens in the browser itself (a management processor's web interface).
- **`Status`** — what's going on. `encryption` and cumulative `traffic`
  counters are common to every protocol (the UI turns the counters into rates);
  `items` are protocol-specific status bar entries — coloured `Indicator`s
  ("Powered on", "Health") or `Text` ("POST: 5000"); `chips` are ongoing things
  shown in the toolbar (a mounted image), optionally dismissable via an action;
  `details` add rows to the connection info popout.

An **`Action`** has an `id`, `label`, optional `icon`, `checked` and `enabled`
flags, a `Control` saying how to collect input (`Button`, or `File` to open the
native file picker), and an optional `Confirmation` that makes the UI ask in a
modal dialog first. The UI reports the invoked action back as a
`SessionCommand::Action { id, value }`; the connector recognises its own id.

Icons are either one of the app's own (`server-remote-icons`) or image data the
connector supplies — an SVG, tinted like the built-in icons, or a raster image.

Recording is deliberately **not** a capability: the app records any session
itself, since it already sees every frame and every input event it sends.

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
   (e.g. `Toggle`/`Choose`) as protocols need it.
4. iLO — effectively from-scratch protocol work.

[`Target`]: ../lib/core/src/transport.rs
