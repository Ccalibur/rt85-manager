//! Pure encoding/decoding of Epomaker HID reports. No I/O lives here, so every
//! packet builder can be unit-tested against captured traffic.
//!
//! Epomaker ships boards built on (at least) two unrelated controller platforms:
//!
//! * [`royuan`]: VID `0x3151` (RT85, RT100, TH80, ...). Uses 64-byte *feature*
//!   reports with a one-byte checksum, in two command sets
//!   ([`royuan::Family`]): yc500 (RT100, reverse-engineered by
//!   `strodgers/epomaker-controller`) and gen2 (RT85, see [`royuan::gen2`]).
//!   Both upload raw RGB565 images/GIFs to the screen.
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
