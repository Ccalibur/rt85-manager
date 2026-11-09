#!/usr/bin/env bash
# Plain tarball laid out like a prefix (bin/, share/...), for a native install:
#   sudo tar -xzf rt85-manager-*.tar.gz -C /usr/local --strip-components=1
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ID=io.github.Ccalibur.Rt85Manager
VERSION="${VERSION:-$(grep -m1 '^version' "$ROOT/Cargo.toml" | cut -d'"' -f2)}"
NAME="rt85-manager-$VERSION-x86_64"
STAGE="$ROOT/target/tarball/$NAME"
BIN="$ROOT/target/portable/release"

rm -rf "$STAGE"
install -Dm755 "$BIN/rt85-manager" "$STAGE/bin/rt85-manager"
install -Dm755 "$BIN/rt85" "$STAGE/bin/rt85"
install -Dm644 "$ROOT/resources/$ID.desktop" "$STAGE/share/applications/$ID.desktop"
install -Dm644 "$ROOT/resources/$ID.metainfo.xml" "$STAGE/share/metainfo/$ID.metainfo.xml"
install -Dm644 "$ROOT/resources/icons/hicolor/scalable/apps/$ID.svg" "$STAGE/share/icons/hicolor/scalable/apps/$ID.svg"
install -Dm644 "$ROOT/resources/icons/hicolor/256x256/apps/$ID.png" "$STAGE/share/icons/hicolor/256x256/apps/$ID.png"
install -Dm644 "$ROOT/resources/udev/70-epomaker-rt85.rules" "$STAGE/share/rt85-manager/70-epomaker-rt85.rules"
install -Dm644 "$ROOT/LICENSE" "$STAGE/share/licenses/rt85-manager/LICENSE"
mkdir -p "$ROOT/dist"
tar -C "$ROOT/target/tarball" -czf "$ROOT/dist/$NAME.tar.gz" "$NAME"
echo "Built $ROOT/dist/$NAME.tar.gz"
