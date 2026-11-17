# ServerRemote — agent guide

For any coding agent working in this repo. Read [README.md](README.md)
(build, layout, translations, credits) and the architecture index,
[docs/architecture/README.md](docs/architecture/README.md) (goal, crates,
transport vs protocol), first; open the parts it links (sessions,
capabilities, TLS, roadmap) when your task touches them. This file covers
what those don't.

## Before starting

Other agent sessions commit to `main` too, sometimes while you work. Run
`git fetch` and read `git log --oneline origin/main -15` before trusting
anything you remember about the code (including this file), rebase onto
`origin/main` before building on it, and re-read a file before rewriting it
whole.

## Checks

CI runs exactly these; run them before calling anything done:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Because of `-D warnings`, a type or function nothing uses yet fails CI as
dead code. Land a feature together with its first use rather than adding
`#[allow(dead_code)]`.

Never screen-capture or drive the GUI to check UI work. The app renders
headlessly through an ignored snapshot test; the UI guides below explain it.

## Rules that apply everywhere

- **Tests never touch the user's real files.** Build the app with
  `Application::new(None, Settings::default())`; persistence takes an
  `Option<&Path>` config directory and stores nothing without one. Tasks
  returned by `update` aren't run in unit tests, so nothing reaches the
  network either.
- **Platform-specific code:** prefer `if cfg!(windows)` over `#[cfg(windows)]`
  items where practical, so the Windows path still type-checks (and isn't
  flagged as dead code) on macOS and Linux.
- **The core stays UI-free:** `lib/core` and the connectors never depend on
  iced. Connector UI is data (`Capabilities`, `Status`), rendered by
  `apps/server-remote/src/ui/contributed.rs`. Extend the Dummy connector when
  adding a new kind of contribution, so it can be exercised without a server.
- **Tooling:** Python helpers are `uv run --script` files with pinned PyPI
  dependencies. Don't download and run third-party binaries (linters and so
  on). Look up the latest version of any crate or GitHub Action before adding
  it.

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
