use crate::{
    fl,
    pages::{self, Ctx, Output},
    telemetry::CpuSampler,
    worker::{DeviceSummary, Job, Worker},
};
use cosmic::{
    Application, ApplicationExt, Core, Element,
    app::Task,
    iced::{Length, Subscription},
    widget::{self, nav_bar},
};
use futures::SinkExt;
use rt85_device::Capabilities;
use std::{sync::{Arc, Mutex}, time::Duration};

pub const APP_ID: &str = "io.github.Ccalibur.Rt85Manager";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Lighting,
    Keys,
    Screen,
    Device,
}

#[derive(Debug, Clone)]
pub enum Message {
    Lighting(pages::lighting::Message),
    Keys(pages::keys::Message),
    Screen(pages::screen::Message),
    Device(pages::device::Message),
    Scanned(Result<(Vec<DeviceSummary>, Option<usize>), String>),
    Progress(usize, usize),
    JobDone(Result<String, String>),
    HotplugTick,
    TakeScreenshot,
    Screenshot(u32, u32, Arc<Vec<u8>>),
    TelemetryTick,
    DismissStatus,
}

pub struct App {
    core: Core,
    nav: nav_bar::Model,
    worker: Arc<Mutex<Worker>>,
    devices: Vec<DeviceSummary>,
    active: Option<usize>,
    scan_error: Option<String>,
    scanning: bool,
    busy: Option<String>,
    progress: Option<(usize, usize)>,
    status: Option<Result<String, String>>,
    lighting: pages::lighting::Page,
    keys: pages::keys::Page,
    screen: pages::screen::Page,
    cpu: CpuSampler,
    demo: bool,
}

impl App {
    fn caps(&self) -> Capabilities {
        self.active.and_then(|i| self.devices.get(i)).map(|d| d.caps).unwrap_or_default()
    }

    fn scan(&mut self) -> Task<Message> {
        if self.demo {
            return Task::none();
        }
        if self.scanning || self.busy.is_some() {
            return Task::none();
        }
        self.scanning = true;
        let worker = self.worker.clone();
        cosmic::task::future(async move {
            let r = tokio::task::spawn_blocking(move || worker.lock().expect("worker lock").scan())
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r.map_err(|e| e.to_string()));
            Message::Scanned(r)
        })
    }

    /// Run a job on the worker; progress and the result come back as messages.
    fn run(&mut self, job: Job, label: Option<String>) -> Task<Message> {
        if self.demo {
            return cosmic::task::message(Message::JobDone(Ok(format!("Demo mode: {} (nothing sent)", job.describe()))));
        }
        let quiet = label.is_none();
        if !quiet {
            self.busy = label;
            self.progress = None;
        }
        let worker = self.worker.clone();
        cosmic::task::stream(cosmic::iced::stream::channel(64, async move |mut tx: futures::channel::mpsc::Sender<Message>| {
            let mut progress_tx = tx.clone();
            let r = tokio::task::spawn_blocking(move || {
                let mut w = worker.lock().expect("worker lock");
                w.run(job, &mut |d, t| {
                    let _ = progress_tx.try_send(Message::Progress(d, t));
                })
            })
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r.map_err(|e| e.to_string()));
            if !quiet || r.is_err() {
                let _ = tx.send(Message::JobDone(r)).await;
            }
        }))
    }

    fn handle<M: Send + 'static>(&mut self, out: Output<M>, wrap: fn(M) -> Message) -> Task<Message> {
        match out {
            Output::None => Task::none(),
            Output::Job(job, label) => self.run(job, Some(label)),
            Output::Task(t) => t.map(move |m| cosmic::Action::App(wrap(m))),
        }
    }

    fn ctx_parts(&self) -> (Capabilities, bool, bool) {
        (self.caps(), self.active.is_some(), self.busy.is_some())
    }

    fn update_title(&mut self) -> Task<Message> {
        let title = self.nav.text(self.nav.active()).unwrap_or_default().to_owned();
        self.set_header_title(title.clone());
        match self.core.main_window_id() {
            Some(id) => self.set_window_title(format!("{title} — {}", fl!("app-title")), id),
            None => Task::none(),
        }
    }
}

impl Application for App {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: ()) -> (Self, Task<Message>) {
        let mut nav = nav_bar::Model::default();
        let pages = [
            (Page::Lighting, fl!("page-lighting"), "keyboard-brightness-symbolic"),
            (Page::Keys, fl!("page-keys"), "input-keyboard-symbolic"),
            (Page::Screen, fl!("page-screen"), "video-display-symbolic"),
            (Page::Device, fl!("page-device"), "preferences-system-symbolic"),
        ];
        for (page, text, icon) in pages {
            nav.insert().text(text).icon(widget::icon::from_name(icon)).data(page);
        }
        nav.activate_position(std::env::var("RT85_PAGE").ok().and_then(|v| v.parse().ok()).unwrap_or(0));

        let mut app = App {
            core,
            nav,
            worker: Arc::new(Mutex::new(Worker::new())),
            devices: Vec::new(),
            active: None,
            scan_error: None,
            scanning: false,
            busy: None,
            progress: None,
            status: None,
            lighting: pages::lighting::Page::new(),
            keys: pages::keys::Page::new(),
            screen: pages::screen::Page::new(),
            cpu: CpuSampler::default(),
            demo: crate::demo::enabled(),
        };
        let mut tasks = vec![app.update_title(), app.scan()];
        if app.demo {
            app.devices = vec![crate::demo::device()];
            app.active = Some(0);
            app.keys.seed_demo();
            app.lighting.seed_demo();
            let out = app.screen.on_caps_changed(app.caps().screen);
            tasks.push(app.handle(out, Message::Screen));
            if let Some(p) = crate::demo::image_path() {
                tasks.push(cosmic::task::message(Message::Screen(pages::screen::Message::Picked(p))));
            }
        }
        if crate::demo::screenshot_path().is_some() {
            tasks.push(cosmic::task::future(async {
                tokio::time::sleep(Duration::from_millis(2500)).await;
                Message::TakeScreenshot
            }));
        }
        (app, Task::batch(tasks))
    }

    fn nav_model(&self) -> Option<&nav_bar::Model> {
        Some(&self.nav)
    }

    fn on_nav_select(&mut self, id: nav_bar::Id) -> Task<Message> {
        self.nav.activate(id);
        self.update_title()
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![cosmic::iced::time::every(Duration::from_secs(3)).map(|_| Message::HotplugTick)];
        if self.screen.cpu_feed || self.screen.temp_feed {
            subs.push(cosmic::iced::time::every(Duration::from_secs(2)).map(|_| Message::TelemetryTick));
        }
        Subscription::batch(subs)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let (caps, connected, busy) = self.ctx_parts();
        let ctx = Ctx { caps: &caps, connected, busy };
        match message {
            Message::Lighting(m) => {
                let out = self.lighting.update(m);
                return self.handle(out, Message::Lighting);
            }
            Message::Keys(m) => {
                let out = self.keys.update(m);
                return self.handle(out, Message::Keys);
            }
            Message::Screen(m) => {
                let out = self.screen.update(m, &ctx);
                return self.handle(out, Message::Screen);
            }
            Message::Device(pages::device::Message::Select(i)) => {
                self.worker.lock().expect("worker lock").select(i);
                self.active = Some(i);
                let out = self.screen.on_caps_changed(self.caps().screen);
                return self.handle(out, Message::Screen);
            }
            Message::Device(pages::device::Message::Rescan) | Message::HotplugTick => return self.scan(),
            Message::Scanned(r) => {
                self.scanning = false;
                match r {
                    Ok((devices, active)) => {
                        self.devices = devices;
                        self.active = active;
                        self.scan_error = None;
                    }
                    Err(e) => self.scan_error = Some(e),
                }
                let out = self.screen.on_caps_changed(self.caps().screen);
                return self.handle(out, Message::Screen);
            }
            Message::Progress(d, t) => self.progress = Some((d, t)),
            Message::JobDone(r) => {
                self.busy = None;
                self.progress = None;
                self.status = Some(r);
            }
            Message::TelemetryTick => {
                if self.busy.is_some() || !connected {
                    return Task::none();
                }
                let mut tasks = Vec::new();
                if self.screen.cpu_feed && let Some(v) = self.cpu.sample() {
                    tasks.push(self.run(Job::Cpu(v), None));
                }
                if self.screen.temp_feed && let Some(v) = crate::telemetry::temperature() {
                    tasks.push(self.run(Job::Temperature(v), None));
                }
                return Task::batch(tasks);
            }
            Message::DismissStatus => self.status = None,
            Message::TakeScreenshot => {
                if let Some(id) = self.core.main_window_id() {
                    return cosmic::iced::window::screenshot(id).map(|s| {
                        cosmic::Action::App(Message::Screenshot(s.size.width, s.size.height, Arc::new(s.rgba.to_vec())))
                    });
                }
            }
            Message::Screenshot(w, h, rgba) => {
                if let Some(path) = crate::demo::screenshot_path()
                    && let Err(e) = image::save_buffer(&path, &rgba, w, h, image::ExtendedColorType::Rgba8)
                {
                    tracing::error!("saving screenshot: {e}");
                }
                return cosmic::iced::exit();
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let space = cosmic::theme::spacing();
        let (caps, connected, busy) = self.ctx_parts();
        let ctx = Ctx { caps: &caps, connected, busy };
        let page = self.nav.active_data::<Page>().copied().unwrap_or(Page::Lighting);

        let body: Element<Message> = match page {
            Page::Lighting => self.lighting.view(&ctx).map(Message::Lighting),
            Page::Keys => self.keys.view(&ctx).map(Message::Keys),
            Page::Screen => self.screen.view(&ctx).map(Message::Screen),
            Page::Device => pages::device::view(&self.devices, self.active, &ctx, self.scan_error.as_deref()).map(Message::Device),
        };

        let mut col = widget::column::with_capacity(4).spacing(space.space_s);
        if !connected && page != Page::Device {
            col = col.push(widget::warning(fl!("no-device")));
        }
        if let Some(label) = &self.busy {
            let mut row = widget::row::with_capacity(2).spacing(space.space_s).push(widget::text::body(label.clone()));
            if let Some((d, t)) = self.progress {
                row = row.push(
                    widget::progress_bar::linear::Linear::new()
                        .progress(d as f32 / t.max(1) as f32)
                        .girth(8.0)
                        .width(Length::Fill),
                );
            }
            col = col.push(row);
        } else if let Some(status) = &self.status {
            let text = match status {
                Ok(s) => s.clone(),
                Err(e) => format!("{}: {e}", fl!("error")),
            };
            col = col.push(
                widget::row::with_children(vec![
                    widget::text::body(text).width(Length::Fill).into(),
                    widget::button::text(fl!("dismiss")).on_press(Message::DismissStatus).into(),
                ])
                .align_y(cosmic::iced::Alignment::Center),
            );
        }
        col = col.push(body);

        widget::scrollable(widget::container(col).padding(space.space_m).width(Length::Fill)).into()
    }
}
