#!/usr/bin/env bash
# Packs a built server-remote binary into an AppImage.
#
# The binary only links glibc (winit and wgpu load the display and GPU
# libraries at run time), so nothing else is bundled; its glibc baseline is
# that of the machine that built it. The image is the AppImage type 2
# runtime with a zstd squashfs appended, so this needs mksquashfs
# (squashfs-tools) and a runtime from
# https://github.com/AppImage/type2-runtime/releases.
#
# Usage: scripts/bundle-appimage.sh <server-remote binary> <runtime> <output .AppImage>
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: $0 <server-remote binary> <runtime> <output .AppImage>" >&2
  exit 2
fi

binary=$1
runtime=$2
output=$3
root=$(cd "$(dirname "$0")/.." && pwd)
id=io.razza.ServerRemote

appdir=$(mktemp -d)
squashfs=$(mktemp -u)
trap 'rm -rf "$appdir" "$squashfs"' EXIT

install -Dm755 "$binary" "$appdir/usr/bin/server-remote"
install -Dm644 "$root/packaging/linux/$id.desktop" "$appdir/usr/share/applications/$id.desktop"
install -Dm644 "$root/packaging/linux/$id.metainfo.xml" "$appdir/usr/share/metainfo/$id.metainfo.xml"
install -Dm644 "$root/apps/desktop/assets/icon.icon/Assets/logo.svg" \
  "$appdir/usr/share/icons/hicolor/scalable/apps/$id.svg"
install -Dm644 "$root/LICENSE-MIT" "$appdir/usr/share/licenses/server-remote/LICENSE-MIT"
install -Dm644 "$root/LICENSE-APACHE" "$appdir/usr/share/licenses/server-remote/LICENSE-APACHE"
install -Dm644 "$root/apps/desktop/assets/fonts/OFL.txt" \
  "$appdir/usr/share/licenses/server-remote/OFL-NunitoSans.txt"

# The AppImage spec wants the desktop entry and icon at the root too.
ln -s "usr/share/applications/$id.desktop" "$appdir/$id.desktop"
ln -s "usr/share/icons/hicolor/scalable/apps/$id.svg" "$appdir/$id.svg"
ln -s "$id.svg" "$appdir/.DirIcon"
cat > "$appdir/AppRun" <<'EOF'
#!/bin/sh
here=$(dirname "$(readlink -f "$0")")
exec "$here/usr/bin/server-remote" "$@"
EOF
chmod 755 "$appdir/AppRun"

mksquashfs "$appdir" "$squashfs" -root-owned -noappend -comp zstd -quiet
cat "$runtime" "$squashfs" > "$output"
chmod 755 "$output"
echo "$output"
