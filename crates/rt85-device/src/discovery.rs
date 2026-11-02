//! Find supported keyboards among the HID devices.

use hidapi::HidApi;
use rt85_protocol::ids::{self, KnownId, Link, Platform};
use std::ffi::CString;

/// One HID interface (one `/dev/hidraw*` node on Linux).
#[derive(Debug, Clone)]
pub struct Interface {
    pub path: CString,
    pub number: i32,
    /// `(usage_page, usage)` of every top-level collection on this node.
    pub usages: Vec<(u16, u16)>,
}

impl Interface {
    pub fn has_page(&self, f: impl Fn(u16) -> bool) -> bool {
        self.usages.iter().any(|(p, _)| f(*p))
    }
}

/// A physical keyboard, with all of its HID interfaces.
#[derive(Debug, Clone)]
pub struct FoundDevice {
    pub vid: u16,
    pub pid: u16,
    pub platform: Platform,
    /// Set when VID:PID is in the known-ID table.
    pub known: Option<&'static KnownId>,
    pub product: String,
    pub manufacturer: String,
    pub serial: String,
    pub interfaces: Vec<Interface>,
}

impl FoundDevice {
    pub fn link(&self) -> Link {
        self.known.map(|k| k.link).unwrap_or(Link::Wired)
    }

    pub fn display_name(&self) -> String {
        if !self.product.is_empty() {
            self.product.clone()
        } else if let Some(k) = self.known {
            format!("Epomaker {}", k.model)
        } else {
            format!("{:04x}:{:04x}", self.vid, self.pid)
        }
    }

    /// The configuration interface:
    /// * vendor-defined usage page (≥ `0xFF00`) if the backend reports one,
    /// * else interface 1 (the QMK raw-HID convention on RDMCTMZT boards),
    /// * else the first interface (what `hid_open(vid, pid)` would pick).
    pub fn config_interface(&self) -> Option<&Interface> {
        let preferred = match self.platform {
            Platform::Rdmctmzt => self.interfaces.iter().find(|i| i.has_page(|p| p == ids::RDMCTMZT_RAW_USAGE_PAGE)),
            Platform::Royuan => None,
        };
        preferred
            .or_else(|| self.interfaces.iter().find(|i| i.has_page(|p| p >= 0xFF00)))
            .or_else(|| self.interfaces.iter().find(|i| i.number == 1 && self.platform == Platform::Rdmctmzt))
            .or_else(|| self.interfaces.first())
    }
}

/// Re-enumerate and return every supported keyboard.
pub fn list(api: &mut HidApi) -> crate::Result<Vec<FoundDevice>> {
    api.refresh_devices()?;
    let mut found: Vec<FoundDevice> = Vec::new();
    for d in api.device_list() {
        let Some((platform, known)) = ids::classify(d.vendor_id(), d.product_id()) else {
            continue;
        };
        let usage = (d.usage_page(), d.usage());
        let iface = Interface { path: d.path().to_owned(), number: d.interface_number(), usages: vec![usage] };
        let serial = d.serial_number().unwrap_or_default().to_owned();
        if let Some(existing) = found
            .iter_mut()
            .find(|f| f.vid == d.vendor_id() && f.pid == d.product_id() && f.serial == serial)
        {
            match existing.interfaces.iter_mut().find(|i| i.path == iface.path) {
                Some(i) if !i.usages.contains(&usage) => i.usages.push(usage),
                Some(_) => {}
                None => existing.interfaces.push(iface),
            }
            continue;
        }
        found.push(FoundDevice {
            vid: d.vendor_id(),
            pid: d.product_id(),
            platform,
            known,
            product: d.product_string().unwrap_or_default().to_owned(),
            manufacturer: d.manufacturer_string().unwrap_or_default().to_owned(),
            serial,
            interfaces: vec![iface],
        });
    }
    for f in &mut found {
        f.interfaces.sort_by_key(|i| i.number);
    }
    Ok(found)
}

/// Path of the RDMCTMZT LCD's data interface, if it is currently enumerated.
pub fn find_lcd(api: &mut HidApi) -> crate::Result<Option<CString>> {
    api.refresh_devices()?;
    let lcd: Vec<_> = api
        .device_list()
        .filter(|d| d.vendor_id() == ids::RDMCTMZT_LCD_VID && d.product_id() == ids::RDMCTMZT_LCD_PID)
        .collect();
    let pick = lcd
        .iter()
        .find(|d| d.usage_page() == 0x00FF)
        .or_else(|| lcd.iter().find(|d| d.interface_number() == 1))
        .or_else(|| lcd.first());
    Ok(pick.map(|d| d.path().to_owned()))
}
