//! Desktop integration: icon theme detection, following the system colour
//! scheme (COSMIC theme, KDE/Breeze colour schemes, GNOME's portal
//! preference) and the user's appearance preferences.

use cosmic::cosmic_config::{Config, ConfigGet, ConfigSet};
use rt85_protocol::layout::Physical;
use cosmic::cosmic_theme::{self, ThemeBuilder, palette::{Srgb, Srgba}};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Desktop {
    Cosmic,
    Kde,
    Gnome,
    Other,
}

impl Desktop {
    pub fn detect() -> Self {
        let current = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_ascii_lowercase();
        let has = |name: &str| current.split(':').any(|d| d == name);
        if has("cosmic") {
            Self::Cosmic
        } else if has("kde") || std::env::var_os("KDE_FULL_SESSION").is_some() {
            Self::Kde
        } else if has("gnome") || has("unity") || has("budgie") || has("pantheon") {
            Self::Gnome
        } else {
            Self::Other
        }
    }
}

// ---------------------------------------------------------------------------
// Icon theme

/// The header bar's window controls look up these names in the icon theme.
const WINDOW_CONTROL_ICONS: [&str; 4] =
    ["window-close-symbolic", "window-maximize-symbolic", "window-minimize-symbolic", "window-restore-symbolic"];

/// libcosmic defaults to the "Cosmic" icon theme, which usually isn't
/// installed on KDE or GNOME, so the minimize/maximize/close buttons come out
/// blank there. Pick the desktop's own icon theme (falling back to well-known
/// ones) that actually provides the window-control icons.
///
/// `Ok(None)`: leave libcosmic's choice alone (COSMIC with its icons present).
/// `Ok(Some(theme))`: use this theme. `Err(())`: no theme has them; the app
/// draws its own built-in buttons instead.
pub fn pick_icon_theme(desktop: Desktop) -> Result<Option<String>, ()> {
    let mut candidates: Vec<String> = Vec::new();
    if desktop == Desktop::Cosmic {
        candidates.push(cosmic::icon_theme::COSMIC.into());
    }
    candidates.extend(system_icon_theme(desktop));
    candidates.extend(["breeze", "Adwaita", "Papirus", "Cosmic", "hicolor"].map(String::from));
    let mut seen = std::collections::HashSet::new();
    candidates.retain(|t| seen.insert(t.clone()));

    for (i, theme) in candidates.into_iter().enumerate() {
        cosmic::icon_theme::set_default(theme.clone());
        let ok = WINDOW_CONTROL_ICONS[..3]
            .iter()
            .all(|name| cosmic::widget::icon::from_name(*name).fallback(None).path().is_some());
        if ok {
            tracing::debug!("icon theme for window controls: {theme}");
            return Ok(if desktop == Desktop::Cosmic && i == 0 { None } else { Some(theme) });
        }
    }
    cosmic::icon_theme::set_default(cosmic::icon_theme::COSMIC);
    tracing::warn!("no installed icon theme provides window-control icons; using built-in ones");
    Err(())
}

fn system_icon_theme(desktop: Desktop) -> Option<String> {
    let kde = || kde_config().get("Icons", "Theme").map(str::to_owned).or(Some("breeze".into()));
    let gnome = || {
        let out = std::process::Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "icon-theme"])
            .output()
            .ok()?;
        let v = String::from_utf8_lossy(&out.stdout).trim().trim_matches('\'').to_owned();
        (!v.is_empty()).then_some(v)
    };
    let gtk = || {
        let ini = Ini::parse(&std::fs::read_to_string(config_home().join("gtk-3.0/settings.ini")).ok()?);
        ini.get("Settings", "gtk-icon-theme-name").map(str::to_owned)
    };
    match desktop {
        Desktop::Cosmic => None,
        Desktop::Kde => kde(),
        Desktop::Gnome => gnome().or_else(gtk),
        Desktop::Other => gtk().or_else(gnome),
    }
}

// ---------------------------------------------------------------------------
// Preferences

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scheme {
    #[default]
    System,
    Light,
    Dark,
}

impl Scheme {
    pub const ALL: [Scheme; 3] = [Scheme::System, Scheme::Light, Scheme::Dark];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Prefs {
    pub scheme: Scheme,
    /// `None` follows the system's frosted-windows setting (COSMIC only).
    pub frosted: Option<bool>,
    /// The keyboard's physical layout; `None` until the user has said.
    pub keyboard_layout: Option<Physical>,
}

impl Prefs {
    fn config() -> Option<Config> {
        Config::new(crate::app::APP_ID, 1).map_err(|e| tracing::warn!("settings unavailable: {e}")).ok()
    }

    pub fn load() -> Self {
        let Some(c) = Self::config() else { return Self::default() };
        let scheme = match c.get::<u8>("color-scheme").unwrap_or(0) {
            1 => Scheme::Light,
            2 => Scheme::Dark,
            _ => Scheme::System,
        };
        let keyboard_layout = match c.get::<String>("keyboard-layout").ok().as_deref() {
            Some("ansi") => Some(Physical::Ansi),
            Some("iso") => Some(Physical::Iso),
            _ => None,
        };
        Self { scheme, frosted: c.get::<Option<bool>>("frosted").ok().flatten(), keyboard_layout }
    }

    pub fn save(&self) {
        let Some(c) = Self::config() else { return };
        let scheme: u8 = match self.scheme {
            Scheme::System => 0,
            Scheme::Light => 1,
            Scheme::Dark => 2,
        };
        let layout = self.keyboard_layout.map(|l| match l {
            Physical::Ansi => "ansi",
            Physical::Iso => "iso",
        });
        if let Err(e) = c
            .set("color-scheme", scheme)
            .and_then(|()| c.set("frosted", self.frosted))
            .and_then(|()| c.set("keyboard-layout", layout))
        {
            tracing::warn!("saving settings: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// KDE colour schemes

/// Colours taken from the active KDE colour scheme (Breeze, Breeze Dark, ...).
#[derive(Debug, Clone, PartialEq)]
pub struct KdeScheme {
    pub name: String,
    window_bg: Srgb,
    window_fg: Srgb,
    view_bg: Srgb,
    accent: Srgb,
    negative: Option<Srgb>,
    neutral: Option<Srgb>,
    positive: Option<Srgb>,
}

impl KdeScheme {
    /// Reads `kdeglobals`, filling gaps from the scheme's `.colors` file.
    pub fn load() -> Option<Self> {
        Self::load_from(kde_config())
    }

    /// The stock Breeze Light/Dark scheme, for when the user forces the
    /// opposite mode of their KDE colour scheme.
    fn breeze(dark: bool) -> Option<Self> {
        let mut globals = Ini::default();
        let name = if dark { "BreezeDark" } else { "BreezeLight" };
        globals.0.entry("General".into()).or_default().insert("ColorScheme".into(), name.into());
        Self::load_from(globals)
    }

    fn load_from(globals: Ini) -> Option<Self> {
        // kdeglobals usually names the scheme; if not, its own colours are
        // authoritative and Breeze Light/Dark (whichever matches) fills gaps.
        let name = match globals.get("General", "ColorScheme") {
            Some(n) => n.to_owned(),
            None => {
                let dark = globals.get("Colors:Window", "BackgroundNormal").and_then(parse_rgb).is_some_and(|c| luma(c) < 0.5);
                if dark { "BreezeDark" } else { "BreezeLight" }.to_owned()
            }
        };
        let mut ini = data_dirs()
            .iter()
            .map(|d| d.join("color-schemes").join(format!("{name}.colors")))
            .find_map(|p| std::fs::read_to_string(p).ok())
            .map(|s| Ini::parse(&s))
            .unwrap_or_default();
        ini.merge(globals);

        let color = |section: &str, key: &str| ini.get(section, key).and_then(parse_rgb);
        let window_bg = color("Colors:Window", "BackgroundNormal")?;
        Some(Self {
            window_fg: color("Colors:Window", "ForegroundNormal").unwrap_or(Srgb::new(0.5, 0.5, 0.5)),
            view_bg: color("Colors:View", "BackgroundNormal").unwrap_or(window_bg),
            accent: color("General", "AccentColor")
                .or_else(|| color("Colors:Selection", "BackgroundNormal"))
                .unwrap_or(Srgb::new(0.24, 0.68, 0.91)),
            negative: color("Colors:View", "ForegroundNegative"),
            neutral: color("Colors:View", "ForegroundNeutral"),
            positive: color("Colors:View", "ForegroundPositive"),
            window_bg,
            name: pretty_scheme_name(&name),
        })
    }

    pub fn is_dark(&self) -> bool {
        luma(self.window_bg) < 0.5
    }

    fn theme(&self) -> cosmic_theme::Theme {
        let opaque = |c: Srgb| Srgba::new(c.red, c.green, c.blue, 1.0);
        let mut b = if self.is_dark() { ThemeBuilder::dark() } else { ThemeBuilder::light() }
            .bg_color(opaque(self.window_bg))
            .primary_container_bg(opaque(self.view_bg))
            .accent(self.accent)
            .text_tint(self.window_fg);
        if let Some(c) = self.negative {
            b = b.destructive(c);
        }
        if let Some(c) = self.neutral {
            b = b.warning(c);
        }
        if let Some(c) = self.positive {
            b = b.success(c);
        }
        b.build()
    }
}

fn pretty_scheme_name(id: &str) -> String {
    match id {
        "BreezeLight" => "Breeze Light".into(),
        "BreezeDark" => "Breeze Dark".into(),
        "BreezeClassic" => "Breeze Classic".into(),
        other => other.into(),
    }
}

/// Modification time of `kdeglobals`, to notice colour scheme changes.
pub fn kdeglobals_mtime() -> Option<SystemTime> {
    std::fs::metadata(config_home().join("kdeglobals")).and_then(|m| m.modified()).ok()
}

fn kde_config() -> Ini {
    let mut ini = Ini::default();
    for path in [PathBuf::from("/etc/xdg/kdeglobals"), config_home().join("kdeglobals")] {
        if let Ok(s) = std::fs::read_to_string(&path) {
            ini.merge(Ini::parse(&s));
        }
    }
    ini
}

fn luma(c: Srgb) -> f32 {
    0.2126 * c.red + 0.7152 * c.green + 0.0722 * c.blue
}

fn parse_rgb(v: &str) -> Option<Srgb> {
    let mut it = v.split(',').map(|p| p.trim().parse::<u8>().ok());
    let (r, g, b) = (it.next()??, it.next()??, it.next()??);
    Some(Srgb::new(r, g, b).into_format())
}

fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".config")))
        .unwrap_or_default()
}

fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(h) = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| {
        std::env::var_os("HOME").map(|h| Path::new(&h).join(".local/share"))
    }) {
        dirs.push(h);
    }
    let sys = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    dirs.extend(sys.split(':').filter(|s| !s.is_empty()).map(PathBuf::from));
    dirs
}

/// Minimal INI reader, enough for kdeglobals and GTK settings.ini.
#[derive(Debug, Default)]
struct Ini(HashMap<String, HashMap<String, String>>);

impl Ini {
    fn parse(text: &str) -> Self {
        let mut ini = Self::default();
        let mut section = String::new();
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            if let Some(s) = line.strip_prefix('[') {
                // KDE nests groups as "[Colors:Header][Inactive]"; keep the full name.
                section = s.trim_end_matches(']').replace("][", "/");
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                // Drop KDE's "[$e]"-style key flags.
                let k = k.split('[').next().unwrap_or(k).trim();
                ini.0.entry(section.clone()).or_default().insert(k.to_owned(), v.trim().to_owned());
            }
        }
        ini
    }

    fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.0.get(section)?.get(key).map(String::as_str).filter(|v| !v.is_empty())
    }

    fn merge(&mut self, other: Ini) {
        for (s, kv) in other.0 {
            self.0.entry(s).or_default().extend(kv);
        }
    }
}

// ---------------------------------------------------------------------------
// Theme resolution

/// What the app's theme is currently based on (shown on the settings page).
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    Cosmic,
    Kde(String),
    Portal,
    Builtin,
}

/// Builds the theme to apply, given the system's current COSMIC-style theme
/// (`system`), the KDE colour scheme if running on KDE, and the preferences.
pub fn resolve(
    desktop: Desktop,
    prefs: Prefs,
    system: &cosmic_theme::Theme,
    kde: Option<&KdeScheme>,
) -> (cosmic::Theme, Source) {
    let follows_system = prefs.scheme == Scheme::System;
    let want_dark = match prefs.scheme {
        Scheme::System => None,
        Scheme::Light => Some(false),
        Scheme::Dark => Some(true),
    };

    let (mut theme, source, custom) = match (desktop, kde) {
        // KDE: use the actual colour scheme when it matches the wanted mode.
        (Desktop::Kde, Some(k)) if want_dark.is_none_or(|d| d == k.is_dark()) => {
            (k.theme(), Source::Kde(k.name.clone()), true)
        }
        (Desktop::Kde, _) if let Some(k) = want_dark.and_then(KdeScheme::breeze) => {
            (k.theme(), Source::Kde(k.name.clone()), true)
        }
        _ if follows_system => (
            system.clone(),
            match desktop {
                Desktop::Cosmic => Source::Cosmic,
                Desktop::Gnome | Desktop::Kde | Desktop::Other => Source::Portal,
            },
            false,
        ),
        _ => {
            let dark = want_dark.unwrap_or(true);
            let t = if desktop == Desktop::Cosmic {
                if dark { cosmic::theme::system_dark() } else { cosmic::theme::system_light() }
                    .cosmic()
                    .clone()
            } else if dark {
                cosmic_theme::Theme::dark_default()
            } else {
                cosmic_theme::Theme::light_default()
            };
            let src = if desktop == Desktop::Cosmic { Source::Cosmic } else { Source::Builtin };
            (t, src, true)
        }
    };

    let mut custom = custom;
    if let Some(f) = prefs.frosted
        && theme.frosted_windows != f
    {
        theme.frosted_windows = f;
        custom = true;
    }

    let theme = Arc::new(theme);
    // A "system" theme keeps tracking COSMIC theme / portal changes on its own.
    let t = if custom { cosmic::Theme::custom(theme) } else { cosmic::Theme::system(theme) };
    (t, source)
}
