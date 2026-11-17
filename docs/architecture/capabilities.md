# Capabilities and status

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
