#!/usr/bin/env bash
# Build Cairnwatch-<version>-x86_64.AppImage on Linux (x86_64).
#
# Prereqs (Debian/Ubuntu):
#   sudo apt install build-essential pkg-config curl file \
#        libxkbcommon-dev libwayland-dev libgl1-mesa-dev
#   curl https://sh.rustup.rs -sSf | sh -s -- -y
#
# Usage: packaging/linux/build-appimage.sh   (run from anywhere)
# Output: dist/Cairnwatch-<version>-x86_64.AppImage
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
DIST="$ROOT/dist"
APPDIR="$DIST/AppDir"
TOOL="$DIST/appimagetool-x86_64.AppImage"

cargo build --release

rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/icons/hicolor/256x256/apps"
install -m755 "$TARGET_DIR/release/cairnwatch" "$APPDIR/usr/bin/cairnwatch"
install -m644 packaging/linux/cairnwatch.desktop "$APPDIR/cairnwatch.desktop"
install -m644 assets/icon.png "$APPDIR/cairnwatch.png"
install -m644 assets/icon.png "$APPDIR/usr/share/icons/hicolor/256x256/apps/cairnwatch.png"
cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/cairnwatch" "$@"
EOF
chmod 755 "$APPDIR/AppRun"

if [ ! -x "$TOOL" ]; then
  curl -fsSL -o "$TOOL" \
    https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
  chmod 755 "$TOOL"
fi

# --appimage-extract-and-run: works without FUSE (WSL, containers).
ARCH=x86_64 "$TOOL" --appimage-extract-and-run "$APPDIR" \
  "$DIST/Cairnwatch-$VERSION-x86_64.AppImage"
echo "Built $DIST/Cairnwatch-$VERSION-x86_64.AppImage"
