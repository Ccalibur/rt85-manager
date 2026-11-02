#!/usr/bin/env bash
# Build a single-file .flatpak bundle from the portable binaries.
# Needs: flatpak install flathub org.flatpak.Builder org.freedesktop.Sdk//25.08 org.freedesktop.Platform//25.08
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
ID=io.github.Ccalibur.Rt85Manager
VERSION="${VERSION:-$(grep -m1 '^version' "$ROOT/Cargo.toml" | cut -d'"' -f2)}"
WORK="$ROOT/target/flatpak"
OUT="$ROOT/dist/RT85_Manager-$VERSION-x86_64.flatpak"

[ -x "$ROOT/target/portable/release/rt85-manager" ] || { echo "run packaging/build-portable.sh first" >&2; exit 1; }
mkdir -p "$WORK" "$ROOT/dist"

flatpak run --filesystem="$ROOT" org.flatpak.Builder \
  --user --force-clean --disable-rofiles-fuse \
  --state-dir="$WORK/state" --repo="$WORK/repo" \
  "$WORK/build" "$ROOT/packaging/flatpak/$ID.yml"

flatpak build-bundle "$WORK/repo" "$OUT" "$ID" \
  --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo
echo "Built $OUT"
