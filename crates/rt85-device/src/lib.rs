//! Talks to the keyboard. Everything here is blocking; the GUI runs it on a
//! worker thread.

pub mod discovery;
pub mod error;
pub mod input;
pub mod keyboard;
pub mod media;
pub mod rdmctmzt;
pub mod royuan;
pub mod uinput;

pub use discovery::{FoundDevice, Interface, list};
pub use error::{Error, Result};
pub use hidapi::HidApi;
pub use keyboard::{Capabilities, Keyboard};

/// Upload progress: `(done, total)` in packets.
pub type Progress<'a> = &'a mut dyn FnMut(usize, usize);

/// The local time, in the form the protocols expect.
pub fn local_now() -> rt85_protocol::time::DateTime {
    use chrono::{Datelike, Timelike};
    let t = chrono::Local::now();
    rt85_protocol::time::DateTime {
        year: t.year() as u16,
        month: t.month() as u8,
        day: t.day() as u8,
        hour: t.hour() as u8,
        minute: t.minute() as u8,
        second: t.second() as u8,
        weekday: t.weekday().num_days_from_monday() as u8,
    }
}
