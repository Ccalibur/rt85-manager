//! Just enough of a HID report-descriptor parser to decode keyboard input
//! reports (usage page 0x07) into the set of pressed keys, for boot-style
//! array reports and NKRO bitmaps alike.

/// One key field inside an input report.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Field {
    /// `count` slots of `size` bits, each holding a usage (minus `logical_min`
    /// plus `usage_min`).
    Array { bit: usize, size: usize, count: usize, usage_min: u16, logical_min: i32 },
    /// One bit per usage, `usage_min..usage_min+count`.
    Bitmap { bit: usize, count: usize, usage_min: u16 },
}

#[derive(Debug, Clone, Default)]
pub struct KeyboardReports {
    /// `(report id, fields)`; id 0 when the descriptor uses no report IDs.
    reports: Vec<(u8, Vec<Field>)>,
    uses_ids: bool,
}

impl KeyboardReports {
    /// Parses a report descriptor. `None` if it has no keyboard-page inputs.
    pub fn parse(desc: &[u8]) -> Option<Self> {
        let mut out = Self::default();
        let mut page: u16 = 0;
        let (mut size, mut count, mut id) = (0usize, 0usize, 0u8);
        let mut logical_min: i32 = 0;
        let mut usage_min: Option<u16> = None;
        let mut usages: Vec<u16> = Vec::new();
        // Bit offset per report id.
        let mut offsets: Vec<(u8, usize)> = Vec::new();

        let mut i = 0;
        while i < desc.len() {
            let prefix = desc[i];
            if prefix == 0xFE {
                // Long item: skip.
                let len = *desc.get(i + 1)? as usize;
                i += 3 + len;
                continue;
            }
            let len = match prefix & 0x03 {
                3 => 4,
                n => n as usize,
            };
            let data = desc.get(i + 1..i + 1 + len)?;
            let unsigned = data.iter().rev().fold(0u32, |a, b| (a << 8) | *b as u32);
            let signed = match len {
                1 => data[0] as i8 as i32,
                2 => i16::from_le_bytes([data[0], data[1]]) as i32,
                4 => unsigned as i32,
                _ => 0,
            };
            match prefix & 0xFC {
                // Main items
                0x80 => {
                    // Input
                    let off = match offsets.iter_mut().find(|(r, _)| *r == id) {
                        Some((_, o)) => o,
                        None => {
                            offsets.push((id, 0));
                            &mut offsets.last_mut().expect("just pushed").1
                        }
                    };
                    let constant = unsigned & 0x01 != 0;
                    let variable = unsigned & 0x02 != 0;
                    if page == 0x07 && !constant {
                        let field = if variable {
                            let min = usage_min.or(usages.first().copied()).unwrap_or(0);
                            Field::Bitmap { bit: *off, count, usage_min: min }
                        } else {
                            Field::Array {
                                bit: *off,
                                size,
                                count,
                                usage_min: usage_min.or(usages.first().copied()).unwrap_or(0),
                                logical_min,
                            }
                        };
                        // Bitmap fields are 1-bit each; ignore odd sizes.
                        if !(variable && size != 1) {
                            match out.reports.iter_mut().find(|(r, _)| *r == id) {
                                Some((_, f)) => f.push(field),
                                None => out.reports.push((id, vec![field])),
                            }
                        }
                    }
                    *off += size * count;
                    usages.clear();
                    usage_min = None;
                }
                // Output / Feature / Collection / End collection: clear locals.
                0x90 | 0xB0 | 0xA0 | 0xC0 => {
                    usages.clear();
                    usage_min = None;
                }
                // Global items
                0x04 => page = unsigned as u16,
                0x14 => logical_min = signed,
                0x74 => size = unsigned as usize,
                0x84 => {
                    id = unsigned as u8;
                    out.uses_ids = true;
                }
                0x94 => count = unsigned as usize,
                // Local items
                0x08 => usages.push(unsigned as u16),
                0x18 => usage_min = Some(unsigned as u16),
                _ => {}
            }
            i += 1 + len;
        }
        (!out.reports.is_empty()).then_some(out)
    }

    /// HID usages pressed in `report` (as read from hidraw: report ID first
    /// when the descriptor uses IDs). `None` for reports without key fields.
    pub fn pressed(&self, report: &[u8]) -> Option<Vec<u16>> {
        let (id, body) = if self.uses_ids { (*report.first()?, report.get(1..)?) } else { (0, report) };
        let fields = &self.reports.iter().find(|(r, _)| *r == id)?.1;
        let bits = |bit: usize, n: usize| -> u32 {
            (0..n).fold(0u32, |acc, k| {
                let b = bit + k;
                let set = body.get(b / 8).is_some_and(|byte| byte >> (b % 8) & 1 == 1);
                acc | ((set as u32) << k)
            })
        };
        let mut keys = Vec::new();
        for f in fields {
            match *f {
                Field::Array { bit, size, count, usage_min, logical_min } => {
                    for s in 0..count {
                        let v = bits(bit + s * size, size) as i32 - logical_min;
                        let usage = usage_min as i32 + v;
                        // 0 = no key, 1..=3 = error/rollover codes.
                        if usage > 3 && usage <= u16::MAX as i32 {
                            keys.push(usage as u16);
                        }
                    }
                }
                Field::Bitmap { bit, count, usage_min } => {
                    for k in 0..count {
                        if bits(bit + k, 1) == 1 {
                            keys.push(usage_min + k as u16);
                        }
                    }
                }
            }
        }
        Some(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Standard boot keyboard descriptor (HID spec, appendix B.1).
    const BOOT: &[u8] = &[
        0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x05, 0x07, 0x19, 0xE0, 0x29, 0xE7, 0x15, 0x00, 0x25, 0x01,
        0x75, 0x01, 0x95, 0x08, 0x81, 0x02, 0x95, 0x01, 0x75, 0x08, 0x81, 0x01, 0x95, 0x05, 0x75, 0x01,
        0x05, 0x08, 0x19, 0x01, 0x29, 0x05, 0x91, 0x02, 0x95, 0x01, 0x75, 0x03, 0x91, 0x01, 0x95, 0x06,
        0x75, 0x08, 0x15, 0x00, 0x25, 0x65, 0x05, 0x07, 0x19, 0x00, 0x29, 0x65, 0x81, 0x00, 0xC0,
    ];

    #[test]
    fn boot_report() {
        let k = KeyboardReports::parse(BOOT).unwrap();
        // Left shift + A + F13
        let r = [0x02, 0, 0x04, 0x68, 0, 0, 0, 0];
        let mut p = k.pressed(&r).unwrap();
        p.sort_unstable();
        assert_eq!(p, vec![0x04, 0x68, 0xE1]);
        assert_eq!(k.pressed(&[0; 8]).unwrap(), Vec::<u16>::new());
    }

    #[test]
    fn nkro_bitmap_with_report_id() {
        // Report 2: 8 modifier bits, then a bitmap of usages 0x04..0x73.
        let desc = [
            0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x85, 0x02, 0x05, 0x07, 0x19, 0xE0, 0x29, 0xE7, 0x15, 0x00,
            0x25, 0x01, 0x75, 0x01, 0x95, 0x08, 0x81, 0x02, 0x19, 0x04, 0x29, 0x73, 0x95, 0x70, 0x81, 0x02,
            0xC0,
        ];
        let k = KeyboardReports::parse(&desc).unwrap();
        let mut r = vec![0u8; 1 + 1 + 14];
        r[0] = 2;
        // usage 0x68 → bitmap bit 0x68-0x04 = 100 → body bit 8 + 100
        let b = 8 + (0x68 - 0x04);
        r[1 + b / 8] |= 1 << (b % 8);
        assert_eq!(k.pressed(&r).unwrap(), vec![0x68]);
        // Other report IDs aren't keys.
        assert!(k.pressed(&[3, 0, 0]).is_none());
    }

    #[test]
    fn vendor_page_is_ignored() {
        let desc = [0x06, 0x00, 0xFF, 0x09, 0x01, 0xA1, 0x01, 0x75, 0x08, 0x95, 0x40, 0x81, 0x02, 0xC0];
        assert!(KeyboardReports::parse(&desc).is_none());
    }
}
