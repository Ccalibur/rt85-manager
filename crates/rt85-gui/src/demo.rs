//! Demo mode (`RT85_DEMO=1`): pretends an RT85 is connected
//! so the UI can be explored and screenshotted without hardware. Nothing is
//! sent anywhere.
//!
//! `RT85_SCREENSHOT=out.png` saves the app's own window to a PNG after a short
//! delay and exits (used for the README screenshots).
//!
//! `RT85_LIVE=media|fps|both` turns the live screen on at start.

use crate::worker::DeviceSummary;
use rt85_device::Capabilities;
use rt85_protocol::ids::{Link, Platform, ROYUAN_VID};

pub fn enabled() -> bool {
    std::env::var_os("RT85_DEMO").is_some_and(|v| v != "0")
}

pub fn screenshot_path() -> Option<std::path::PathBuf> {
    std::env::var_os("RT85_SCREENSHOT").map(Into::into)
}

/// `RT85_LIVE=media|fps|both` starts with the live screen on.
pub fn live_mode() -> crate::live::Mode {
    use crate::live::Mode;
    match std::env::var("RT85_LIVE").as_deref() {
        Ok("media") => Mode::Media,
        Ok("fps") => Mode::Fps,
        Ok("both") => Mode::Both,
        _ => Mode::Off,
    }
}

/// `RT85_LAYOUT=ansi|iso` picks the keyboard layout without asking.
pub fn layout() -> Option<rt85_protocol::layout::Physical> {
    std::env::var("RT85_LAYOUT").ok()?.parse().ok()
}

/// Image to preload on the Screen page.
pub fn image_path() -> Option<std::path::PathBuf> {
    std::env::var_os("RT85_DEMO_IMAGE").map(Into::into)
}

pub fn device() -> DeviceSummary {
    DeviceSummary {
        name: "Epomaker RT85 (demo)".into(),
        vid: ROYUAN_VID,
        pid: 0x5002,
        platform: Platform::Royuan,
        link: Link::Wired,
        model: Some("RT85"),
        board: rt85_protocol::layout::Board::Rt85,
        firmware: None,
        manufacturer: "Demo mode, nothing is sent".into(),
        interfaces: vec!["#2 /dev/hidraw-demo [ffff:0002]".into()],
        hid_paths: Vec::new(),
        caps: Capabilities::for_device(Platform::Royuan, rt85_protocol::layout::Board::Rt85),
    }
}
