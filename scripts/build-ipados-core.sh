#!/usr/bin/env bash
# Builds the app core for the iPadOS app: a static library per platform, its
# C headers and module map, and the Swift bindings. The Xcode project runs
# this before compiling; it can also be run by hand.
#
#   scripts/build-ipados-core.sh [--release] [iphoneos|iphonesimulator]...
#
# Without a platform it builds both. Outputs, ignored by git:
#   target/ipados/<platform>/libserver_remote_ffi.a
#   target/ipados/include/ (ServerRemoteCoreFFI.h, module.modulemap)
#   apps/ipados/Generated/ServerRemoteCore.swift
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
profile="debug"
cargo_profile=()
platforms=()
for arg in "$@"; do
    case "$arg" in
        --release)
            profile="release"
            cargo_profile=(--release)
            ;;
        iphoneos | iphonesimulator) platforms+=("$arg") ;;
        *)
            echo "unknown argument: $arg" >&2
            exit 2
            ;;
    esac
done
if [[ ${#platforms[@]} -eq 0 ]]; then
    platforms=(iphoneos iphonesimulator)
fi

out="$root/target/ipados"
library="libserver_remote_ffi.a"

rust_target() {
    case "$1" in
        iphoneos) echo aarch64-apple-ios ;;
        iphonesimulator) echo aarch64-apple-ios-sim ;;
    esac
}

# Matches the app's deployment target, so C dependencies (aws-lc) build for
# the same iOS version.
export IPHONEOS_DEPLOYMENT_TARGET=17.0

cd "$root"
for platform in "${platforms[@]}"; do
    target="$(rust_target "$platform")"
    cargo build --locked -p server-remote-ffi --lib --target "$target" \
        ${cargo_profile[@]+"${cargo_profile[@]}"}
    mkdir -p "$out/$platform"
    cp "target/$target/$profile/$library" "$out/$platform/$library"
done

# The bindings are the same for every platform; read them from one library.
built="target/$(rust_target "${platforms[0]}")/$profile/$library"
mkdir -p "$out/include" "$root/apps/ipados/Generated"
cargo run --locked -q -p server-remote-ffi --bin uniffi-bindgen-swift -- \
    --headers --modulemap --module-name ServerRemoteCoreFFI \
    --modulemap-filename module.modulemap "$built" "$out/include"
cargo run --locked -q -p server-remote-ffi --bin uniffi-bindgen-swift -- \
    --swift-sources "$built" "$root/apps/ipados/Generated"
