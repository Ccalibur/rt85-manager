//! Royuan platform (VID `0x3151`): 64-byte feature reports.
//!
//! Most reports have the same 8-byte header: `cmd`, six parameter bytes, then
//! a checksum over those 7 bytes (see [`crate::checksum`]). The payload goes
//! in bytes 8..64.

pub mod gen2;
pub mod keys;
pub mod lighting;
pub mod screen;
pub mod wireless;

use crate::{REPORT_LEN, Report, checksum::ones_complement_sum};

/// Which command set a Royuan board speaks. Boards share the framing and the
/// screen/clock commands but differ in everything else.
///
/// * `Yc500`: older boards such as the RT100 (`strodgers/epomaker-controller`).
/// * `Gen2`: newer YC3123-based boards such as the RT85, Glyph and RT75. See
///   [`gen2`] for where the layouts come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Family {
    Yc500,
    Gen2,
}

/// Bytes available after the 8-byte header.
pub const PAYLOAD_LEN: usize = REPORT_LEN - 8;

/// Build `[header(7) | checksum | payload | zero padding]`.
pub(crate) fn framed(header: [u8; 7], payload: &[u8]) -> Report {
    debug_assert!(payload.len() <= PAYLOAD_LEN);
    let mut r = [0u8; REPORT_LEN];
    r[..7].copy_from_slice(&header);
    r[7] = ones_complement_sum(&header);
    r[8..8 + payload.len()].copy_from_slice(payload);
    r
}

/// Unframed report: the bytes as given, zero padded.
pub(crate) fn raw(bytes: &[u8]) -> Report {
    let mut r = [0u8; REPORT_LEN];
    r[..bytes.len()].copy_from_slice(bytes);
    r
}

/// Poll / keep-alive. The reply is read with a feature-report GET.
pub fn poll() -> Report {
    raw(&[0xF7])
}

#[cfg(test)]
pub(crate) fn hex(r: &[u8]) -> String {
    r.iter().map(|b| format!("{b:02x}")).collect()
}
