//! A virtual keyboard (`/dev/uinput`) that types macros.

use rt85_protocol::{keycode::to_evdev, macros::Action};
use std::{fs::File, io::Write, os::fd::AsRawFd, thread::sleep, time::Duration};

const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const SYN_REPORT: u16 = 0;

// ioctl numbers from <linux/uinput.h> ('U' = 0x55).
const UI_SET_EVBIT: libc::c_ulong = 0x4004_5564;
const UI_SET_KEYBIT: libc::c_ulong = 0x4004_5565;
const UI_DEV_SETUP: libc::c_ulong = 0x405C_5503;
const UI_DEV_CREATE: libc::c_ulong = 0x5501;
const UI_DEV_DESTROY: libc::c_ulong = 0x5502;

#[repr(C)]
struct InputId {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
}

#[repr(C)]
struct UinputSetup {
    id: InputId,
    name: [u8; 80],
    ff_effects_max: u32,
}

#[repr(C)]
struct InputEvent {
    time: libc::timeval,
    kind: u16,
    code: u16,
    value: i32,
}

/// Gap between key events, so apps see distinct presses.
const KEY_GAP: Duration = Duration::from_millis(8);

pub struct VirtualKeyboard {
    file: File,
}

impl VirtualKeyboard {
    pub fn new() -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new().write(true).open("/dev/uinput")?;
        let fd = file.as_raw_fd();
        let check = |r: libc::c_int| if r < 0 { Err(std::io::Error::last_os_error()) } else { Ok(()) };
        // SAFETY: plain ioctls on an open uinput fd with correctly sized args.
        unsafe {
            check(libc::ioctl(fd, UI_SET_EVBIT, EV_KEY as libc::c_int))?;
            for code in 1..=255u16 {
                check(libc::ioctl(fd, UI_SET_KEYBIT, code as libc::c_int))?;
            }
            let mut setup = UinputSetup {
                id: InputId { bustype: 0x06 /* BUS_VIRTUAL */, vendor: 0x3151, product: 0x0085, version: 1 },
                name: [0; 80],
                ff_effects_max: 0,
            };
            let name = b"RT85 Manager macros";
            setup.name[..name.len()].copy_from_slice(name);
            check(libc::ioctl(fd, UI_DEV_SETUP, &setup))?;
            check(libc::ioctl(fd, UI_DEV_CREATE))?;
        }
        // Give the compositor a moment to pick the new device up.
        sleep(Duration::from_millis(300));
        Ok(Self { file })
    }

    fn emit(&mut self, kind: u16, code: u16, value: i32) -> std::io::Result<()> {
        let ev = InputEvent { time: libc::timeval { tv_sec: 0, tv_usec: 0 }, kind, code, value };
        // SAFETY: InputEvent is repr(C) plain data; we write its bytes.
        let bytes = unsafe {
            std::slice::from_raw_parts((&ev as *const InputEvent).cast::<u8>(), std::mem::size_of::<InputEvent>())
        };
        self.file.write_all(bytes)
    }

    fn key(&mut self, usage: u8, down: bool) -> std::io::Result<()> {
        let Some(code) = to_evdev(usage) else { return Ok(()) };
        self.emit(EV_KEY, code, down as i32)?;
        self.emit(EV_SYN, SYN_REPORT, 0)?;
        sleep(KEY_GAP);
        Ok(())
    }

    /// Plays the actions; releases anything still held if it fails midway.
    pub fn play(&mut self, actions: &[Action]) -> std::io::Result<()> {
        let mut held: Vec<u8> = Vec::new();
        let result = (|| {
            for a in actions {
                match *a {
                    Action::Down(u) => {
                        self.key(u, true)?;
                        held.push(u);
                    }
                    Action::Up(u) => {
                        self.key(u, false)?;
                        held.retain(|h| *h != u);
                    }
                    Action::Wait(ms) => sleep(Duration::from_millis(ms.into())),
                }
            }
            Ok(())
        })();
        for u in held {
            let _ = self.key(u, false);
        }
        result
    }
}

impl Drop for VirtualKeyboard {
    fn drop(&mut self) {
        // SAFETY: destroying the device we created on this fd.
        unsafe {
            libc::ioctl(self.file.as_raw_fd(), UI_DEV_DESTROY);
        }
    }
}

#[cfg(test)]
mod tests {
    /// Needs write access to /dev/uinput: `cargo test -p rt85-device -- --ignored`
    #[test]
    #[ignore]
    fn creates_and_destroys_device() {
        let kb = super::VirtualKeyboard::new().expect("open /dev/uinput");
        let devices = std::fs::read_to_string("/proc/bus/input/devices").unwrap();
        assert!(devices.contains("RT85 Manager macros"));
        drop(kb);
    }
}
