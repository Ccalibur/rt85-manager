//! Screen: RGB565 image/animation upload (`0xA5`/`0x25`), clear (`0xAC`),
//! clock (`0x28`), and the CPU/temperature widgets (`0x22`/`0x2A`).

use super::{PAYLOAD_LEN, framed, raw};
use crate::{ProtocolError, Report, checksum::ones_complement_sum, rgb565, time::DateTime};

/// Screen geometry as the upload protocol sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenSpec {
    pub width: u16,
    pub height: u16,
}

/// RT100 screen.
pub const RT100_SCREEN: ScreenSpec = ScreenSpec { width: 162, height: 173 };
/// RT85 screen: 1.47" 320x172 panel, landscape. Size and upload format come
/// from a USB capture of the official driver (v4 3.2.22) doing a full-screen
/// upload, published by the keybored project.
pub const RT85_SCREEN: ScreenSpec = ScreenSpec { width: 320, height: 172 };

/// The screen of each supported board.
pub const fn spec_for(board: crate::layout::Board) -> ScreenSpec {
    match board {
        crate::layout::Board::Rt85 => RT85_SCREEN,
        crate::layout::Board::Rt100 => RT100_SCREEN,
    }
}

/// Largest frame the compact `0xA5` header can describe (16-bit size, 8-bit
/// width and height). Bigger still images use the extended header.
pub const COMPACT_MAX_BYTES: usize = u16::MAX as usize;
/// Largest still image the extended header carries (24-bit size).
pub const EXTENDED_MAX_BYTES: usize = 0xFF_FFFF;

/// Animated frames must have both dimensions aligned to this (the firmware's
/// framebuffer is 4 KiB page aligned).
pub const ANIMATION_ALIGN: u16 = 64;
pub const MAX_FRAMES: usize = 255;

/// Convert a tightly packed RGBA8 buffer (`width*height*4`, row-major) into
/// the board's pixel stream: big-endian RGB565 in *column-major* order
/// (the reference tool flips vertically then rotates 90° clockwise, which
/// is a transpose).
pub fn encode_rgba(rgba: &[u8], width: u16, height: u16) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    assert_eq!(rgba.len(), w * h * 4, "RGBA buffer does not match dimensions");
    let mut out = Vec::with_capacity(w * h * 2);
    for x in 0..w {
        for y in 0..h {
            let p = &rgba[(y * w + x) * 4..][..4];
            // Composite onto black.
            let a = p[3] as u16;
            let c = |v: u8| ((v as u16 * a) / 255) as u8;
            out.extend_from_slice(&rgb565::from_rgb(c(p[0]), c(p[1]), c(p[2])).to_be_bytes());
        }
    }
    out
}

/// Encoded frames ready for upload. Every frame must be `width*height*2` bytes.
#[derive(Debug, Clone)]
pub struct Animation {
    pub width: u16,
    pub height: u16,
    /// Delay per frame in ms (max 255; 0 for a still image).
    pub delay_ms: u8,
    pub frames: Vec<Vec<u8>>,
}

impl Animation {
    pub fn frame_len(&self) -> usize {
        self.width as usize * self.height as usize * 2
    }

    /// Whether this upload needs the extended (full-screen) header: a single
    /// frame wider than 255 px or bigger than 64 KiB. Only the single-frame
    /// form has been seen from the official driver, so animations keep the
    /// compact header.
    pub fn is_extended(&self) -> bool {
        self.frames.len() == 1 && (self.frame_len() > COMPACT_MAX_BYTES || self.width > 255 || self.height > 255)
    }

    pub fn reports(&self) -> Result<Vec<Report>, ProtocolError> {
        let n = self.frames.len();
        if n == 0 || n > MAX_FRAMES {
            return Err(ProtocolError::OutOfRange { what: "frame count", value: n as u32, max: MAX_FRAMES as u32 });
        }
        let frame_len = self.frame_len();
        let extended = self.is_extended();
        if extended {
            if frame_len > EXTENDED_MAX_BYTES || self.height > 255 {
                return Err(ProtocolError::PayloadTooLarge { len: frame_len, max: EXTENDED_MAX_BYTES });
            }
        } else {
            if frame_len > COMPACT_MAX_BYTES {
                return Err(ProtocolError::PayloadTooLarge { len: frame_len, max: COMPACT_MAX_BYTES });
            }
            if self.width > 255 || self.height > 255 {
                return Err(ProtocolError::OutOfRange {
                    what: "screen dimension",
                    value: self.width.max(self.height) as u32,
                    max: 255,
                });
            }
        }

        let reports_per_frame = frame_len.div_ceil(PAYLOAD_LEN);
        let mut out = Vec::with_capacity(1 + n * reports_per_frame);

        // Compact init:  a5 00 [frames] [delay] [size LE16] 00 [cs] 00 00 [w] [h]
        // Extended init: same, with only the low bits of size and width in
        // place, then width >> 8 at byte 14 and size >> 16 at byte 16.
        let size = (frame_len as u32).to_le_bytes();
        let head = [0xA5, 0x00, n as u8, self.delay_ms, size[0], size[1], 0x00];
        let w = self.width.to_le_bytes();
        let mut init = vec![
            head[0], head[1], head[2], head[3], head[4], head[5], head[6],
            ones_complement_sum(&head), 0, 0, w[0], self.height as u8,
        ];
        if extended {
            init.extend_from_slice(&[0, 0, w[1], 0, size[2]]);
        }
        out.push(raw(&init));

        for (fi, frame) in self.frames.iter().enumerate() {
            if frame.len() != frame_len {
                return Err(ProtocolError::PayloadTooLarge { len: frame.len(), max: frame_len });
            }
            for (seq, chunk) in frame.chunks(PAYLOAD_LEN).enumerate() {
                let last = seq + 1 == reports_per_frame;
                let s = (seq as u16).to_le_bytes();
                // The extended stream ends with 0x28; sending the compact
                // 0x34 there blanks the screen.
                let kind = match (last, extended) {
                    (false, _) => 0x38,
                    (true, false) => 0x34,
                    (true, true) => 0x28,
                };
                // 1-based frame number
                out.push(framed([0x25, 0x00, (fi + 1) as u8, 0x00, s[0], s[1], kind], chunk));
            }
        }
        Ok(out)
    }
}

/// Wipe the uploaded image (telemetry widgets are unaffected).
pub fn clear() -> Report {
    framed([0xAC, 0, 0, 0, 0, 0, 0], &[])
}

pub fn set_time(t: DateTime) -> Report {
    let y = t.year.to_be_bytes();
    framed([0x28, 0, 0, 0, 0, 0, 0], &[y[0], y[1], t.month, t.day, t.hour, t.minute, t.second])
}

/// CPU usage widget, 0..=100 %.
pub fn set_cpu(percent: u8) -> Report {
    framed(
        [0x22, 0, 0, 0, 0, 0, 0],
        &[0x63, 0x00, 0x7F, 0x00, 0x04, 0x00, 0x08, 0x00, percent.min(100)],
    )
}

/// Temperature ("weather") widget, °C 0..=99.
pub fn set_temperature(celsius: u8) -> Report {
    framed([0x2A, 0, 0, 0, 0, 0, 0], &[celsius.min(99)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::royuan::hex;

    #[test]
    fn fixed_commands_match_reference() {
        assert!(hex(&clear()).starts_with("ac00000000000053"));
        assert!(hex(&set_temperature(42)).starts_with("2a000000000000d52a"));
        assert!(hex(&set_cpu(50)).starts_with("22000000000000dd63007f000400080032"));
        let t = DateTime { year: 2026, month: 11, day: 2, hour: 21, minute: 43, second: 7, weekday: 0 };
        assert!(hex(&set_time(t)).starts_with("28000000000000d707ea0b02152b07"));
    }

    #[test]
    fn still_image_matches_reference_header() {
        let s = RT100_SCREEN;
        let red = [255u8, 0, 0, 255].repeat(s.width as usize * s.height as usize);
        let anim = Animation {
            width: s.width,
            height: s.height,
            delay_ms: 0,
            frames: vec![encode_rgba(&red, s.width, s.height)],
        };
        let r = anim.reports().unwrap();
        // Static image init captured by epomaker-controller.
        assert!(hex(&r[0]).starts_with("a5000100f4da008b0000a2ad"));
        assert_eq!(r.len(), 1 + 1001);
        assert_eq!(&r[1][..8], &[0x25, 0, 1, 0, 0, 0, 0x38, ones_complement_sum(&[0x25, 0, 1, 0, 0, 0, 0x38])]);
        assert_eq!(&r[1][8..12], &[0xF8, 0x00, 0xF8, 0x00]);
        // Footer: seq 1000 = 0x03E8 LE, kind 0x34
        assert_eq!(&r[1001][..7], &[0x25, 0, 1, 0, 0xE8, 0x03, 0x34]);
    }

    #[test]
    fn rt85_full_screen_matches_official_driver() {
        let s = RT85_SCREEN;
        let red = [255u8, 0, 0, 255].repeat(s.width as usize * s.height as usize);
        let anim = Animation {
            width: s.width,
            height: s.height,
            delay_ms: 0,
            frames: vec![encode_rgba(&red, s.width, s.height)],
        };
        assert!(anim.is_extended());
        let r = anim.reports().unwrap();
        // Init and stream shape captured from Epomaker's driver (fullscreen2.pcapng).
        assert_eq!(hex(&r[0]), format!("a500010000ae00ab000040ac0000010001{}", "00".repeat(47)));
        assert_eq!(r.len(), 1 + 1966);
        assert_eq!(&r[1][..12], &[0x25, 0, 1, 0, 0, 0, 0x38, 0xA1, 0xF8, 0, 0xF8, 0]);
        assert!(r[1..1966].iter().all(|x| x[6] == 0x38));
        // Last report: seq 1965 = 0x07AD, kind 0x28, 40 pixel bytes then zero padding.
        assert_eq!(&r[1966][..8], &[0x25, 0, 1, 0, 0xAD, 0x07, 0x28, 0xFD]);
        assert!(r[1966][8..48].chunks(2).all(|p| p == [0xF8, 0x00]));
        assert!(r[1966][48..].iter().all(|&b| b == 0));
    }

    #[test]
    fn animations_keep_compact_header() {
        let anim = Animation { width: 192, height: 128, delay_ms: 66, frames: vec![vec![0; 192 * 128 * 2]; 2] };
        assert!(!anim.is_extended());
        let r = anim.reports().unwrap();
        assert!(r[0][12..].iter().all(|&b| b == 0));
        // Too big for the compact header and not a still: rejected.
        let big = Animation { width: 320, height: 128, delay_ms: 66, frames: vec![vec![0; 320 * 128 * 2]; 2] };
        assert!(big.reports().is_err());
    }

    #[test]
    fn encode_is_column_major() {
        // 2x1 image: red, blue  →  column 0 = red, column 1 = blue
        let px = [255, 0, 0, 255, 0, 0, 255, 255];
        assert_eq!(encode_rgba(&px, 2, 1), vec![0xF8, 0x00, 0x00, 0x1F]);
        // 1x2 image: red over blue → same output (single column)
        assert_eq!(encode_rgba(&px, 1, 2), vec![0xF8, 0x00, 0x00, 0x1F]);
    }
}
