#!/usr/bin/env bash
# Package target/portable/release/* into an AppImage.
# System libraries (glibc, libudev, libxkbcommon, Mesa/Vulkan, Wayland, X11)
# are deliberately not bundled: GPU drivers and udev must match the host.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
ID=io.github.Ccalibur.Rt85Manager
VERSION="${VERSION:-$(grep -m1 '^version' "$ROOT/Cargo.toml" | cut -d'"' -f2)}"
BIN="$ROOT/target/portable/release"
WORK="$ROOT/target/appimage"
TOOLS="$ROOT/target/tools"
APPDIR="$WORK/AppDir"
OUT="$ROOT/dist/RT85_Manager-$VERSION-x86_64.AppImage"

[ -x "$BIN/rt85-manager" ] || { echo "run packaging/build-portable.sh first" >&2; exit 1; }

rm -rf "$APPDIR"
install -Dm755 "$BIN/rt85-manager" "$APPDIR/usr/bin/rt85-manager"
install -Dm755 "$BIN/rt85" "$APPDIR/usr/bin/rt85"
install -Dm755 "$ROOT/packaging/appimage/AppRun" "$APPDIR/AppRun"
install -Dm644 "$ROOT/resources/$ID.desktop" "$APPDIR/usr/share/applications/$ID.desktop"
install -Dm644 "$ROOT/resources/$ID.metainfo.xml" "$APPDIR/usr/share/metainfo/$ID.appdata.xml"
install -Dm644 "$ROOT/resources/icons/hicolor/scalable/apps/$ID.svg" "$APPDIR/usr/share/icons/hicolor/scalable/apps/$ID.svg"
install -Dm644 "$ROOT/resources/udev/70-epomaker-rt85.rules" "$APPDIR/usr/share/rt85-manager/70-epomaker-rt85.rules"
ln -s "usr/share/applications/$ID.desktop" "$APPDIR/$ID.desktop"
ln -s "usr/share/icons/hicolor/scalable/apps/$ID.svg" "$APPDIR/$ID.svg"
ln -s "$ID.svg" "$APPDIR/.DirIcon"

mkdir -p "$TOOLS" "$ROOT/dist"
TOOL="$TOOLS/appimagetool-x86_64.AppImage"
if [ ! -x "$TOOL" ]; then
    curl -fL -o "$TOOL" https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
    chmod +x "$TOOL"
fi
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$TOOL" --no-appstream "$APPDIR" "$OUT"
echo "Built $OUT"
