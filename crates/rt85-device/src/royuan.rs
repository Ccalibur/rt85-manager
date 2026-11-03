//! Royuan-platform keyboards (feature reports).

use crate::{Progress, Result};
use hidapi::{HidApi, HidDevice};
use rt85_protocol::{
    REPORT_LEN, Report,
    royuan::{self, keys::RgbFrame, lighting::Profile, screen::{self, Animation}, wireless},
    time::DateTime,
};
use std::time::{Duration, Instant};
use tracing::{debug, trace};

/// Minimum spacing between reports (the reference tool uses 1 ms).
const MIN_GAP: Duration = Duration::from_millis(1);
/// Spacing for full-screen image streams. Epomaker's driver sends them about
/// 5 ms apart, and the RT85 drops reports when pushed much faster.
const FULL_SCREEN_GAP: Duration = Duration::from_millis(5);

pub struct RoyuanKeyboard {
    dev: HidDevice,
    last_send: Option<Instant>,
}

impl RoyuanKeyboard {
    pub fn open(api: &HidApi, path: &std::ffi::CStr) -> Result<Self> {
        debug!(?path, "opening royuan interface");
        Ok(Self { dev: api.open_path(path)?, last_send: None })
    }

    pub fn send(&mut self, report: &Report) -> Result<()> {
        self.send_spaced(report, MIN_GAP)
    }

    fn send_spaced(&mut self, report: &Report, gap: Duration) -> Result<()> {
        if let Some(t) = self.last_send {
            let elapsed = t.elapsed();
            if elapsed < gap {
                std::thread::sleep(gap - elapsed);
            }
        }
        trace!(report = %hex(report), "feature report");
        self.dev.send_feature_report(report)?;
        self.last_send = Some(Instant::now());
        Ok(())
    }

    pub fn send_all(&mut self, reports: &[Report], progress: Progress) -> Result<()> {
        self.send_all_spaced(reports, MIN_GAP, progress)
    }

    fn send_all_spaced(&mut self, reports: &[Report], gap: Duration, progress: Progress) -> Result<()> {
        let total = reports.len();
        for (i, r) in reports.iter().enumerate() {
            self.send_spaced(r, gap)?;
            progress(i + 1, total);
        }
        Ok(())
    }

    /// Send a poll and read the reply.
    pub fn poll(&mut self) -> Result<Vec<u8>> {
        self.send(&royuan::poll())?;
        let mut buf = [0u8; REPORT_LEN + 1];
        let n = self.dev.get_feature_report(&mut buf)?;
        Ok(buf[..n].to_vec())
    }

    /// Wake the board behind a 2.4 GHz dongle. Returns whether it answered.
    pub fn wireless_init(&mut self) -> Result<bool> {
        if wireless::is_awake(&self.poll()?) {
            return Ok(true);
        }
        for r in wireless::init_sequence() {
            self.poll()?;
            self.send(&r)?;
        }
        Ok(wireless::is_awake(&self.poll()?))
    }

    pub fn set_profile(&mut self, p: &Profile) -> Result<()> {
        self.send(&p.report())
    }

    pub fn set_key_colors(&mut self, frames: &[RgbFrame]) -> Result<()> {
        let reports = royuan::keys::rgb_frames(frames);
        self.send_all(&reports, &mut |_, _| {})
    }

    pub fn remap(&mut self, key_index: u8, hid_code: u8) -> Result<()> {
        self.send(&royuan::keys::remap(key_index, hid_code))
    }

    pub fn upload(&mut self, anim: &Animation, progress: Progress) -> Result<()> {
        let reports = anim.reports()?;
        let gap = if anim.is_extended() { FULL_SCREEN_GAP } else { MIN_GAP };
        debug!(frames = anim.frames.len(), w = anim.width, h = anim.height, reports = reports.len(), ?gap, "uploading");
        self.send_all_spaced(&reports, gap, progress)
    }

    pub fn clear_screen(&mut self) -> Result<()> {
        self.send(&screen::clear())
    }

    pub fn set_time(&mut self, t: DateTime) -> Result<()> {
        self.send(&screen::set_time(t))
    }

    pub fn set_cpu(&mut self, percent: u8) -> Result<()> {
        self.send(&screen::set_cpu(percent))
    }

    pub fn set_temperature(&mut self, celsius: u8) -> Result<()> {
        self.send(&screen::set_temperature(celsius))
    }
}

pub(crate) fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
