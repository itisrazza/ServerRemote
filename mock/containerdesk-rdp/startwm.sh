#!/usr/bin/env bash
# xrdp-sesman runs this as the logged-in user to start their desktop session.

if [ -r /etc/profile.d/path.sh ]; then
    . /etc/profile.d/path.sh
fi

exec dbus-launch --exit-with-session xfce4-session
