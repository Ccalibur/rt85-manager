#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("HID error: {0}")]
    Hid(#[from] hidapi::HidError),
    #[error(transparent)]
    Protocol(#[from] rt85_protocol::ProtocolError),
    #[error("image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("no supported keyboard found")]
    NotFound,
    #[error("{0} is not supported on this keyboard")]
    Unsupported(&'static str),
    #[error("the LCD device (1919:1919) did not appear after the wake-up sequence")]
    LcdMissing,
    #[error("device did not answer: {0}")]
    Timeout(&'static str),
    #[error("short write ({0} bytes)")]
    ShortWrite(usize),
}

pub type Result<T> = std::result::Result<T, Error>;
