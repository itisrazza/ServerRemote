ARG BASE_IMAGE=ghcr.io/itisrazza/containerdesk:testing
FROM ${BASE_IMAGE}

RUN DEBIAN_FRONTEND=noninteractive apt-get update
RUN DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    xvfb x11vnc

COPY ./start.sh /usr/bin/start-session
EXPOSE 5900

USER user
ENTRYPOINT [ "/usr/bin/start-session" ]
