//! Built-in screen: image/GIF upload, clock, CPU/temperature widgets.

use super::{Ctx, Output};
use crate::{
    fl,
    live::{self, Mode, Snapshot, card::Card, fps},
    worker::Job,
};
use cosmic::{Element, iced::Length, widget};
use rt85_device::media::{self, Fit};
use rt85_protocol::{rdmctmzt::Qgif, royuan::screen::{Animation, ScreenSpec}};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub enum Message {
    Pick,
    Picked(PathBuf),
    PickCancelled,
    Prepared(Result<Arc<Prepared>, String>),
    Fit(usize),
    Upload,
    Clear,
    SyncTime,
    CpuFeed(bool),
    TempFeed(bool),
    LiveMode(usize),
    LiveRefresh(usize),
    FpsGauge(bool),
    LiveTick,
    LivePolled(Box<Snapshot>),
    LiveRendered(Arc<Card>, Option<TrackKey>, bool),
    SetupMangoHud,
}

#[derive(Debug)]
pub enum Payload {
    Rgb565(Arc<Animation>),
    Qgif(Arc<Qgif>),
}

#[derive(Debug)]
pub struct Prepared {
    pub payload: Payload,
    pub preview: Option<widget::image::Handle>,
    pub summary: String,
}

pub struct Page {
    file: Option<PathBuf>,
    fit: Fit,
    fit_names: Vec<String>,
    prepared: Option<Arc<Prepared>>,
    error: Option<String>,
    preparing: bool,
    pub cpu_feed: bool,
    pub temp_feed: bool,
    /// Screen size the current preview was made for.
    spec: Option<ScreenSpec>,
    live: Live,
}

/// Title, artist, playing.
type TrackKey = (String, String, bool);

/// Live screen state.
struct Live {
    mode: Mode,
    mode_names: Vec<String>,
    refresh: usize,
    refresh_names: Vec<String>,
    fps_gauge: bool,
    polling: bool,
    rendering: bool,
    snap: Snapshot,
    preview: Option<widget::image::Handle>,
    /// Hash and track of the last image sent, and when.
    sent: Option<(u64, Option<TrackKey>, bool, Instant)>,
    setup_error: Option<String>,
}

/// Seconds between progress refreshes; `None` = only when the track changes.
const REFRESH: [Option<u64>; 4] = [None, Some(15), Some(30), Some(60)];
/// Never upload more often than this, whatever changes.
const MIN_UPLOAD_GAP: Duration = Duration::from_secs(5);

const FITS: [Fit; 3] = [Fit::Cover, Fit::Contain, Fit::Stretch];

impl Page {
    pub fn new() -> Self {
        Self {
            file: None,
            fit: Fit::Cover,
            fit_names: vec![fl!("fit-cover"), fl!("fit-contain"), fl!("fit-stretch")],
            prepared: None,
            error: None,
            preparing: false,
            cpu_feed: false,
            temp_feed: false,
            spec: None,
            live: Live {
                mode: crate::demo::live_mode(),
                mode_names: vec![fl!("live-off"), fl!("live-media"), fl!("live-fps"), fl!("live-both")],
                refresh: 2,
                refresh_names: vec![
                    fl!("refresh-track"),
                    fl!("refresh-every", secs = 15),
                    fl!("refresh-every", secs = 30),
                    fl!("refresh-every", secs = 60),
                ],
                fps_gauge: false,
                polling: false,
                rendering: false,
                snap: Snapshot::default(),
                preview: None,
                sent: None,
                setup_error: None,
            },
        }
    }

    /// Whether the app should send `LiveTick`s.
    pub fn live_active(&self) -> bool {
        self.live.mode != Mode::Off || self.live.fps_gauge
    }

    /// FPS for the CPU gauge, when that's switched on and a game is running.
    pub fn gauge_fps(&self) -> Option<u8> {
        self.live.fps_gauge.then_some(())?;
        self.live.snap.fps.as_ref().map(|f| f.fps.min(100) as u8)
    }

    fn live_render(&mut self) -> Output<Message> {
        if self.live.rendering || self.live.mode == Mode::Off {
            return Output::None;
        }
        self.live.rendering = true;
        let spec = self.spec.unwrap_or(rt85_protocol::royuan::screen::RT100_SCREEN);
        let mode = self.live.mode;
        let media = self.live.snap.media.clone().filter(|_| mode.media());
        let fps = self.live.snap.fps.clone().filter(|_| mode.fps());
        let key = media.as_ref().map(|m| m.key());
        let has_fps = fps.is_some();
        let a = cosmic::theme::active().cosmic().accent_color();
        let accent = [a.red, a.green, a.blue].map(|c| (c * 255.0).round() as u8);
        Output::Task(cosmic::task::future(async move {
            let card = tokio::task::spawn_blocking(move || {
                live::card::render(spec.width.into(), spec.height.into(), media.as_ref(), fps.as_ref(), accent)
            })
            .await
            .expect("render task");
            Message::LiveRendered(Arc::new(card), key, has_fps)
        }))
    }

    /// Decide whether the freshly rendered image goes to the keyboard.
    fn live_upload(&mut self, card: &Card, key: Option<TrackKey>, has_fps: bool, ctx: &Ctx) -> Output<Message> {
        let Some(spec) = self.spec else { return Output::None };
        if !ctx.connected || ctx.busy || !ctx.caps.screen_rgb565 {
            return Output::None;
        }
        let now = Instant::now();
        let due = match &self.live.sent {
            None => true,
            Some((hash, k, f, at)) => {
                let since = now.duration_since(*at);
                let changed_track = *k != key || *f != has_fps;
                let refresh = REFRESH[self.live.refresh].is_some_and(|s| since >= Duration::from_secs(s));
                *hash != card.hash && since >= MIN_UPLOAD_GAP && (changed_track || refresh)
            }
        };
        if !due {
            return Output::None;
        }
        self.live.sent = Some((card.hash, key, has_fps, now));
        let frame = rt85_protocol::royuan::screen::encode_rgba(card.rgba.as_raw(), spec.width, spec.height);
        let anim = Animation { width: spec.width, height: spec.height, delay_ms: 0, frames: vec![frame] };
        Output::QuietJob(Job::Upload(Arc::new(anim)))
    }

    /// Re-prepare when the connected keyboard (and so the screen size) changes.
    pub fn on_caps_changed(&mut self, spec: Option<ScreenSpec>) -> Output<Message> {
        if spec == self.spec {
            return Output::None;
        }
        self.spec = spec;
        self.prepare()
    }

    fn prepare(&mut self) -> Output<Message> {
        let (Some(path), Some(spec)) = (self.file.clone(), self.spec) else {
            return Output::None;
        };
        self.preparing = true;
        self.error = None;
        let fit = self.fit;
        Output::Task(cosmic::task::future(async move {
            let r = tokio::task::spawn_blocking(move || prepare(&path, spec, fit))
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r);
            Message::Prepared(r.map(Arc::new))
        }))
    }

    pub fn update(&mut self, msg: Message, ctx: &Ctx) -> Output<Message> {
        match msg {
            Message::Pick => {
                return Output::Task(cosmic::task::future(async {
                    use cosmic::dialog::file_chooser;
                    let filter = file_chooser::FileFilter::new("Images")
                        .glob("*.png").glob("*.jpg").glob("*.jpeg").glob("*.gif")
                        .glob("*.bmp").glob("*.webp").glob("*.qgif");
                    let dialog = file_chooser::open::Dialog::new().title(fl!("choose-image")).filter(filter);
                    match dialog.open_file().await {
                        Ok(resp) => match resp.url().to_file_path() {
                            Ok(p) => Message::Picked(p),
                            Err(()) => Message::PickCancelled,
                        },
                        Err(_) => Message::PickCancelled,
                    }
                }));
            }
            Message::PickCancelled => {}
            Message::Picked(p) => {
                self.file = Some(p);
                self.prepared = None;
                return self.prepare();
            }
            Message::Prepared(r) => {
                self.preparing = false;
                match r {
                    Ok(p) => self.prepared = Some(p),
                    Err(e) => {
                        self.prepared = None;
                        self.error = Some(e);
                    }
                }
            }
            Message::Fit(i) => {
                self.fit = FITS[i];
                return self.prepare();
            }
            Message::Upload => {
                if let Some(p) = &self.prepared {
                    let job = match &p.payload {
                        Payload::Rgb565(a) if ctx.caps.screen_rgb565 => Job::Upload(a.clone()),
                        Payload::Qgif(q) if ctx.caps.screen_qgif => Job::Qgif(q.clone()),
                        _ => {
                            self.error = Some(fl!("wrong-format"));
                            return Output::None;
                        }
                    };
                    return Output::Job(job, fl!("uploading"));
                }
            }
            Message::Clear => return Output::Job(Job::ClearScreen, fl!("clearing")),
            Message::SyncTime => return Output::Job(Job::SyncTime, fl!("syncing-time")),
            Message::CpuFeed(v) => self.cpu_feed = v,
            Message::TempFeed(v) => self.temp_feed = v,
            Message::LiveMode(i) => {
                self.live.mode = Mode::ALL[i];
                self.live.sent = None;
                if self.live.mode == Mode::Off {
                    self.live.preview = None;
                }
                return self.update(Message::LiveTick, ctx);
            }
            Message::LiveRefresh(i) => self.live.refresh = i,
            Message::FpsGauge(v) => {
                self.live.fps_gauge = v;
                return self.update(Message::LiveTick, ctx);
            }
            Message::LiveTick => {
                if self.live.polling || !self.live_active() {
                    return Output::None;
                }
                self.live.polling = true;
                let (m, f) = (self.live.mode.media(), self.live.mode.fps() || self.live.fps_gauge);
                return Output::Task(cosmic::task::future(async move {
                    Message::LivePolled(Box::new(live::poll(m, f).await))
                }));
            }
            Message::LivePolled(snap) => {
                self.live.polling = false;
                if *snap != self.live.snap || self.live.preview.is_none() {
                    self.live.snap = *snap;
                    return self.live_render();
                }
            }
            Message::LiveRendered(card, key, has_fps) => {
                self.live.rendering = false;
                let (w, h) = card.rgba.dimensions();
                self.live.preview = Some(widget::image::Handle::from_rgba(w, h, card.rgba.as_raw().clone()));
                return self.live_upload(&card, key, has_fps, ctx);
            }
            Message::SetupMangoHud => {
                self.live.setup_error = fps::enable_autostart().err().map(|e| e.to_string());
                self.live.snap.fps_status = Some(fps::status());
            }
        }
        Output::None
    }

    pub fn view(&self, ctx: &Ctx) -> Element<'_, Message> {
        let space = cosmic::theme::spacing();
        let idle = ctx.connected && !ctx.busy;
        let caps = ctx.caps;

        let mut image = widget::settings::section().title(fl!("image"));
        if let Some(s) = caps.screen {
            image = image.add(widget::settings::item(fl!("screen-size"), widget::text::body(format!("{} × {}", s.width, s.height))));
        }
        let file_label = self
            .file
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| fl!("no-file"));
        image = image
            .add(widget::settings::item(
                file_label,
                widget::button::standard(fl!("choose")).on_press(Message::Pick),
            ))
            .add(widget::settings::item(
                fl!("fit"),
                widget::dropdown(&self.fit_names, FITS.iter().position(|f| *f == self.fit), Message::Fit),
            ));

        let mut preview_col = widget::column::with_capacity(4).spacing(space.space_xs);
        if self.preparing {
            preview_col = preview_col.push(widget::text::body(fl!("preparing")));
        }
        if let Some(e) = &self.error {
            preview_col = preview_col.push(widget::warning(e.clone()));
        }
        if let Some(p) = &self.prepared {
            if let Some(h) = &p.preview {
                preview_col = preview_col.push(widget::image(h.clone()).width(Length::Fixed(240.0)));
            }
            preview_col = preview_col.push(widget::text::caption(p.summary.clone()));
        }
        let can_upload = idle && self.prepared.is_some() && (caps.screen_rgb565 || caps.screen_qgif);
        let actions = widget::row::with_children(vec![
            widget::button::suggested(fl!("upload")).on_press_maybe(can_upload.then_some(Message::Upload)).into(),
            widget::button::destructive(fl!("clear-screen"))
                .on_press_maybe((idle && caps.screen_clear).then_some(Message::Clear))
                .into(),
        ])
        .spacing(space.space_xs);

        let mut widgets = widget::settings::section().title(fl!("widgets")).add(widget::settings::item(
            fl!("clock"),
            widget::button::standard(fl!("sync-now")).on_press_maybe((idle && caps.time_sync).then_some(Message::SyncTime)),
        ));
        if caps.telemetry {
            widgets = widgets
                .add(widget::settings::item(fl!("cpu-feed"), widget::toggler(self.cpu_feed).on_toggle(Message::CpuFeed)))
                .add(widget::settings::item(fl!("temp-feed"), widget::toggler(self.temp_feed).on_toggle(Message::TempFeed)));
        }

        let mut col = vec![image.into(), preview_col.into(), actions.into(), widgets.into(), self.live_view(ctx)];
        if ctx.connected && !caps.screen_rgb565 && !caps.screen_qgif {
            col.insert(0, super::unsupported(fl!("unsupported-screen")));
        }
        widget::settings::view_column(col).into()
    }
}

impl Page {
    fn live_view(&self, ctx: &Ctx) -> Element<'_, Message> {
        let space = cosmic::theme::spacing();
        let l = &self.live;
        let mut section = widget::settings::section()
            .title(fl!("live-screen"))
            .add(
                widget::settings::item::builder(fl!("live-show"))
                    .description(fl!("live-show-help"))
                    .control(widget::dropdown(&l.mode_names, Mode::ALL.iter().position(|m| *m == l.mode), Message::LiveMode)),
            )
            .add(
                widget::settings::item::builder(fl!("live-refresh"))
                    .description(fl!("live-refresh-help"))
                    .control(widget::dropdown(&l.refresh_names, Some(l.refresh), Message::LiveRefresh)),
            );

        if l.mode.media() {
            let status = match (&l.snap.media, &l.snap.media_error) {
                (Some(m), _) => {
                    let state = if m.playing { fl!("playing") } else { fl!("paused") };
                    if m.artist.is_empty() { format!("{} ({state})", m.title) } else { format!("{} — {} ({state})", m.title, m.artist) }
                }
                (None, Some(e)) => format!("{}: {e}", fl!("error")),
                (None, None) => fl!("no-media"),
            };
            section = section.add(widget::settings::item(fl!("now-playing"), widget::text::body(status)));
        }

        if l.mode.fps() || l.fps_gauge {
            let control: Element<Message> = match (&l.snap.fps, l.snap.fps_status) {
                (Some(f), _) => widget::text::body(format!("{} — {} FPS", f.game, f.fps)).into(),
                (None, Some(fps::Status::NotInstalled)) => widget::text::body(fl!("mangohud-missing")).into(),
                (None, Some(fps::Status::NeedsAutostart)) => {
                    widget::button::standard(fl!("mangohud-setup")).on_press(Message::SetupMangoHud).into()
                }
                (None, _) => widget::text::body(fl!("no-game")).into(),
            };
            section = section.add(
                widget::settings::item::builder(fl!("game-fps")).description(fl!("game-fps-help")).control(control),
            );
        }
        if ctx.caps.telemetry {
            section = section.add(
                widget::settings::item::builder(fl!("fps-gauge"))
                    .description(fl!("fps-gauge-help"))
                    .toggler(l.fps_gauge, Message::FpsGauge),
            );
        }

        let mut col = widget::column::with_capacity(4).spacing(space.space_xs).push(section);
        if ctx.connected && !ctx.caps.screen_rgb565 && l.mode != Mode::Off {
            col = col.push(widget::warning(fl!("live-unsupported")));
        }
        if let Some(e) = &l.setup_error {
            col = col.push(widget::warning(e.clone()));
        }
        if let Some(h) = &l.preview {
            col = col
                .push(widget::text::caption(fl!("live-preview")))
                .push(widget::image(h.clone()).width(Length::Fixed(240.0)));
        }
        col.into()
    }
}

fn prepare(path: &std::path::Path, spec: ScreenSpec, fit: Fit) -> Result<Prepared, String> {
    let is_qgif = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("qgif"));
    if is_qgif {
        let q = Qgif::new(std::fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let summary = format!("QGIF, {} KiB", q.bytes().len() / 1024);
        return Ok(Prepared { payload: Payload::Qgif(Arc::new(q)), preview: None, summary });
    }
    let anim = media::load(path, spec, fit).map_err(|e| e.to_string())?;
    // Preview: the first frame, decoded again at the target size.
    let img = image::open(path).map_err(|e| e.to_string())?;
    let rgba = media::fit(&img, anim.width.into(), anim.height.into(), fit);
    let preview = Some(widget::image::Handle::from_rgba(rgba.width(), rgba.height(), rgba.into_raw()));
    let summary = format!("{} frame(s), {} × {}", anim.frames.len(), anim.width, anim.height);
    Ok(Prepared { payload: Payload::Rgb565(Arc::new(anim)), preview, summary })
}
