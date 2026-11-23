# Sessions

Everything here lives in `server-remote-core`, which doesn't depend on iced.

## `Connector`

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

## `SessionHandle`

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
  fresh status snapshot), `Disconnected { reason }`. A `Disconnected` (or
  connect error) before the first `Connected` closes the tab and shows the
  reason in a **Failed to connect** dialog on the page the connection was
  started from; one after it stays in the tab.
- **`InputEvent`** — `PointerMoved` (absolute), `PointerMotion` (relative
  deltas), `PointerButton`, `Key { keysym, .. }` (X11 keysyms, matching what RFB
  and Spice expect).
- **`SessionCommand`** — non-input requests, currently `Action { id, value }`
  to invoke a declarative action, with the `ActionValue` its control collected
  (e.g. a picked file).
- **`Frame`** — a decoded framebuffer, RGBA8, row-major, `width * height * 4`
  bytes. Full-frame for now; regional/damage updates are a later optimisation.

## Threading model

- Each session runs on its own background async task (tokio). It owns the socket
  and the protocol state machine.
- Communication with the UI is exclusively through the `SessionHandle` channels.
  No shared mutable state, no locks across the boundary.
- `server_remote_app::connection::run` connects, hands the app the session's
  own `input`/`commands` senders, then pumps events out until the session
  ends. The app sends input and commands straight down those senders. The
  desktop app runs it on an iced `Subscription`, the iPadOS app's core on a
  tokio runtime of its own (see [Front ends and the app core](front-ends.md)).
