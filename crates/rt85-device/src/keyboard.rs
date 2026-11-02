//! One handle over both platforms, plus what each can do.

use crate::{
    Error, FoundDevice, Progress, Result, rdmctmzt::RdmctmztKeyboard, royuan::RoyuanKeyboard,
};
use hidapi::HidApi;
use rt85_protocol::{
    ids::{Link, Platform},
    rdmctmzt::Qgif,
    royuan::{keys::RgbFrame, lighting::Profile, screen::{Animation, RT100_SCREEN, ScreenSpec}},
    time::DateTime,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    pub lighting_modes: bool,
    pub per_key_rgb: bool,
    pub remap: bool,
    pub time_sync: bool,
    pub telemetry: bool,
    /// Upload decoded images/GIFs (RGB565).
    pub screen_rgb565: bool,
    /// Upload pre-encoded QGIF files.
    pub screen_qgif: bool,
    pub screen_clear: bool,
    /// Size of the uploadable image area, when known.
    pub screen: Option<ScreenSpec>,
}

impl Capabilities {
    pub fn for_platform(platform: Platform) -> Self {
        match platform {
            Platform::Royuan => Capabilities {
                lighting_modes: true,
                per_key_rgb: true,
                remap: true,
                time_sync: true,
                telemetry: true,
                screen_rgb565: true,
                screen_qgif: false,
                screen_clear: true,
                // TODO: RT85-specific size once confirmed.
                screen: Some(RT100_SCREEN),
            },
            Platform::Rdmctmzt => Capabilities {
                time_sync: true,
                screen_qgif: true,
                screen: Some(ScreenSpec { width: 240, height: 135 }),
                ..Default::default()
            },
        }
    }
}

pub enum Keyboard {
    Royuan(RoyuanKeyboard),
    Rdmctmzt(RdmctmztKeyboard),
}

impl Keyboard {
    pub fn open(api: &HidApi, dev: &FoundDevice) -> Result<Self> {
        let iface = dev.config_interface().ok_or(Error::NotFound)?;
        Self::open_path(api, dev, &iface.path)
    }

    /// Open a specific interface (to try another one when probing).
    pub fn open_path(api: &HidApi, dev: &FoundDevice, path: &std::ffi::CStr) -> Result<Self> {
        Ok(match dev.platform {
            Platform::Royuan => {
                let mut kb = RoyuanKeyboard::open(api, path)?;
                if dev.link() == Link::Dongle && !kb.wireless_init()? {
                    tracing::warn!("keyboard did not answer the 2.4 GHz wake-up sequence");
                }
                Keyboard::Royuan(kb)
            }
            Platform::Rdmctmzt => Keyboard::Rdmctmzt(RdmctmztKeyboard::new(path.to_owned())),
        })
    }

    pub fn capabilities(&self) -> Capabilities {
        Capabilities::for_platform(match self {
            Keyboard::Royuan(_) => Platform::Royuan,
            Keyboard::Rdmctmzt(_) => Platform::Rdmctmzt,
        })
    }


    fn royuan(&mut self, what: &'static str) -> Result<&mut RoyuanKeyboard> {
        match self {
            Keyboard::Royuan(k) => Ok(k),
            _ => Err(Error::Unsupported(what)),
        }
    }

    pub fn set_profile(&mut self, p: &Profile) -> Result<()> {
        self.royuan("lighting modes")?.set_profile(p)
    }

    pub fn set_key_colors(&mut self, frames: &[RgbFrame]) -> Result<()> {
        self.royuan("per-key RGB")?.set_key_colors(frames)
    }

    pub fn remap(&mut self, key_index: u8, hid_code: u8) -> Result<()> {
        self.royuan("key remapping")?.remap(key_index, hid_code)
    }

    pub fn upload_animation(&mut self, a: &Animation, progress: Progress) -> Result<()> {
        self.royuan("RGB565 image upload")?.upload(a, progress)
    }

    pub fn upload_qgif(&mut self, api: &mut HidApi, q: &Qgif, now: DateTime, progress: Progress) -> Result<()> {
        match self {
            Keyboard::Rdmctmzt(k) => k.upload_qgif(api, q, now, progress),
            _ => Err(Error::Unsupported("QGIF upload")),
        }
    }

    pub fn clear_screen(&mut self) -> Result<()> {
        self.royuan("clearing the screen")?.clear_screen()
    }

    pub fn set_time(&mut self, api: &mut HidApi, t: DateTime) -> Result<()> {
        match self {
            Keyboard::Royuan(k) => k.set_time(t),
            Keyboard::Rdmctmzt(k) => k.sync_time(api, t),
        }
    }

    pub fn set_cpu(&mut self, percent: u8) -> Result<()> {
        self.royuan("CPU widget")?.set_cpu(percent)
    }

    pub fn set_temperature(&mut self, celsius: u8) -> Result<()> {
        self.royuan("temperature widget")?.set_temperature(celsius)
    }
}
