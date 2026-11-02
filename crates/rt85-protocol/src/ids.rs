//! USB identifiers for known boards on both platforms.
//!
//! The RT85's own IDs are not confirmed yet. Once `lsusb` output is available,
//! add them to the matching table below.

/// Which controller platform a board is built on. See the crate docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Platform {
    Royuan,
    Rdmctmzt,
}

/// How the board is connected to the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Link {
    Wired,
    Dongle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownId {
    pub vid: u16,
    pub pid: u16,
    pub platform: Platform,
    pub link: Link,
    pub model: &'static str,
}

pub const ROYUAN_VID: u16 = 0x3151;
pub const RDMCTMZT_VID: u16 = 0x36B0;

/// LCD sub-device that RDMCTMZT boards expose after the init sequence.
pub const RDMCTMZT_LCD_VID: u16 = 0x1919;
pub const RDMCTMZT_LCD_PID: u16 = 0x1919;
/// QMK-style raw HID usage page used for the RDMCTMZT init interface.
pub const RDMCTMZT_RAW_USAGE_PAGE: u16 = 0xFF60;

pub const KNOWN: &[KnownId] = &[
    // Royuan / RT100 family (epomaker-controller default.json)
    KnownId { vid: ROYUAN_VID, pid: 0x4010, platform: Platform::Royuan, link: Link::Wired, model: "RT100" },
    KnownId { vid: ROYUAN_VID, pid: 0x4015, platform: Platform::Royuan, link: Link::Wired, model: "RT100" },
    KnownId { vid: ROYUAN_VID, pid: 0x5002, platform: Platform::Royuan, link: Link::Wired, model: "RT100" },
    KnownId { vid: ROYUAN_VID, pid: 0x4011, platform: Platform::Royuan, link: Link::Dongle, model: "RT100" },
    KnownId { vid: ROYUAN_VID, pid: 0x4016, platform: Platform::Royuan, link: Link::Dongle, model: "RT100" },
    // RDMCTMZT / RT82 family (rt82display)
    KnownId { vid: RDMCTMZT_VID, pid: 0x30A3, platform: Platform::Rdmctmzt, link: Link::Wired, model: "RT82" },
];

/// Exact match against [`KNOWN`], falling back to a VID-only platform guess so
/// unknown boards (such as the RT85) still get detected and can be probed.
pub fn classify(vid: u16, pid: u16) -> Option<(Platform, Option<&'static KnownId>)> {
    if let Some(k) = KNOWN.iter().find(|k| k.vid == vid && k.pid == pid) {
        return Some((k.platform, Some(k)));
    }
    match vid {
        ROYUAN_VID => Some((Platform::Royuan, None)),
        RDMCTMZT_VID => Some((Platform::Rdmctmzt, None)),
        _ => None,
    }
}
