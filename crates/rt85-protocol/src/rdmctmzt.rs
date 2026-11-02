//! RDMCTMZT platform (VID `0x36B0`, e.g. RT82). Packets start with `0xAA, cmd`.
//!
//! Two USB devices are involved:
//! * the keyboard (`36B0:xxxx`), raw-HID interface 1 (usage page `0xFF60`).
//!   Reports are written as-is (64 bytes, no report-ID prefix).
//! * the LCD (`1919:1919`), interface 1. It only enumerates after
//!   [`kbd_wake_lcd`] has been sent to the keyboard. Each write is a report-ID
//!   `0x00` followed by the 64-byte packet, and the device answers every
//!   packet with a 64-byte read (used as flow control).
//!
//! The LCD expects **QGIF**, a proprietary compressed format made by the vendor's
//! web tool (image.rdmctmzt.com). There's no clean-room encoder yet, so
//! [`Qgif`] only validates and uploads already-encoded files.

use crate::{ProtocolError, REPORT_LEN, Report, time::DateTime};

pub const MAGIC: u8 = 0xAA;
/// Data bytes per `0x19` packet.
pub const DATA_CHUNK: usize = 56;
/// Constant that appears in most LCD packets (it's 56, the chunk size).
const P38: u8 = 0x38;

/// Build `AA cmd` + `args`, zero padded.
pub fn packet(cmd: u8, args: &[u8]) -> Report {
    let mut r = [0u8; REPORT_LEN];
    r[0] = MAGIC;
    r[1] = cmd;
    r[2..2 + args.len()].copy_from_slice(args);
    r
}

// ---- keyboard device ------------------------------------------------------

/// `AA E2` then `AA E0` ×5: makes the `1919:1919` LCD device (re-)enumerate.
pub fn kbd_wake_lcd() -> Vec<Report> {
    let mut v = vec![packet(0xE2, &[])];
    v.extend(std::iter::repeat_n(packet(0xE0, &[]), 5));
    v
}

/// "getGifCount" sent to the keyboard right before a transfer.
pub fn kbd_gif_count(screens: u8) -> Report {
    packet(0xE3, &[0, 0, 0, screens, 0, 0, screens])
}

// ---- LCD device -----------------------------------------------------------

pub fn lcd_query() -> Report {
    packet(0x10, &[])
}
pub fn lcd_start() -> Report {
    packet(0x11, &[])
}
pub fn lcd_screen_query() -> Report {
    packet(0x12, &[0, 0, 0, P38])
}
pub fn lcd_status() -> Report {
    packet(0x1C, &[])
}
pub fn lcd_prepare() -> Report {
    packet(0x1B, &[0, 0, 0, P38])
}
pub fn lcd_download_mode(screens: u8) -> Report {
    packet(0xE3, &[0, 0, 0, screens, 0, 0, screens])
}
pub fn lcd_screen_prep() -> Report {
    packet(0x14, &[0, 0, 0, P38])
}
pub fn lcd_end() -> Report {
    packet(0x1A, &[])
}

/// Clock sync (`0x17`). Every field is sent as separate decimal digits.
pub fn lcd_set_time(t: DateTime) -> Report {
    let d = |v: u16, i: u32| ((v / 10u16.pow(i)) % 10) as u8;
    let (y, mo, da, h, mi, s) =
        (t.year, t.month as u16, t.day as u16, t.hour as u16, t.minute as u16, t.second as u16);
    packet(
        0x17,
        &[
            0, 0, 0, P38, 0, 0,
            d(y, 3), d(y, 2), d(y, 1), d(y, 0),
            d(mo, 1), d(mo, 0),
            t.weekday % 7,
            d(da, 1), d(da, 0),
            d(h, 1), d(h, 0),
            d(mi, 1), d(mi, 0),
            d(s, 1), d(s, 0),
        ],
    )
}

/// Number of 64 KiB flash blocks the firmware erases for `size` bytes.
pub fn erase_blocks(size: usize) -> u8 {
    (size.div_ceil(65536) + 1).min(u8::MAX as usize) as u8
}

/// The five packets that announce a transfer (`0x15` ×3, `0x16`, `0x18`).
/// After the last one the device erases flash, so poll [`lcd_status`] until
/// it answers before sending data.
pub fn lcd_transfer_setup(size: usize) -> [Report; 5] {
    let e = erase_blocks(size);
    let s = (size as u32).to_le_bytes();
    [
        packet(0x15, &[0, 0, 0, P38, 0, 0, 0, 1, e, 0, 0, s[0], s[1], s[2]]),
        packet(0x15, &[0x38, 0, 0, P38]),
        packet(0x15, &[0x70, 0, 0, 0x10]),
        packet(0x16, &[0, 0, 0, P38]),
        packet(0x18, &[0, 0, 0, 1, 0, 0, e]),
    ]
}

/// `AA 19 [offset LE24] [len] 00 00 [56 bytes]`
pub fn lcd_data(offset: usize, chunk: &[u8]) -> Report {
    assert!(chunk.len() <= DATA_CHUNK);
    let o = (offset as u32).to_le_bytes();
    let mut r = packet(0x19, &[o[0], o[1], o[2], chunk.len() as u8, 0, 0]);
    r[8..8 + chunk.len()].copy_from_slice(chunk);
    r
}

pub fn lcd_data_packets(data: &[u8]) -> impl Iterator<Item = Report> + '_ {
    data.chunks(DATA_CHUNK).enumerate().map(|(i, c)| lcd_data(i * DATA_CHUNK, c))
}

/// A validated QGIF file.
#[derive(Debug, Clone)]
pub struct Qgif(Vec<u8>);

impl Qgif {
    /// Largest file the reference tool reports as reliable.
    pub const SOFT_LIMIT: usize = 64 * 1024;
    /// Offset is 24-bit.
    pub const HARD_LIMIT: usize = 1 << 24;

    pub fn new(mut data: Vec<u8>) -> Result<Self, ProtocolError> {
        if data.len() < 10 || &data[..4] != b"QGIF" {
            return Err(ProtocolError::NotQgif);
        }
        if data.len() > Self::HARD_LIMIT {
            return Err(ProtocolError::PayloadTooLarge { len: data.len(), max: Self::HARD_LIMIT });
        }
        // rt82display: the web tool's v5 header must be patched to 3 for the RT82 to show it.
        if data[5] == 0x05 {
            data[5] = 0x03;
        }
        Ok(Self(data))
    }

    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_digits_match_capture() {
        // PROTOCOL.md "config" packet: aa 17 00 00 00 38 00 00 02 00 02 06 00 02 03 00 04 00 09 01
        let t = DateTime { year: 2026, month: 2, day: 4, hour: 9, minute: 10, second: 0, weekday: 3 };
        let p = lcd_set_time(t);
        assert_eq!(&p[..20], &[0xAA, 0x17, 0, 0, 0, 0x38, 0, 0, 2, 0, 2, 6, 0, 2, 3, 0, 4, 0, 9, 1]);
    }

    #[test]
    fn transfer_setup_matches_capture() {
        // aa 15 00 00 00 38 00 00 00 01 07 00 00 f9 f0 05 (size 389369)
        let s = lcd_transfer_setup(389_369);
        assert_eq!(&s[0][..16], &[0xAA, 0x15, 0, 0, 0, 0x38, 0, 0, 0, 1, 7, 0, 0, 0xF9, 0xF0, 0x05]);
    }

    #[test]
    fn data_packets() {
        let data: Vec<u8> = (0..120u8).collect();
        let p: Vec<_> = lcd_data_packets(&data).collect();
        assert_eq!(p.len(), 3);
        assert_eq!(&p[1][..9], &[0xAA, 0x19, 56, 0, 0, 56, 0, 0, 56]);
        assert_eq!(&p[2][..8], &[0xAA, 0x19, 112, 0, 0, 8, 0, 0]);
    }

    #[test]
    fn qgif_validation() {
        assert!(Qgif::new(b"GIF89a....".to_vec()).is_err());
        let q = Qgif::new(b"QGIF\x00\x05\x00\x00\x00\x00".to_vec()).unwrap();
        assert_eq!(q.bytes()[5], 3);
    }
}
