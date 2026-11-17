---
name: ui-mockup-match
description: Implement or adjust ServerRemote UI to match a Figma mockup (a PNG or screenshot from the user), verifying by rendering the app headlessly and measuring both images. Use whenever the user shares a mockup, asks for something to "look like this", or reports misalignment, fuzziness or wrong sizes.
---

# Matching a mockup

Mockups are Figma exports at **2×**. "Close enough by eye" isn't the bar:
measure positions, sizes and colours in the mockup and in a render of the app,
then fix the differences. Never screen-capture; everything here is headless.

## 1. Measure the mockup

`measure.py` (next to this file) runs with `uv`, no setup needed. Coordinates
are image pixels; output is also given in logical points (÷2).

```sh
M=.claude/skills/ui-mockup-match/measure.py
$M pixel mockup.png 800 300                      # colour at a point
$M box   mockup.png 530 80 680 220 --colour '#443dff' --tolerance 60
$M box   mockup.png 680 165 850 235              # anything unlike the region's background
$M glyph mockup.png 535 415 600 460              # first letter's bounds + implied font size
```

Tips from past sessions:

- **Font size:** measure a flat-topped capital with no descender (H, E, L,
  M). Round letters (S, O, C) overshoot and read about 5% large. The design
  uses 12/13px body text; a mockup's body text may be 1px smaller than the
  app's (body went from 12px to 13px for low-DPI legibility).
- **Font axes:** a font property in the mockup ("width 115", "weight 900")
  means the Nunito Sans variable axis value. iced can't set axes at runtime,
  so add a static instance to `scripts/instance-fonts.py`, generate it from
  the upstream variable font (google/fonts `ofl/nunitosans`), and add a
  `Font` constant in `ui/style.rs`. `Stretch` is matched through the OS/2
  width class, so give the instance the right class (115 → 6, SemiExpanded).
- **Colours** map to palette roles; find the closest role in the list above
  `extended_palette` in `ui/style.rs` rather than hard-coding the value.
- **Icons in the mockup** carry padding inside their 16×16 box (Carbon icons
  usually leave 1–2px), so a visible edge isn't the icon's layout edge.
- **Baselines:** text of different sizes side by side should share a
  baseline. iced aligns line boxes, not baselines, so offset the smaller text
  by `style::baseline(big) - style::baseline(small)`.
- **Ask, don't guess:** mockups can be inconsistent (only one row of keys was
  bold once). Ask before treating an outlier as a slip, and ask what a
  mockup leaves out (missing controls, link targets, where removed features
  went).

## 2. Render the app

```sh
SNAPSHOT_DIR=<scratch dir> cargo test -p server-remote render_review_snapshots -- --ignored
```

This writes `<name>-wgpu.png` at 2× for every state listed in
`render_review_snapshots` (`apps/server-remote/src/main.rs`). Each state is a
name, whether to feed in a connected iLO-like session first (toolbar menus,
chips and status bar items, via `feed_ilo_like_session`), and the app
`Message`s that set the screen up. Add a state for the screen you're
changing. Window sizes come from the `size` match: give a state the mockup's
frame size to compare like with like (`settings-about-mockup` is 800×292), or
list it in `NARROW` to check a reflowing layout at 640px.

- Raster images decode on a worker thread, so the harness renders each
  state twice with a short pause and keeps the second frame. It builds the
  view only once, though, so it can't catch a raster handle created inside
  `view` (which never appears in the real app); see `iced-widget-gotchas`.
- The test app reports `Checking…` for updates, because the default settings
  check on launch.
- States can switch theme or language first (see the `light` and `romanian`
  helpers in the test); check new UI in both themes and in Romanian.

## 3. Compare and iterate

Run the same `measure.py` commands on the render and diff the numbers. Fix
the layout constants, re-render, and repeat until positions agree within a
pixel. Report what still differs and why (e.g. text is 1px larger by design)
instead of claiming an exact match.

To look at a render yourself, crop it first (Pillow via
`uv run --with pillow python -I -c ...`) so the detail is legible.
