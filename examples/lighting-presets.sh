#!/usr/bin/env bash
# Switch between a few lighting presets.
#   ./lighting-presets.sh work | gaming | night | off
# Bind it to a shortcut in your desktop settings to flip presets quickly.
set -euo pipefail
RT85="${RT85:-rt85}"   # or: RT85="flatpak run --command=rt85 io.github.Ccalibur.Rt85Manager"

case "${1:-}" in
  work)   $RT85 lighting --mode always-on      --brightness 2 --color f2ece0 ;;
  gaming) $RT85 lighting --mode spectrum-cycle --brightness 4 --speed 4 --dazzle ;;
  night)  $RT85 lighting --mode breathing      --brightness 1 --speed 1 --color ff6a00 ;;
  off)    $RT85 lighting --mode always-on      --brightness 0 ;;
  *) echo "usage: $0 work|gaming|night|off" >&2; exit 1 ;;
esac
