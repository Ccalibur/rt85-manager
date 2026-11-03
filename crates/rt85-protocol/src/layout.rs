//! Physical key layouts: which LED/matrix index each key has, and where it sits.
//!
//! Two JSON files describe a board:
//! * a key table: `[{"name": "ESC", "value": 0, "display_str": "Esc"}, ...]`.
//!   `value` is the board's internal index, used for per-key RGB and remapping.
//! * a KLE-style (keyboard-layout-editor.com) layout: rows of key names, plus
//!   `{x, y, w, h}` objects that adjust the position or size of the next key.

use crate::ProtocolError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyDef {
    pub name: String,
    #[serde(rename = "value")]
    pub index: u8,
    #[serde(rename = "display_str", default)]
    pub label: Option<String>,
}

/// One key placed on the board, in key units (1u = one standard key).
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedKey {
    pub name: String,
    pub label: String,
    pub index: u8,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Physical key arrangement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Physical {
    /// US ANSI: wide Enter on the home row, `\` above it, 2.25u left Shift.
    Ansi,
    /// UK ISO: tall Enter, `#` beside it, an extra `\` key beside left Shift.
    Iso,
}

impl Physical {
    pub const ALL: [Physical; 2] = [Physical::Ansi, Physical::Iso];

    pub fn label(self) -> &'static str {
        match self {
            Physical::Ansi => "US ANSI",
            Physical::Iso => "UK ISO",
        }
    }

    /// HID usages that only exist on this arrangement's extra keys.
    fn missing_usages(self) -> &'static [u8] {
        match self {
            // No "Non-US #" or "Non-US \" keys on ANSI boards.
            Physical::Ansi => &[0x32, 0x64],
            Physical::Iso => &[],
        }
    }

    /// Remap targets that make sense for this arrangement, with names that
    /// match its legends.
    pub fn keycodes(self) -> Vec<(u8, &'static str)> {
        crate::keycode::KEYCODES
            .iter()
            .filter(|(c, _)| !self.missing_usages().contains(c))
            .map(|&(c, n)| match (self, c) {
                (Physical::Iso, 0x32) => (c, "#"),
                (Physical::Iso, 0x64) => (c, "\\"),
                (Physical::Iso, 0x31) => (c, "\\ (US)"),
                _ => (c, n),
            })
            .collect()
    }
}

impl std::str::FromStr for Physical {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "ansi" | "us" | "us-ansi" => Ok(Physical::Ansi),
            "iso" | "uk" | "uk-iso" => Ok(Physical::Iso),
            _ => Err(format!("unknown layout {s:?} (use ansi or iso)")),
        }
    }
}

/// Which keyboard the layout is drawn for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Board {
    /// 75% ("85") board: F-row, nav column, arrows; joystick + screen.
    #[default]
    Rt85,
    /// Full-size reference board with numpad.
    Rt100,
}

impl Board {
    /// The board for a detected model name (`KnownId::model`); unknown
    /// devices are assumed to be the RT85 this project targets.
    pub fn for_model(model: Option<&str>) -> Self {
        match model {
            Some("RT100") => Board::Rt100,
            _ => Board::Rt85,
        }
    }
}

impl std::str::FromStr for Board {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "rt85" => Ok(Board::Rt85),
            "rt100" => Ok(Board::Rt100),
            _ => Err(format!("unknown board {s:?} (use rt85 or rt100)")),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Layout {
    pub name: String,
    pub keys: Vec<PlacedKey>,
}

impl Layout {
    pub fn parse(name: &str, keymap_json: &str, kle_json: &str) -> Result<Self, ProtocolError> {
        let defs: Vec<KeyDef> =
            serde_json::from_str(keymap_json).map_err(|e| ProtocolError::Layout(e.to_string()))?;
        let rows: Vec<Vec<serde_json::Value>> =
            serde_json::from_str(kle_json).map_err(|e| ProtocolError::Layout(e.to_string()))?;

        let mut keys = Vec::new();
        let mut y = 0.0f32;
        for row in rows {
            let mut x = 0.0f32;
            let (mut w, mut h) = (1.0f32, 1.0f32);
            for item in row {
                match item {
                    serde_json::Value::Object(props) => {
                        let get = |k: &str| props.get(k).and_then(|v| v.as_f64()).map(|v| v as f32);
                        x += get("x").unwrap_or(0.0);
                        y += get("y").unwrap_or(0.0);
                        if let Some(v) = get("w") {
                            w = v;
                        }
                        if let Some(v) = get("h") {
                            h = v;
                        }
                    }
                    serde_json::Value::String(key_name) => {
                        let def = defs.iter().find(|d| d.name == key_name).ok_or_else(|| {
                            ProtocolError::Layout(format!("key {key_name:?} missing from key table"))
                        })?;
                        keys.push(PlacedKey {
                            label: def.label.clone().unwrap_or_else(|| key_name.clone()),
                            name: key_name,
                            index: def.index,
                            x,
                            y,
                            w,
                            h,
                        });
                        x += w;
                        w = 1.0;
                        h = 1.0;
                    }
                    other => {
                        return Err(ProtocolError::Layout(format!("unexpected layout item {other}")));
                    }
                }
            }
            y += 1.0;
        }
        Ok(Self { name: name.to_owned(), keys })
    }

    pub fn for_board(board: Board, physical: Physical) -> Self {
        match board {
            Board::Rt85 => Self::rt85(physical),
            Board::Rt100 => Self::rt100(physical),
        }
    }

    /// RT85, 75%. Indices are provisional (see `layouts/README.md`).
    pub fn rt85(physical: Physical) -> Self {
        let (name, kle) = match physical {
            Physical::Iso => ("RT85 75% (UK ISO)", include_str!("../layouts/rt85-uk-iso.json")),
            Physical::Ansi => ("RT85 75% (US ANSI)", include_str!("../layouts/rt85-us-ansi.json")),
        };
        Self::parse(name, include_str!("../layouts/rt85-keymap.json"), kle).expect("bundled layout is valid")
    }

    /// Reference layout from the RT100.
    pub fn rt100(physical: Physical) -> Self {
        let (name, kle) = match physical {
            Physical::Iso => ("RT100 (UK ISO)", include_str!("../layouts/rt100-uk-iso.json")),
            Physical::Ansi => ("RT100 (US ANSI)", include_str!("../layouts/rt100-us-ansi.json")),
        };
        Self::parse(name, include_str!("../layouts/rt100-keymap.json"), kle).expect("bundled layout is valid")
    }

    pub fn rt100_uk_iso() -> Self {
        Self::rt100(Physical::Iso)
    }

    /// Bounding box in key units.
    pub fn size(&self) -> (f32, f32) {
        self.keys.iter().fold((0.0, 0.0), |(w, h), k| (w.max(k.x + k.w), h.max(k.y + k.h)))
    }

    pub fn key(&self, name: &str) -> Option<&PlacedKey> {
        self.keys.iter().find(|k| k.name == name)
    }
}

/// The HID usage a key sends out of the box, by layout key name, so a key that
/// was remapped (e.g. for a macro) can be put back. `None` for Fn/Dial.
pub fn default_usage(name: &str) -> Option<u8> {
    let b = name.as_bytes();
    if b.len() == 1 && b[0].is_ascii_uppercase() {
        return Some(0x04 + (b[0] - b'A'));
    }
    if let Some(d) = name.strip_prefix("NUMROW_").and_then(|d| d.parse::<u8>().ok()) {
        return Some(if d == 0 { 0x27 } else { 0x1D + d });
    }
    if let Some(n) = name.strip_prefix('F').and_then(|n| n.parse::<u8>().ok()).filter(|n| (1..=12).contains(n)) {
        return Some(0x39 + n);
    }
    if let Some(d) = name.strip_prefix("NUMPAD_").and_then(|d| d.parse::<u8>().ok()) {
        return Some(if d == 0 { 0x62 } else { 0x58 + d });
    }
    Some(match name {
        "ESC" => 0x29,
        "BACKQUOTE" => 0x35,
        "TAB" => 0x2B,
        "CAPS" => 0x39,
        "LEFT_SHIFT" => 0xE1,
        "LEFT_CTRL" => 0xE0,
        "LEFT_WIN" => 0xE3,
        "LEFT_ALT" => 0xE2,
        "SPACE" => 0x2C,
        "RIGHT_ALT" => 0xE6,
        "RIGHT_CTRL" => 0xE4,
        "RIGHT_SHIFT" => 0xE5,
        "NUMROW_MINUS" => 0x2D,
        "NUMROW_EQUAL" => 0x2E,
        "BACKSPACE" => 0x2A,
        "OPEN_SQBR" => 0x2F,
        "CLOSE_SQBR" => 0x30,
        "ANSI_BACKSLASH" => 0x31,
        "HASH" => 0x32,
        "BACKSLASH" => 0x64,
        "SEMICOLON" => 0x33,
        "QUOTE" => 0x34,
        "COMMA" => 0x36,
        "DOT" => 0x37,
        "SLASH" => 0x38,
        "ENTER" => 0x28,
        "DEL" => 0x4C,
        "HOME" => 0x4A,
        "END" => 0x4D,
        "PGUP" => 0x4B,
        "PGDOWN" => 0x4E,
        "UP" => 0x52,
        "DOWN" => 0x51,
        "LEFT" => 0x50,
        "RIGHT" => 0x4F,
        "NUMLOCK" => 0x53,
        "NUMPAD_SLASH" => 0x54,
        "NUMPAD_ASTERISK" => 0x55,
        "NUMPAD_MINUS" => 0x56,
        "NUMPAD_PLUS" => 0x57,
        "NUMPAD_ENTER" => 0x58,
        "NUMPAD_DOT" => 0x63,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_layout_parses() {
        let l = Layout::rt100_uk_iso();
        assert!(l.keys.len() > 90);
        let esc = l.key("ESC").unwrap();
        assert_eq!((esc.x, esc.y, esc.index), (0.0, 0.0, 0));
        // "x": 1 gap after Esc
        assert_eq!(l.key("F1").unwrap().x, 2.0);
        let (w, h) = l.size();
        assert!(w > 18.0 && h > 5.0, "{w}x{h}");
        // Left Ctrl is index 5 (first column) and sits bottom-left.
        let ctrl = l.key("LEFT_CTRL").unwrap();
        assert_eq!((ctrl.x, ctrl.index), (0.0, 5));
    }

    #[test]
    fn ansi_layout_matches_iso_outline() {
        let iso = Layout::rt100(Physical::Iso);
        let ansi = Layout::rt100(Physical::Ansi);
        assert_eq!(iso.size(), ansi.size());
        assert!(ansi.key("HASH").is_none() && ansi.key("BACKSLASH").is_none());
        assert_eq!(ansi.keys.len(), iso.keys.len() - 1);
        let enter = ansi.key("ENTER").unwrap();
        assert_eq!((enter.w, enter.h), (2.25, 1.0));
        assert_eq!(ansi.key("LEFT_SHIFT").unwrap().w, 2.25);
        // Numpad stays put.
        assert_eq!(ansi.key("NUMPAD_4").unwrap().x, iso.key("NUMPAD_4").unwrap().x);
        assert_eq!(ansi.key("NUMPAD_7").unwrap().x, iso.key("NUMPAD_7").unwrap().x);
        // No duplicate indices on either board.
        for l in [&iso, &ansi] {
            let mut idx: Vec<u8> = l.keys.iter().map(|k| k.index).collect();
            idx.sort_unstable();
            idx.dedup();
            assert_eq!(idx.len(), l.keys.len(), "{}", l.name);
        }
    }

    #[test]
    fn rt85_is_a_75_percent_board() {
        let ansi = Layout::rt85(Physical::Ansi);
        let iso = Layout::rt85(Physical::Iso);
        assert_eq!(ansi.keys.len(), 82);
        assert_eq!(iso.keys.len(), 83);
        for l in [&ansi, &iso] {
            assert!(l.key("NUMLOCK").is_none(), "no numpad on a 75% board");
            let (w, h) = l.size();
            assert_eq!((w, h), (16.25, 6.25), "{}", l.name);
            // Nav column lines up.
            let x = l.key("DEL").unwrap().x;
            for k in ["HOME", "PGUP", "PGDOWN", "END", "RIGHT"] {
                assert_eq!(l.key(k).unwrap().x, x, "{k} in {}", l.name);
            }
            assert_eq!(l.key("UP").unwrap().x, l.key("DOWN").unwrap().x);
            let mut idx: Vec<u8> = l.keys.iter().map(|k| k.index).collect();
            idx.sort_unstable();
            idx.dedup();
            assert_eq!(idx.len(), l.keys.len(), "duplicate index in {}", l.name);
        }
        assert_eq!(Board::for_model(Some("RT100")), Board::Rt100);
        assert_eq!(Board::for_model(None), Board::Rt85);
    }

    #[test]
    fn every_key_has_a_default_usage() {
        for l in [Layout::rt85(Physical::Ansi), Layout::rt85(Physical::Iso), Layout::rt100(Physical::Iso)] {
            for k in &l.keys {
                if k.name != "FN" && k.name != "DIAL" {
                    assert!(default_usage(&k.name).is_some(), "{}", k.name);
                }
            }
        }
        assert_eq!(default_usage("A"), Some(0x04));
        assert_eq!(default_usage("NUMROW_0"), Some(0x27));
        assert_eq!(default_usage("F12"), Some(0x45));
        assert_eq!(default_usage("NUMPAD_1"), Some(0x59));
        assert_eq!(default_usage("NUMPAD_0"), Some(0x62));
    }

    #[test]
    fn keycodes_follow_layout() {
        assert!(!Physical::Ansi.keycodes().iter().any(|(c, _)| *c == 0x32 || *c == 0x64));
        assert!(Physical::Iso.keycodes().contains(&(0x32, "#")));
    }
}
