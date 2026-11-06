# AGENTS.md

Notes for anyone (human or AI) working on this repository. If code and this
file disagree, trust the code and update this file.

## What this project is

RT85 Manager is a Rust workspace with a libcosmic GUI and a CLI for configuring
Epomaker RT85 keyboards on Linux. It also supports related Royuan and RDMCTMZT
boards. Licence: GPL-3.0-only. Edition 2024, Rust 1.93+.

It talks to `/dev/hidraw*` nodes directly; there is no daemon and no network
service.

## Repository and remotes

This checkout is the `testing` fork of the project:

- `origin` → `https://github.com/Ccalibur/rt85-manager.git` (upstream, public)
- `testing` → `https://github.com/Ccalibur/rt85-manager-testing.git` (private)
- Active branch: `testing`
- `main` mirrors the upstream release and tracks `origin/main`

Do not push to `origin/main` or rewrite `main` unless explicitly asked. Work on
`testing`, then push with:

```sh
git push testing testing
```

Because this is a standalone repository (not a GitHub fork), upstream changes
are merged manually:

```sh
git fetch origin
git switch testing
git merge origin/main
```

## Commands

```sh
just run                 # GUI from source
just cli list            # CLI from source
just check               # cargo clippy --workspace --all-targets -- -D warnings
just test                # cargo test --workspace
just build-release       # optimized workspace build
just portable            # release build inside the Freedesktop SDK
just dist                # AppImage + Flatpak + tarball into dist/
```

Useful direct commands:

```sh
cargo check -p rt85-gui
cargo test -p rt85-protocol
cargo test -p rt85-device
cargo run -p rt85-gui
cargo run -p rt85-cli -- list
```

Build requirements: `libudev` and `libxkbcommon` development headers (Arch:
`systemd-libs libxkbcommon`; Fedora: `systemd-devel libxkbcommon-devel`). The
GUI pulls libcosmic from git, so the first build is slow.

There is no CI configuration in the repository. Run `just check` and
`just test` locally before calling a change done.

## Workspace layout

```
crates/
  rt85-protocol/   pure packet encoding/decoding, no I/O
  rt85-device/     blocking hidapi access, discovery, backends, media
  rt85-cli/        `rt85` command-line tool
  rt85-gui/        `rt85-manager` libcosmic app
examples/          CLI-based scripts and systemd units
packaging/         AppImage, Flatpak and tarball build scripts
resources/         .desktop, metainfo, icon, udev rule
docs/              protocol notes and screenshots
```

### `rt85-protocol`

No filesystem, HID, or network I/O. Packet builders are unit-tested against
captured traffic, so protocol changes generally need a test.

- `ids.rs` — USB IDs, platform/link classification
- `layout.rs`, `keycode.rs` — key positions, names, remapping
- `royuan/` — RT85/RT100 platform: `gen2`, `lighting`, `keys`, `screen`,
  `wireless`
- `rdmctmzt.rs` — RT82-family command set and QGIF support
- `hid_report.rs`, `checksum.rs`, `time.rs`, `rgb565.rs`, `macros.rs`

The two platforms are unrelated at the byte level. Do not assume a command or
report format from one applies to the other.

### `rt85-device`

Everything here is blocking and runs on a worker thread from the GUI.

- `discovery.rs` — enumerate supported keyboards and their interfaces
- `keyboard.rs` — `Keyboard` enum and per-board `Capabilities`
- `royuan.rs`, `rdmctmzt.rs` — transports for each platform
- `input.rs` — read the keyboard's own key reports for macro triggers
- `media.rs` — image/GIF loading and screen sizing
- `uinput.rs` — virtual keyboard used to play macros
- `permissions.rs` — hidraw permission detection and automatic udev setup

### `rt85-gui`

- `app.rs` — application state, `Message`/`update`, subscriptions
- `worker.rs` — owns the HID handle behind a mutex; `Job` enum for device work
- `pages/` — lighting, keys, macros, screen, device, settings
- `live/` — live screen cards (media, FPS, system info)
- `appearance.rs` — COSMIC/KDE theme handling and persisted preferences
- `demo.rs` — hardware-free demo and screenshot modes
- `i18n.rs` — Fluent loader; strings in `i18n/<lang>/rt85-gui.ftl`

### `rt85-cli`

Clap-based CLI. `rt85 list`, `rt85 modes`, `rt85 keys`, lighting/colour/remap
commands, screen uploads, telemetry, and `rt85 setup-udev`.

## Important flows

### Device scan and permissions

`Worker::scan()` enumerates devices, and on the first scan that finds an
inaccessible keyboard it calls `rt85_device::permissions::ensure_access()`.
That installs the embedded udev rule through `pkexec` (one polkit prompt),
reloads udev, waits for the ACL, and then re-enumerates. The GUI only attempts
this once per process; **Rescan** resets the flag so a cancelled prompt can be
retried. The CLI does the same before opening a device.

The udev rule is embedded with `include_str!`, so it works from any install
layout. Native/tarball/AppImage installs can do this automatically. Flatpak
cannot write host udev rules; it must show the manual fallback instead.

### GUI ↔ device boundary

Never do HID I/O on the UI thread. `pages` emit `Output`/`Job` values, and
`App::update` dispatches them to the worker with `spawn_blocking`. Progress and
completion come back as `Message::Progress` and `Message::JobDone`.

### Settings

Preferences use cosmic-config with `APP_ID = io.github.Ccalibur.Rt85Manager`,
schema version 1. Keys currently include `color-scheme`, `frosted`, and
`keyboard-layout`. The CLI reads the keyboard layout from the same location.

### Localisation

The GUI embeds `crates/rt85-gui/i18n/` with `rust-embed` and uses Fluent via the
`fl!` macro. Only English exists at the moment. Any new user-visible string
should be added to `i18n/en/rt85-gui.ftl` rather than hard-coded.

## Hardware and protocol notes

- RT85: `3151:5002` wired, `3151:5006` 2.4 GHz dongle. Uses the Royuan **gen2**
  command set. Some `3151:5002` batches are RT100s; the CLI has a `--board`
  override.
- RT100: `3151:4010`, `3151:4015` wired and `4011`, `4016` dongles. Uses the
  older yc500 command set.
- RDMCTMZT boards (`36b0:*`) use output reports beginning with `0xAA`. Their LCD
  appears as a separate `1919:1919` device only after an init sequence, and it
  expects QGIF data.
- The RT85 has no system-info page. Its `0xAC` command erases flash, so
  telemetry is disabled in `Capabilities` for gen2 boards. Do not send it.
- Hotplug, firmware quirks, timing, and actual lighting/screen output cannot be
  verified without a real keyboard. Say so when a change needs hardware
  testing.

## Testing policy

Before saying a change works:

```sh
just check
just test
```

Some tests are `#[ignore]`d because they need `/dev/uinput` or a display. GUI
visual changes can be checked with `RT85_DEMO=1` and screenshots. Protocol
changes should keep the captured-traffic tests passing; prefer adding a test
for a new packet. Changes to permissions should keep the Flatpak fallback
message accurate.

Do not claim hardware behaviour was tested unless it actually was.

## Style and conventions

- Match the surrounding code. The tree is **not** rustfmt-clean at default
  settings, and there is no `rustfmt.toml`; running `cargo fmt --all` creates a
  large unrelated diff. Format only the lines you touch, by hand.
- Long lines are common and accepted. Prefer readability over a strict width.
- Libraries use `thiserror`; the CLI uses `anyhow`. The GUI reports failures
  through its status/toast messages.
- Use `tracing` for library and GUI diagnostics. The CLI intentionally writes
  normal output to stdout and hints/errors to stderr.
- Comments should explain *why* (protocol quirks, hardware constraints), not
  restate the code.
- Keep public API changes small and consistent with existing naming.
- Do not rewrite keymaps, layouts, or packet builders without capture data or
  tests to justify the change.

## Git conventions

- Small, focused commits with imperative subjects, e.g. “Set up the udev rule
  automatically through polkit”.
- Keep unrelated formatting or refactors out of functional commits.
- `testing` is the integration branch for this fork. Push it to the `testing`
  remote, not to upstream.
- Do not force-push shared history without explicit confirmation.
- Update this file or `README.md` when behaviour or commands change.
