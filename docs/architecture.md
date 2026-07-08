# Architecture

> Status: early sketch. This documents the shape we're building toward, not a
> finished system. Expect it to change as the first real protocol lands.

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
| `server-remote-vnc` | VNC / RFB. |
| `server-remote-spice` | Spice. |
| `server-remote-ilo` | HPE iLO Remote Console. |

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

A running session is represented by a pair of channels, not a `dyn Session`
object. The protocol implementation owns the connection on a background task and
holds the far end of both channels; the UI holds this handle.

```rust
pub struct SessionHandle {
    pub events: mpsc::Receiver<SessionEvent>, // session -> UI
    pub input:  mpsc::Sender<InputEvent>,     // UI -> session
}
```

Dropping the handle closes `input`, which the implementation treats as a request
to shut down. This gives type erasure for free (all sessions are the same
concrete type) and keeps `core` free of any UI dependency.

- **`SessionEvent`** — `Connected { size }`, `Frame(Frame)`,
  `Resized { .. }`, `Disconnected { reason }`.
- **`InputEvent`** — `PointerMoved`, `PointerButton`, `Key { keysym, .. }`
  (X11 keysyms, matching what RFB and Spice expect).
- **`Frame`** — a decoded framebuffer, RGBA8, row-major, `width * height * 4`
  bytes. Full-frame for now; regional/damage updates are a later optimisation.

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
- The GUI turns the `events` receiver into an iced subscription and sends
  `InputEvent`s from its `update` loop. (This wiring is the next piece of work
  and is not built yet.)

## Roadmap

1. Wire `DummyConnector` into the GUI end to end: a connection tab that renders
   the streamed framebuffer. Proves the `SessionHandle` ↔ iced integration.
2. Real TLS transport (`rustls` / `tokio-rustls`) with the trust-on-first-use
   flow, driven from a UI prompt.
3. First real protocol — VNC is the easiest (mature crates, simple framebuffer
   model) and a good shakedown for the abstractions before Spice and iLO, which
   are effectively from-scratch protocol work.

[`Target`]: ../crates/server-remote-core/src/transport.rs
