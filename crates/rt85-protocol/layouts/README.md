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
joystick and screen, which aren't keys). **Provisional:** the RT85 index table
hasn't been captured, so `rt85-keymap.json` reuses the RT100 index of each key
the two boards share; Home (85) and End (88) are guesses that follow the
RT100's column-major numbering. Find the real values with `rt85 probe-key N`.
