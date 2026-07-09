#!/usr/bin/env bash

IMAGE_NAME="ghcr.io/itisrazza/containerdesk:testing"
HERE="$(cd "$(dirname "$0")" && pwd)"

set -xeuo pipefail

docker build --tag "$IMAGE_NAME" "$HERE"
