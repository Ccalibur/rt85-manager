//! Per-key RGB frames (`0x18`/`0x19`) and key remapping (`0x13`).

use super::{PAYLOAD_LEN, framed};
use crate::Report;

/// Reports per RGB frame. 7 × 56 bytes = 130 RGB key slots.
pub const REPORTS_PER_FRAME: usize = 7;
pub const MAX_KEYS: usize = REPORTS_PER_FRAME * PAYLOAD_LEN / 3;

/// One frame of per-key colours, indexed by the board's key index.
#[derive(Debug, Clone)]
pub struct RgbFrame {
    pub colors: Vec<(u8, u8, u8)>,
    /// Frame duration (the firmware's unit is unconfirmed; captures use 50 for 50 ms).
    pub time: u8,
}

impl RgbFrame {
    pub fn solid(color: (u8, u8, u8), key_count: usize) -> Self {
        Self { colors: vec![color; key_count.min(MAX_KEYS)], time: 0 }
    }
}

/// Start report followed by `7 × frames` data reports.
pub fn rgb_frames(frames: &[RgbFrame]) -> Vec<Report> {
    let total = frames.len().min(u8::MAX as usize) as u8;
    let mut out = Vec::with_capacity(1 + frames.len() * REPORTS_PER_FRAME);
    out.push(framed([0x18, 0, 0, 0, 0, 0, 0], &[]));
    for (fi, frame) in frames.iter().take(total as usize).enumerate() {
        let mut flat = [0u8; REPORTS_PER_FRAME * PAYLOAD_LEN];
        for (i, &(r, g, b)) in frame.colors.iter().take(MAX_KEYS).enumerate() {
            flat[i * 3..i * 3 + 3].copy_from_slice(&[r, g, b]);
        }
        for (ri, chunk) in flat.chunks(PAYLOAD_LEN).enumerate() {
            out.push(framed([0x19, ri as u8, fi as u8, total, frame.time, 0, 0], chunk));
        }
    }
    out
}

/// Remap the key at internal `key_index` to send HID usage `hid_code`
/// (see [`crate::keycode`]).
pub fn remap(key_index: u8, hid_code: u8) -> Report {
    framed([0x13, 0, key_index, 0, 0, 0, 0], &[0, 0, hid_code])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::royuan::hex;

    #[test]
    fn remap_layout() {
        // epomaker-controller: "1300{idx}00000000{0xFF-(0x13+idx)}0000{code}"
        let r = remap(0x05, 0x29);
        assert!(hex(&r).starts_with("13000500000000e7000029"));
        assert_eq!(&r[..11], &[0x13, 0, 5, 0, 0, 0, 0, 0xFF - 0x18, 0, 0, 0x29]);
    }

    #[test]
    fn rgb_structure() {
        let reports = rgb_frames(&[RgbFrame::solid((1, 2, 3), 100)]);
        assert_eq!(reports.len(), 1 + REPORTS_PER_FRAME);
        assert_eq!(&reports[0][..8], &[0x18, 0, 0, 0, 0, 0, 0, 0xE7]);
        assert_eq!(&reports[1][..11], &[0x19, 0, 0, 1, 0, 0, 0, 0xE5, 1, 2, 3]);
        // key 100 → byte 300 → report 5 (300/56), offset 20; it is beyond the 100 keys so zero
        assert_eq!(reports[6][8 + 20], 0);
        // key 99 → byte 297 → report 5, offset 17
        assert_eq!(&reports[6][8 + 17..8 + 20], &[1, 2, 3]);
    }
}
