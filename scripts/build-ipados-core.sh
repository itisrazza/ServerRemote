#!/usr/bin/env bash
# Builds the app core for the iPadOS app: ServerRemoteCore.xcframework (the
# static library for devices and the simulator) and its Swift bindings.
#
#   scripts/build-ipados-core.sh [--release]
#
# Outputs, both ignored by git:
#   target/ipados/ServerRemoteCore.xcframework
#   apps/ipados/ServerRemote/Generated/ServerRemoteCore.swift
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
profile="debug"
cargo_profile=()
if [[ "${1:-}" == "--release" ]]; then
    profile="release"
    cargo_profile=(--release)
fi

targets=(aarch64-apple-ios aarch64-apple-ios-sim)
out="$root/target/ipados"
generated="$root/apps/ipados/ServerRemote/Generated"
library="libserver_remote_ffi.a"

# Matches the app's deployment target, so C dependencies (aws-lc) build for
# the same iOS version.
export IPHONEOS_DEPLOYMENT_TARGET=17.0

cd "$root"
for target in "${targets[@]}"; do
    cargo build --locked -p server-remote-ffi --lib --target "$target" ${cargo_profile[@]+"${cargo_profile[@]}"}
done

rm -rf "$out"
mkdir -p "$out/headers" "$generated"
cargo run --locked -q -p server-remote-ffi --bin uniffi-bindgen-swift -- \
    --headers --modulemap \
    --module-name ServerRemoteCoreFFI --modulemap-filename module.modulemap \
    "target/aarch64-apple-ios-sim/$profile/$library" "$out/headers"
cargo run --locked -q -p server-remote-ffi --bin uniffi-bindgen-swift -- \
    --swift-sources \
    "target/aarch64-apple-ios-sim/$profile/$library" "$generated"

xcodebuild -create-xcframework \
    -library "target/aarch64-apple-ios/$profile/$library" -headers "$out/headers" \
    -library "target/aarch64-apple-ios-sim/$profile/$library" -headers "$out/headers" \
    -output "$out/ServerRemoteCore.xcframework"
