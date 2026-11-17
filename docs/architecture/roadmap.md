# Roadmap

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
