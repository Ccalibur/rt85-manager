# RT85 Manager

A COSMIC (libcosmic) desktop app and CLI for configuring the Epomaker RT85 on
Linux: lighting effects, per-key colours, key remapping, and the built-in
screen (image/GIF upload, clock sync, CPU/temperature widgets).

> **Status:** the protocol code is ported from two reverse-engineered projects
> for sibling boards (see `docs/protocol.md`). It hasn't been tested on an
> RT85 yet. Start with `rt85 list`.

## Layout
```
crates/
  rt85-protocol/   packet builders + parsers, no I/O (unit-tested against captures)
  rt85-device/     hidapi discovery, transports, image/GIF loading
  rt85-cli/        `rt85` command-line tool
  rt85-gui/        `rt85-manager` libcosmic app
resources/         .desktop, metainfo, icon, udev rule
docs/protocol.md   protocol notes
```

## Build & run
```sh
just run                 # GUI
just cli list            # what's connected
just cli list --all      # every HID device (find an unknown RT85)
just cli lighting --mode breathing --color 00ffaa
just cli upload cat.gif
just test
```
Debug logging: `RUST_LOG=rt85=trace`. Open the GUI on a given page with `RT85_PAGE=0..3`.

## Permissions
```sh
sudo install -Dm644 resources/udev/70-epomaker-rt85.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger   # then replug
```

## Credits
Protocol details come from
[strodgers/epomaker-controller](https://github.com/strodgers/epomaker-controller) (MIT;
the bundled RT100 layout JSON is from there) and
[guysoft/rt82display](https://github.com/guysoft/rt82display) (GPL-3.0).
