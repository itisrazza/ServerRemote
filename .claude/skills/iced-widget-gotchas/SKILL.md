---
name: iced-widget-gotchas
description: Known iced 0.14 behaviours and traps for ServerRemote — writing custom widgets or wrappers, overlays, layout, images, SVG icons, text sizing, themes and system integration. Use when writing or debugging anything under apps/server-remote/src/ui that goes beyond composing stock widgets, or when iced behaves unexpectedly.
---

# iced 0.14 gotchas

**Read the source, not memory or docs.** The exact versions are in
`~/.cargo/registry/src/*/iced_*-0.14.*` (check `Cargo.lock` for which ones).
Most of the facts below were only found there. Search it with `grep` before
designing around an assumption.

## Wrapping the whole view

`ui::arrow_cursor` and `ui::pixel_snap` are the pattern for a widget that
changes behaviour across the whole app:

- Delegate *everything* to the content (`tag`, `state`, `children`, `diff`,
  `size`, `size_hint`, `layout`, `operate`, `update`, `draw`,
  `mouse_interaction`), so the widget tree is unchanged.
- Also wrap what `overlay()` returns, recursively (the wrapper's own
  `overlay()` wraps nested overlays too). Dropdowns and popovers live in
  overlays and are missed otherwise.
- Overlay layouts are in window coordinates. Widget layouts are relative to
  their parent, so a wrapper that needs absolute positions (like
  `pixel_snap`) must be the root of the view.
- Keep both root wrappers outermost in `Application::view`; anything
  outside them (a stacked dialog, say) loses pixel snapping and the cursor
  fix.
- Cursors: `mouse_area(..).interaction(..)` only applies where its child
  reports `Interaction::None`, so it can't override a button's pointing hand.
  `arrow_cursor` maps `Pointer` to `Idle` at the root instead.

## Overlays and dialogs

- Dropdowns are `ui::popover` overlays. iced updates nested overlays
  innermost first and hands outer ones an unavailable cursor while the inner
  one is under it, so a submenu passes no dismiss message: it swallows clicks
  on itself and leaves outside clicks to the top-level menu (otherwise
  clicking a parent entry with a submenu open closes everything instead).
- `stack![..]` takes its size from its first layer. A dialog backdrop
  stacked over a view only covers that view, so the base must fill the
  window (`Length::Fill` both ways), or the backdrop stops short on short
  pages.

## Layout and crispness

- iced lays out in fractional logical pixels: centring, text widths
  (13px × 1.3 line height = 16.9px) and fill portions all land between
  pixels. `pixel_snap` rounds every node's edges to physical pixels using the
  window's scale factor (from `window::scale_factor` at boot plus
  `Rescaled` events). Round both edges of a box independently, so
  neighbours that touch keep touching.
- Text is snapped vertically by the text engine (cosmic-text truncates y)
  but positioned horizontally in quarter-pixel steps, so centred text can
  sit slightly between pixels. Text clips to the viewport, not its own
  bounds, so rounding a text node's edges never cuts glyphs.
- Scroll offsets aren't snapped; trackpad scrolling can stop mid-pixel.
- `Row`/`Column::align_y` aligns line boxes, not baselines (see
  `style::baseline`).
- A vertical `scrollable` sets its content's minimum width to its own, so a
  fixed- or max-width column inside one is stretched. Put the column in a
  full-width `container` inside the scrollable.
- A container's padding is measured from its outer edge and its border sits
  inside it: 8px of space inside a 1px border is `padding(9)`.
- `push_maybe` is gone in 0.14; `push` accepts an `Option<impl Into<Element>>`.

## Text and fonts

- The renderer's default text size (`Settings::default_text_size`) is fixed
  when the renderer starts. It can't change per window or scale factor short
  of setting an explicit size on every widget.
- Fonts are matched by family, weight and stretch. Stretch comes from the
  OS/2 width class, so a static instance must set the class to match its
  `wdth` (see `scripts/instance-fonts.py`).
- Fluent wraps arguments in Unicode isolation marks by default, which draw
  as boxes; `i18n.rs` turns that off.

## Images and SVGs

- `image::Handle::from_rgba`/`from_bytes` get a fresh ID per call, so making
  one per frame re-uploads every frame and strobes at large sizes. Small
  connector-supplied raster icons are cached by content hash in
  `ui::icon::Cache`, refreshed when capabilities or status arrive.
  (`svg::Handle::from_memory` hashes its data, so SVG handles are stable and
  can be built per frame.) Streaming
  content (the remote display, `ui/kvm_display.rs`) uses a persistent `wgpu`
  texture drawn through `renderer.draw_primitive()`, which is why the app
  depends on `iced_wgpu` directly.
- Raster images decode on a worker thread: the first frame after creating a
  handle shows nothing. Static images are fine; just don't test for them in
  a single-frame snapshot.
- `image::Image::border_radius` exists in 0.14 (round avatars).
- `Svg` ignores the surrounding text colour and renders `fill="currentColor"`
  or white SVGs in the source colour (black). Always set `.style()`; see
  `ui::themed_icon`.
- `Svg<'a>` is invariant in `'a` (its style class is an associated type), so
  an `Svg<'static>` won't coerce to a shorter lifetime. Pass `Icon` values
  (or `ui::icon::Handle`) around and build the `Svg` where it's rendered.

## Custom widgets

- `widget::shader::Shader` has no `operation()` hook, so it can't take
  keyboard focus. A widget that must own focus (like `KvmDisplay`) has to
  implement `advanced::Widget` itself.
- A `responsive` content closure gets the available size. The Settings tab
  uses it to switch to stacked layouts below a width threshold.

## Headless rendering (`iced_test`)

- `Simulator` renders at 2× through wgpu. `Snapshot::matches_image(path)`
  names the file `<stem>-<renderer>.png` and only writes it when absent,
  which is why `render_review_snapshots` deletes old ones first.
- Session subscriptions don't run in the simulator; feed connector state in
  as `SessionEvent` messages (`feed_ilo_like_session`).

## Themes and system

- `Theme::custom_with_fn` builds a new `Arc` per call, and `theme()` is
  called often, so build each theme once (`LazyLock` in `ui/style.rs`).
- `iced::system::theme()` (task) and `theme_changes()` (subscription) give
  the OS light/dark mode. `Mode::None` means unknown; the app treats it as
  dark. Linux detection is on via iced's default `linux-theme-detection`
  feature.
- The boot task runs only *after* the main window opens, so window queries
  (`window::latest()`, `scale_factor`) work there.
- `application(boot, ..)` takes any `Fn() -> (State, Task)`, so `main` loads
  settings once and passes them in through a closure.
- No built-in file dialogs or URL opening: the app uses `rfd`
  (`AsyncFileDialog`, run as a `Task`) and `open::that_detached`.
- wgpu picks its graphics API (DX12, Vulkan, ...) only from the `WGPU_BACKEND`
  environment variable, which `main` sets from the Windows graphics-backend
  setting.
