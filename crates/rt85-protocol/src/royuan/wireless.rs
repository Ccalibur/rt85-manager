//! 2.4 GHz dongle wake-up sequence (copied verbatim from a capture; meaning unknown).

use super::raw;
use crate::Report;

const SEQUENCE: &[&[u8]] = &[
    &[0xFE, 0x40],
    &[0xF6, 0x0A],
    &[0x8F, 0, 0, 0, 0, 0, 0, 0x70], &[0xFC],
    &[0x87, 0, 0, 0, 0, 0, 0, 0x78], &[0xFC],
    &[0x80, 0, 0, 0, 0, 0, 0, 0x7F], &[0xFC],
    &[0xAD, 0, 0, 0, 0, 0, 0, 0x52], &[0xFC],
    &[0x84, 0, 0, 0, 0, 0, 0, 0x7B], &[0xFC],
    &[0x85, 0, 0, 0, 0, 0, 0, 0x7A], &[0xFC],
    &[0x87, 0, 0, 0, 0, 0, 0, 0x78], &[0xFC],
    &[0x86, 0, 0, 0, 0, 0, 0, 0x79], &[0xFC],
    &[0x91, 0, 0, 0, 0, 0, 0, 0x6E], &[0xFC],
    &[0x92, 0, 0, 0, 0, 0, 0, 0x6D], &[0xFC],
    &[0x97, 0, 0, 0, 0, 0, 0, 0x68], &[0xFC],
];

pub fn init_sequence() -> Vec<Report> {
    SEQUENCE.iter().map(|b| raw(b)).collect()
}

/// A poll reply containing this means the board is awake behind the dongle.
pub fn is_awake(poll_reply: &[u8]) -> bool {
    poll_reply.windows(4).any(|w| w == [0x01, 0x01, 0x01, 0x68])
}
