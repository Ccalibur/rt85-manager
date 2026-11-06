//! Watches a keyboard's own key reports (its hidraw nodes) for macro trigger
//! keys. Reading hidraw doesn't take the keys away from the system, and the
//! udev rule the app installs (or that can be installed by hand) also lets us
//! read it.

use rt85_protocol::hid_report::KeyboardReports;
use std::{
    collections::HashSet,
    ffi::CStr,
    fs::File,
    io::Read,
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    time::Duration,
};

/// The descriptor of `/dev/hidrawN`, from sysfs (no ioctl needed).
fn descriptor(node: &Path) -> Option<Vec<u8>> {
    let name = node.file_name()?;
    std::fs::read(PathBuf::from("/sys/class/hidraw").join(name).join("device/report_descriptor")).ok()
}

/// Nodes among `paths` that carry keyboard input, with their decoders.
pub fn keyboard_nodes(paths: &[&CStr]) -> Vec<(PathBuf, KeyboardReports)> {
    paths
        .iter()
        .filter_map(|p| {
            let path = PathBuf::from(p.to_str().ok()?);
            let k = KeyboardReports::parse(&descriptor(&path)?)?;
            Some((path, k))
        })
        .collect()
}

/// Blocks, calling `on_press(usage)` for every newly pressed key that
/// `wanted` accepts, until `keep_going` returns false or the device goes away.
pub fn watch(
    path: &Path,
    reports: &KeyboardReports,
    wanted: impl Fn(u16) -> bool,
    mut on_press: impl FnMut(u16),
    keep_going: impl Fn() -> bool,
) -> std::io::Result<()> {
    let mut file = File::open(path)?;
    let fd = file.as_raw_fd();
    let mut held: HashSet<u16> = HashSet::new();
    let mut buf = [0u8; 256];
    while keep_going() {
        let mut pfd = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
        // SAFETY: one valid pollfd for the duration of the call.
        let n = unsafe { libc::poll(&mut pfd, 1, Duration::from_millis(250).as_millis() as i32) };
        if n < 0 {
            let e = std::io::Error::last_os_error();
            if e.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(e);
        }
        if n == 0 {
            continue;
        }
        if pfd.revents & (libc::POLLERR | libc::POLLHUP) != 0 {
            return Err(std::io::Error::other("keyboard disconnected"));
        }
        let len = file.read(&mut buf)?;
        let Some(pressed) = reports.pressed(&buf[..len]) else { continue };
        let now: HashSet<u16> = pressed.into_iter().filter(|u| wanted(*u)).collect();
        for &u in now.difference(&held) {
            on_press(u);
        }
        held = now;
    }
    Ok(())
}
