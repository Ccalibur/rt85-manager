//! One handle over both platforms, plus what each can do.

use crate::{
    Error, FoundDevice, Progress, Result, rdmctmzt::RdmctmztKeyboard, royuan::RoyuanKeyboard,
};
use hidapi::HidApi;
use rt85_protocol::{
    ids::{Link, Platform},
    rdmctmzt::Qgif,
    layout::Board,
    royuan::{Family, gen2::Identity, keys::RgbFrame, lighting::Profile, screen::{self, Animation, ScreenSpec}},
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
    /// Clear the uploaded image (yc500).
    pub screen_clear: bool,
    /// Erase the screen's pictures (gen2 `0xAC`, the vendor "Clear Screen").
    pub screen_erase: bool,
    /// Size of the uploadable image area, when known.
    pub screen: Option<ScreenSpec>,
    /// Royuan command set (decides the lighting effects on offer).
    pub family: Option<Family>,
    /// Screen language switch (gen2).
    pub screen_language: bool,
}

impl Capabilities {
    /// What a board can do. The screen size follows the board, since the
    /// RT85 and some RT100 batches share a USB ID.
    pub fn for_device(platform: Platform, board: Board) -> Self {
        match platform {
            Platform::Royuan => {
                let gen2 = board.family() == Family::Gen2;
                Capabilities {
                    lighting_modes: true,
                    per_key_rgb: true,
                    remap: true,
                    time_sync: true,
                    // The RT85 has no system-info page, and its 0xAC erases flash.
                    telemetry: !gen2,
                    screen_rgb565: true,
                    screen_qgif: false,
                    screen_clear: !gen2,
                    screen_erase: gen2,
                    screen: Some(screen::spec_for(board)),
                    family: Some(board.family()),
                    screen_language: gen2,
                }
            }
            Platform::Rdmctmzt => Capabilities {
                time_sync: true,
                screen_qgif: true,
                // QGIF files carry their own frames; only the size is used here.
                screen: Some(ScreenSpec { width: 240, height: 135, max_frames: 0, animation_align: 1 }),
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
    /// Open the keyboard, speaking `board`'s command set.
    pub fn open(api: &HidApi, dev: &FoundDevice, board: Board) -> Result<Self> {
        let iface = dev.config_interface().ok_or(Error::NotFound)?;
        Self::open_path(api, dev, &iface.path, board)
    }

    /// Open a specific interface (to try another one when probing).
    pub fn open_path(api: &HidApi, dev: &FoundDevice, path: &std::ffi::CStr, board: Board) -> Result<Self> {
        Ok(match dev.platform {
            Platform::Royuan => {
                let mut kb = RoyuanKeyboard::open(api, path, board.family())?;
                if dev.link() == Link::Dongle && !kb.wireless_init()? {
                    tracing::warn!("keyboard did not answer the 2.4 GHz wake-up sequence");
                }
                Keyboard::Royuan(kb)
            }
            Platform::Rdmctmzt => Keyboard::Rdmctmzt(RdmctmztKeyboard::new(path.to_owned())),
        })
    }

    pub fn capabilities(&self, board: Board) -> Capabilities {
        let platform = match self {
            Keyboard::Royuan(_) => Platform::Royuan,
            Keyboard::Rdmctmzt(_) => Platform::Rdmctmzt,
        };
        Capabilities::for_device(platform, board)
    }

    /// Ask a Royuan board which model it is (command `0x8F`). Several
    /// boards share one USB ID, so this is how the RT85 is told apart.
    pub fn identify(&mut self) -> Result<Option<Identity>> {
        match self {
            Keyboard::Royuan(k) => k.identify(),
            Keyboard::Rdmctmzt(_) => Ok(None),
        }
    }

    /// Switch command set after identifying the board.
    pub fn set_board(&mut self, board: Board) {
        if let Keyboard::Royuan(k) = self {
            k.set_family(board.family());
        }
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

    pub fn clear_screen(&mut self, progress: Progress) -> Result<()> {
        self.royuan("clearing the screen")?.clear_screen(progress)
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

    pub fn toggle_language(&mut self) -> Result<()> {
        self.royuan("the screen language switch")?.toggle_language()
    }
}
