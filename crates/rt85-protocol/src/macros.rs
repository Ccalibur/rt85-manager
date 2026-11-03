//! Macro definitions and how they turn into key presses.
//!
//! The firmware's own macro command hasn't been reverse-engineered, so macros
//! are played by the host: a key is remapped (on the keyboard) to an unused
//! F13–F24 code, and the app types the steps when it sees that code.

use crate::layout::Physical;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    #[serde(rename = "super")]
    pub meta: bool,
}

impl Mods {
    pub const SHIFT: Mods = Mods { ctrl: false, shift: true, alt: false, meta: false };

    /// Left-hand modifier usages that are held.
    pub fn usages(self) -> Vec<u8> {
        [(self.ctrl, 0xE0), (self.shift, 0xE1), (self.alt, 0xE2), (self.meta, 0xE3)]
            .into_iter()
            .filter_map(|(on, u)| on.then_some(u))
            .collect()
    }

    pub fn label(self) -> String {
        let mut s = String::new();
        for (on, n) in [(self.ctrl, "Ctrl+"), (self.shift, "Shift+"), (self.alt, "Alt+"), (self.meta, "Super+")] {
            if on {
                s += n;
            }
        }
        s
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Step {
    /// Tap a key (HID usage), with modifiers held.
    Key { usage: u8, #[serde(default)] mods: Mods },
    /// Type text.
    Text { text: String },
    /// Wait.
    Delay { ms: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Macro {
    pub name: String,
    /// F13–F24 usage the keyboard sends for this macro.
    pub trigger: u8,
    /// LED/matrix index of the key remapped to `trigger`, once assigned.
    #[serde(default)]
    pub key: Option<u8>,
    pub steps: Vec<Step>,
}

/// One low-level action for the virtual keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Down(u8),
    Up(u8),
    Wait(u32),
}

impl Macro {
    /// The press/release/wait sequence. `layout` picks how text maps to keys,
    /// which must match the layout the OS is set to.
    pub fn actions(&self, layout: Physical) -> Vec<Action> {
        let mut out = Vec::new();
        let tap = |out: &mut Vec<Action>, usage: u8, mods: Mods| {
            let mods = mods.usages();
            out.extend(mods.iter().map(|&m| Action::Down(m)));
            out.push(Action::Down(usage));
            out.push(Action::Up(usage));
            out.extend(mods.iter().rev().map(|&m| Action::Up(m)));
        };
        for step in &self.steps {
            match step {
                Step::Key { usage, mods } => tap(&mut out, *usage, *mods),
                Step::Text { text } => {
                    for c in text.chars() {
                        if let Some((usage, mods)) = char_to_key(c, layout) {
                            tap(&mut out, usage, mods);
                        }
                    }
                }
                Step::Delay { ms } => out.push(Action::Wait(*ms)),
            }
        }
        out
    }
}

/// The key (and Shift) that types `c` on a US or UK keyboard layout.
pub fn char_to_key(c: char, layout: Physical) -> Option<(u8, Mods)> {
    let plain = Mods::default();
    let shift = Mods::SHIFT;
    if c.is_ascii_lowercase() {
        return Some((0x04 + (c as u8 - b'a'), plain));
    }
    if c.is_ascii_uppercase() {
        return Some((0x04 + (c as u8 - b'A'), shift));
    }
    if let Some(d) = c.to_digit(10) {
        return Some((if d == 0 { 0x27 } else { 0x1D + d as u8 }, plain));
    }
    let uk = layout == Physical::Iso;
    let k = match c {
        ' ' => (0x2C, plain),
        '\n' => (0x28, plain),
        '\t' => (0x2B, plain),
        '!' => (0x1E, shift),
        '"' if uk => (0x1F, shift),
        '@' if !uk => (0x1F, shift),
        '£' if uk => (0x20, shift),
        '#' if !uk => (0x20, shift),
        '$' => (0x21, shift),
        '%' => (0x22, shift),
        '^' => (0x23, shift),
        '&' => (0x24, shift),
        '*' => (0x25, shift),
        '(' => (0x26, shift),
        ')' => (0x27, shift),
        '-' => (0x2D, plain),
        '_' => (0x2D, shift),
        '=' => (0x2E, plain),
        '+' => (0x2E, shift),
        '[' => (0x2F, plain),
        '{' => (0x2F, shift),
        ']' => (0x30, plain),
        '}' => (0x30, shift),
        ';' => (0x33, plain),
        ':' => (0x33, shift),
        '\'' => (0x34, plain),
        '@' => (0x34, shift),  // UK
        '"' => (0x34, shift),  // US
        '#' => (0x32, plain),  // UK
        '~' if uk => (0x32, shift),
        '~' => (0x35, shift),
        '`' => (0x35, plain),
        '¬' if uk => (0x35, shift),
        '\\' if uk => (0x64, plain),
        '|' if uk => (0x64, shift),
        '\\' => (0x31, plain),
        '|' => (0x31, shift),
        ',' => (0x36, plain),
        '<' => (0x36, shift),
        '.' => (0x37, plain),
        '>' => (0x37, shift),
        '/' => (0x38, plain),
        '?' => (0x38, shift),
        _ => return None,
    };
    Some(k)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_maps_per_layout() {
        assert_eq!(char_to_key('@', Physical::Ansi), Some((0x1F, Mods::SHIFT)));
        assert_eq!(char_to_key('@', Physical::Iso), Some((0x34, Mods::SHIFT)));
        assert_eq!(char_to_key('"', Physical::Iso), Some((0x1F, Mods::SHIFT)));
        assert_eq!(char_to_key('#', Physical::Iso), Some((0x32, Mods::default())));
        assert_eq!(char_to_key('\\', Physical::Iso), Some((0x64, Mods::default())));
        assert_eq!(char_to_key('\\', Physical::Ansi), Some((0x31, Mods::default())));
        assert_eq!(char_to_key('0', Physical::Ansi), Some((0x27, Mods::default())));
        assert_eq!(char_to_key('1', Physical::Ansi), Some((0x1E, Mods::default())));
        assert_eq!(char_to_key('é', Physical::Ansi), None);
    }

    #[test]
    fn actions_hold_modifiers_around_the_key() {
        let m = Macro {
            name: "copy".into(),
            trigger: 0x68,
            key: None,
            steps: vec![
                Step::Key { usage: 0x06, mods: Mods { ctrl: true, ..Default::default() } },
                Step::Delay { ms: 50 },
                Step::Text { text: "Hi".into() },
            ],
        };
        use Action::*;
        assert_eq!(
            m.actions(Physical::Ansi),
            vec![
                Down(0xE0), Down(0x06), Up(0x06), Up(0xE0),
                Wait(50),
                Down(0xE1), Down(0x0B), Up(0x0B), Up(0xE1),
                Down(0x0C), Up(0x0C),
            ]
        );
    }

    #[test]
    fn serde_round_trip() {
        let m = Macro { name: "x".into(), trigger: 0x69, key: Some(3), steps: vec![Step::Delay { ms: 5 }] };
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains(r#""type":"delay""#));
        assert_eq!(serde_json::from_str::<Macro>(&json).unwrap(), m);
    }
}
