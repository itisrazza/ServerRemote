#!/usr/bin/env bash
set -euo pipefail

# xrdp runs as root: the session manager needs it to authenticate over PAM and
# spawn the per-user Xorg session. It also wants a runtime dir and a system bus.
mkdir -p /var/run/xrdp/sockdir /run/dbus
chown xrdp:xrdp /var/run/xrdp /var/run/xrdp/sockdir 2>/dev/null || true
dbus-daemon --system --fork

# The session manager spawns the Xorg (xorgxrdp) session on login; the RDP
# listener runs in the foreground and keeps the container alive.
xrdp-sesman --nodaemon &
exec xrdp --nodaemon
