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
  one per frame re-uploads every frame and strobes at large sizes. Streaming
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

## Custom widgets

- `widget::shader::Shader` has no `operation()` hook, so it can't take
  keyboard focus. A widget that must own focus (like `KvmDisplay`) has to
  implement `advanced::Widget` itself.
- A `responsive` content closure gets the available size. The Settings tab
  uses it to switch to stacked layouts below a width threshold.

## Themes and system

- `Theme::custom_with_fn` builds a new `Arc` per call, and `theme()` is
  called often, so build each theme once (`LazyLock` in `ui/style.rs`).
- `iced::system::theme()` (task) and `theme_changes()` (subscription) give
  the OS light/dark mode. `Mode::None` means unknown; the app treats it as
  dark. Linux detection is on via iced's default `linux-theme-detection`
  feature.
- The boot task runs only *after* the main window opens, so window queries
  (`window::latest()`, `scale_factor`) work there.
