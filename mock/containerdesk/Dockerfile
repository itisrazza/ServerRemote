FROM debian:13-slim

RUN DEBIAN_FRONTEND=noninteractive apt-get update
RUN DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    sudo bash-completion                                                      \
    libxinerama1 libxi6 xserver-xorg-core xserver-xorg-input-all              \
    x11-utils xfonts-base x11-apps                                            \
    dialog whiptail yad                                                       \
    dbus dbus-bin dbus-x11                                                    \
    xfwm4 xfconf xfce4-session xfdesktop4                                     \
    xfce4-panel xfce4-whiskermenu-plugin xfce4-appfinder                      \
    thunar xfce4-terminal xfce4-settings mousepad                             \
    xfce4-taskmanager nitrogen                                                \
    adwaita-icon-theme gnome-themes-extra                                     \
    greybird-gtk-theme elementary-xfce-icon-theme                             \
    fonts-hack-ttf ttf-bitstream-vera

COPY ./hostname /etc/hostname

RUN useradd -m -G sudo -s /bin/bash user
RUN passwd -d user

COPY ./path.sh /etc/profile.d/path.sh
COPY --chown=user:user ./user /home/user
