//! Built-in screen: image/GIF upload, clock, CPU/temperature widgets.

use super::{Ctx, Output};
use crate::{fl, worker::Job};
use cosmic::{Element, iced::Length, widget};
use rt85_device::media::{self, Fit};
use rt85_protocol::{rdmctmzt::Qgif, royuan::screen::{Animation, ScreenSpec}};
use std::{path::PathBuf, sync::Arc};

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
}

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
        }
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

        let mut col = vec![image.into(), preview_col.into(), actions.into(), widgets.into()];
        if ctx.connected && !caps.screen_rgb565 && !caps.screen_qgif {
            col.insert(0, super::unsupported(fl!("unsupported-screen")));
        }
        widget::settings::view_column(col).into()
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
