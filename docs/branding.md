# Branding

What every front end, package and listing should agree on. The sources of
truth are named in each section: change those, not copies.

## Name

**ServerRemote**: one word, capital S and R, in prose, titles, store
listings and the UI, in every language. Not "Server Remote" or "Serverremote".

| Where | Form |
|---|---|
| Executable, crates, archives | `server-remote` |
| App and package identifier | `io.razza.ServerRemote` |
| Config directory | `ProjectDirs("io", "razza", "ServerRemote")` (`apps/core/src/persistence/store.rs`) |

The identifier is the same on every platform: the macOS and iOS bundle ID,
the Flatpak and AppStream ID, the desktop entry and Wayland app ID.
Changing it orphans existing installs and settings.

## Descriptions

| Use | Text |
|---|---|
| Summary (AppStream, store subtitle) | Remote console client for servers and virtual machines |
| Tagline (website, long descriptions) | A native remote console client for servers and virtual machines, with support for HPE iLO and Spice. |
| Status | Early and experimental |

Longer descriptions follow the website's protocol and feature sections
([`website/index.html`](../website/index.html)); the AppStream description
([`packaging/linux/io.razza.ServerRemote.metainfo.xml`](../packaging/linux/io.razza.ServerRemote.metainfo.xml))
is a trimmed copy of them.

## Publisher

| | |
|---|---|
| Name | Raresh Nistor (the ASCII spelling, as in store listings and `Cargo.toml`) |
| Copyright line | Raresh Nistor. MIT OR Apache-2.0. |
| Homepage | https://serverremote.razza.io/ |
| Source | https://github.com/itisrazza/ServerRemote |
| Issues | https://github.com/itisrazza/ServerRemote/issues |

## Logo

<img src="../logo.svg" alt="" width="64" height="64">

The logo is a 16×16 SVG in the logo blue with white details:
[`apps/desktop/assets/icon.icon/Assets/logo.svg`](../apps/desktop/assets/icon.icon/Assets/logo.svg),
with an identical copy at the repository root for the README. Everything
else is made from it:

| Asset | Made by |
|---|---|
| macOS and iOS app icon (Liquid Glass) | [`icon.icon`](../apps/desktop/assets/icon.icon), an Icon Composer document over the logo, compiled by `actool` |
| Window icon (Windows, Linux) | Rendered at run time ([`app_icon.rs`](../apps/desktop/src/app_icon.rs)) |
| Windows `.exe` and installer icon | [`scripts/render-icons.py`](../scripts/render-icons.py) → `packaging/windows/icon.ico` |
| Linux icon | The SVG itself, installed as `io.razza.ServerRemote.svg` |

After changing the logo, update both copies and rerun
`scripts/render-icons.py`. Outside Icon Composer the logo is used bare, with
no tile or background.

## Colours

Defined in [`apps/core/src/brand.rs`](../apps/core/src/brand.rs), which
every front end uses.

| Role | Colour |
|---|---|
| Logo blue | `#443dff` |
| Accent, light theme | `#443dff` with white text |
| Accent, dark theme | `#726dff` (the logo blue a quarter of the way to white) with `#121212` text |
| Store banner, light | `#d4d2ff` (AppStream `branding`) |
| Store banner, dark | `#1c1a4d` (AppStream `branding`) |

The neutral greys, and the status colours (ok, warning, inactive), are the
app's theme palettes in
[`apps/desktop/src/ui/style.rs`](../apps/desktop/src/ui/style.rs); the
website mirrors them in [`website/style.css`](../website/style.css).

## Type

| Use | Font |
|---|---|
| UI | Nunito Sans (SIL OFL 1.1), as static instances from [`scripts/instance-fonts.py`](../scripts/instance-fonts.py): Medium, Bold, Condensed Medium |
| Display headings | Nunito Sans Semi-Expanded Black |
| Console text (website) | Commit Mono |

The iPadOS app uses the system font.

## Screenshots

[`website/screenshot-light.png`](../website/screenshot-light.png) and
[`screenshot-dark.png`](../website/screenshot-dark.png), 2048×1536 (1024×768
at 2×). The AppStream metadata uses the light one; store listings should use
these too.
