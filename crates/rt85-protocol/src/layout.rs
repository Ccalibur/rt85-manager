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

    /// Reference layout from the RT100 (until an RT85 one exists).
    pub fn rt100_uk_iso() -> Self {
        Self::parse(
            "RT100 (UK ISO)",
            include_str!("../layouts/rt100-keymap.json"),
            include_str!("../layouts/rt100-uk-iso.json"),
        )
        .expect("bundled layout is valid")
    }

    /// Bounding box in key units.
    pub fn size(&self) -> (f32, f32) {
        self.keys.iter().fold((0.0, 0.0), |(w, h), k| (w.max(k.x + k.w), h.max(k.y + k.h)))
    }

    pub fn key(&self, name: &str) -> Option<&PlacedKey> {
        self.keys.iter().find(|k| k.name == name)
    }
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
    }
}
