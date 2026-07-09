#!/usr/bin/env bash
set -euo pipefail

cd
source /etc/profile.d/path.sh

export DISPLAY=":0"

# Virtual framebuffer size, as WIDTHxHEIGHTxDEPTH. Override with GEOMETRY.
GEOMETRY="${GEOMETRY:-800x600x24}"
#GEOMETRY="${GEOMETRY:-1280x800x24}"

# Run a real X server against a virtual framebuffer rather than Xvnc's own
# server. Xvnc doesn't implement Xinerama, which brings LibreOffice (and the
# whole session) down; Xvfb behaves like a normal X server, so it doesn't.
Xvfb "$DISPLAY" -screen 0 "$GEOMETRY" &
xvfb_pid=$!

# Share the virtual display over VNC.
x11vnc -display "$DISPLAY" -forever -shared -nopw -quiet &
x11vnc_pid=$!

# Ctrl+C brings the desktop down; the EXIT trap then stops the server and the
# virtual display. Logging out ends xfce4-session on its own, hitting the same
# path.
session_pid=""
shutdown() {
    [ -n "$session_pid" ] && kill "$session_pid" 2>/dev/null || true
    kill "$x11vnc_pid" "$xvfb_pid" 2>/dev/null || true
}
trap shutdown EXIT
trap 'exit 0' INT TERM

xfce4-session &
session_pid=$!
wait "$session_pid" || true
