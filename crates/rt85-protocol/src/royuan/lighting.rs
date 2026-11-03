//! Built-in lighting effects (`0x07`). Both families use the same report
//! (`07 effect speed brightness option|colour-mode R G B cs`, checksum at
//! byte 8), but they number the effects differently, and gen2 sends the speed
//! reversed (`4 - speed`).

use super::{Family, raw};
use crate::{Report, checksum::ones_complement_sum};
use serde::{Deserialize, Serialize};

/// One built-in effect as the UI shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Effect {
    /// Effect ID on the wire.
    pub id: u8,
    pub label: &'static str,
    /// Takes a speed.
    pub speed: bool,
    /// Takes a colour (and the rainbow switch).
    pub color: bool,
    /// Effect-specific choices (direction, picture bank, ...), sent in the
    /// high nibble of the option byte.
    pub options: &'static [&'static str],
}

impl Effect {
    /// Name for the command line: lower case, words joined with `-`.
    pub fn slug(&self) -> String {
        self.label.to_lowercase().replace(' ', "-")
    }
}

const DIRECTIONS: &[&str] = &["Direction 1", "Direction 2", "Direction 3", "Direction 4"];

const fn fx(id: u8, label: &'static str, speed: bool, color: bool, options: &'static [&'static str]) -> Effect {
    Effect { id, label, speed, color, options }
}

/// RT100 (yc500) effects, from `strodgers/epomaker-controller`.
pub const YC500_EFFECTS: &[Effect] = &[
    fx(0x01, "Always on", true, true, DIRECTIONS),
    fx(0x02, "Breathing", true, true, DIRECTIONS),
    fx(0x03, "Spectrum cycle", true, true, DIRECTIONS),
    fx(0x04, "Drift", true, true, DIRECTIONS),
    fx(0x05, "Waves ripple", true, true, DIRECTIONS),
    fx(0x06, "Stars twinkle", true, true, DIRECTIONS),
    fx(0x07, "Steady stream", true, true, DIRECTIONS),
    fx(0x08, "Shadowing", true, true, DIRECTIONS),
    fx(0x09, "Peaks rising", true, true, DIRECTIONS),
    fx(0x0A, "Sine wave", true, true, DIRECTIONS),
    fx(0x0B, "Caispring surging", true, true, DIRECTIONS),
    fx(0x0C, "Flowers blooming", true, true, DIRECTIONS),
    fx(0x0E, "Laser", true, true, DIRECTIONS),
    fx(0x0F, "Peak turn", true, true, DIRECTIONS),
    fx(0x10, "Inclined rain", true, true, DIRECTIONS),
    fx(0x11, "Snow", true, true, DIRECTIONS),
    fx(0x12, "Meteor", true, true, DIRECTIONS),
    fx(0x13, "Through the snow", true, true, DIRECTIONS),
    fx(0x15, "Light shadow", true, true, DIRECTIONS),
];

/// Effect 13 shows a user picture (per-key colours); option = bank 0..4.
pub const GEN2_PICTURE: u8 = 13;
const GEN2_SCREEN_COLOR: u8 = 21;
const GEN2_MUSIC: u8 = 22;

/// RT85 (gen2) main backlight effects: the 22 the vendor app offers for the
/// RT85, with its IDs, option names and speed/colour support.
pub const GEN2_EFFECTS: &[Effect] = &[
    fx(1, "Solid", false, true, &[]),
    fx(2, "Breathing", true, true, &[]),
    fx(3, "Neon", true, false, &[]),
    fx(4, "Wave", true, true, &["Right", "Left", "Down", "Up"]),
    fx(5, "Ripple", true, true, &[]),
    fx(6, "Raindrop", true, true, &[]),
    fx(7, "Snake", true, true, &["Zigzag", "Spiral"]),
    fx(8, "Reactive", true, true, &[]),
    fx(9, "Converge", true, true, &[]),
    fx(10, "Sine wave", true, true, &[]),
    fx(11, "Kaleidoscope", true, true, &["Outward", "Inward"]),
    fx(12, "Line wave", true, true, &["Right", "Left"]),
    fx(14, "Laser", true, true, &[]),
    fx(15, "Circle wave", true, true, &["Counter-clockwise", "Clockwise"]),
    fx(16, "Dazzling", true, true, &[]),
    fx(17, "Rain", true, true, &[]),
    fx(18, "Meteor", true, true, &[]),
    fx(19, "Reactive off", true, true, &[]),
    fx(GEN2_PICTURE, "User picture", false, false, &["Picture 1", "Picture 2", "Picture 3", "Picture 4", "Picture 5"]),
    fx(GEN2_MUSIC, "Music", false, true, &["Upright", "Separate", "Intersect"]),
    fx(GEN2_SCREEN_COLOR, "Screen colour", false, false, &[]),
    fx(0, "Off", false, false, &[]),
];

pub fn effects(family: Family) -> &'static [Effect] {
    match family {
        Family::Yc500 => YC500_EFFECTS,
        Family::Gen2 => GEN2_EFFECTS,
    }
}

pub fn effect(family: Family, id: u8) -> Option<&'static Effect> {
    effects(family).iter().find(|e| e.id == id)
}

pub fn max_speed(family: Family) -> u8 {
    match family {
        Family::Yc500 => 5,
        Family::Gen2 => 4,
    }
}

pub const MAX_BRIGHTNESS: u8 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    /// Effect ID (see [`effects`]).
    pub effect: u8,
    /// 0..=[`max_speed`]
    pub speed: u8,
    /// 0..=4
    pub brightness: u8,
    /// Rainbow ("dazzle") colours instead of `rgb`.
    pub dazzle: bool,
    /// Index into the effect's `options` (direction, picture bank, ...).
    pub option: u8,
    pub rgb: (u8, u8, u8),
}

impl Default for Profile {
    fn default() -> Self {
        Self { effect: 1, speed: 4, brightness: 4, dazzle: false, option: 0, rgb: (255, 255, 255) }
    }
}

impl Profile {
    pub fn report(&self, family: Family) -> Report {
        let option = (self.option & 0x0F) << 4;
        let colour_mode = if self.dazzle { 0x08 } else { 0x07 };
        let brightness = self.brightness.min(MAX_BRIGHTNESS);
        let body = match family {
            Family::Yc500 => {
                let (r, g, b) = self.rgb;
                [0x07, self.effect, self.speed.min(5), brightness, option | colour_mode, r, g, b]
            }
            Family::Gen2 => {
                let speed = 4 - self.speed.min(4);
                // The vendor app never sends pure white; it uses FA FF FA.
                let (r, g, b) = if self.rgb == (255, 255, 255) { (0xFA, 0xFF, 0xFA) } else { self.rgb };
                match self.effect {
                    GEN2_PICTURE => [0x07, self.effect, speed, brightness, option, 0, 200, 200],
                    GEN2_SCREEN_COLOR => [0x07, self.effect, speed, brightness, 0, r, g, b],
                    GEN2_MUSIC => [0x07, self.effect, speed, brightness, option | if self.dazzle { 0 } else { 4 }, r, g, b],
                    _ => [0x07, self.effect, speed, brightness, option | colour_mode, r, g, b],
                }
            }
        };
        let mut r = raw(&body);
        r[8] = ones_complement_sum(&body); // checksum at byte 8 for this command
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::royuan::hex;

    #[test]
    fn matches_capture() {
        // tests/data/EpomakerCommand-cycle-light-modes-command.txt, line 1
        let p = Profile { effect: 0x01, rgb: (0xB4, 0xB4, 0xB4), ..Default::default() };
        assert!(hex(&p.report(Family::Yc500)).starts_with("0701040407b4b4b4cc000000"));
        // line 3: spectrum cycle, speed 2, black
        let p = Profile { effect: 0x03, speed: 2, rgb: (0, 0, 0), ..Default::default() };
        assert!(hex(&p.report(Family::Yc500)).starts_with("0703020407000000e8"));
    }

    #[test]
    fn gen2_matches_vendor_encoder() {
        // The vendor's solid-white example: speed byte 4 (UI speed 0), white as FA FF FA.
        let p = Profile { effect: 1, speed: 0, ..Default::default() };
        let r = p.report(Family::Gen2);
        assert_eq!(&r[..8], &[0x07, 1, 4, 4, 0x07, 0xFA, 0xFF, 0xFA]);
        assert_eq!(r[8], ones_complement_sum(&r[..8]));
        // Wave left, rainbow.
        let p = Profile { effect: 4, speed: 3, option: 1, dazzle: true, ..Default::default() };
        assert_eq!(&p.report(Family::Gen2)[..5], &[0x07, 4, 1, 4, 0x18]);
        // Picture bank 2.
        let p = Profile { effect: GEN2_PICTURE, option: 2, ..Default::default() };
        assert_eq!(&p.report(Family::Gen2)[4..8], &[0x20, 0, 200, 200]);
    }

    #[test]
    fn rt85_offers_the_vendor_effects() {
        assert_eq!(GEN2_EFFECTS.len(), 22);
        let mut ids: Vec<u8> = GEN2_EFFECTS.iter().map(|e| e.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 22);
        assert!(effect(Family::Gen2, 20).is_none(), "music-3 is not an RT85 effect");
    }
}
