//! Owns the HID handle. Every call blocks, so the app runs them on
//! `spawn_blocking` behind a mutex.

use rt85_device::{Capabilities, FoundDevice, HidApi, Keyboard};
use rt85_protocol::{
    ids::{Link, Platform},
    layout::Board,
    royuan::gen2::Identity,
    rdmctmzt::Qgif,
    royuan::{keys::RgbFrame, lighting::Profile, screen::Animation},
};
use std::sync::Arc;

/// What the UI needs to know about a connected keyboard.
#[derive(Debug, Clone)]
pub struct DeviceSummary {
    pub name: String,
    pub vid: u16,
    pub pid: u16,
    pub platform: Platform,
    pub link: Link,
    pub model: Option<&'static str>,
    /// Board whose command set, layout and screen are used.
    pub board: Board,
    /// Firmware version, when the keyboard answered the identify command.
    pub firmware: Option<String>,
    pub manufacturer: String,
    pub interfaces: Vec<String>,
    /// hidraw node of every interface (the macro listener reads these).
    pub hid_paths: Vec<String>,
    pub caps: Capabilities,
}

#[derive(Debug, Clone)]
pub enum Job {
    Profile(Profile),
    KeyColors(Vec<RgbFrame>),
    Remap { index: u8, code: u8 },
    Upload(Arc<Animation>),
    Qgif(Arc<Qgif>),
    ClearScreen,
    SyncTime,
    Cpu(u8),
    Temperature(u8),
    ToggleLanguage,
    /// Several jobs in a row (e.g. restore one key, remap another).
    Batch(Vec<Job>),
}

impl Job {
    pub fn describe(&self) -> String {
        match self {
            Job::Profile(_) => "set the lighting".into(),
            Job::KeyColors(_) => "send key colours".into(),
            Job::Remap { index, code } => {
                format!("remap key {index} to {}", rt85_protocol::keycode::name(*code).unwrap_or("?"))
            }
            Job::Upload(a) => format!("upload {} frame(s)", a.frames.len()),
            Job::Qgif(_) => "upload QGIF".into(),
            Job::ClearScreen => "clear the screen".into(),
            Job::SyncTime => "sync the clock".into(),
            Job::Cpu(v) => format!("show CPU {v}%"),
            Job::Temperature(v) => format!("show {v} °C"),
            Job::ToggleLanguage => "switch the screen language".into(),
            Job::Batch(jobs) => jobs.iter().map(Job::describe).collect::<Vec<_>>().join(", "),
        }
    }
}

pub struct Worker {
    api: Option<HidApi>,
    found: Vec<FoundDevice>,
    /// Identify reply per found device (same order as `found`).
    identities: Vec<Option<Identity>>,
    active: Option<usize>,
    kb: Option<Keyboard>,
}

impl Worker {
    pub fn new() -> Self {
        Self { api: None, found: Vec::new(), identities: Vec::new(), active: None, kb: None }
    }

    fn api(&mut self) -> rt85_device::Result<&mut HidApi> {
        if self.api.is_none() {
            self.api = Some(HidApi::new()?);
        }
        Ok(self.api.as_mut().expect("just set"))
    }

    /// Re-enumerate. Keeps the active device if it is still connected.
    pub fn scan(&mut self) -> rt85_device::Result<(Vec<DeviceSummary>, Option<usize>)> {
        let prev = self.active.and_then(|i| self.found.get(i)).map(|f| (f.vid, f.pid, f.serial.clone()));
        let found = rt85_device::list(self.api()?)?;
        let active = prev
            .and_then(|(v, p, s)| found.iter().position(|f| f.vid == v && f.pid == p && f.serial == s))
            .or(if found.is_empty() { None } else { Some(0) });
        if active != self.active || active.is_none() || found.len() != self.found.len() {
            self.kb = None;
        }
        // Keep earlier answers; ask boards we haven't seen yet. Several share
        // 3151:5002, and only the keyboard itself knows which one it is.
        let key = |f: &FoundDevice| (f.vid, f.pid, f.serial.clone());
        let old: Vec<_> = self.found.iter().map(key).zip(self.identities.iter().copied()).collect();
        let mut identities = Vec::with_capacity(found.len());
        for f in &found {
            let known = old.iter().find(|(k, _)| *k == key(f)).map(|(_, id)| *id);
            identities.push(match known {
                Some(id) => id,
                None => self.identify(f),
            });
        }
        self.found = found;
        self.identities = identities;
        self.active = active;
        Ok((self.found.iter().zip(&self.identities).map(|(f, id)| summary(f, *id)).collect(), active))
    }

    /// Ask a wired Royuan board for its model ID. Failures just mean we
    /// fall back to the USB ID.
    fn identify(&mut self, f: &FoundDevice) -> Option<Identity> {
        if f.platform != Platform::Royuan || f.link() != Link::Wired {
            return None;
        }
        let api = self.api().ok()?;
        let mut kb = Keyboard::open(api, f, guess_board(f)).ok()?;
        match kb.identify() {
            Ok(id) => id,
            Err(e) => {
                tracing::debug!("identify failed: {e}");
                None
            }
        }
    }

    pub fn select(&mut self, index: usize) {
        if index < self.found.len() && Some(index) != self.active {
            self.active = Some(index);
            self.kb = None;
        }
    }

    pub fn run(&mut self, job: Job, progress: &mut dyn FnMut(usize, usize)) -> rt85_device::Result<String> {
        let result = self.run_inner(job, progress);
        if result.is_err() {
            // Drop the handle so the next job reopens it (e.g. after a replug).
            self.kb = None;
        }
        result
    }

    fn run_inner(&mut self, job: Job, progress: &mut dyn FnMut(usize, usize)) -> rt85_device::Result<String> {
        if let Job::Batch(jobs) = job {
            let mut last = String::new();
            for j in jobs {
                last = self.run_inner(j, progress)?;
            }
            return Ok(last);
        }
        let dev = self.active.and_then(|i| self.found.get(i)).cloned().ok_or(rt85_device::Error::NotFound)?;
        if self.kb.is_none() {
            let board = board_for(&dev, self.active.and_then(|i| self.identities.get(i).copied()).flatten());
            let api = self.api()?;
            self.kb = Some(Keyboard::open(api, &dev, board)?);
        }
        let api = self.api.as_mut().expect("opened above");
        let kb = self.kb.as_mut().expect("opened above");
        Ok(match job {
            Job::Profile(p) => {
                kb.set_profile(&p)?;
                "Lighting set".into()
            }
            Job::KeyColors(frames) => {
                kb.set_key_colors(&frames)?;
                "Key colours sent".into()
            }
            Job::Remap { index, code } => {
                kb.remap(index, code)?;
                format!("Key {index} remapped to {}", rt85_protocol::keycode::name(code).unwrap_or("?"))
            }
            Job::Upload(anim) => {
                kb.upload_animation(&anim, progress)?;
                format!("Uploaded {} frame(s)", anim.frames.len())
            }
            Job::Qgif(q) => {
                kb.upload_qgif(api, &q, rt85_device::local_now(), progress)?;
                "QGIF uploaded".into()
            }
            Job::ClearScreen => {
                kb.clear_screen()?;
                "Screen cleared".into()
            }
            Job::SyncTime => {
                kb.set_time(api, rt85_device::local_now())?;
                "Clock synchronised".into()
            }
            Job::Cpu(v) => {
                kb.set_cpu(v)?;
                format!("CPU {v}%")
            }
            Job::Temperature(v) => {
                kb.set_temperature(v)?;
                format!("{v} °C")
            }
            Job::ToggleLanguage => {
                kb.toggle_language()?;
                "Screen language switched".into()
            }
            Job::Batch(_) => unreachable!("handled above"),
        })
    }
}

/// Board from the USB ID alone.
fn guess_board(f: &FoundDevice) -> Board {
    Board::for_model(f.known.map(|k| k.model))
}

/// Board from the identify reply, else from the USB ID.
fn board_for(f: &FoundDevice, id: Option<Identity>) -> Board {
    id.and_then(|i| Board::for_device_id(i.device_id)).unwrap_or_else(|| guess_board(f))
}

fn summary(f: &FoundDevice, id: Option<Identity>) -> DeviceSummary {
    let board = board_for(f, id);
    let identified = id.and_then(|i| Board::for_device_id(i.device_id)).map(|b| match b {
        Board::Rt85 => "RT85",
        Board::Rt100 => "RT100",
    });
    DeviceSummary {
        name: f.display_name(),
        vid: f.vid,
        pid: f.pid,
        platform: f.platform,
        link: f.link(),
        model: identified.or(f.known.map(|k| k.model)),
        board,
        firmware: id.map(|i| i.firmware_string()),
        manufacturer: f.manufacturer.clone(),
        interfaces: f
            .interfaces
            .iter()
            .map(|i| {
                let u: Vec<String> = i.usages.iter().map(|(p, u)| format!("{p:04x}:{u:04x}")).collect();
                format!("#{} {} [{}]", i.number, i.path.to_string_lossy(), u.join(" "))
            })
            .collect(),
        hid_paths: f.interfaces.iter().map(|i| i.path.to_string_lossy().into_owned()).collect(),
        caps: Capabilities::for_device(f.platform, board),
    }
}
