//! Access to the keyboard's `/dev/hidraw*` nodes.
//!
//! On Linux those nodes are root-only unless a udev rule tags the Epomaker
//! boards with `uaccess`, which gives the logged-in user an ACL. Installing
//! such a rule needs root exactly once.
//!
//! Rather than asking the user to copy files and reload udev by hand, the app
//! installs the bundled rule itself through polkit (`pkexec`): the desktop
//! shows its normal authentication dialog, and after that the keyboard is
//! readable/writable without any manual step.

use crate::FoundDevice;
use std::{
    ffi::{CString, OsStr},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

/// File name used in every udev rules directory.
pub const RULE_FILENAME: &str = "70-epomaker-rt85.rules";
/// The rule shipped with the app, embedded so it works from any install layout.
pub const RULE_TEXT: &str = include_str!("../../../resources/udev/70-epomaker-rt85.rules");

/// Where udev looks for rules, in the order it reads them.
const RULE_DIRS: &[&str] = &[
    "/etc/udev/rules.d",
    "/run/udev/rules.d",
    "/usr/lib/udev/rules.d",
    "/lib/udev/rules.d",
];

const RELOAD: &str = "udevadm control --reload-rules && udevadm trigger --subsystem-match=hidraw";

/// Is the rule already installed anywhere udev reads?
pub fn rule_installed() -> bool {
    installed_rule().is_some()
}

/// Contents of the copy of the rule udev actually uses: a file in an earlier
/// directory overrides one with the same name in a later one.
fn installed_rule() -> Option<String> {
    RULE_DIRS
        .iter()
        .map(|dir| Path::new(dir).join(RULE_FILENAME))
        .find(|path| path.exists())
        // Unreadable counts as outdated, so we replace it rather than give up.
        .map(|path| std::fs::read_to_string(path).unwrap_or_default())
}

/// What automatic setup should do about an inaccessible keyboard.
#[derive(Debug, PartialEq, Eq)]
enum Plan {
    /// No rule yet: install it.
    Install,
    /// An older or different rule (e.g. one without a newly supported vendor):
    /// replace it.
    Update,
    /// Our exact rule is already installed, so asking for root again won't
    /// help; the device usually just needs replugging.
    Replug,
}

fn plan(installed: Option<&str>) -> Plan {
    match installed {
        None => Plan::Install,
        Some(text) if text == RULE_TEXT => Plan::Replug,
        Some(_) => Plan::Update,
    }
}

/// Are we running as root? Then every hidraw node is already openable.
pub fn is_root() -> bool {
    // SAFETY: geteuid has no preconditions.
    unsafe { libc::geteuid() == 0 }
}

/// Are we inside a Flatpak sandbox? It can read `/run/udev` but cannot write
/// `/etc/udev/rules.d` on the host, so the rule must be installed there.
pub fn in_flatpak() -> bool {
    Path::new("/.flatpak-info").exists() || std::env::var_os("FLATPAK_ID").is_some()
}

/// Can this process open the node read-write?
fn accessible(path: &Path) -> bool {
    let Ok(cpath) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `cpath` is a valid NUL-terminated path.
    unsafe { libc::access(cpath.as_ptr(), libc::R_OK | libc::W_OK) == 0 }
}

fn node_paths(devices: &[FoundDevice]) -> impl Iterator<Item = PathBuf> + '_ {
    devices
        .iter()
        .flat_map(|d| d.interfaces.iter())
        .map(|i| PathBuf::from(OsStr::from_bytes(i.path.to_bytes())))
}

/// Does at least one of the devices' HID nodes need a udev rule?
pub fn needs_access(devices: &[FoundDevice]) -> bool {
    !is_root() && node_paths(devices).any(|path| !accessible(&path))
}

/// The rule is current but the nodes still aren't accessible. Automatic
/// setup must not prompt again here, or every scan and CLI command would.
const REPLUG_HINT: &str = "the udev rule is installed but the keyboard still isn't accessible; \
     unplug and replug it (or run `rt85 setup-udev` to reload udev)";

/// Flatpak cannot change the host's udev configuration.
const FLATPAK_HINT: &str = "the Flatpak sandbox can't install udev rules on the host; \
     install resources/udev/70-epomaker-rt85.rules once (see the README), then reopen RT85 Manager";

fn pkexec_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("PATH")
        && let Some(pkexec) = std::env::split_paths(&path).map(|dir| dir.join("pkexec")).find(|p| p.is_file())
    {
        return Some(pkexec);
    }
    ["/usr/bin/pkexec", "/bin/pkexec"].iter().map(PathBuf::from).find(|p| p.is_file())
}

/// A command that runs `sh -c <script>` as root: directly when we already are
/// root, otherwise through polkit's `pkexec`.
fn root_command() -> Result<Command, String> {
    if is_root() {
        return Ok(Command::new("/bin/sh"));
    }
    let pkexec = pkexec_path().ok_or_else(|| {
        "pkexec (polkit) isn't installed, so the udev rule can't be set up automatically".to_string()
    })?;
    let mut cmd = Command::new(pkexec);
    cmd.arg("sh");
    Ok(cmd)
}

fn run_root(script: &str, arg: Option<&Path>) -> Result<(), String> {
    let mut cmd = root_command()?;
    cmd.arg("-c").arg(script).arg("sh");
    if let Some(arg) = arg {
        cmd.arg(arg);
    }
    let status = cmd.status().map_err(|e| format!("could not run the setup command: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("authorisation was cancelled or failed ({status})"))
    }
}

/// Write the bundled rule to a private temporary file. `create_new` keeps a
/// malicious local user from planting a symlink or an existing file for us.
fn temp_rule_file() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir();
    let mut last = String::new();
    for attempt in 0..10 {
        let path = dir.join(format!("rt85-{}-{attempt}-{RULE_FILENAME}", std::process::id()));
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                use std::io::Write;
                return file
                    .write_all(RULE_TEXT.as_bytes())
                    .map(|()| path)
                    .map_err(|e| format!("could not write a temporary file: {e}"));
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => last = e.to_string(),
            Err(e) => return Err(format!("could not create a temporary file: {e}")),
        }
    }
    Err(format!("could not create a temporary file: {last}"))
}

fn install_rule() -> Result<(), String> {
    if in_flatpak() {
        return Err(FLATPAK_HINT.into());
    }

    let tmp = temp_rule_file()?;
    // One authorisation prompt: install the rule, then let udev apply the new
    // uaccess ACL to the nodes that are already plugged in.
    let script = format!("install -Dm644 \"$1\" /etc/udev/rules.d/{RULE_FILENAME} && {RELOAD}");
    let result = run_root(&script, Some(&tmp));
    let _ = std::fs::remove_file(&tmp);
    result
}

/// Reload udev and re-apply the rule to existing devices (root via pkexec).
fn apply_rule() -> Result<(), String> {
    if in_flatpak() {
        return Err(FLATPAK_HINT.into());
    }
    run_root(RELOAD, None)
}

/// Wait up to a couple of seconds for udev to apply the new uaccess ACL.
fn wait_for_access(devices: &[FoundDevice]) {
    for _ in 0..20 {
        if !needs_access(devices) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Install or update the rule, or reload udev if it is already current. Used
/// by the explicit `rt85 setup-udev` command, so it may always prompt.
pub fn setup() -> Result<String, String> {
    if in_flatpak() {
        return Err(FLATPAK_HINT.into());
    }
    match plan(installed_rule().as_deref()) {
        Plan::Install => {
            install_rule()?;
            Ok("Installed the udev rule. Unplug and replug the keyboard if it still isn't detected.".into())
        }
        Plan::Update => {
            install_rule()?;
            Ok("Updated the udev rule. Unplug and replug the keyboard if it still isn't detected.".into())
        }
        Plan::Replug => {
            apply_rule()?;
            Ok("The udev rule is already installed; reloaded it.".into())
        }
    }
}

/// Make sure the app may open `devices`, installing the rule on first use.
///
/// Only prompts for root when the rule is missing or outdated. If the current
/// rule is installed and the nodes are still inaccessible, it returns the
/// replug hint instead of prompting again.
///
/// Returns `Ok(None)` when no change was needed, `Ok(Some(message))` after a
/// successful change, and `Err` when it couldn't be done automatically.
pub fn ensure_access(devices: &[FoundDevice]) -> Result<Option<String>, String> {
    if !needs_access(devices) {
        return Ok(None);
    }
    if in_flatpak() {
        return Err(FLATPAK_HINT.into());
    }
    let message = match plan(installed_rule().as_deref()) {
        Plan::Replug => return Err(REPLUG_HINT.into()),
        Plan::Install => {
            install_rule()?;
            "Installed the udev rule automatically. If the keyboard still doesn't respond, replug it."
        }
        Plan::Update => {
            install_rule()?;
            "Updated the udev rule automatically. If the keyboard still doesn't respond, replug it."
        }
    };
    wait_for_access(devices);
    Ok(Some(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_rule_covers_every_supported_vendor() {
        assert!(RULE_TEXT.contains("idVendor}==\"3151\""));
        assert!(RULE_TEXT.contains("idVendor}==\"36b0\""));
        assert!(RULE_TEXT.contains("idVendor}==\"1919\""));
        assert!(RULE_TEXT.contains("TAG+=\"uaccess\""));
    }

    #[test]
    fn missing_rule_is_installed() {
        assert_eq!(plan(None), Plan::Install);
    }

    #[test]
    fn current_rule_never_prompts_again() {
        assert_eq!(plan(Some(RULE_TEXT)), Plan::Replug);
    }

    #[test]
    fn outdated_rule_is_replaced() {
        assert_eq!(plan(Some("# an older rule\n")), Plan::Update);
        assert_eq!(plan(Some("")), Plan::Update);
    }

    #[test]
    fn no_devices_need_no_access() {
        assert!(!needs_access(&[]));
    }

    #[test]
    fn a_missing_node_needs_access_unless_root() {
        let device = FoundDevice {
            vid: 0x3151,
            pid: 0x4010,
            platform: rt85_protocol::ids::Platform::Royuan,
            known: None,
            product: String::new(),
            manufacturer: String::new(),
            serial: String::new(),
            interfaces: vec![crate::Interface {
                path: CString::new("/nonexistent/rt85-test-hidraw").unwrap(),
                number: 0,
                usages: Vec::new(),
            }],
        };
        assert_eq!(needs_access(&[device]), !is_root());
    }

}
