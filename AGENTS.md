# ServerRemote — agent guide

For any coding agent working in this repo. Read [README.md](README.md)
(build, layout, translations, credits) and
[docs/architecture.md](docs/architecture.md) (connectors, sessions,
capabilities) first. This file covers what those don't.

## Checks

CI runs exactly these; run them before calling anything done:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Never screen-capture or drive the GUI to check UI work. The app renders
headlessly through an ignored snapshot test; the UI guides below explain it.

## UI guides

Detailed guides live in `.claude/skills/`. Claude Code loads them as skills;
other agents should read the matching file before starting:

| Read before… | Guide |
|---|---|
| any change to what the app shows | [ui-change-checklist](.claude/skills/ui-change-checklist/SKILL.md) |
| matching a mockup, or checking layout, size or sharpness | [ui-mockup-match](.claude/skills/ui-mockup-match/SKILL.md) (includes `measure.py`) |
| custom widgets, wrappers, overlays, images, or iced misbehaving | [iced-widget-gotchas](.claude/skills/iced-widget-gotchas/SKILL.md) |

The rules that bite most often:

- Colours come from theme palette roles only; there's a dark and a light
  theme.
- Every user-facing string is `fl!("id")`, added to both the English and the
  Romanian `.ftl` file.
- `ui::pixel_snap` and `ui::arrow_cursor` wrap the whole view and must stay
  outermost.
- Generated files (`assets/contributors/generated.rs`, the font instances)
  come from scripts in `scripts/`; rerun those rather than editing.

## Protocol traps

- **SPICE:** `SPICE_MSGC_DISPLAY_INIT` is mandatory in practice: spice-server
  sends nothing on the display channel until it arrives, despite the spec
  calling it optional.
- **Async reads:** never put a multi-`.await` read (header then payload)
  directly in a `tokio::select!` branch. If another branch wins mid-read, the
  bytes already read are lost and framing desyncs. Give each socket a reader
  task that sends whole messages over an `mpsc` channel (`spawn_reader` in
  `lib/spice`).
- **Frames:** drain all queued draw messages before sending one coalesced
  `Frame`; sending a frame per draw makes the display strobe.
