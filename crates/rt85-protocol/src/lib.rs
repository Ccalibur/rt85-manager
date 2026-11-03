//! Pure encoding/decoding of Epomaker HID reports. No I/O lives here, so every
//! packet builder can be unit-tested against captured traffic.
//!
//! Epomaker ships boards built on (at least) two unrelated controller platforms,
//! and the RT85 is expected to be one of them:
//!
//! * [`royuan`]: VID `0x3151` (RT100, TH80, ...). Uses 64-byte *feature* reports
//!   with a one-byte checksum. Supports lighting, per-key RGB, remapping, time,
//!   CPU/temperature widgets, and raw RGB565 image/GIF upload. Reverse-engineered
//!   by `strodgers/epomaker-controller`.
//! * [`rdmctmzt`]: VID `0x36B0` (RT82, Womier/Gamakay SK80, ...). Uses 64-byte
//!   *output* reports starting with `0xAA`. The LCD is a separate USB device
//!   (`1919:1919`) that only shows up after an init sequence, and it expects
//!   QGIF data. Reverse-engineered by `guysoft/rt82display`.

pub mod checksum;
pub mod error;
pub mod hid_report;
pub mod ids;
pub mod keycode;
pub mod layout;
pub mod macros;
pub mod rdmctmzt;
pub mod rgb565;
pub mod royuan;
pub mod time;

pub use error::ProtocolError;

/// Size of every HID report used by both platforms.
pub const REPORT_LEN: usize = 64;
pub type Report = [u8; REPORT_LEN];
