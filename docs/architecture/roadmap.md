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
- ~~Real TLS transport~~ — `rustls` / `tokio-rustls` in
  `server-remote-core::tls`, with the trust-on-first-use flow driven from a
  prompt in the connection tab (see [TLS trust](tls.md)).
- ~~Certificate checking settings~~ — Strict, Ask or Off in Settings ›
  Security, with saved trusted and blocked certificates per host and port.
  Approved certificates may use legacy 1024-bit RSA keys, as older iLOs do.
- ~~Connection manager~~ — saved connections as cards with a thumbnail of
  their last screen (the protocol's icon until there is one), filtered by
  group in a sidebar. A right click edits, regroups, exports (as TOML,
  without the password) or deletes one; Add and Import sit above the grid.

In progress:

- Spice, on top of `spice-rs` — connects, renders, and takes mouse (absolute
  position + buttons) and keyboard input.
- iLO 4, from scratch against the [iLO 4 protocol notes](../protocols/ilo4.md)
  — logs in over HTTPS, negotiates the KVM and command connections, decodes
  the video, and takes keyboard (HID reports) and absolute mouse input, with a
  Power menu, Ctrl+Alt+Del, and power, health and POST status. Tested against
  a mock iLO in CI, and the video decoder against a real capture locally.
  Still to come:
  - Settle the open questions on real hardware: colour channel order,
    keyboard and mouse, the power opcode.
  - Virtual media.
  - Seize or share a console another user holds (today that fails with an
    error).

Next:

1. VNC polish for real networks: richer encodings (ZRLE/Tight), TLS and more
   authentication types, and damage updates instead of full-frame snapshots;
   nearest-neighbour image filtering.
2. Pointer capture for `PointerMode::Relative`, and grow the `Control` vocabulary
   (e.g. `Toggle`/`Choose`) as protocols need it.
