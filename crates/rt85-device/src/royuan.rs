//! Royuan-platform keyboards (feature reports).

use crate::{Error, Progress, Result};
use hidapi::{HidApi, HidDevice};
use rt85_protocol::{
    REPORT_LEN, Report,
    royuan::{
        self, Family,
        gen2::{self, Identity},
        keys::RgbFrame,
        lighting::{self, Profile},
        screen::{self, Animation},
        wireless,
    },
    time::DateTime,
};
use std::time::{Duration, Instant};
use tracing::{debug, trace, warn};

/// Minimum spacing between reports (the reference tool uses 1 ms).
const MIN_GAP: Duration = Duration::from_millis(1);
/// Spacing for gen2 (RT85) image streams. Epomaker's driver sends them about
/// 5 ms apart (measured in a capture), and the controller has no flow
/// control: reports pushed too fast are acknowledged and dropped.
const GEN2_IMAGE_GAP: Duration = Duration::from_millis(5);
/// Time between sending a request and reading its reply.
const REPLY_DELAY: Duration = Duration::from_millis(10);
/// After a picture the keyboard refuses the next upload for 1-5 s; keep
/// asking for this long before giving up.
const PREPARE_TIMEOUT: Duration = Duration::from_secs(10);
const PREPARE_RETRY: Duration = Duration::from_millis(200);

pub struct RoyuanKeyboard {
    dev: HidDevice,
    family: Family,
    last_send: Option<Instant>,
}

impl RoyuanKeyboard {
    pub fn open(api: &HidApi, path: &std::ffi::CStr, family: Family) -> Result<Self> {
        debug!(?path, ?family, "opening royuan interface");
        Ok(Self { dev: api.open_path(path)?, family, last_send: None })
    }

    pub fn family(&self) -> Family {
        self.family
    }

    pub fn set_family(&mut self, family: Family) {
        self.family = family;
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

    /// Read a feature report (64 bytes, report ID stripped).
    fn read(&mut self) -> Result<Vec<u8>> {
        let mut buf = [0u8; REPORT_LEN + 1];
        let n = self.dev.get_feature_report(&mut buf)?;
        let reply = gen2::unwrap_reply(&buf[..n]).to_vec();
        trace!(reply = %hex(&reply), "feature reply");
        Ok(reply)
    }

    /// Send a request and read its reply.
    pub fn exchange(&mut self, report: &Report) -> Result<Vec<u8>> {
        self.send(report)?;
        std::thread::sleep(REPLY_DELAY);
        self.read()
    }

    /// Send a poll and read the reply.
    pub fn poll(&mut self) -> Result<Vec<u8>> {
        self.exchange(&royuan::poll())
    }

    /// Ask the keyboard which model it is. `None` if the reply isn't an
    /// identify reply (e.g. older firmware).
    pub fn identify(&mut self) -> Result<Option<Identity>> {
        let reply = self.exchange(&gen2::identify())?;
        let id = gen2::parse_identity(&reply);
        debug!(?id, "identify");
        Ok(id)
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
        self.send(&p.report(self.family))
    }

    /// Per-key colours. yc500 streams RGB frames; gen2 stores the first
    /// frame as user picture 1 and switches the lighting to it.
    pub fn set_key_colors(&mut self, frames: &[RgbFrame]) -> Result<()> {
        match self.family {
            Family::Yc500 => {
                let reports = royuan::keys::rgb_frames(frames);
                self.send_all(&reports, &mut |_, _| {})
            }
            Family::Gen2 => {
                let colors = frames.first().map(|f| f.colors.as_slice()).unwrap_or_default();
                self.send_all(&gen2::picture(0, colors), &mut |_, _| {})?;
                let show = Profile { effect: lighting::GEN2_PICTURE, option: 0, ..Default::default() };
                self.set_profile(&show)
            }
        }
    }

    pub fn remap(&mut self, key_index: u8, hid_code: u8) -> Result<()> {
        match self.family {
            Family::Yc500 => self.send(&royuan::keys::remap(key_index, hid_code)),
            Family::Gen2 => {
                // Writes go to a profile, so change the one in use.
                let profile = gen2::parse_profile(&self.exchange(&gen2::get_profile())?).unwrap_or_else(|| {
                    warn!("no profile in the 0x84 reply; writing profile 0");
                    0
                });
                self.send(&gen2::set_key(profile, key_index, gen2::key_action(hid_code)))
            }
        }
    }

    pub fn upload(&mut self, anim: &Animation, progress: Progress) -> Result<()> {
        let reports = anim.reports()?;
        debug!(frames = anim.frames.len(), w = anim.width, h = anim.height, reports = reports.len(), "uploading");
        match self.family {
            Family::Yc500 => self.send_all(&reports, progress),
            Family::Gen2 => {
                let (prepare, pixels) = reports.split_first().expect("reports start with the prepare");
                self.prepare(prepare)?;
                std::thread::sleep(Duration::from_millis(100));
                self.send_all_spaced(pixels, GEN2_IMAGE_GAP, progress)
            }
        }
    }

    /// Send the upload's prepare report until the keyboard accepts it.
    fn prepare(&mut self, prepare: &Report) -> Result<()> {
        let deadline = Instant::now() + PREPARE_TIMEOUT;
        loop {
            let reply = self.exchange(prepare)?;
            match screen::prepare_accepted(&reply) {
                Some(true) => return Ok(()),
                // Not an a5 reply at all: this firmware may not acknowledge.
                // The capture of Epomaker's driver shows it carrying on.
                None => {
                    warn!(reply = %hex(&reply), "no acknowledgement for the upload; sending anyway");
                    return Ok(());
                }
                Some(false) if Instant::now() >= deadline => {
                    return Err(Error::Timeout("the keyboard is still busy with the last picture"));
                }
                Some(false) => std::thread::sleep(PREPARE_RETRY),
            }
        }
    }

    pub fn clear_screen(&mut self) -> Result<()> {
        match self.family {
            Family::Yc500 => self.send(&screen::clear()),
            // 0xAC erases flash on gen2.
            Family::Gen2 => Err(Error::Unsupported("clearing the screen")),
        }
    }

    pub fn set_time(&mut self, t: DateTime) -> Result<()> {
        self.send(&screen::set_time(t))
    }

    pub fn set_cpu(&mut self, percent: u8) -> Result<()> {
        self.yc500_only("CPU widget")?;
        self.send(&screen::set_cpu(percent))
    }

    pub fn set_temperature(&mut self, celsius: u8) -> Result<()> {
        self.yc500_only("temperature widget")?;
        self.send(&screen::set_temperature(celsius))
    }

    pub fn toggle_language(&mut self) -> Result<()> {
        match self.family {
            Family::Gen2 => self.send(&gen2::toggle_language()),
            Family::Yc500 => Err(Error::Unsupported("the screen language switch")),
        }
    }

    fn yc500_only(&self, what: &'static str) -> Result<()> {
        match self.family {
            Family::Yc500 => Ok(()),
            Family::Gen2 => Err(Error::Unsupported(what)),
        }
    }
}

pub(crate) fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
