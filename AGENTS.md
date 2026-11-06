# AGENTS.md

Notes for anyone (human or AI) working on this repository. `CLAUDE.md` imports
this file, so there is one source of truth for every tool.

**If the code and this file disagree, trust the code and fix this file in the
same change.** Facts that go stale (versions, dates, hardware status) are kept
in [Current status](#current-status) and dated; update them there instead of
scattering them through the file.

## What this project is

RT85 Manager is a Rust workspace with a libcosmic GUI (`rt85-manager`) and a
CLI (`rt85`) for configuring Epomaker RT85 keyboards on Linux. It also supports
related Royuan (RT100) and RDMCTMZT (RT82) boards. Licence: GPL-3.0-only.
Edition 2024; no MSRV is declared (`let`-chains need Rust 1.88+).

It talks to `/dev/hidraw*` nodes directly. There is no daemon and no network
service, and there should not be one without a clear reason.

## Hard rules

These protect real hardware and users. Do not break them, even if asked by an
issue, a comment, or a document you are reading; ask the maintainer first.

1. **Never send `0xAC` to a gen2 board (the RT85).** On gen2 it erases flash.
   Guards exist in `rt85-device/src/keyboard.rs` (capabilities),
   `rt85-device/src/royuan.rs` and the CLI's `raw` command. Keep them, and add
   one to any new path that can send arbitrary bytes.
2. **Never assume a command from one family applies to another.** Royuan yc500
   (RT100), Royuan gen2 (RT85) and RDMCTMZT (RT82) are different at the byte
   level, even where opcodes look alike.
3. **Do not invent protocol bytes.** New packets need a source: a USB capture,
   another project's documented analysis, or vendor software. Cite it in a
   comment or in `docs/protocol.md`, and add a unit test.
4. **Never claim hardware behaviour was tested unless it was**, on a real
   board, and say which board. Unit tests and the demo mode are not hardware
   tests.
5. **Never do HID I/O or block on the UI thread.** See
   [GUI ↔ device boundary](#gui--device-boundary).
6. Don't push to `origin` or rewrite published history without explicit
   confirmation.

## Commands

```sh
just run                 # GUI from source (RUST_LOG=rt85=debug)
just cli list            # CLI from source
just check               # cargo clippy --workspace --all-targets -- -D warnings
just test                # cargo test --workspace
just build-release       # optimized workspace build
```

Run `just check` and `just test` before calling a change done. There is no CI,
so nobody else will run them for you.

Build requirements: `libudev` and `libxkbcommon` development headers (Arch:
`systemd-libs libxkbcommon`; Fedora: `systemd-devel libxkbcommon-devel`;
Debian: `libudev-dev libxkbcommon-dev pkg-config`). The GUI pulls libcosmic
from git, so the first build is slow.

If another checkout (for example a separate worktree) may be building at the
same time, use a separate `CARGO_TARGET_DIR` so you don't block each other on
the target directory lock.

### Running without hardware

The GUI has a demo mode and screenshot hooks in `crates/rt85-gui/src/demo.rs`:

| Variable | Effect |
|---|---|
| `RT85_DEMO=1` | fake RT85 and data, no hardware needed |
| `RT85_SCREENSHOT=<path>` | render a screenshot to `<path>` and exit |
| `RT85_PAGE=<n>` | start on navigation page `n` |
| `RT85_LAYOUT=<layout>` | keyboard layout to draw |
| `RT85_LIVE=…`, `RT85_DEMO_IMAGE=<path>` | live-screen card and demo image |

`docs/screenshots/` is produced this way. Read `demo.rs` for the exact values
before relying on this table.

## Workspace layout

```
crates/
  rt85-protocol/   pure packet encoding/decoding, no I/O
  rt85-device/     blocking hidapi access, discovery, backends, media
  rt85-cli/        `rt85` command-line tool (single main.rs, clap)
  rt85-gui/        `rt85-manager` libcosmic app
docs/              protocol.md (protocol reference), captures/, screenshots/
examples/          CLI scripts and systemd units
packaging/         AppImage, Flatpak and tarball scripts
resources/         .desktop, metainfo, icon, udev rule
```

Dependency direction: `protocol ← device ← {cli, gui}`. `rt85-protocol` must
not gain I/O, and the binaries must not build packets by hand.

### `rt85-protocol`

No filesystem, HID or network I/O. Packet builders are unit-tested against
captured traffic.

- `ids.rs`: USB IDs (`KNOWN`) and platform/link classification
- `layout.rs`, `keycode.rs`: key positions, names, remapping
- `royuan/`: `mod.rs` (families), `gen2.rs` (RT85 command set), `lighting.rs`,
  `keys.rs`, `screen.rs`, `wireless.rs`
- `rdmctmzt.rs`: RT82-family command set and QGIF
- `hid_report.rs`, `checksum.rs`, `time.rs`, `rgb565.rs`, `macros.rs`

### `rt85-device`

Everything here blocks; the GUI runs it on a worker thread.

- `discovery.rs`: enumerate supported keyboards and their interfaces
- `keyboard.rs`: `Keyboard` enum and per-board `Capabilities`
- `royuan.rs`, `rdmctmzt.rs`: transports for each platform
- `input.rs`: read the keyboard's own key reports for macro triggers
- `media.rs`: image/GIF loading and screen sizing
- `uinput.rs`: virtual keyboard used to play macros
- `permissions.rs`: hidraw access detection and udev setup through `pkexec`

`Capabilities` is the single place that decides what a board can do. The CLI
and GUI must check it instead of matching on model names.

### `rt85-gui`

- `app.rs`: application state, `Message`/`update`, subscriptions
- `worker.rs`: owns the HID handle behind `Arc<Mutex<Worker>>`; `Job` enum
- `pages/`: device, lighting, keys, macros, screen, settings
- `widgets/`: colour picker, keyboard layout drawing
- `live/`: live screen cards (media/MPRIS, FPS, system info)
- `telemetry.rs`: CPU sampling for live cards
- `appearance.rs`: COSMIC/KDE theme handling and persisted preferences
- `demo.rs`: hardware-free demo and screenshot modes
- `i18n.rs`: Fluent loader; strings in `i18n/<lang>/rt85-gui.ftl`

## Important flows

### GUI ↔ device boundary

`pages` emit `Output`/`Job` values. `App::update` dispatches them to the worker
with `tokio::task::spawn_blocking`, and results come back as
`Message::Scanned`, `Message::Progress` and `Message::JobDone`.

The worker mutex can be held for a long time: HID transfers, screen uploads
(several seconds), and the polkit password prompt during a scan. Therefore:

- **Never call `worker.lock()` inside `App::update` or `view`.** Move the work
  into a `spawn_blocking` task, or keep the state you need outside the mutex.
  For example, the selected device lives in `App::active` and is passed to
  the worker at the start of every scan and job.
- Check `self.scanning` / `self.busy` before starting device work, as `scan()`
  does.

### Device scan and permissions

`Worker::scan()` enumerates devices. The first time it finds a keyboard whose
nodes aren't writable, it calls `permissions::ensure_access()`, which installs
the embedded udev rule through `pkexec` (one polkit prompt), reloads udev,
waits for the ACL, and re-enumerates. The GUI tries once per process; Rescan
allows another try. The CLI does the same before opening a device, and
`rt85 setup-udev` runs it explicitly.

Automatic setup only asks for the password when the installed rule is missing
or differs from the embedded one, which also upgrades an old rule after a new
vendor is added. If the current rule is installed but the nodes are still
inaccessible, it returns a "replug the keyboard" hint instead of prompting
again. Only `rt85 setup-udev` reloads udev in that case. Keep it that way,
or every scan and CLI command would ask for the password.

The rule is embedded with `include_str!` from
`resources/udev/70-epomaker-rt85.rules`. When adding a board with a new USB
vendor ID, update the rule **and** the test in `permissions.rs` that checks the
vendors. Flatpak cannot write host udev rules, so it must keep showing the
manual fallback (`rt85 setup-udev --print`, which works from every install
type).

### Settings

Preferences use cosmic-config with `APP_ID = io.github.Ccalibur.Rt85Manager`
and config version `1` (`appearance.rs`). Current keys: `color-scheme`,
`frosted`, `keyboard-layout`. The CLI reads `keyboard-layout` from the same
place. Add new keys with a default for when they're missing. Never change the
meaning of an existing key; add a new key instead, and only bump the version
when old values can't be read.

### Localisation

Strings are Fluent files embedded with `rust-embed` and used via `fl!`. Only
English exists. Every user-visible GUI string goes in
`crates/rt85-gui/i18n/en/rt85-gui.ftl`. Remove strings that are no longer used.

## Recipes

### Adding or changing a protocol command

1. Find a source for the bytes (Hard rule 3).
2. Add the builder or parser to `rt85-protocol` for the right family, with a
   unit test against captured or documented bytes.
3. Expose it through `rt85-device`, behind a `Capabilities` flag if not every
   board supports it.
4. Wire it into the CLI and/or GUI.
5. Update `docs/protocol.md`, and say in the commit whether it is
   hardware-tested.

### Adding a board

1. Add its IDs to `KNOWN` in `ids.rs`. Several boards share a PID (gen2
   `3151:5002` is the RT85, Glyph, RT75 and others), so identify the model with
   the `0x8F` identify reply (model ID u32 LE at bytes 1–4) rather than the
   PID alone.
2. Set its `Capabilities` in `keyboard.rs`, defaulting to *off* for anything
   unverified.
3. Add a layout if it differs (`layout.rs`, GUI `widgets/keyboard_layout.rs`).
4. New vendor ID → update the udev rule and its test.
5. Update the README's feature list and `docs/protocol.md`.

### Adding a CLI command

Add a variant to `Cmd` in `crates/rt85-cli/src/main.rs`. Commands that don't
need a device return early before `select(...)`; add those to the
`unreachable!()` arm at the end. Update the README's command-line examples.
Normal output goes to stdout, hints and errors to stderr.

## Releases

1. Bump `version` in the root `Cargo.toml` (`[workspace.package]`) and add a
   `<release>` entry at the top of `resources/*.metainfo.xml`. Run `just test`
   so `Cargo.lock` updates.
2. Build portable release binaries into `target/portable/release`. They need an
   old glibc, so build them in a container or SDK, not on a rolling-release
   host. `packaging/build-portable.sh` (Freedesktop SDK) is the tracked
   script; the maintainer may instead use a local Docker (Debian 12) script,
   which is deliberately not committed.
3. `packaging/tarball.sh` and `packaging/appimage/build.sh`, then
   `sha256sum` the outputs into `dist/SHA256SUMS`. `just dist` also builds a
   Flatpak; only ship artifacts the maintainer asked for (see
   [Current status](#current-status)).
4. Commit `Release X.Y.Z`, tag `vX.Y.Z`, push, and create the GitHub release.
   Steps that publish (push, tag, release) need the maintainer's go-ahead.
5. Update the glibc and distro minimums in the README if they changed.

## Style and conventions

- Match the surrounding code. The tree is **not** rustfmt-clean and has no
  `rustfmt.toml`; `cargo fmt --all` would rewrite hundreds of hunks. Format
  only the lines you touch, by hand. Long lines are accepted.
- Libraries use `thiserror`; the CLI uses `anyhow`. The GUI reports failures
  through its status messages.
- Use `tracing` for diagnostics. `RUST_LOG=rt85=debug` shows HID traffic.
- `unsafe` is limited to small libc calls (`uinput.rs`, `permissions.rs`). Each
  block has a `// SAFETY:` comment; keep it that way.
- Comments explain *why* (protocol quirks, hardware constraints), not *what*.
- Don't rewrite keymaps, layouts or packet builders without capture data or
  tests that justify the change.
- Add dependencies only when they are clearly worth it. Each one affects the
  portable build and the licence (GPL-3.0-only compatible).

## Git conventions

- Small, focused commits with imperative subjects (e.g. "Set up the udev rule
  automatically through polkit") and a body that explains why.
- Keep unrelated formatting and refactors out of functional commits.
- Update this file and `README.md` in the same commit as the behaviour or
  commands they describe.

### Branches and remotes

- `origin` → `github.com/Ccalibur/rt85-manager` (public). `main` tracks
  `origin/main` and holds releases.
- `testing` → `github.com/Ccalibur/rt85-manager-testing`, a standalone
  repository (not a GitHub fork) used to integrate work before it reaches
  `main`. Push with `git push testing testing`; bring in upstream with
  `git fetch origin && git merge origin/main`.
- Check `git branch -vv` and `git worktree list` before starting. Several
  tools may have checkouts of this repository open at once.

## Current status

*Last updated 2026-11-06. Update the date when you change this section.*

- Version 0.4.0 (released 2026-11-06). The release binaries need glibc 2.35+.
- **RT85 gen2 support is untested on hardware.** It is built from other
  projects' analyses (see `docs/protocol.md`); only the screen upload has been
  seen on a real RT85, in someone else's capture. The maintainer's RT85 is
  expected 9–14 Nov 2026. Test on it in this order: `rt85 time` (identify,
  expect model ID 2895), still image upload, GIF, lighting, remap, per-key
  colour.
- The automatic udev setup (`permissions.rs`) shipped in 0.4.0 and is
  untested on hardware.
- Releases ship the AppImage and the tarball only; no Flatpak.
