# Examples

Scripts built on the `rt85` CLI. They assume `rt85` is on your `PATH`. With the
Flatpak, set `RT85="flatpak run --command=rt85 io.github.Ccalibur.Rt85Manager"`;
with the AppImage, set `RT85="/path/to/RT85_Manager.AppImage --cli"`.

| File | What it does |
|---|---|
| [`lighting-presets.sh`](lighting-presets.sh) | `work` / `gaming` / `night` / `off` lighting presets, handy on a desktop shortcut |
| [`screen-slideshow.sh`](screen-slideshow.sh) | Cycles the screen through a folder of images and GIFs |
| [`host-monitor.sh`](host-monitor.sh) | Sends CPU usage and temperature to the screen widgets every few seconds |
| [`systemd/rt85-clock-sync.*`](systemd) | User timer that syncs the keyboard clock at login and hourly |
| [`systemd/rt85-host-monitor.service`](systemd) | Runs `host-monitor.sh` in the background |
