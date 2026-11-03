//! "gen2" (YC3123) command set, used by the RT85, Glyph and RT75.
//!
//! The layouts come from two independent analyses of Epomaker's 3.2.22
//! driver: `ramarivera/epomaker-driver-linux` (MIT, `docs/glyph-protocol.md`,
//! `docs/rt85.md`) and `dniminenn/sharkfin` (GPL-3, `protocol/gen2.rs`).
//! Neither has been checked against a real RT85 yet.
//!
//! Commands this module deliberately leaves out: `0xAC` erases flash on gen2
//! (the yc500 "clear screen" byte), and `0x22` (system info) isn't offered
//! on the RT85.

use super::{PAYLOAD_LEN, framed};
use crate::Report;

/// Internal model ID the keyboard reports for itself (see [`identify`]).
pub const RT85_ID: u32 = 2895;
/// RT100 (yc500) internal ID, from the vendor catalog.
pub const RT100_ID: u32 = 1379;

/// Key slots in a gen2 matrix (128 four-byte actions = 512 bytes).
pub const SLOTS: usize = 128;
/// RGB slots in a user picture: 126 × RGB = 378 bytes.
pub const PICTURE_KEYS: usize = 126;
/// User picture banks (selected with lighting effect 13, option 0..4).
pub const PICTURES: u8 = 5;

/// Identify request. Every Royuan family answers it.
pub fn identify() -> Report {
    framed([0x8F, 0, 0, 0, 0, 0, 0], &[])
}

/// What [`identify`] returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity {
    /// Internal model ID (e.g. 2895 for the RT85). Several boards share one
    /// USB ID, so this is the reliable way to tell them apart.
    pub device_id: u32,
    /// USB firmware version, e.g. `0x0102` for 1.02.
    pub firmware: u16,
    /// The board is in its bootloader.
    pub boot: bool,
}

impl Identity {
    pub fn firmware_string(&self) -> String {
        format!("{}.{:02x}", self.firmware >> 8, self.firmware & 0xFF)
    }
}

/// Parse an identify reply (64 bytes, report ID already stripped).
pub fn parse_identity(reply: &[u8]) -> Option<Identity> {
    if reply.len() < 12 || reply[0] != 0x8F {
        return None;
    }
    let device_id = u32::from_le_bytes([reply[1], reply[2], reply[3], reply[4]]);
    if device_id == 0 {
        return None;
    }
    Some(Identity {
        device_id,
        firmware: u16::from_le_bytes([reply[7], reply[8]]),
        boot: reply[9] == 1,
    })
}

/// Strip the report ID from a feature-report read. Linux returns either the
/// 64 payload bytes, or the report ID followed by them.
pub fn unwrap_reply(buf: &[u8]) -> &[u8] {
    match buf.len() {
        65 if buf[0] == 0 => &buf[1..],
        _ => buf,
    }
}

/// Ask for the active profile (0..=3 on the RT85); the reply has it at `[1]`.
pub fn get_profile() -> Report {
    framed([0x84, 0, 0, 0, 0, 0, 0], &[])
}

pub fn parse_profile(reply: &[u8]) -> Option<u8> {
    (reply.len() > 1 && reply[0] == 0x84).then(|| reply[1])
}

/// A plain key action: send HID usage `code`.
pub fn key_action(code: u8) -> [u8; 4] {
    [0, 0, code, 0]
}

/// Write one key slot in `profile`: `0A profile slot 00 00 commit mode cs`,
/// action at `[8..12]`. Committing stores it in flash.
pub fn set_key(profile: u8, slot: u8, action: [u8; 4]) -> Report {
    framed([0x0A, profile, slot, 0, 0, 1, 0], &action)
}

/// Write user picture `bank` (0..=4): per-key colours by matrix slot, in
/// seven reports `0C bank FF page len last 00 cs` + up to 56 RGB bytes.
pub fn picture(bank: u8, colors: &[(u8, u8, u8)]) -> Vec<Report> {
    let mut flat = [0u8; PICTURE_KEYS * 3];
    for (i, &(r, g, b)) in colors.iter().take(PICTURE_KEYS).enumerate() {
        flat[i * 3..i * 3 + 3].copy_from_slice(&[r, g, b]);
    }
    let pages = flat.len().div_ceil(PAYLOAD_LEN);
    flat.chunks(PAYLOAD_LEN)
        .enumerate()
        .map(|(page, chunk)| {
            let last = (page + 1 == pages) as u8;
            framed([0x0C, bank.min(PICTURES - 1), 0xFF, page as u8, chunk.len() as u8, last, 0], chunk)
        })
        .collect()
}

/// Toggle the screen's language (the vendor app only ever sends 1).
pub fn toggle_language() -> Report {
    framed([0x27, 1, 0, 0, 0, 0, 0], &[])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::royuan::hex;

    #[test]
    fn identify_matches_vendor_bytes() {
        assert!(hex(&identify()).starts_with("8f00000000000070"));
        let mut reply = [0u8; 64];
        reply[..10].copy_from_slice(&[0x8F, 0x4F, 0x0B, 0, 0, 0, 0, 0x02, 0x01, 0]);
        let id = parse_identity(&reply).unwrap();
        assert_eq!(id.device_id, RT85_ID);
        assert_eq!(id.firmware_string(), "1.02");
        assert!(!id.boot);
        assert_eq!(parse_identity(&[0xA5; 64]), None);
        let mut wrapped = vec![0u8];
        wrapped.extend_from_slice(&reply);
        assert_eq!(unwrap_reply(&wrapped)[0], 0x8F);
    }

    #[test]
    fn key_write_layout() {
        let r = set_key(3, 9, key_action(0x05));
        assert_eq!(&r[..12], &[0x0A, 3, 9, 0, 0, 1, 0, 0xFF - 0x17, 0, 0, 0x05, 0]);
    }

    #[test]
    fn picture_is_seven_pages() {
        let r = picture(0, &[(1, 2, 3); PICTURE_KEYS]);
        assert_eq!(r.len(), 7);
        assert_eq!(&r[0][..7], &[0x0C, 0, 0xFF, 0, 56, 0, 0]);
        assert_eq!(&r[0][8..11], &[1, 2, 3]);
        // 378 - 6 * 56 = 42 bytes on the last page, flagged as last.
        assert_eq!(&r[6][..7], &[0x0C, 0, 0xFF, 6, 42, 1, 0]);
        assert!(r[6][8 + 42..].iter().all(|&b| b == 0));
    }

    #[test]
    fn language_toggle_matches_vendor_bytes() {
        assert!(hex(&toggle_language()).starts_with("27010000000000d7"));
    }
}
