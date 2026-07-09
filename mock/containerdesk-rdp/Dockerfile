ARG BASE_IMAGE=ghcr.io/itisrazza/containerdesk:testing
FROM ${BASE_IMAGE}

RUN DEBIAN_FRONTEND=noninteractive apt-get update
RUN DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    xrdp xorgxrdp

# xrdp authenticates via PAM, so the passwordless base user needs a password.
# Test credentials: user / user.
RUN echo 'user:user' | chpasswd

# Launch our XFCE session instead of xrdp's default window manager.
COPY ./startwm.sh /etc/xrdp/startwm.sh
COPY ./start.sh /usr/bin/start-session

EXPOSE 3389
ENTRYPOINT [ "/usr/bin/start-session" ]
