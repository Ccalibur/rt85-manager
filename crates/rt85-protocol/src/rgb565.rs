//! RGB888 → RGB565 conversion shared by both screen protocols.

/// Truncating conversion (matches epomaker-controller's `encode_rgb565`).
pub const fn from_rgb(r: u8, g: u8, b: u8) -> u16 {
    ((r as u16 & 0xF8) << 8) | ((g as u16 & 0xFC) << 3) | (b as u16 >> 3)
}

pub const fn to_rgb(p: u16) -> (u8, u8, u8) {
    let r = ((p & 0xF800) >> 8) as u8;
    let g = ((p & 0x07E0) >> 3) as u8;
    let b = ((p & 0x001F) << 3) as u8;
    (r | (r >> 5), g | (g >> 6), b | (b >> 5))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primaries() {
        assert_eq!(from_rgb(255, 0, 0), 0xF800);
        assert_eq!(from_rgb(0, 255, 0), 0x07E0);
        assert_eq!(from_rgb(0, 0, 255), 0x001F);
        assert_eq!(to_rgb(0xFFFF), (255, 255, 255));
    }
}
