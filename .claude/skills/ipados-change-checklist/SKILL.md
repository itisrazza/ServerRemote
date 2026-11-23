---
name: ipados-change-checklist
description: Checklist for changing the ServerRemote iPadOS app or the FFI it uses — SwiftUI views under apps/ipados, records or calls in apps/ffi, settings, icons, or anything the iPad app shows. Use before starting and again before calling the work done.
---

# Changing the iPadOS app

The rules themselves are in
[front ends and the app core](../../../docs/architecture/front-ends.md#rules);
read them first. This is the order of work that keeps to them.

## Before writing Swift

1. **Is it behaviour?** If the change decides anything (what to save,
   whether to connect, which page shows), add or extend an `app::Event` in
   `apps/core`, with a test there, so the desktop app gets it too. Only then
   expose it.
2. **Expose it in `apps/ffi`** as a call on `Core` or a field of a record,
   converting types and nothing more. Check the new names against Swift's
   and SwiftUI's (no `Menu`, `Link`, `Color`, `Protocol`, `Image`, `Text`…).
   Return a `Result` with a specific error for anything that can fail. Add a
   test in `apps/ffi` if it does any lookup or conversion.
3. **New text** goes in `apps/core/i18n/en/server-remote.ftl` and
   `apps/core/i18n/ro/server-remote.ftl`, then `model.localize("id")` (with
   `["name": value]` arguments). Reuse an existing message where one fits.

## While writing Swift

- Design for the iPad. iPhone is best-effort: it has to keep building and
  working (no iPad-only APIs; layouts that survive a compact width), but
  iPad wins where they pull apart.

- System controls and layout: `Form`, `List`, `Menu`, `.alert`,
  `.confirmationDialog`, sheets, toolbar items (glass comes with them),
  SF Symbols for the app's own controls. No custom chrome copied from the
  desktop.
- Colour: `Color.brand` (and `Color.onBrand` on it) or the status colours;
  nothing else literal.
- Connector icons through `ConnectorIcon`; protocol icons and the logo
  through `IconView`. A new built-in icon in `lib/icons` needs an SF Symbol
  in `ConnectorIcon.symbols` (the `apps/ffi` test says which).
- A new setting: see the Settings rules; on iPad it's a row on its
  section's page in `Settings/`, or a new `SettingsSection` with its own
  file.
- No `try?`: log with `Logger` or show the error.
- Read state from `model.state` and act through `model.core`; keep `@State`
  for view state only.

## Before calling it done

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
scripts/check-ipados.sh
```

`check-ipados.sh` builds for the simulator and devices with Swift warnings
as errors, as CI does. Don't capture the simulator's screen to check the
result: say what to look at, and the user runs it from Xcode.

Update [front ends and the app core](../../../docs/architecture/front-ends.md)
and the [roadmap](../../../docs/architecture/roadmap.md) if what the app does
or how it's built changed.
