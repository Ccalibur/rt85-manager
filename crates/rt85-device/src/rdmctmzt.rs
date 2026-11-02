//! RDMCTMZT-platform keyboards: keyboard raw-HID interface + separate LCD device.

use crate::{Error, Progress, Result, discovery, royuan::hex};
use hidapi::{HidApi, HidDevice};
use rt85_protocol::{REPORT_LEN, Report, rdmctmzt as p, time::DateTime};
use std::{ffi::CString, thread::sleep, time::{Duration, Instant}};
use tracing::{debug, trace, warn};

const KBD_GAP: Duration = Duration::from_millis(20);
const POLL: Duration = Duration::from_millis(300);
/// How long to wait for the LCD to (re-)enumerate. Linux can take 1–2 s.
const LCD_TIMEOUT: Duration = Duration::from_secs(6);
const READ_TIMEOUT_MS: i32 = 1000;

pub struct RdmctmztKeyboard {
    kbd_path: CString,
}

/// The `1919:1919` LCD. Every write is answered with a 64-byte read.
struct Lcd(HidDevice);

impl Lcd {
    fn send(&self, r: &Report) -> Result<Option<Vec<u8>>> {
        let mut buf = [0u8; REPORT_LEN + 1];
        buf[1..].copy_from_slice(r); // report ID 0
        trace!(report = %hex(r), "lcd write");
        let n = self.0.write(&buf)?;
        if n < REPORT_LEN {
            return Err(Error::ShortWrite(n));
        }
        let mut resp = [0u8; REPORT_LEN];
        let n = self.0.read_timeout(&mut resp, READ_TIMEOUT_MS)?;
        Ok((n > 0).then(|| resp[..n].to_vec()))
    }
}

impl RdmctmztKeyboard {
    pub fn new(kbd_path: CString) -> Self {
        Self { kbd_path }
    }

    fn open_kbd(&self, api: &HidApi) -> Result<HidDevice> {
        Ok(api.open_path(&self.kbd_path)?)
    }

    fn send_kbd(dev: &HidDevice, r: &Report) -> Result<()> {
        trace!(report = %hex(r), "kbd write");
        dev.write(r)?;
        sleep(KBD_GAP);
        Ok(())
    }

    /// Wake the LCD and open it. If it was already present we wait for it to
    /// drop off and come back, because writes to the stale node fail.
    fn wake_lcd(&self, api: &mut HidApi, kbd: &HidDevice) -> Result<Lcd> {
        let before = discovery::find_lcd(api)?;
        for r in p::kbd_wake_lcd() {
            Self::send_kbd(kbd, &r)?;
        }
        let start = Instant::now();
        if before.is_some() {
            while start.elapsed() < LCD_TIMEOUT && discovery::find_lcd(api)?.is_some() {
                sleep(POLL);
            }
        }
        loop {
            sleep(POLL);
            if let Some(path) = discovery::find_lcd(api)? {
                debug!(?path, "LCD appeared");
                return Ok(Lcd(api.open_path(&path)?));
            }
            if start.elapsed() > LCD_TIMEOUT {
                return Err(Error::LcdMissing);
            }
        }
    }

    fn lcd_handshake(lcd: &Lcd, now: DateTime) -> Result<()> {
        for r in [p::lcd_query(), p::lcd_set_time(now), p::lcd_start(), p::lcd_status(), p::lcd_query(),
                  p::lcd_screen_query(), p::lcd_start(), p::lcd_status()] {
            lcd.send(&r)?;
        }
        Ok(())
    }

    /// Upload an already-encoded QGIF file (single screen).
    pub fn upload_qgif(&self, api: &mut HidApi, qgif: &p::Qgif, now: DateTime, progress: Progress) -> Result<()> {
        let data = qgif.bytes();
        if data.len() > p::Qgif::SOFT_LIMIT {
            warn!(size = data.len(), "QGIF is over 64 KiB; the RT82 firmware may show artifacts");
        }

        let kbd = self.open_kbd(api)?;
        let lcd = self.wake_lcd(api, &kbd)?;
        Self::lcd_handshake(&lcd, now)?;

        // The web tool sends getGifCount on the keyboard with both devices open.
        Self::send_kbd(&kbd, &p::kbd_gif_count(1))?;
        drop(kbd);

        let result = (|| {
            for r in [p::lcd_prepare(), p::lcd_download_mode(1), p::lcd_screen_prep()] {
                lcd.send(&r)?;
            }
            for r in p::lcd_transfer_setup(data.len()) {
                lcd.send(&r)?;
            }

            // Flash erase: poll status until the device answers.
            let erase = p::erase_blocks(data.len());
            let deadline = Instant::now() + Duration::from_secs(u64::from(erase).max(2));
            let mut ready = false;
            while Instant::now() < deadline {
                sleep(POLL);
                if lcd.send(&p::lcd_status())?.is_some() {
                    ready = true;
                    break;
                }
            }
            if !ready {
                sleep(Duration::from_secs(1));
            }

            let total = data.len().div_ceil(p::DATA_CHUNK);
            for (i, r) in p::lcd_data_packets(data).enumerate() {
                lcd.send(&r)?;
                progress(i + 1, total);
            }

            lcd.send(&p::lcd_end())?;
            Self::sync_time_on(&lcd, now)
        })();

        if result.is_err() {
            // Try to leave "Downloading…" mode so the screen isn't stuck.
            let _ = lcd.send(&p::lcd_end());
            let _ = lcd.send(&p::lcd_start());
        }
        result
    }

    fn sync_time_on(lcd: &Lcd, now: DateTime) -> Result<()> {
        lcd.send(&p::lcd_query())?;
        lcd.send(&p::lcd_set_time(now))?;
        lcd.send(&p::lcd_start())?;
        Ok(())
    }

    pub fn sync_time(&self, api: &mut HidApi, now: DateTime) -> Result<()> {
        let kbd = self.open_kbd(api)?;
        let lcd = self.wake_lcd(api, &kbd)?;
        drop(kbd);
        Self::sync_time_on(&lcd, now)
    }

    /// Write one raw 64-byte report to the keyboard interface (for probing).
    pub fn send_raw_kbd(&self, api: &HidApi, r: &Report) -> Result<()> {
        let kbd = self.open_kbd(api)?;
        Self::send_kbd(&kbd, r)
    }
}
