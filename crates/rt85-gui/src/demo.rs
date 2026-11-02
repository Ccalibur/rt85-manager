//! Demo mode (`RT85_DEMO=1`): pretends an RT100-family keyboard is connected
//! so the UI can be explored and screenshotted without hardware. Nothing is
//! sent anywhere.
//!
//! `RT85_SCREENSHOT=out.png` saves the app's own window to a PNG after a short
//! delay and exits (used for the README screenshots).

use crate::worker::DeviceSummary;
use rt85_device::Capabilities;
use rt85_protocol::ids::{Link, Platform, ROYUAN_VID};

pub fn enabled() -> bool {
    std::env::var_os("RT85_DEMO").is_some_and(|v| v != "0")
}

pub fn screenshot_path() -> Option<std::path::PathBuf> {
    std::env::var_os("RT85_SCREENSHOT").map(Into::into)
}

/// Image to preload on the Screen page.
pub fn image_path() -> Option<std::path::PathBuf> {
    std::env::var_os("RT85_DEMO_IMAGE").map(Into::into)
}

pub fn device() -> DeviceSummary {
    DeviceSummary {
        name: "Epomaker keyboard (demo)".into(),
        vid: ROYUAN_VID,
        pid: 0x4010,
        platform: Platform::Royuan,
        link: Link::Wired,
        model: Some("Demo"),
        manufacturer: "Demo mode, nothing is sent".into(),
        interfaces: vec!["#1 /dev/hidraw-demo [ff00:0001]".into()],
        caps: Capabilities::for_platform(Platform::Royuan),
    }
}
