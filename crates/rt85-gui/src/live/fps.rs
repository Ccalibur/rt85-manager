//! Game FPS, read from MangoHud's live CSV log.
//!
//! Linux has no system-wide FPS counter; MangoHud measures it inside the game.
//! With logging on, it appends (and flushes) one CSV row per `log_interval`
//! to `<output_folder>/<game>_<date>_<time>.csv`. We read the newest file's
//! last row while it is still being written.

use std::{
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
    time::{Duration, SystemTime},
};

#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub fps: u32,
    pub game: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    NotInstalled,
    /// Installed, but logging only starts on a key press.
    NeedsAutostart,
    Ready,
}

/// A log not written for this long means the game has closed.
const STALE: Duration = Duration::from_secs(4);

fn config_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("MANGOHUD_CONFIGFILE") {
        return Some(p.into());
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("MangoHud/MangoHud.conf"))
}

fn config_value(key: &str) -> Option<String> {
    let text = std::fs::read_to_string(config_path()?).ok()?;
    text.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .filter(|(k, _)| k.trim() == key)
        .map(|(_, v)| v.trim().to_owned())
        .next_back()
}

fn installed() -> bool {
    config_path().is_some_and(|p| p.exists())
        || std::env::var_os("PATH")
            .is_some_and(|path| std::env::split_paths(&path).any(|d| d.join("mangohud").exists()))
}

pub fn status() -> Status {
    if !installed() {
        Status::NotInstalled
    } else if config_value("autostart_log").and_then(|v| v.parse::<f32>().ok()).is_some() {
        Status::Ready
    } else {
        Status::NeedsAutostart
    }
}

/// Turn on automatic logging in MangoHud.conf (keeps everything else, and
/// adds an output folder and a 1 s interval only if those aren't set).
pub fn enable_autostart() -> std::io::Result<()> {
    let path = config_path().ok_or_else(|| std::io::Error::other("no config directory"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let mut add = String::from("\n# Added by RT85 Manager so it can show game FPS on the keyboard screen\nautostart_log=1\n");
    if config_value("output_folder").is_none() {
        let folder = default_folder();
        std::fs::create_dir_all(&folder)?;
        add += &format!("output_folder={}\n", folder.display());
    }
    if config_value("log_interval").is_none() {
        add += "log_interval=1000\n";
    }
    let sep = if existing.is_empty() || existing.ends_with('\n') { "" } else { "\n" };
    std::fs::write(&path, format!("{existing}{sep}{add}"))
}

fn default_folder() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_default()
        .join("rt85-manager/mangohud")
}

fn log_folder() -> Option<PathBuf> {
    config_value("output_folder")
        .map(|v| match v.strip_prefix("~/") {
            Some(rest) => std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join(rest),
            None => PathBuf::from(v),
        })
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
}

/// FPS of the game currently logging, if any.
pub fn read() -> Option<Reading> {
    let now = SystemTime::now();
    let (path, _) = std::fs::read_dir(log_folder()?)
        .ok()?
        .flatten()
        .filter(|e| {
            let n = e.file_name();
            let n = n.to_string_lossy();
            n.ends_with(".csv") && !n.ends_with("_summary.csv")
        })
        .filter_map(|e| Some((e.path(), e.metadata().ok()?.modified().ok()?)))
        .filter(|(_, m)| now.duration_since(*m).unwrap_or_default() < STALE)
        .max_by_key(|(_, m)| *m)?;

    // Only the tail matters; the file grows for the whole session.
    let mut f = std::fs::File::open(&path).ok()?;
    let len = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(len.saturating_sub(4096))).ok()?;
    let mut tail = String::new();
    f.read_to_string(&mut tail).ok()?;
    let fps: f32 = tail.lines().rev().find_map(|l| l.split(',').next()?.trim().parse().ok())?;

    Some(Reading { fps: fps.round().max(0.0) as u32, game: game_name(&path) })
}

/// `Cyberpunk2077.exe_2026-11-03_10-15-02.csv` → `Cyberpunk2077`
fn game_name(path: &std::path::Path) -> String {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let mut parts: Vec<&str> = stem.rsplitn(3, '_').collect();
    let name = if parts.len() == 3 { parts.pop().unwrap_or(&stem) } else { &stem };
    name.trim_end_matches(".exe").to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_name_strips_timestamp() {
        let p = std::path::Path::new("/x/Cyberpunk2077.exe_2026-11-03_10-15-02.csv");
        assert_eq!(game_name(p), "Cyberpunk2077");
        let p = std::path::Path::new("/x/my_game_2026-11-03_10-15-02.csv");
        assert_eq!(game_name(p), "my_game");
    }
}
