//! `rt85` — command-line tool for the RT85 (and related Epomaker boards).
//! Mainly for probing the protocol; the GUI uses the same library.

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand, ValueEnum};
use rt85_device::{FoundDevice, HidApi, Keyboard, media};
use rt85_protocol::{
    REPORT_LEN, keycode,
    layout::{Board, Layout, Physical},
    rdmctmzt::Qgif,
    royuan::{
        keys::RgbFrame,
        lighting::{self, MAX_BRIGHTNESS, Profile},
    },
};
use std::{io::Write, path::PathBuf};

#[derive(Parser)]
#[command(name = "rt85", version, about = "Configure Epomaker keyboards (RT85 target)")]
struct Cli {
    /// Pick a device by VID:PID in hex (e.g. 3151:4010) when several are connected.
    #[arg(long, global = true, value_parser = parse_vid_pid)]
    device: Option<(u16, u16)>,
    /// Use this HID interface number instead of the auto-detected one.
    #[arg(long, global = true)]
    interface: Option<i32>,
    /// Physical layout: ansi (US) or iso (UK). Defaults to the one chosen in
    /// RT85 Manager, else ansi.
    #[arg(long, global = true)]
    layout: Option<Physical>,
    /// Board: rt85 (75%, 320x172 screen) or rt100. Picks the key layout and
    /// screen size. Defaults to the connected keyboard's model, else rt85.
    #[arg(long, global = true)]
    board: Option<Board>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List supported keyboards and their HID interfaces.
    List {
        /// Show every HID device, not only known vendors (to find an unknown RT85).
        #[arg(long)]
        all: bool,
    },
    /// Sync the keyboard clock to the local time.
    Time,
    /// Set the built-in lighting effect.
    Lighting {
        /// Effect name, see `rt85 modes` (the RT85 and RT100 have different ones).
        #[arg(long)]
        mode: Option<String>,
        #[arg(long, default_value_t = 4)]
        speed: u8,
        #[arg(long, default_value_t = 4)]
        brightness: u8,
        /// Colour as RRGGBB.
        #[arg(long, value_parser = parse_rgb, default_value = "ffffff")]
        color: (u8, u8, u8),
        /// Rainbow colours instead of --color.
        #[arg(long)]
        dazzle: bool,
        /// Effect option, counting from 0: direction, picture number, ... (see `rt85 modes`).
        #[arg(long, alias = "direction", default_value_t = 0)]
        option: u8,
    },
    /// List the lighting modes of the board (--board, default rt85).
    Modes,
    /// Set every key to one colour (per-key RGB).
    Color {
        #[arg(value_parser = parse_rgb)]
        color: (u8, u8, u8),
    },
    /// Light a single key index (to work out the RT85 key table).
    ProbeKey {
        index: u8,
        #[arg(value_parser = parse_rgb, default_value = "ff0000")]
        color: (u8, u8, u8),
    },
    /// Remap a key: `rt85 remap CAPS "Left Ctrl"` or by numbers `rt85 remap 3 0xe0`.
    Remap { key: String, to: String },
    /// List keys in the bundled layout with their indices.
    Keys,
    /// Upload an image or GIF to the screen.
    Upload {
        file: PathBuf,
        #[arg(long, value_enum, default_value_t = FitArg::Cover)]
        fit: FitArg,
        /// Override the screen size as WxH (e.g. 240x135).
        #[arg(long, value_parser = parse_size)]
        size: Option<(u16, u16)>,
    },
    /// Upload a pre-encoded QGIF file (RDMCTMZT boards).
    Qgif { file: PathBuf },
    /// Clear the uploaded image (RT100 only; refused on the RT85, where the command erases flash).
    Clear,
    /// Switch the screen's language (RT85; each use flips it).
    Language,
    /// Show a number on the CPU widget.
    Cpu { percent: u8 },
    /// Show a number on the temperature widget.
    Temp { celsius: u8 },
    /// Send a raw report (hex). Feature report on Royuan boards, output report on RDMCTMZT.
    Raw {
        hex: String,
        /// Read a feature report back afterwards (Royuan).
        #[arg(long)]
        read: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum FitArg {
    Cover,
    Contain,
    Stretch,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    let physical = cli.layout.or_else(gui_layout).unwrap_or(Physical::Ansi);
    let mut api = HidApi::new().context("initialising hidapi")?;

    match &cli.cmd {
        Cmd::List { all } => return list(&mut api, *all),
        Cmd::Modes => {
            let family = cli.board.unwrap_or_default().family();
            for e in lighting::effects(family) {
                let options = if e.options.is_empty() { String::new() } else { format!("  options: {}", e.options.join(", ")) };
                println!("{:<20} {}{options}", e.slug(), e.label);
            }
            return Ok(());
        }
        Cmd::Keys => {
            let l = Layout::for_board(cli.board.unwrap_or_default(), physical);
            println!("# {} (slot numbers; RT85 ones are from Epomaker's default key map)", l.name);
            for k in &l.keys {
                println!("{:>3}  {:<16} {}", k.index, k.name, k.label);
            }
            return Ok(());
        }
        _ => {}
    }

    let found = select(&mut api, cli.device)?;
    let guess = cli.board.unwrap_or_else(|| Board::for_model(found.known.map(|k| k.model)));
    let mut kb = match cli.interface {
        Some(n) => {
            let iface = found
                .interfaces
                .iter()
                .find(|i| i.number == n)
                .ok_or_else(|| anyhow!("interface {n} not found"))?;
            Keyboard::open_path(&api, &found, &iface.path, guess)?
        }
        None => Keyboard::open(&api, &found, guess)?,
    };
    // Several boards share 3151:5002; ask the keyboard which one it is.
    let identity = if found.link() == rt85_protocol::ids::Link::Wired { kb.identify().ok().flatten() } else { None };
    let board = cli.board.or_else(|| identity.and_then(|i| Board::for_device_id(i.device_id))).unwrap_or(guess);
    kb.set_board(board);
    let who = match identity {
        Some(i) => format!(", model id {} firmware {}", i.device_id, i.firmware_string()),
        None => String::new(),
    };
    eprintln!("Using {} ({:04x}:{:04x}, {:?}{who}) as {board:?}", found.display_name(), found.vid, found.pid, found.platform);
    let caps = kb.capabilities(board);

    match cli.cmd {
        Cmd::Time => {
            let now = rt85_device::local_now();
            kb.set_time(&mut api, now)?;
            println!("Clock set to {:04}-{:02}-{:02} {:02}:{:02}:{:02}", now.year, now.month, now.day, now.hour, now.minute, now.second);
        }
        Cmd::Lighting { mode, speed, brightness, color, dazzle, option } => {
            let family = board.family();
            let max_speed = lighting::max_speed(family);
            if speed > max_speed || brightness > MAX_BRIGHTNESS {
                bail!("speed is 0-{max_speed}, brightness is 0-{MAX_BRIGHTNESS}");
            }
            let effect = match mode {
                Some(m) => lighting::effects(family)
                    .iter()
                    .find(|e| e.slug() == m.to_lowercase())
                    .ok_or_else(|| anyhow!("unknown mode {m:?} for the {board:?}; see `rt85 modes --board ...`"))?,
                None => &lighting::effects(family)[0],
            };
            let options = effect.options.len();
            if option > 0 && option as usize >= options {
                bail!("{} takes options 0-{}", effect.label, options.saturating_sub(1));
            }
            kb.set_profile(&Profile { effect: effect.id, speed, brightness, dazzle, option, rgb: color })?;
            println!("Lighting set to {}", effect.label);
        }
        Cmd::Color { color } => {
            let n = Layout::for_board(board, physical).keys.iter().map(|k| k.index as usize + 1).max().unwrap_or(0);
            kb.set_key_colors(&[RgbFrame::solid(color, n)])?;
        }
        Cmd::ProbeKey { index, color } => {
            let mut colors = vec![(0, 0, 0); index as usize + 1];
            colors[index as usize] = color;
            kb.set_key_colors(&[RgbFrame { colors, time: 0 }])?;
        }
        Cmd::Remap { key, to } => {
            let layout = Layout::for_board(board, physical);
            let index = match parse_num(&key) {
                Some(n) => n,
                None => layout.key(&key.to_uppercase()).map(|k| k.index).ok_or_else(|| anyhow!("unknown key {key:?}; see `rt85 keys`"))?,
            };
            let code = parse_num(&to).or_else(|| keycode::from_name(&to)).ok_or_else(|| anyhow!("unknown HID key {to:?}"))?;
            kb.remap(index, code)?;
            println!("Key {index} → {} (0x{code:02x})", keycode::name(code).unwrap_or("?"));
        }
        Cmd::Upload { file, fit, size } => {
            let spec = caps.screen.ok_or_else(|| anyhow!("this keyboard has no RGB565 screen"))?;
            let spec = size.map_or(spec, |(w, h)| spec.with_size(w, h));
            let fit = match fit {
                FitArg::Cover => media::Fit::Cover,
                FitArg::Contain => media::Fit::Contain,
                FitArg::Stretch => media::Fit::Stretch,
            };
            let anim = media::load(&file, spec, fit)?;
            eprintln!("{} frame(s) at {}x{}", anim.frames.len(), anim.width, anim.height);
            kb.upload_animation(&anim, &mut progress_bar)?;
            eprintln!();
        }
        Cmd::Qgif { file } => {
            let q = Qgif::new(std::fs::read(&file)?)?;
            kb.upload_qgif(&mut api, &q, rt85_device::local_now(), &mut progress_bar)?;
            eprintln!();
        }
        Cmd::Clear => kb.clear_screen()?,
        Cmd::Language => kb.toggle_language()?,
        Cmd::Cpu { percent } => kb.set_cpu(percent)?,
        Cmd::Temp { celsius } => kb.set_temperature(celsius)?,
        Cmd::Raw { hex, read } => {
            let bytes = parse_hex(&hex)?;
            if bytes.len() > REPORT_LEN {
                bail!("at most {REPORT_LEN} bytes");
            }
            let mut r = [0u8; REPORT_LEN];
            r[..bytes.len()].copy_from_slice(&bytes);
            match &mut kb {
                Keyboard::Royuan(k) => {
                    if r[0] == 0xAC && board.family() == rt85_protocol::royuan::Family::Gen2 {
                        bail!("refusing to send 0xAC: on the RT85 it erases flash");
                    }
                    if read {
                        let reply = k.exchange(&r)?;
                        println!("{}", reply.iter().map(|b| format!("{b:02x}")).collect::<String>());
                    } else {
                        k.send(&r)?;
                    }
                }
                Keyboard::Rdmctmzt(k) => k.send_raw_kbd(&api, &r)?,
            }
        }
        Cmd::List { .. } | Cmd::Modes | Cmd::Keys => unreachable!(),
    }
    Ok(())
}

fn list(api: &mut HidApi, all: bool) -> Result<()> {
    if all {
        api.refresh_devices()?;
        for d in api.device_list() {
            println!(
                "{:04x}:{:04x} if={} page=0x{:04x} usage=0x{:04x} {:?} {:?} {}",
                d.vendor_id(), d.product_id(), d.interface_number(), d.usage_page(), d.usage(),
                d.manufacturer_string().unwrap_or(""), d.product_string().unwrap_or(""),
                d.path().to_string_lossy()
            );
        }
        return Ok(());
    }
    let found = rt85_device::list(api)?;
    if found.is_empty() {
        println!("No supported keyboard found. Try `rt85 list --all` and check permissions (udev rule).");
    }
    for f in &found {
        let known = f.known.map(|k| k.model).unwrap_or("unknown model");
        println!("{} — {:04x}:{:04x} {:?}/{:?} ({known})", f.display_name(), f.vid, f.pid, f.platform, f.link());
        let cfg = f.config_interface().map(|i| i.path.clone());
        for i in &f.interfaces {
            let mark = if Some(&i.path) == cfg.as_ref() { "*" } else { " " };
            let usages: Vec<String> = i.usages.iter().map(|(p, u)| format!("{p:04x}:{u:04x}")).collect();
            println!("  {mark} if={} {} usages=[{}]", i.number, i.path.to_string_lossy(), usages.join(" "));
        }
    }
    Ok(())
}

fn select(api: &mut HidApi, want: Option<(u16, u16)>) -> Result<FoundDevice> {
    let found = rt85_device::list(api)?;
    let mut it = found.into_iter().filter(|f| want.is_none_or(|(v, p)| f.vid == v && f.pid == p));
    it.next().ok_or_else(|| anyhow!("no supported keyboard found (see `rt85 list --all`)"))
}

fn progress_bar(done: usize, total: usize) {
    if done == total || done.is_multiple_of(20) {
        eprint!("\r  {done}/{total} ({:.0}%)", done as f32 * 100.0 / total.max(1) as f32);
        let _ = std::io::stderr().flush();
    }
}

fn parse_rgb(s: &str) -> Result<(u8, u8, u8), String> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 {
        return Err("expected RRGGBB".into());
    }
    let v = u32::from_str_radix(s, 16).map_err(|e| e.to_string())?;
    Ok(((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

fn parse_vid_pid(s: &str) -> Result<(u16, u16), String> {
    let (v, p) = s.split_once(':').ok_or("expected VID:PID")?;
    let h = |x: &str| u16::from_str_radix(x, 16).map_err(|e| e.to_string());
    Ok((h(v)?, h(p)?))
}

fn parse_size(s: &str) -> Result<(u16, u16), String> {
    let (w, h) = s.split_once('x').ok_or("expected WxH")?;
    Ok((w.parse().map_err(|_| "bad width")?, h.parse().map_err(|_| "bad height")?))
}

fn parse_num(s: &str) -> Option<u8> {
    match s.strip_prefix("0x") {
        Some(h) => u8::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}

fn parse_hex(s: &str) -> Result<Vec<u8>> {
    let s: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if !s.len().is_multiple_of(2) {
        bail!("odd number of hex digits");
    }
    (0..s.len()).step_by(2).map(|i| Ok(u8::from_str_radix(&s[i..i + 2], 16)?)).collect()
}

/// The layout picked in RT85 Manager (stored by cosmic-config as RON, e.g. `Some("iso")`).
fn gui_layout() -> Option<Physical> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))?;
    let v = std::fs::read_to_string(base.join("cosmic/io.github.Ccalibur.Rt85Manager/v1/keyboard-layout")).ok()?;
    let v = v.trim().trim_start_matches("Some(").trim_end_matches(')').trim_matches('"');
    v.parse().ok()
}
