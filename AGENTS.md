# ServerRemote — agent guide

For any coding agent working in this repo. Read [README.md](README.md)
(build, layout, translations, credits) and the architecture index,
[docs/architecture/README.md](docs/architecture/README.md) (goal, crates,
transport vs protocol), first; open the parts it links (sessions,
capabilities, TLS, roadmap) when your task touches them. This file covers
what those don't.

## Before starting

Work on a branch off `main` (`feature/<name>`, `fix/<name>`) and merge it
through a PR; nothing is committed to `main` directly. Fixes for a release
branch land on `main` first and are cherry-picked; see
[docs/releasing.md](docs/releasing.md) for branches, release candidates and
tagging.

Other agent sessions merge into `main` too, sometimes while you work. Run
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

CI also builds the store build (without the update check):

```sh
cargo clippy -p server-remote-app -p server-remote --no-default-features --all-targets -- -D warnings
```

and, on a macOS runner, the iPadOS app. On macOS, after changing the app
core's public API, `apps/ffi` or `apps/ipados`, run that too:

```sh
scripts/check-ipados.sh
```

**Docs are part of done.** When feature work lands or a feature's status
changes, update the docs in the same change: the README's status line,
protocol table and crate list; the status line and crate table in
[docs/architecture/README.md](docs/architecture/README.md); the
[roadmap](docs/architecture/roadmap.md) (move items between Done, In
progress and Next); the architecture page for the area you changed; the
crate `description` in `apps/desktop/Cargo.toml`; the caveats in the
[protocol notes](docs/protocols/README.md); and the protocol table and
feature list on the [website](website/index.html).

Because of `-D warnings`, a type or function nothing uses yet fails CI as
dead code. Land a feature together with its first use rather than adding
`#[allow(dead_code)]`.

Never screen-capture or drive the GUI to check UI work. The app renders
headlessly through an ignored snapshot test; the UI guides below explain it.

## Rules that apply everywhere

- **Protocols are clean-room.** Follow
  [implementing a protocol](docs/protocols/implementing.md). A spec is
  written first, and a separate session implements it from that spec
  alone. If your session has read a first-party client, a decompile or
  another implementation's source, it works on the spec only.

- **Tests never touch the user's real files.** Build the app with
  `App::new(None, Settings::default())` (or the desktop's
  `Application::new`, which takes the same); persistence takes an
  `Option<&Path>` config directory and stores nothing without one. Effects
  and tasks returned by `update` aren't run in unit tests, so nothing reaches
  the network either. A test that needs saved files gives the app a
  temporary directory.
- **Platform-specific code:** prefer `if cfg!(windows)` over `#[cfg(windows)]`
  items where practical, so the Windows path still type-checks (and isn't
  flagged as dead code) on macOS and Linux.
- **The cores stay UI-free:** `lib/core`, the connectors and the app core
  (`apps/core`) never depend on iced or any other UI toolkit. Application
  logic belongs in `apps/core`, so every front end gets it; a front end keeps
  only view state (see
  [front ends](docs/architecture/front-ends.md)). Connector UI is data
  (`Capabilities`, `Status`), rendered by `apps/desktop/src/ui/contributed.rs`
  and `apps/ipados/ServerRemote/ConnectionView.swift`. Extend the Dummy connector when
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
| any change to the iPadOS app or `apps/ffi` | [ipados-change-checklist](.claude/skills/ipados-change-checklist/SKILL.md) |

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
