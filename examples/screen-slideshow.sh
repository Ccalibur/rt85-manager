#!/usr/bin/env bash
# Rotate the keyboard screen through every image/GIF in a folder.
#   ./screen-slideshow.sh ~/Pictures/rt85 15m
set -euo pipefail
RT85="${RT85:-rt85}"
DIR="${1:?usage: $0 <folder> [interval, default 10m]}"
EVERY="${2:-10m}"

shopt -s nullglob nocaseglob
while true; do
  for f in "$DIR"/*.{png,jpg,jpeg,gif,webp}; do
    echo "showing $f"
    $RT85 upload "$f" --fit cover || echo "upload failed, will retry next round" >&2
    sleep "$EVERY"
  done
done
