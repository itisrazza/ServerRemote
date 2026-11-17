---
name: ui-change-checklist
description: Checklist for adding or changing anything visible in the ServerRemote iced app — new screens, controls, settings, menu items, dialogs, icons or text. Use before writing UI code and again before calling UI work done.
---

# Changing the UI

The app has two themes, two languages and two common display densities. Most
UI bugs here come from building for one of each. Work through this list.

## While building

- **Colours:** use palette roles only (`theme.extended_palette()`); the roles
  are listed above `extended_palette` in `ui/style.rs`. Put any new look in a
  style function in `style.rs`. A literal colour will be wrong in either the
  dark or the light theme.
- **Text:** every user-facing string is `fl!("id")`. Add the message to
  `i18n/en/server-remote.ftl` *and* `i18n/ro/server-remote.ftl`. Use Fluent
  variables (`{ $count }`) and plural selectors, never `format!` around
  translated fragments. Romanian has three plural categories: `one` (1),
  `few` (0, 2–19, decimals) and `other` (20 and up). `*[other]` covers
  `few` when the two forms read the same; otherwise add a `[few]` arm.
  Leave names literal (ServerRemote, VNC, DirectX 12, QuickTime). Language
  names are written in their own language ("Română").
- **Types:** because text is looked up at runtime, labels are `String`. If a
  helper takes `&'static str`, change it to take `String`; don't leak.
- **Sizes:** use `style::TEXT_SIZE` (13), `style::CONTROL_HEIGHT` (28),
  `style::SPACING` (4) and `style::ICON_SIZE` (16). Buttons are
  `CONTROL_HEIGHT` tall with their label centred in a `container`.
- **Fixed widths:** menus are `MENU_WIDTH` (200) wide and wrap long labels
  onto the next item. Romanian labels often run longer than English, so
  check menus in Romanian, and prefer a shorter translation to a wider menu.
- **Icons:** Carbon SVGs go in `lib/icons/assets/` (16×16 viewBox or scaled,
  `fill="white"`) and are listed in `lib/icons/src/lib.rs`. Copy the path
  data exactly from the Carbon source; don't retype it. Always tint
  (`ui::themed_icon`), since an `Svg` renders black otherwise.
- **Settings:** a new setting is a field in `persistence/settings.rs` (with a
  default; files from older versions must still load), a `Change` variant and
  `apply` arm in `ui/settings.rs`, and a `setting(...)` row. Add a case to
  `changes_apply_to_their_setting`. Something that must take effect
  immediately (theme, language) is handled where the app applies changes
  (`Application::change_setting`) or read in `Application::theme`.
- **Dialogs** go through `ui::dialog::view`; app-level ones are an
  `AppModal` variant in `main.rs`, connection-level ones a `Modal` in
  `ui/connection.rs`.
- **App-wide effects from a connection tab** (opening Settings, changing a
  setting, closing the tab, picking a folder) are `connection::Message`
  variants that `Application::update` intercepts before forwarding; the
  connection's own `update` only lists them for exhaustiveness.
- **Connector-contributed UI** (toolbar menus, links, chips, status bar
  items) is declared as data in `lib/core` (`Capabilities`, `Status`) and
  rendered in `ui/contributed.rs`. Don't special-case a protocol in the UI;
  add to the vocabulary, and show it off in the Dummy connector.
- **Narrow windows:** settings rows stack below a width threshold, and
  switches always stay beside their label. Check new layouts narrow too.
- **Raster images:** create each `image::Handle` once and reuse it; one
  made inside `view` never draws (see `iced-widget-gotchas`).
- **Generated files:** don't hand-edit `assets/contributors/generated.rs` or
  the fonts; rerun their scripts in `scripts/`.

## Before calling it done

1. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo test --workspace`.
2. Add or extend a state in `render_review_snapshots`, render it, and look
   at it in **dark, light and Romanian**, and narrow if it's a layout that
   reflows (see the `ui-mockup-match` skill for the command and measuring
   tool).
3. Think about **1× displays**: the layout is snapped to physical pixels
   (`ui::pixel_snap` at the root), but anything you draw at a computed offset
   inside a widget isn't. Keep borders and lines on layout edges.
4. In the hand-off, name what can't be judged headlessly (animation, hover
   feel, real 1× sharpness) so the user knows what to look at.
