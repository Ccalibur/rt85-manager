//! USB HID keyboard usage IDs (HID Usage Tables, page 0x07) with display names.
//! The Royuan remap command takes one of these as the new key.

/// `(usage, name)`, in table order.
pub const KEYCODES: &[(u8, &str)] = &[
    (0x04, "A"), (0x05, "B"), (0x06, "C"), (0x07, "D"), (0x08, "E"), (0x09, "F"),
    (0x0A, "G"), (0x0B, "H"), (0x0C, "I"), (0x0D, "J"), (0x0E, "K"), (0x0F, "L"),
    (0x10, "M"), (0x11, "N"), (0x12, "O"), (0x13, "P"), (0x14, "Q"), (0x15, "R"),
    (0x16, "S"), (0x17, "T"), (0x18, "U"), (0x19, "V"), (0x1A, "W"), (0x1B, "X"),
    (0x1C, "Y"), (0x1D, "Z"),
    (0x1E, "1"), (0x1F, "2"), (0x20, "3"), (0x21, "4"), (0x22, "5"),
    (0x23, "6"), (0x24, "7"), (0x25, "8"), (0x26, "9"), (0x27, "0"),
    (0x28, "Enter"), (0x29, "Esc"), (0x2A, "Backspace"), (0x2B, "Tab"), (0x2C, "Space"),
    (0x2D, "-"), (0x2E, "="), (0x2F, "["), (0x30, "]"), (0x31, "\\"), (0x32, "ISO #"),
    (0x33, ";"), (0x34, "'"), (0x35, "`"), (0x36, ","), (0x37, "."), (0x38, "/"),
    (0x39, "Caps Lock"),
    (0x3A, "F1"), (0x3B, "F2"), (0x3C, "F3"), (0x3D, "F4"), (0x3E, "F5"), (0x3F, "F6"),
    (0x40, "F7"), (0x41, "F8"), (0x42, "F9"), (0x43, "F10"), (0x44, "F11"), (0x45, "F12"),
    (0x46, "Print Screen"), (0x47, "Scroll Lock"), (0x48, "Pause"), (0x49, "Insert"),
    (0x4A, "Home"), (0x4B, "Page Up"), (0x4C, "Delete"), (0x4D, "End"), (0x4E, "Page Down"),
    (0x4F, "Right"), (0x50, "Left"), (0x51, "Down"), (0x52, "Up"),
    (0x53, "Num Lock"), (0x54, "KP /"), (0x55, "KP *"), (0x56, "KP -"), (0x57, "KP +"),
    (0x58, "KP Enter"), (0x59, "KP 1"), (0x5A, "KP 2"), (0x5B, "KP 3"), (0x5C, "KP 4"),
    (0x5D, "KP 5"), (0x5E, "KP 6"), (0x5F, "KP 7"), (0x60, "KP 8"), (0x61, "KP 9"),
    (0x62, "KP 0"), (0x63, "KP ."), (0x64, "ISO \\"), (0x65, "Menu"),
    (0x68, "F13"), (0x69, "F14"), (0x6A, "F15"), (0x6B, "F16"), (0x6C, "F17"), (0x6D, "F18"),
    (0x6E, "F19"), (0x6F, "F20"), (0x70, "F21"), (0x71, "F22"), (0x72, "F23"), (0x73, "F24"),
    (0xE0, "Left Ctrl"), (0xE1, "Left Shift"), (0xE2, "Left Alt"), (0xE3, "Left Super"),
    (0xE4, "Right Ctrl"), (0xE5, "Right Shift"), (0xE6, "Right Alt"), (0xE7, "Right Super"),
];

pub fn name(code: u8) -> Option<&'static str> {
    KEYCODES.iter().find(|(c, _)| *c == code).map(|(_, n)| *n)
}

pub fn from_name(name: &str) -> Option<u8> {
    KEYCODES.iter().find(|(_, n)| n.eq_ignore_ascii_case(name)).map(|(c, _)| *c)
}

/// Linux evdev key code (`KEY_*`) for a HID keyboard usage, as the kernel's
/// `hid-input` maps them. Used to replay macros through uinput.
pub fn to_evdev(usage: u8) -> Option<u16> {
    #[rustfmt::skip]
    const TABLE: [u16; 0x74] = [
          0,   0,   0,   0,  30,  48,  46,  32,  18,  33,  34,  35,  23,  36,  37,  38,
         50,  49,  24,  25,  16,  19,  31,  20,  22,  47,  17,  45,  21,  44,   2,   3,
          4,   5,   6,   7,   8,   9,  10,  11,  28,   1,  14,  15,  57,  12,  13,  26,
         27,  43,  43,  39,  40,  41,  51,  52,  53,  58,  59,  60,  61,  62,  63,  64,
         65,  66,  67,  68,  87,  88,  99,  70, 119, 110, 102, 104, 111, 107, 109, 106,
        105, 108, 103,  69,  98,  55,  74,  78,  96,  79,  80,  81,  75,  76,  77,  71,
         72,  73,  82,  83,  86, 127, 116, 117, 183, 184, 185, 186, 187, 188, 189, 190,
        191, 192, 193, 194,
    ];
    match usage {
        0xE0 => Some(29),  // left ctrl
        0xE1 => Some(42),  // left shift
        0xE2 => Some(56),  // left alt
        0xE3 => Some(125), // left meta
        0xE4 => Some(97),  // right ctrl
        0xE5 => Some(54),  // right shift
        0xE6 => Some(100), // right alt
        0xE7 => Some(126), // right meta
        u => TABLE.get(u as usize).copied().filter(|&c| c != 0),
    }
}

/// F13..=F24: keys no board has, used as macro triggers.
pub const MACRO_TRIGGERS: std::ops::RangeInclusive<u8> = 0x68..=0x73;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evdev_codes() {
        assert_eq!(to_evdev(0x04), Some(30)); // A
        assert_eq!(to_evdev(0x28), Some(28)); // Enter
        assert_eq!(to_evdev(0x64), Some(86)); // ISO \ = KEY_102ND
        assert_eq!(to_evdev(0x68), Some(183)); // F13
        assert_eq!(to_evdev(0x73), Some(194)); // F24
        assert_eq!(to_evdev(0xE3), Some(125));
        assert_eq!(to_evdev(0x00), None);
        // Every named key has a code.
        for (u, n) in KEYCODES {
            assert!(to_evdev(*u).is_some(), "{n}");
        }
    }
}
