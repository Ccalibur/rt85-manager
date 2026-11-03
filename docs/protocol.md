# Epomaker HID protocol notes

The RT85 is a **Royuan** board (VID `0x3151`, wired `3151:5002`, dongle
`3151:5006`) built on the YC3123 controller, which speaks the newer **gen2**
command set. The RT100 is Royuan too, but speaks the older **yc500** set.
Epomaker also makes boards on an unrelated platform (RDMCTMZT, e.g. the RT82),
which the code supports as well.

| Platform / family | VID | Example boards | Reference |
|---|---|---|---|
| Royuan, yc500 | `0x3151` | RT100, TH80 | [strodgers/epomaker-controller](https://github.com/strodgers/epomaker-controller) |
| Royuan, gen2 (YC3123) | `0x3151` | **RT85**, Glyph, RT75 | [ramarivera/epomaker-driver-linux](https://github.com/ramarivera/epomaker-driver-linux) (MIT), [dniminenn/sharkfin](https://github.com/dniminenn/sharkfin) (GPL-3), [keybored](https://github.com/MoniMoghrabi/keybored) |
| RDMCTMZT | `0x36B0` (+ LCD `1919:1919`) | RT82, Womier SK80 | [guysoft/rt82display](https://github.com/guysoft/rt82display) |

The gen2 layouts come from those projects' analyses of Epomaker's 3.2.22
driver; only the screen upload has been seen on a real RT85 (keybored's USB
capture). Nothing here has been tested by this project on an RT85 yet.

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
| `a5` | Image/animation prepare | see below |
| `25` | Image data | see below |
| `ac` | Clear image (**yc500 only**; erases flash on gen2) | `ac 00.. 53` |
| `8f` | Identify (both families) | reply: `8f idLE32 .. .. fwLE16 boot`, e.g. id 2895 = RT85, 1379 = RT100 |
| `f7` | Poll; reply via feature GET | |
| `fe 40`, … | 2.4 GHz wake-up | fixed sequence, reply contains `01 01 01 68` once awake |

### Screen upload (both families)

Pixels are RGB565 **big-endian**, column-major (outer loop x, inner loop y).

- Prepare: `a5 bank frames delay sizeLE[0] sizeLE[1] 00 cs`, then the image
  box's left/top/right/bottom low bytes at 8..12 and high bytes at 12..16, and
  size bits 16..32 at 16..18. A still has `frames` 1 and goes to `bank` 0..4;
  an animation has `bank` 0.
- Data: `25 frame frames delay chunkLE16 len cs` + up to 56 bytes. `frame` is
  the frame index (or bank), the chunk counter restarts at 0 for each frame,
  and `len` is the number of pixel bytes in the report: 56 (`38`) except on a
  frame's last report (`34` for the RT100's 56052-byte frame, `28` for the
  RT85's 110080 bytes).
- An animation is prepared once, then all frames are sent.

Checked against two captures: the RT100's (`a5000100f4da008b0000a2ad`, from
epomaker-controller) and the RT85's full-screen still
(`a5 00 01 00 00 ae 00 ab 00 00 40 ac 00 00 01 00 01`, Epomaker's driver,
keybored's `recon/capture/fullscreen2.pcapng`).

| | RT100 (yc500) | RT85 (gen2) |
|---|---|---|
| Screen | 162 × 173 | 320 × 172 (1.47") |
| Animations | 64-px aligned, ≤ 56 frames | full screen, ≤ 51 frames (6 MiB, 5 still banks) |
| Report spacing | 1 ms | 5 ms (Epomaker's driver; no flow control) |
| Prepare reply | not read | `a5 01` = go, `a5 00` = busy for 1-5 s after the last picture; the app retries for 10 s |

A full-screen RT85 still is 1966 reports, about 10 s. A 51-frame GIF is about
100 000 reports, so roughly 8 minutes. The firmware shows a progress bar
during uploads.

## Royuan gen2 (RT85)

Same framing as above. What differs from yc500, per the vendor-driver
analyses:

| Cmd | Purpose | Layout |
|---|---|---|
| `84` | Active profile (read) | reply `[1]` = 0..3 |
| `0a` / `8a` | Key matrix write / read | single key: `0a profile slot 00 00 commit(1) mode cs` + 4-byte action at 8..12. Read: `8a profile ff page(0-7) mode cs`, 8 × 64 bytes |
| `10` / `90` | Fn layer write / read | `10 os(0 win, 1 mac) layer slot ..` + action |
| `0b` / `8b` | On-board macros | 256 bytes per slot, written in 56-byte chunks |
| `0c` / `8c` | User picture (per-key colours) | `0c bank(0-4) ff page(0-6) len last 00 cs` + RGB by slot; 126 keys = 378 bytes |
| `07` | Main lighting | `07 effect 4-speed bright opt\|mode r g b cs8`; white is sent as `fa ff fa`; effect 13 shows user picture `opt` |
| `08` | Side lighting | like `07`, speed sent directly |
| `11` / `91` | Sleep timers | four u16 LE seconds at 8..16 |
| `27` | Screen language | `27 01` flips it |
| `ac` | Flash erase | **never sent** |

Key actions are 4 bytes: `00 mods usage 00` for a key, `03 00 usageLE16` for
media keys, `09 mode slot 00` for a macro. Slots are matrix positions, not
HID codes; `crates/rt85-protocol/layouts/rt85-keymap.json` has the RT85's,
from Epomaker's default matrix. The joystick is slots 96-100.

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
- Everything gen2 except the screen still needs checking on a real board:
  identify reply, lighting, remap (`0a`), per-key colours (`0c`).
- Does the keyboard acknowledge the screen prepare (`a5 01`)? If it doesn't,
  the app logs a warning and sends anyway.
- Which physical key is slot 84 (PrtSc in the default map, drawn beside Up)?
