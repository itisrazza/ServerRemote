# Roadmap

Done:

- ~~Wire `DummyConnector` into the GUI end to end~~ — a connection tab renders
  the streamed framebuffer and drives a capability-driven toolbar (the dummy's
  `Reset gradient` action, among others). Closing the tab disconnects the
  session.
- ~~First real protocol: VNC, end to end~~ — connects, renders (Raw +
  CopyRect composited into a full framebuffer), and takes mouse (position +
  buttons, coordinates scaled to the framebuffer) and keyboard (X11 keysyms)
  input. Not complete: see VNC polish under Next.
- ~~Connector-contributed UI~~ — connectors add toolbar menus, links, chips
  and status bar items as data (see [Capabilities and status](capabilities.md)),
  with confirmation dialogs and a native file picker for actions that need
  them. The Dummy connector demonstrates every kind.

In progress:

- Spice, on top of `spice-rs` — connects, renders, and takes mouse (absolute
  position + buttons) and keyboard input.

Next:

1. iLO, starting soon — effectively from-scratch protocol work, against the
   [iLO 4 protocol notes](../protocols/ilo4.md). Record a real iLO 4's
   traffic and build a mock iLO server that replays it, so the connector is
   tested without hardware (see
   [Testing during implementation](../protocols/ilo4.md#testing-during-implementation)).
2. Real TLS transport (`rustls` / `tokio-rustls`) with the trust-on-first-use
   flow, driven from a UI prompt.
3. VNC polish for real networks: richer encodings (ZRLE/Tight), TLS and more
   authentication types, and damage updates instead of full-frame snapshots;
   nearest-neighbour image filtering.
4. Pointer capture for `PointerMode::Relative`, and grow the `Control` vocabulary
   (e.g. `Toggle`/`Choose`) as protocols need it.
