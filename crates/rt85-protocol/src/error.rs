#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("value {value} out of range for {what} (max {max})")]
    OutOfRange { what: &'static str, value: u32, max: u32 },
    #[error("payload is {len} bytes, at most {max} are supported")]
    PayloadTooLarge { len: usize, max: usize },
    #[error("not a QGIF file (bad magic)")]
    NotQgif,
    #[error("invalid layout JSON: {0}")]
    Layout(String),
}
