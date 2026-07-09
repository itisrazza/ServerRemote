#!/usr/bin/env bash

IMAGE_NAME="ghcr.io/itisrazza/containerdesk-rdp:testing"
HERE="$(cd "$(dirname "$0")" && pwd)"

set -xeuo pipefail

# Build the protocol-agnostic base this image layers on.
"$HERE/../containerdesk/build.sh"

docker build --tag "$IMAGE_NAME" "$HERE"
exec docker run --name containerdesk-rdp --rm -p 3389:3389 "$IMAGE_NAME"
