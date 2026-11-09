//! Screen: RGB565 image/animation upload (`0xA5`/`0x25`), clock (`0x28`),
//! clear (`0xAC`) and, on yc500 boards only, the CPU/temperature widgets
//! (`0x22`/`0x2A`). On gen2 boards (RT85) `0xAC` erases the screen's flash
//! and the board confirms with `0x2C`; it must only be sent from the guarded
//! `royuan::clear_screen` path.

use super::{PAYLOAD_LEN, framed, raw};
use crate::{ProtocolError, Report, checksum::ones_complement_sum, rgb565, time::DateTime};

/// Screen geometry and limits as the upload protocol sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenSpec {
    pub width: u16,
    pub height: u16,
    /// Most animation frames the screen memory holds.
    pub max_frames: u8,
    /// Animation frames are shrunk to a multiple of this in both directions
    /// (64 on the RT100, whose firmware works in 4 KiB pages); 1 means
    /// animations use the full screen.
    pub animation_align: u16,
}

impl ScreenSpec {
    /// Same limits, another size (the CLI's `--size`).
    pub fn with_size(self, width: u16, height: u16) -> Self {
        Self { width, height, ..self }
    }
}

/// RT100 screen (limits from `strodgers/epomaker-controller`).
pub const RT100_SCREEN: ScreenSpec = ScreenSpec { width: 162, height: 173, max_frames: 56, animation_align: 64 };
/// RT85 screen: 1.47" 320x172 panel, landscape. The size and upload format
/// were captured from the official driver by the keybored project; the frame
/// limit is the vendor app's own formula for its 6 MiB of screen memory
/// (`ramarivera/epomaker-driver-linux`, docs/display.md):
/// floor((6 MiB - 4 KiB) / (27 × 4 KiB per frame)) - 5 still banks = 51.
pub const RT85_SCREEN: ScreenSpec = ScreenSpec { width: 320, height: 172, max_frames: 51, animation_align: 1 };

/// The screen of each supported board.
pub const fn spec_for(board: crate::layout::Board) -> ScreenSpec {
    match board {
        crate::layout::Board::Rt85 => RT85_SCREEN,
        crate::layout::Board::Rt100 => RT100_SCREEN,
    }
}

/// Largest frame the upload header can describe (32-bit size).
pub const MAX_FRAME_BYTES: usize = u32::MAX as usize;
/// Chunk indices are 16-bit.
const MAX_CHUNKS: usize = u16::MAX as usize + 1;

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

    /// The upload: one prepare report, then every frame's pixels.
    ///
    /// Layout as in the vendor driver (both families share it):
    /// * prepare `a5 bank frames delay size[0] size[1] 00 cs`, then the
    ///   box's left/top/right/bottom low bytes at 8..12 and high bytes at
    ///   12..16, and size bits 16..32 at 16..18. The box is the whole image.
    /// * data `25 frame frames delay chunkLE16 len cs` + up to 56 bytes, the
    ///   chunk index restarting at 0 for every frame. `len` is the number of
    ///   pixel bytes in the report (56, less on the last one).
    ///
    /// Stills go to bank 0. The RT100 capture (`a5000100f4da008b0000a2ad`,
    /// last chunk `..34`) and the RT85 one (`a500010000ae00ab000040ac0000
    /// 010001`, last chunk `..28`) are both this layout.
    pub fn reports(&self) -> Result<Vec<Report>, ProtocolError> {
        let n = self.frames.len();
        if n == 0 || n > MAX_FRAMES {
            return Err(ProtocolError::OutOfRange { what: "frame count", value: n as u32, max: MAX_FRAMES as u32 });
        }
        let frame_len = self.frame_len();
        if frame_len == 0 || frame_len > MAX_FRAME_BYTES || frame_len.div_ceil(PAYLOAD_LEN) > MAX_CHUNKS {
            return Err(ProtocolError::PayloadTooLarge { len: frame_len, max: MAX_CHUNKS * PAYLOAD_LEN });
        }

        let reports_per_frame = frame_len.div_ceil(PAYLOAD_LEN);
        let mut out = Vec::with_capacity(1 + n * reports_per_frame);

        let size = (frame_len as u32).to_le_bytes();
        let (w, h) = (self.width.to_le_bytes(), self.height.to_le_bytes());
        let head = [0xA5, 0x00, n as u8, self.delay_ms, size[0], size[1], 0x00];
        out.push(raw(&[
            head[0], head[1], head[2], head[3], head[4], head[5], head[6],
            ones_complement_sum(&head),
            0, 0, w[0], h[0], // left, top, right, bottom (low bytes)
            0, 0, w[1], h[1], // ... high bytes
            size[2], size[3],
        ]));

        for (fi, frame) in self.frames.iter().enumerate() {
            if frame.len() != frame_len {
                return Err(ProtocolError::PayloadTooLarge { len: frame.len(), max: frame_len });
            }
            for (seq, chunk) in frame.chunks(PAYLOAD_LEN).enumerate() {
                let s = (seq as u16).to_le_bytes();
                out.push(framed([0x25, fi as u8, n as u8, self.delay_ms, s[0], s[1], chunk.len() as u8], chunk));
            }
        }
        Ok(out)
    }
}

/// Did the keyboard accept a prepare report? The reply echoes `a5` and has
/// 1 at `[1]` when ready, 0 while it is still busy with the last picture.
pub fn prepare_accepted(reply: &[u8]) -> Option<bool> {
    (reply.len() > 1 && reply[0] == 0xA5).then(|| reply[1] == 1)
}

/// Remove the uploaded image. On yc500 boards this clears the picture; on
/// gen2 boards (RT85) it erases the screen's flash, which is the vendor
/// app's "Clear Screen" and brings the clock dashboard back. The erase takes
/// tens of seconds and finishes with [`erase_done`].
pub fn clear() -> Report {
    framed([0xAC, 0, 0, 0, 0, 0, 0], &[])
}

/// Did the keyboard finish the flash erase? The notification is input report
/// ID 5 with `0x2C` as its first data byte (the vendor app's
/// `SET_FLASHCHIPERASE`). Linux keeps the report ID at `[0]` for numbered
/// reports; accept the bare byte too, in case a node delivers them unnumbered.
pub fn erase_done(report: &[u8]) -> bool {
    matches!(report, [0x05, 0x2C, ..] | [0x2C, ..])
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
    fn erase_completion_notification() {
        // Vendor app: input report ID 5, first data byte SET_FLASHCHIPERASE.
        assert!(erase_done(&[0x05, 0x2C]));
        assert!(erase_done(&[0x05, 0x2C, 0, 0]));
        // An unnumbered node may deliver the same byte without the prefix.
        assert!(erase_done(&[0x2C]));
        // Unrelated reports from the same endpoint.
        assert!(!erase_done(&[]));
        assert!(!erase_done(&[0x05]));
        assert!(!erase_done(&[0x05, 0x0F, 0x01]));
        assert!(!erase_done(&[0x25, 0, 1, 0]));
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
        let r = anim.reports().unwrap();
        // Init and stream shape captured from Epomaker's driver (keybored's fullscreen2.pcapng).
        assert_eq!(hex(&r[0]), format!("a500010000ae00ab000040ac0000010001{}", "00".repeat(47)));
        assert_eq!(r.len(), 1 + 1966);
        assert_eq!(&r[1][..12], &[0x25, 0, 1, 0, 0, 0, 0x38, 0xA1, 0xF8, 0, 0xF8, 0]);
        assert!(r[1..1966].iter().all(|x| x[6] == 0x38));
        // Last report: seq 1965 = 0x07AD, 40 (0x28) pixel bytes then zero padding.
        assert_eq!(&r[1966][..8], &[0x25, 0, 1, 0, 0xAD, 0x07, 0x28, 0xFD]);
        assert!(r[1966][8..48].chunks(2).all(|p| p == [0xF8, 0x00]));
        assert!(r[1966][48..].iter().all(|&b| b == 0));
    }

    #[test]
    fn animation_frames_are_indexed_and_counted() {
        let len = 320 * 172 * 2;
        let anim = Animation { width: 320, height: 172, delay_ms: 66, frames: vec![vec![0; len]; 3] };
        let r = anim.reports().unwrap();
        // One prepare for the whole animation: frame count 3, delay 66.
        assert_eq!(&r[0][..4], &[0xA5, 0, 3, 66]);
        assert_eq!(r.len(), 1 + 3 * 1966);
        // Frame 2's first chunk: index 2 of 3, chunk counter back at 0.
        let f2 = &r[1 + 2 * 1966];
        assert_eq!(&f2[..7], &[0x25, 2, 3, 66, 0, 0, 56]);
    }

    #[test]
    fn prepare_reply() {
        assert_eq!(prepare_accepted(&[0xA5, 1, 0]), Some(true));
        assert_eq!(prepare_accepted(&[0xA5, 0, 0]), Some(false));
        assert_eq!(prepare_accepted(&[0x00; 64]), None);
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
