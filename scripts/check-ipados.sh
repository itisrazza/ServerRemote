#!/usr/bin/env bash
# Builds the iPadOS app for the simulator and for devices, unsigned, with
# Swift warnings as errors: what CI's iPadOS job runs. Needs macOS, Xcode 26
# or later and the aarch64-apple-ios(-sim) Rust targets.
#
#   scripts/check-ipados.sh
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root/apps/ipados"

for destination in 'generic/platform=iOS Simulator' 'generic/platform=iOS'; do
    echo "Building for $destination"
    xcodebuild build \
        -project ServerRemote.xcodeproj \
        -scheme ServerRemote \
        -destination "$destination" \
        -derivedDataPath build \
        -quiet \
        CODE_SIGNING_ALLOWED=NO \
        SWIFT_TREAT_WARNINGS_AS_ERRORS=YES
done
