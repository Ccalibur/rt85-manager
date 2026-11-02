#!/usr/bin/env bash
# Feed CPU usage and the hottest temperature sensor to the screen widgets.
# (The GUI does the same with the "Show CPU usage / temperature" toggles;
# this is for running it headless, e.g. as a systemd user service.)
set -euo pipefail
RT85="${RT85:-rt85}"
INTERVAL="${INTERVAL:-2}"

read_cpu() { awk '/^cpu /{print $2+$3+$4+$5+$6+$7+$8, $5+$6}' /proc/stat; }
read -r prev_total prev_idle < <(read_cpu)
while sleep "$INTERVAL"; do
  read -r total idle < <(read_cpu)
  dt=$((total - prev_total)); di=$((idle - prev_idle))
  prev_total=$total; prev_idle=$idle
  (( dt > 0 )) && $RT85 cpu $(( 100 * (dt - di) / dt ))

  temp=$(cat /sys/class/thermal/thermal_zone*/temp 2>/dev/null | sort -n | tail -1)
  [ -n "$temp" ] && $RT85 temp $(( temp / 1000 ))
done
