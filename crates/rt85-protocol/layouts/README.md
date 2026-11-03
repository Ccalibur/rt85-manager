Key index tables (`*-keymap.json`) and KLE-style physical layouts.

`rt100-*` are taken from strodgers/epomaker-controller (MIT). They're used as a
reference/fallback until an RT85 table has been captured: the RT85 has fewer
keys, so its LED/matrix indices will differ.

`rt100-us-ansi.json` is derived from the UK ISO layout. The ANSI `\` key is
given the ISO `#` key's index (75): on ANSI/ISO-compatible PCBs those two
usually share a matrix position (HID usages 0x31/0x32). Not yet confirmed on
hardware. Both layouts place Left Ctrl (index 5, the first column of the
column-major index table) bottom-left; the upstream KLE file had the two Ctrl
names swapped.

`rt85-*` describe the RT85's 75% board (82 keys ANSI, 83 ISO, plus the
joystick and screen). `rt85-keymap.json` gives each key its slot in the
RT85's key matrix, read from the default matrix in Epomaker's 3.2.22 driver
(`rt85-matrices.json` in `ramarivera/epomaker-driver-linux`, MIT). The same
slot numbers address per-key colours (user pictures). Not yet checked on a
real board:

- The default matrix has no End key: its fifth navigation key is Print Screen
  (slot 84; Fn+Home sends End), so the key beside Up is labelled PrtSc. Which
  physical key sends what still needs a look at the board.
- The ISO-only key next to Left Shift (`BACKSLASH`, slot 10) isn't in the
  vendor's (ANSI) default matrix; 10 is the free slot in that column.
- Joystick slots: 96 volume down, 97 next lighting effect, 98 volume up,
  99 mute (press), 100 previous lighting effect (108/109 repeat volume up/down).
  Remap them by number, e.g. `rt85 remap 99 0xe2`.
