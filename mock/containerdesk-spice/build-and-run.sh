#!/usr/bin/env bash

IMAGE_NAME="ghcr.io/itisrazza/containerdesk-spice:testing"
HERE="$(cd "$(dirname "$0")" && pwd)"

set -xeuo pipefail

# Build the protocol-agnostic base this image layers on.
"$HERE/../containerdesk/build.sh"

docker build --tag "$IMAGE_NAME" "$HERE"
exec docker run --name containerdesk-spice --rm -p 5900:5900 "$IMAGE_NAME"
