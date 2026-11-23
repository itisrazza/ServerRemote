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

It's a native iPadOS 26 app, in its Liquid Glass design, and runs on
iPhone too: nothing in it is iPad-only. It has a sidebar of
pages and open connections, system forms, menus and alerts, and SF Symbols,
including for the connectors' built-in icons (each built-in icon carries its
name); the connection's status is a toolbar button with a popover. It shares
content rather than looks with the desktop app: the protocols' and custom
connector icons (rendered to PNGs by the core with resvg), the logo, the
accent colour (`server_remote_app::brand`, derived from the logo), and
every string.

Settings is a list of sections, each its own page in
`apps/ipados/ServerRemote/Settings/`: General (appearance, language),
Display (scaling), Security (certificate checking and the certificate
lists) and About (what `server_remote_app::about` holds). A new section is
a case of `SettingsSection` and a view beside it.

To build it, open `apps/ipados/ServerRemote.xcodeproj` in Xcode. A build
phase runs `scripts/build-ipados-core.sh` for the platform being built: it
builds the Rust core as a static library under `target/ipados/<platform>/`,
with its C headers in `target/ipados/include/`, and generates the Swift
bindings into `apps/ipados/Generated/`. The project finds the library
through search paths rather than an XCFramework, so a clean checkout builds
in one go. It needs the `aarch64-apple-ios` and `aarch64-apple-ios-sim` Rust
targets. Its app icon is the desktop's Icon Composer file,
`apps/desktop/assets/icon.icon`, which Xcode compiles directly.
`scripts/check-ipados.sh` builds it for the simulator and for
devices, unsigned and with Swift warnings as errors, as CI's iPadOS job
does on a `macos-26` runner.

Android will use the same `apps/ffi` crate through UniFFI's Kotlin bindings.

## App Store declarations

The iPadOS app declares two things the App Store asks about:

- **Privacy** (`apps/ipados/ServerRemote/PrivacyInfo.xcprivacy`): no
  tracking and no collected data, since everything stays on the device and
  it only connects to servers the user adds; and the "required reason" APIs
  a release build imports. Today those are file timestamps (`stat`,
  `fstat`, from reading settings, sessions, thumbnails and picked files)
  and system boot time (`clock_gettime_nsec_np`, from measuring elapsed
  time), both used by Rust's standard library and tokio rather than called
  directly. After adding a dependency, list a release build's imports
  (`nm -u` on the app binary) and declare any new category. Debug builds
  import more (unused code isn't stripped), so check a release one.
- **Encryption** (`ITSAppUsesNonExemptEncryption = NO`): the app only uses
  standard, published encryption (TLS through rustls, and the standard
  ciphers iLO's console protocol specifies) to talk to the user's own
  servers, which falls under the exemption for it. This is a declaration
  the developer makes to Apple, and through it to US export rules;
  revisit it if the app ever adds its own or non-standard cryptography.

## Store builds

The update check sits behind the `update-check` feature of
`server-remote-app` (and of `server-remote`), on by default. Builds
distributed through a store (Microsoft Store, Flathub, the App Store) are
built without it: the store updates the app, so nothing is fetched and the
About section hides the check. The FFI crate never enables it.

## Rules

What keeps the front ends sharing one app rather than drifting into two.

### Where code goes

- **Behaviour belongs in `apps/core`.** Deciding what happens (saving,
  connecting, validating, choosing a page, reporting a failure) is an
  `app::Event` handled by `App::update`, with its tests beside it. A front
  end turns gestures into events and draws the state; if two front ends
  would need the same `if`, it belongs in the core.
- **`apps/ffi` only translates.** It converts records and enums, looks
  things up by id (actions, icons, certificates by position) and runs the
  core's effects. Nothing there decides anything the desktop app doesn't
  also get.
- **Front ends keep only view state:** what's open, half-typed or scrolled,
  and platform plumbing (file pickers, key capture, the clipboard).
- **No UI toolkit below the front ends.** `lib/core`, the connectors and
  `apps/core` depend on neither iced nor anything Apple's; platform-specific
  mappings (SF Symbols for icons, UIKit key codes) live in the front end that
  needs them.

### The FFI surface

- **Records by value, refreshed whole.** The front end reads `AppState`
  after every `Shell::changed`; keep records to what's shown. Large data
  (screens, thumbnails, icons) is fetched on its own call, only when needed.
- **Names mustn't clash in Swift or Kotlin.** UniFFI keeps Rust's names, and
  the generated Swift sits in the app's own module, so a record called
  `Menu`, `Link`, `Color` or `Protocol` hides SwiftUI's or Objective-C's.
  Prefix it (`ActionMenu`, `WebLink`, `ConnectionProtocol`).
- **Errors are returned, not swallowed.** An exported call that can fail
  returns a `Result` with a specific error; the front end logs or shows it,
  never `try?`s it away.
- **A changed FFI surface is checked by building the app**
  (`scripts/check-ipados.sh`, or CI); the bindings are regenerated on every
  build.

### Look

- **Each front end follows its platform.** The desktop app has its own
  design; the iPadOS app uses iPadOS 26's (Liquid Glass): system controls,
  forms, menus, alerts and SF Symbols. They share content, not chrome.
- **Shared content:** every string (`apps/core/i18n`, in every language),
  the logo, the accent colour from `server_remote_app::brand`, and the
  protocols' and connectors' icons. No literal colours besides the status
  ones (connected, warning, error).
- **Built-in icons map to SF Symbols on iPad**, so connector icons sit
  evenly with the app's own (`ConnectorIcon` in `Icons.swift`). A new icon
  in `lib/icons` needs a symbol there; a test in `apps/ffi` fails until it
  has one, or is listed as one to draw from its image.

### Settings

- **A setting is added once, in the core,** as a field with a default (old
  files must still load), a `Change` and its `apply` arm. Each front end
  then shows it if it means something there: the desktop's settings page,
  and on iPad `SettingsState` and `SettingChange` in `apps/ffi` and a row on
  the right page.
- **On iPad, each section is a page** in `apps/ipados/ServerRemote/Settings/`;
  a new section is a `SettingsSection` case and a view beside it. Settings
  for features the iPad doesn't have yet (recording, the virtual keyboard,
  the update check) stay off it.

### Platform limits

- **iPadOS or iOS 26 or later**, for its design, so Xcode 26 or later to
  build it.
- **iPad first, iPhone best-effort.** Design for the iPad; where the two
  pull apart, the iPad wins. On iPhone the app should still build, launch
  and work, so keep it free of iPad-only APIs and let layouts adapt to a
  compact width (the connection toolbar folds into one menu there, as it
  does in a narrow split on iPad), but it needn't be polished for it.
  CI uses the `macos-26` runner's default Xcode.
- **Sessions don't survive the background yet.** iOS suspends the app
  within seconds; the session runners are kept separable from `App` so a
  keep-alive (or reconnecting on return) can be added without reshaping it.
- **No screen capture to check UI work**, on either platform. The desktop
  renders review snapshots headlessly; the iPad app is checked by building
  it, and by a person running it.

