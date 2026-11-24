#!/usr/bin/env bash
# Wraps a built server-remote binary in ServerRemote.app.
#
# The icon is compiled from apps/desktop/assets/icon.icon, which needs
# Xcode 26's actool. The bundle is ad-hoc signed so it runs locally;
# release.yml re-signs it with the Developer ID.
#
# Usage: scripts/bundle-macos.sh <server-remote binary> <output dir>
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 <server-remote binary> <output dir>" >&2
  exit 2
fi

binary=$1
out=$2
root=$(cd "$(dirname "$0")/.." && pwd)
app="$out/ServerRemote.app"

version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)
if [[ -z "$version" ]]; then
  echo "error: no workspace version in $root/Cargo.toml" >&2
  exit 1
fi

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

sed "s/@VERSION@/$version/g" "$root/packaging/macos/Info.plist" > "$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist" > /dev/null

cp "$binary" "$app/Contents/MacOS/server-remote"
cp "$root/LICENSE-MIT" "$root/LICENSE-APACHE" "$app/Contents/Resources/"
cp "$root/apps/desktop/assets/fonts/OFL.txt" "$app/Contents/Resources/OFL-NunitoSans.txt"

partial_plist=$(mktemp)
trap 'rm -f "$partial_plist"' EXIT
xcrun actool "$root/apps/desktop/assets/icon.icon" \
  --compile "$app/Contents/Resources" \
  --platform macosx \
  --minimum-deployment-target 11.0 \
  --app-icon icon \
  --output-partial-info-plist "$partial_plist" \
  --output-format human-readable-text \
  --errors --warnings

codesign --force --sign - "$app"
echo "$app"
