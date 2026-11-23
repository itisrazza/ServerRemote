# Front ends and the app core

ServerRemote has two front ends, the desktop app (iced) and the iPadOS app
(SwiftUI), with Android to follow. They share the application itself, not
just the protocols: everything apart from drawing the UI lives in
`server-remote-app` (`apps/core`), which doesn't depend on a UI toolkit.

## The app core

`server_remote_app::App` owns what the app knows:

- the page on show (`Page`: manager, connect, a connection, settings)
- the open connections (`connection::Connection`)
- saved sessions, settings, trusted and blocked certificates
- the update check's status
- app-wide dialogs (`app::Dialog`): errors, and a connection that failed
  before connecting, reported on the page it was opened from.

Front ends send it `app::Event`s and carry out the `app::Effect`s that
`update` returns. Effects are the things only a front end can do, or that
must not block the UI thread:

| Effect | The front end… | …then sends |
|---|---|---|
| `PickFile` | asks for a file for a connection's action | `connection::Event::FilePicked` |
| `ExportSession` | asks where to save a session and writes it | `Exported` |
| `ImportSessions` | asks for a file and reads it | `Imported` |
| `CheckForUpdates` | runs `update::check` | `UpdateChecked` |
| `SaveThumbnail` | runs `ThumbnailSource::save` off the UI thread | `ThumbnailSaved` |
| `Thumbnail` | shows a saved session's new thumbnail | |

A `Connection` keeps what its session has reported (phase, screen,
capabilities, status, traffic rates) and the dialogs it raises (confirming
an action, trusting a certificate). Input arrives as `connection::Event`s
already in the protocols' terms: keysyms and framebuffer coordinates. The
`keys` module turns keys into keysyms; each front end maps its own key
events first.

`connection::run(params, output)` connects and reports on a session (see
[Sessions](sessions.md)) until it ends or `output` closes; dropping its
future tears the session down. `Connection::params` says what to run it
with, and changes when a new attempt is needed, after a certificate is
trusted for instance. It is `None` once the connection has ended. Front ends
keep one runner going per connection whose params are set.

Translations live here too (`apps/core/i18n`), so both front ends show the
same text.

## The desktop front end

`apps/desktop` (`server-remote`) wraps `App` in an iced application. It
keeps only view state: window size and scale, the connect form's fields, the
manager's open menus and dialogs, the settings page's section, and for each
connection tab its menus, keyboard focus and virtual keyboard and trackpad
(`ui::connection::State`, by connection id).

Each connection runs on an iced `Subscription` keyed by its params. A tab's
`update` turns UI messages into `connection::Event`s, which go through the
app (so it can apply certificate settings and report early failures), and
`settle` then catches the tab up: focusing the screen on its first frame,
refreshing raster icons when capabilities or status change.

## The iPadOS front end

`apps/ffi` (`server-remote-ffi`) exposes `App` through
[UniFFI](https://mozilla.github.io/uniffi-rs/). Its `Core` object:

- owns the app behind a lock, and runs connections on its own tokio runtime,
  restarting a runner when a connection's params change
- hands the front end plain records (`AppState`, `ConnectionState`) and
  copies of screens and thumbnails on request
- tells the front end when to read them again, and asks it for file pickers,
  through the `Shell` trait the front end implements.

`apps/ipados` is the SwiftUI app. `AppModel` holds a `Core` and refreshes its
state at most once per run loop, copying the screen only for the connection
on show. Hardware keys arrive as USB HID usages, which `keys::hid_keysym`
maps; on-screen typing is sent as text. A touch moves the pointer and holds
the left button.

It's styled like the desktop app: the same palette (`Style.swift`, following
`apps/desktop/src/ui/style.rs`), Nunito Sans and the Carbon and protocol
icons, which the core provides (`fonts`, and icons rendered to PNGs with
resvg, connectors' own included), and the same layout of title bar tabs,
toolbar, status bar and dialogs. Sizes are the desktop's scaled by 1.25 for
touch (`Metrics.scale`).

To build it, open `apps/ipados/ServerRemote.xcodeproj` in Xcode. A build
phase runs `scripts/build-ipados-core.sh` for the platform being built: it
builds the Rust core as a static library under `target/ipados/<platform>/`,
with its C headers in `target/ipados/include/`, and generates the Swift
bindings into `apps/ipados/Generated/`. The project finds the library
through search paths rather than an XCFramework, so a clean checkout builds
in one go. It needs the `aarch64-apple-ios` and `aarch64-apple-ios-sim` Rust
targets.
Android will use the same `apps/ffi` crate through UniFFI's Kotlin bindings.

## Store builds

The update check sits behind the `update-check` feature of
`server-remote-app` (and of `server-remote`), on by default. Builds
distributed through a store (Microsoft Store, Flathub, the App Store) are
built without it: the store updates the app, so nothing is fetched and the
About section hides the check. The FFI crate never enables it.
