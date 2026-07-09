#!/usr/bin/env bash
set -euo pipefail

cd
source /etc/profile.d/path.sh

export DISPLAY=":0"

# Display resolution, as WIDTHxHEIGHT. Override with GEOMETRY.
GEOMETRY="${GEOMETRY:-800x600}"

# Xspice is a real Xorg server with the SPICE server built in, so — unlike
# Xvnc — it implements Xinerama and doesn't take LibreOffice down. It serves
# the SPICE protocol directly on --port.
Xspice \
    --port 5900 \
    --disable-ticketing \
    --geometry "$GEOMETRY" \
    "$DISPLAY" &
xspice_pid=$!

# Wait for the display to come up before starting the desktop.
for _ in $(seq 30); do
    xdpyinfo -display "$DISPLAY" >/dev/null 2>/dev/null && break
    sleep 0.2
done

# Ctrl+C brings the desktop down; the EXIT trap then stops Xspice. Logging out
# ends xfce4-session on its own, hitting the same path.
session_pid=""
shutdown() {
    [ -n "$session_pid" ] && kill "$session_pid" 2>/dev/null || true
    kill "$xspice_pid" 2>/dev/null || true
}
trap shutdown EXIT
trap 'exit 0' INT TERM

xfce4-session &
session_pid=$!
wait "$session_pid" || true
