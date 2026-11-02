/// `0xFF - (sum(bytes) & 0xFF)`, the checksum used throughout the Royuan protocol.
pub fn ones_complement_sum(bytes: &[u8]) -> u8 {
    0xFF - bytes.iter().fold(0u8, |acc, b| acc.wrapping_add(*b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_known_values() {
        // Time command header: 28 00 00 00 00 00 00 [d7]
        assert_eq!(ones_complement_sum(&[0x28, 0, 0, 0, 0, 0, 0]), 0xD7);
        // Clear screen: ac .. [53]
        assert_eq!(ones_complement_sum(&[0xAC]), 0x53);
    }
}
