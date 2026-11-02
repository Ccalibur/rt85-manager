//! Built-in lighting effects ("profile" command `0x07`).

use super::raw;
use crate::{Report, checksum::ones_complement_sum};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Mode {
    AlwaysOn = 0x01,
    Breathing = 0x02,
    SpectrumCycle = 0x03,
    Drift = 0x04,
    WavesRipple = 0x05,
    StarsTwinkle = 0x06,
    SteadyStream = 0x07,
    Shadowing = 0x08,
    PeaksRising = 0x09,
    SineWave = 0x0A,
    CaispringSurging = 0x0B,
    FlowersBlooming = 0x0C,
    Laser = 0x0E,
    PeakTurn = 0x0F,
    InclinedRain = 0x10,
    Snow = 0x11,
    Meteor = 0x12,
    ThroughTheSnow = 0x13,
    LightShadow = 0x15,
}

impl Mode {
    pub const ALL: &[Mode] = &[
        Mode::AlwaysOn, Mode::Breathing, Mode::SpectrumCycle, Mode::Drift, Mode::WavesRipple,
        Mode::StarsTwinkle, Mode::SteadyStream, Mode::Shadowing, Mode::PeaksRising, Mode::SineWave,
        Mode::CaispringSurging, Mode::FlowersBlooming, Mode::Laser, Mode::PeakTurn,
        Mode::InclinedRain, Mode::Snow, Mode::Meteor, Mode::ThroughTheSnow, Mode::LightShadow,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Mode::AlwaysOn => "Always on",
            Mode::Breathing => "Breathing",
            Mode::SpectrumCycle => "Spectrum cycle",
            Mode::Drift => "Drift",
            Mode::WavesRipple => "Waves ripple",
            Mode::StarsTwinkle => "Stars twinkle",
            Mode::SteadyStream => "Steady stream",
            Mode::Shadowing => "Shadowing",
            Mode::PeaksRising => "Peaks rising",
            Mode::SineWave => "Sine wave",
            Mode::CaispringSurging => "Caispring surging",
            Mode::FlowersBlooming => "Flowers blooming",
            Mode::Laser => "Laser",
            Mode::PeakTurn => "Peak turn",
            Mode::InclinedRain => "Inclined rain",
            Mode::Snow => "Snow",
            Mode::Meteor => "Meteor",
            Mode::ThroughTheSnow => "Through the snow",
            Mode::LightShadow => "Light shadow",
        }
    }
}

/// Effect direction, stored in the high nibble of the option byte. Its
/// meaning depends on the mode (e.g. drift left/right, bloom in/out).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Direction {
    #[default]
    D0,
    D1,
    D2,
    D3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub mode: Mode,
    /// 0..=5
    pub speed: u8,
    /// 0..=4
    pub brightness: u8,
    /// Rainbow ("dazzle") colours instead of `rgb`.
    pub dazzle: bool,
    pub direction: Direction,
    pub rgb: (u8, u8, u8),
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            mode: Mode::AlwaysOn,
            speed: 4,
            brightness: 4,
            dazzle: false,
            direction: Direction::D0,
            rgb: (255, 255, 255),
        }
    }
}

pub const MAX_SPEED: u8 = 5;
pub const MAX_BRIGHTNESS: u8 = 4;

impl Profile {
    pub fn report(&self) -> Report {
        let option = (self.direction as u8) << 4 | if self.dazzle { 0x08 } else { 0x07 };
        let body = [
            0x07,
            self.mode as u8,
            self.speed.min(MAX_SPEED),
            self.brightness.min(MAX_BRIGHTNESS),
            option,
            self.rgb.0,
            self.rgb.1,
            self.rgb.2,
        ];
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
        let p = Profile { mode: Mode::AlwaysOn, rgb: (0xB4, 0xB4, 0xB4), ..Default::default() };
        assert!(hex(&p.report()).starts_with("0701040407b4b4b4cc000000"));
        // line 3: spectrum cycle, speed 2, black
        let p = Profile { mode: Mode::SpectrumCycle, speed: 2, rgb: (0, 0, 0), ..Default::default() };
        assert!(hex(&p.report()).starts_with("0703020407000000e8"));
    }
}
