## Window

tab-connect = Connect to…
tab-manager = Connection manager
tab-settings = Settings
connection-closed = Connection closed

## Shared buttons

button-ok = OK
button-cancel = Cancel
button-close = Close

## Dialogs

error-open-link = Couldn't open the link
connection-failed-title = Failed to connect
connection-failed-unknown = The connection closed before it was established.
copy-details = Copy details
quit-title = Quit?
quit-message =
    { $count ->
        [one] The open connection will be closed.
       *[other] All open connections will be closed.
    }
quit-confirm = Quit
licences-title = Third-party licences
licences-icons = Icons: Carbon Design System by IBM, under the Apache License 2.0.
licences-font = Font: Nunito Sans by the Nunito Sans Project Authors, under the SIL Open Font License 1.1.
recording-folder-dialog-title = Save recordings to
file-dialog-supported = Supported files

## Connect tab

connect-name = Name
connect-hostname = Host
connect-hostname-placeholder = example.com
connect-hostname-placeholder-port = example.com:{ $port }
connect-username = Username
connect-username-placeholder = Administrator
connect-password = Password
connect-save = Save
connect-save-tooltip = Save to the connection manager
connect-save-title = Save connection
connect-connect = Connect

## Connection manager

manager-empty = No saved connections yet. Add one, or import a file.
manager-all = All ({ $count })
manager-group = { $group } ({ $count })
manager-add = Add
manager-import = Import
manager-edit = Edit…
manager-export = Export…
manager-delete = Delete…
manager-group-menu = Group
manager-group-none = None
manager-new-group = New group…
manager-new-group-title = New group
manager-group-name = Group name
manager-new-group-create = Create
manager-add-title = Add connection
manager-edit-title = Edit connection
manager-delete-title = Delete { $name }?
manager-delete-message = The saved connection is removed for good.
manager-delete-confirm = Delete
manager-import-dialog-title = Import connections
manager-export-dialog-title = Export connection
manager-file-type = ServerRemote connections
manager-import-failed = Couldn't import connections
manager-export-failed = Couldn't export the connection

## Connection

status-connecting = Connecting
status-connecting-ellipsis = Connecting…
status-connected = Connected
status-disconnected = Disconnected
close-tab = Close tab
waiting-for-frame = Waiting for first frame…
certificate-title = Untrusted certificate
certificate-message =
    { $host } presented a certificate that can't be verified. It may be self-signed or expired. Continue only if this fingerprint matches the one you expect:

    { $fingerprint }
certificate-block = Block
certificate-connect-once = Connect once
certificate-always-trust = Always trust
certificate-blocked = You blocked this server's certificate. Unblock it in Settings › Security to connect.
certificate-refused-strict = The server's certificate isn't from a recognised authority, and certificate checking is set to Strict.
certificate-not-trusted = The server's certificate wasn't trusted.
info-protocol = Protocol
info-address = Address
info-status = Status
info-resolution = Resolution
info-pointer = Pointer
info-frames = Frames received
info-encryption = Encryption
info-title = Connection details
info-received = Received
info-sent = Sent
pointer-absolute = Absolute
pointer-relative = Relative
encryption-none = None
menu-virtual-keyboard = Virtual keyboard
menu-virtual-trackpad = Virtual trackpad
menu-input-settings = Input settings…
menu-zoom-out = Zoom out
menu-zoom-in = Zoom in
menu-display-settings = Display settings…
menu-start-recording = Start recording
menu-recording-settings = Recording settings…
trackpad-left = Left
trackpad-middle = Middle
trackpad-right = Right

## Settings

section-general = General
section-display = Display
section-input = Input
section-recording = Recording
section-security = Security
section-about = About

setting-confirm-quit = Confirm before quitting
setting-confirm-quit-description = Ask before closing the window while connections are open.
setting-appearance = Appearance
setting-appearance-description = Light or dark colours.
setting-language = Language
setting-graphics-backend = Graphics backend
setting-graphics-backend-description = DirectX 12 keeps up with window resizing better; try Vulkan if drawing misbehaves. Takes effect when the app is next opened.
setting-scaling = Scaling
setting-scaling-description = How the remote display fits the window.
setting-keep-aspect-ratio = Keep aspect ratio
setting-keep-aspect-ratio-description = Letterbox the remote display rather than stretch it.
setting-display-scale = Display scale
setting-display-scale-description = How remote pixels map to this screen's.
setting-keyboard-style = Keyboard style
setting-keyboard-style-description = The virtual keyboard's layout when a connection opens.
setting-keyboard-on-connect = Show virtual keyboard on connect
setting-keyboard-on-connect-description = Open the virtual keyboard as soon as a connection opens.
setting-keyboard-layout = Keyboard layout
setting-keyboard-layout-description = The remote machine's keyboard layout.
setting-recording-folder = Save to
setting-recording-folder-description = Where new recordings are saved.
setting-recording-format = Format
setting-recording-format-description = ServerRemote recordings also keep keyboard and mouse input.
setting-recording-quality = Quality
setting-recording-quality-description = Higher quality makes larger files.
setting-check-on-launch = Check for updates at launch
setting-updates = Updates
setting-certificate-checking = Certificate checking
certificate-checking-strict = Strict
certificate-checking-ask = Ask
certificate-checking-off = Off
certificate-checking-strict-description = Only connect to servers whose certificate is issued by a recognised authority.
certificate-checking-ask-description = Ask before connecting to a server with an unrecognised certificate. You can trust it once or remember it.
certificate-checking-off-description = Accept any certificate without asking. Anyone on the network can pose as the server and see your passwords.
trusted-certificates = Trusted certificates
trusted-certificates-description = These servers connect without asking, as long as they present the same certificate.
blocked-certificates = Blocked certificates
blocked-certificates-description = These certificates are always refused.
certificates-none = None yet.
certificate-forget = Forget
certificate-unblock = Unblock

appearance-system = Match system
appearance-light = Light
appearance-dark = Dark
language-system = Match system
scaling-fit = Scale to fit
scaling-actual = Actual size
display-scale-automatic = Automatic
keyboard-style-compact = Compact
keyboard-style-tenkeyless = Tenkeyless
keyboard-style-full = Full size
keyboard-layout-us = US
keyboard-layout-uk = UK
keyboard-layout-german = German
keyboard-layout-french = French
quality-low = Low
quality-medium = Medium
quality-high = High
quality-lossless = Lossless
recording-folder-other = Other…
recording-folder-videos = Videos

update-not-checked = Not checked yet.
update-checking = Checking…
update-up-to-date = You're on the latest version.
update-no-releases = No releases have been published yet.
update-available = Version { $version } is available.
update-failed = Couldn't check for updates: { $error }
update-check-now = Check now
update-check-again = Check again
update-try-again = Try again
update-download = Download

## About

about-website = Website
about-made-by = Made by:
about-licence = Licence
about-licence-description = ServerRemote is licenced under the MIT or Apache 2.0 License, at your option.
about-mit-license = MIT License
about-apache-license = Apache 2.0 License
about-third-party-licences = Third-party licences
