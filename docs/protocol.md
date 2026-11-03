# Epomaker HID protocol notes

The RT85's controller hasn't been identified yet. Epomaker uses at least two
unrelated platforms, and the code supports both:

| Platform | VID | Example boards | Reference |
|---|---|---|---|
| Royuan | `0x3151` | RT100, TH80 | [strodgers/epomaker-controller](https://github.com/strodgers/epomaker-controller) |
| RDMCTMZT | `0x36B0` (+ LCD `1919:1919`) | RT82, Womier SK80 | [guysoft/rt82display](https://github.com/guysoft/rt82display) |

**First step with a real RT85:** `rt85 list --all` (or `lsusb`). Add its VID:PID to
`crates/rt85-protocol/src/ids.rs` and fill in the table below.

| RT85 mode | VID | PID | Config interface |
|---|---|---|---|
| Wired | | | |
| 2.4 GHz | | | |

## Royuan (`0x3151`)

64-byte **feature reports** (`HIDIOCSFEATURE`); byte 0 is the command. Most
reports use an 8-byte header: `cmd p1..p6 checksum`, where
`checksum = 0xFF - (sum(bytes[0..7]) & 0xFF)`. The payload is bytes 8..64 (56 bytes).
Reports are spaced ≥ 1 ms apart.

| Cmd | Purpose | Layout |
|---|---|---|
| `07` | Lighting effect | `07 mode speed(0-5) bright(0-4) opt r g b cs` (cs at byte **8**). `opt` = direction<<4 \| (dazzle ? 08 : 07) |
| `13` | Remap key | `13 00 idx 00 00 00 00 cs 00 00 hid_usage` |
| `18` | Per-key RGB start | `18 00.. e7` |
| `19` | Per-key RGB data | `19 report(0-6) frame total_frames frame_time 00 00 cs` + 56 bytes of RGB triplets by key index |
| `22` | CPU widget | `22 00.. dd 63 00 7f 00 04 00 08 00 pct` |
| `28` | Clock | `28 00.. d7 yearBE16 mon day h m s` |
| `2a` | Temperature widget | `2a 00.. d5 °C` |
| `a5` | Image/animation start | `a5 00 frames delay_ms sizeLE16 00 cs 00 00 w h` |
| `25` | Image data | `25 00 frame(1-based) 00 seqLE16 kind cs` + 56 bytes; kind `38` = data, `34` = last report of the frame |
| `ac` | Clear image | `ac 00.. 53` |
| `f7` | Poll; reply via feature GET | |
| `fe 40`, … | 2.4 GHz wake-up | fixed sequence, reply contains `01 01 01 68` once awake |

Image pixels: RGB565 **big-endian**, column-major (outer loop x, inner loop y).
RT100 screen is 162 × 173, so one frame = 56052 bytes = 1001 reports. Animation
frames must be 64-px aligned in both dimensions; the reference tool caps them at 56 frames.

### RT85 full-screen stills (320 × 172)

The RT85 is a Royuan board (wired `3151:5002`, dongle `3151:5006`) with a
1.47" 320 × 172 panel. One frame is 110080 bytes, too big for the compact
header, so Epomaker's driver (v4 3.2.22) extends it. The format below is
decoded from that driver's USB capture, published by
[keybored](https://github.com/MoniMoghrabi/keybored) (`recon/capture/fullscreen2.pcapng`):

- Init: `a5 00 01 00 sizeLE16(low) 00 cs 00 00 w&ff h 00 00 w>>8 00 size>>16`,
  i.e. `a5 00 01 00 00 ae 00 ab 00 00 40 ac 00 00 01 00 01` for 320 × 172.
- 1966 data reports: `38` on every report except the final one, which is `28`.
  Sending `34` at the end of an oversized stream blanks the screen.
- About 5 ms between reports (the capture's median gap is 5.0 ms), so an
  upload takes about 10 s. The firmware shows a progress bar during it.
- Only single-frame stills have been seen in the extended form. Animations
  keep the compact header, which limits them to 192 × 128 on the RT85.
- keybored reports that the built-in CPU/memory/temperature widgets are
  driven some other way on the RT85, so `22`/`2a` may do nothing there.

## RDMCTMZT (`0x36B0`)

Packets start `AA cmd`. Keyboard raw-HID interface 1 (usage page `0xFF60`):
64-byte output reports, no report-ID prefix. LCD `1919:1919` interface 1: report
ID `00` + 64 bytes, and every write gets a 64-byte read (flow control).

1. Keyboard: `AA E2`, `AA E0` ×5 → the LCD device (re-)enumerates (up to ~2 s on Linux).
2. LCD: `10`, `17` (clock), `11`, `1C`, `10`, `12`, `11`, `1C`.
3. Keyboard: `AA E3 00 00 00 01 00 00 01` (gif count).
4. LCD: `1B`, `E3`, `14`, then `15` ×3 / `16` / `18` (size, erase blocks = ceil(size/64K)+1).
5. Poll `1C` until the flash erase finishes.
6. `19`: `AA 19 offsetLE24 len 00 00` + 56 data bytes.
7. `1A` (end), then `10`, `17` (clock), `11`.

Clock (`17`) sends each field as decimal digits:
`Y Y Y Y  M M  weekday  D D  h h  m m  s s` (weekday 0 = Monday).

The screen takes **QGIF** (240 × 135). The real encoder is the vendor's WASM
(image.rdmctmzt.com), and there's no clean-room encoder yet, so the app only
uploads pre-encoded `.qgif` files on this platform.

## Open questions for the RT85
- Does the joystick have a remappable index?
- How does the official driver upload full-screen GIFs (multi-frame extended header)?
- Key index table (light keys one at a time with `rt85 probe-key N`).
- Can the keymap be read back from the keyboard?
