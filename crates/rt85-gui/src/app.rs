use crate::{
    appearance::{self, Desktop, KdeScheme, Prefs, Source},
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
use rt85_device::uinput::VirtualKeyboard;
use rt85_protocol::{
    keycode::MACRO_TRIGGERS,
    layout::{Board, Physical},
};
use std::{sync::{Arc, Mutex}, time::Duration};

pub const APP_ID: &str = "io.github.Ccalibur.Rt85Manager";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Lighting,
    Keys,
    Macros,
    Screen,
    Device,
    Settings,
}

pub struct Flags {
    pub desktop: Desktop,
    /// No icon theme has the window-control icons; draw our own buttons.
    pub builtin_window_controls: bool,
}

// Fallback window-control icons, used only when no icon theme provides them.
const ICON_MINIMIZE: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><path d="M4 8.5h8" stroke="#000" stroke-width="1.5" stroke-linecap="round" fill="none"/></svg>"##;
const ICON_MAXIMIZE: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><rect x="4" y="4" width="8" height="8" rx="1" stroke="#000" stroke-width="1.5" fill="none"/></svg>"##;
const ICON_RESTORE: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><rect x="3.5" y="5.5" width="7" height="7" rx="1" stroke="#000" stroke-width="1.5" fill="none"/><path d="M6 3.5h5.5a1 1 0 0 1 1 1V10" stroke="#000" stroke-width="1.5" fill="none"/></svg>"##;
const ICON_CLOSE: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><path d="M4.5 4.5l7 7M11.5 4.5l-7 7" stroke="#000" stroke-width="1.5" stroke-linecap="round" fill="none"/></svg>"##;

#[derive(Debug, Clone)]
pub enum Message {
    Lighting(pages::lighting::Message),
    Keys(pages::keys::Message),
    Screen(pages::screen::Message),
    Device(pages::device::Message),
    Settings(pages::settings::Message),
    Macros(pages::macros::Message),
    /// A macro trigger key (F13–F24) was pressed on the keyboard.
    MacroTrigger(u16),
    MacroListener(Result<String, String>),
    MacroPlayed(String, Result<(), String>),
    AppearanceTick,
    /// First-run layout question: radio choice, then confirm.
    LayoutChoice(Physical),
    LayoutConfirm,
    WindowMinimize,
    WindowMaximize,
    WindowClose,
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
    settings: pages::settings::Page,
    macros: pages::macros::Page,
    /// Virtual keyboard for playing macros, created on first use.
    player: Arc<Mutex<Option<VirtualKeyboard>>>,
    cpu: CpuSampler,
    demo: bool,
    desktop: Desktop,
    builtin_window_controls: bool,
    prefs: Prefs,
    /// Pending answer in the "which layout?" dialog.
    layout_choice: Option<Physical>,
    kde: Option<KdeScheme>,
    kde_mtime: Option<std::time::SystemTime>,
    theme_source: Source,
    /// Last system (COSMIC / portal) theme seen, before our overrides.
    system_theme: Arc<cosmic::cosmic_theme::Theme>,
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
            Output::QuietJob(job) => self.run(job, None),
            Output::Task(t) => t.map(move |m| cosmic::Action::App(wrap(m))),
        }
    }

    fn ctx_parts(&self) -> (Capabilities, bool, bool) {
        (self.caps(), self.active.is_some(), self.busy.is_some())
    }

    /// Re-resolves and applies the app theme from the system theme, the
    /// desktop's colour scheme and the user's preferences.
    fn apply_appearance(&mut self) -> Task<Message> {
        let (theme, source) = appearance::resolve(self.desktop, self.prefs, &self.system_theme, self.kde.as_ref());
        self.theme_source = source;
        cosmic::command::set_theme(theme)
    }

    fn frosted_effective(&self) -> bool {
        self.prefs.frosted.unwrap_or(self.system_theme.frosted_windows)
    }

    fn physical(&self) -> Physical {
        self.prefs.keyboard_layout.unwrap_or(Physical::Ansi)
    }

    /// Board drawn on the Keys/Macros pages follows the detected model.
    fn sync_board(&mut self) {
        let model = self.active.and_then(|i| self.devices.get(i)).and_then(|d| d.model);
        self.keys.set_board(Board::for_model(model));
    }

    /// hidraw nodes of the active keyboard, for the macro listener.
    fn macro_paths(&self) -> Vec<String> {
        self.active.and_then(|i| self.devices.get(i)).map(|d| d.hid_paths.clone()).unwrap_or_default()
    }

    fn play_macro(&mut self, m: &rt85_protocol::macros::Macro, delay: Option<Duration>) -> Task<Message> {
        let actions = m.actions(self.physical());
        let name = m.name.clone();
        let player = self.player.clone();
        cosmic::task::future(async move {
            if let Some(d) = delay {
                tokio::time::sleep(d).await;
            }
            let r = tokio::task::spawn_blocking(move || {
                let mut slot = player.lock().expect("player lock");
                if slot.is_none() {
                    *slot = Some(VirtualKeyboard::new()?);
                }
                slot.as_mut().expect("just created").play(&actions)
            })
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r.map_err(|e| e.to_string()));
            Message::MacroPlayed(name, r)
        })
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
    type Flags = Flags;
    type Message = Message;
    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(mut core: Core, flags: Flags) -> (Self, Task<Message>) {
        if flags.builtin_window_controls {
            core.window.show_minimize = false;
            core.window.show_maximize = false;
            core.window.show_close = false;
        }
        let mut nav = nav_bar::Model::default();
        // Alternatives for icon themes that lack the first name (e.g. Breeze
        // has no keyboard-brightness or video-display symbolic icons).
        let pages: [(Page, String, &[&'static str]); 6] = [
            (Page::Lighting, fl!("page-lighting"), &["keyboard-brightness-symbolic", "brightness-high-symbolic", "display-brightness-symbolic"]),
            (Page::Keys, fl!("page-keys"), &["input-keyboard-symbolic"]),
            (Page::Macros, fl!("page-macros"), &["media-playlist-repeat-symbolic", "media-repeat-all-symbolic", "view-list-symbolic"]),
            (Page::Screen, fl!("page-screen"), &["video-display-symbolic", "monitor-symbolic", "computer-symbolic"]),
            (Page::Device, fl!("page-device"), &["preferences-system-symbolic"]),
            (Page::Settings, fl!("page-settings"), &["emblem-system-symbolic", "preferences-system-symbolic"]),
        ];
        for (page, text, icons) in pages {
            let fallbacks = icons[1..].iter().map(|n| std::borrow::Cow::Borrowed(*n)).collect();
            let icon = widget::icon::from_name(icons[0]).fallback(Some(widget::icon::IconFallback::Names(fallbacks)));
            nav.insert().text(text).icon(icon).data(page);
        }
        nav.activate_position(std::env::var("RT85_PAGE").ok().and_then(|v| v.parse().ok()).unwrap_or(0));

        let system_theme = Arc::new(core.system_theme().cosmic().clone());
        let kde = (flags.desktop == Desktop::Kde).then(KdeScheme::load).flatten();
        let mut prefs = Prefs::load();
        if let Some(l) = crate::demo::layout() {
            prefs.keyboard_layout = Some(l);
        }
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
            keys: pages::keys::Page::new(prefs.keyboard_layout.unwrap_or(Physical::Ansi)),
            screen: pages::screen::Page::new(),
            settings: pages::settings::Page::new(),
            macros: pages::macros::Page::new(),
            player: Arc::new(Mutex::new(None)),
            cpu: CpuSampler::default(),
            demo: crate::demo::enabled(),
            desktop: flags.desktop,
            builtin_window_controls: flags.builtin_window_controls,
            prefs,
            layout_choice: None,
            kde,
            kde_mtime: appearance::kdeglobals_mtime(),
            theme_source: Source::Builtin,
            system_theme,
        };
        let mut tasks = vec![app.update_title(), app.scan(), app.apply_appearance()];
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

    /// Asks for the keyboard's layout until it has been answered once.
    fn dialog(&self) -> Option<Element<'_, Message>> {
        if self.prefs.keyboard_layout.is_some() {
            return None;
        }
        let space = cosmic::theme::spacing();
        let options = Physical::ALL.iter().map(|&p| {
            let label = widget::column::with_children(vec![
                widget::text::body(pages::settings::layout_name(p)).into(),
                widget::text::caption(pages::settings::layout_hint(p)).into(),
            ])
            .spacing(2);
            widget::radio(label, p, self.layout_choice, Message::LayoutChoice).width(Length::Fill).into()
        });
        let control = widget::column::with_children(options.collect::<Vec<_>>()).spacing(space.space_s);
        Some(
            widget::dialog()
                .title(fl!("layout-question"))
                .body(fl!("layout-question-body"))
                .control(control)
                .primary_action(
                    widget::button::suggested(fl!("continue"))
                        .on_press_maybe(self.layout_choice.map(|_| Message::LayoutConfirm)),
                )
                .into(),
        )
    }

    fn nav_model(&self) -> Option<&nav_bar::Model> {
        Some(&self.nav)
    }

    fn header_end(&self) -> Vec<Element<'_, Message>> {
        if !self.builtin_window_controls {
            return Vec::new();
        }
        let button = |svg: &'static [u8], msg: Message| {
            widget::button::icon(widget::icon::from_svg_bytes(svg).symbolic(true))
                .padding(8)
                .class(cosmic::theme::Button::HeaderBar)
                .icon_size(16)
                .on_press(msg)
        };
        let max = if self.core.window.is_maximized { ICON_RESTORE } else { ICON_MAXIMIZE };
        vec![
            widget::row::with_children(vec![
                button(ICON_MINIMIZE, Message::WindowMinimize).into(),
                button(max, Message::WindowMaximize).into(),
                button(ICON_CLOSE, Message::WindowClose).into(),
            ])
            .spacing(cosmic::theme::spacing().space_xxs)
            .align_y(cosmic::iced::Alignment::Center)
            .into(),
        ]
    }

    fn system_theme_update(&mut self, _keys: &[&'static str], new_theme: &cosmic::cosmic_theme::Theme) -> Task<Message> {
        self.system_theme = Arc::new(new_theme.clone());
        self.apply_appearance()
    }

    fn system_theme_mode_update(&mut self, _keys: &[&'static str], mode: &cosmic::cosmic_theme::ThemeMode) -> Task<Message> {
        let t = if mode.is_dark { cosmic::theme::system_dark() } else { cosmic::theme::system_light() };
        self.system_theme = Arc::new(t.cosmic().clone());
        self.apply_appearance()
    }

    fn on_nav_select(&mut self, id: nav_bar::Id) -> Task<Message> {
        self.nav.activate(id);
        self.update_title()
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![cosmic::iced::time::every(Duration::from_secs(3)).map(|_| Message::HotplugTick)];
        if self.screen.live_active() {
            subs.push(cosmic::iced::time::every(Duration::from_secs(1)).map(|_| Message::Screen(pages::screen::Message::LiveTick)));
        }
        if self.screen.cpu_feed || self.screen.temp_feed || self.screen.live_active() {
            subs.push(cosmic::iced::time::every(Duration::from_secs(2)).map(|_| Message::TelemetryTick));
        }
        let paths = self.macro_paths();
        if self.macros.enabled && !paths.is_empty() && !self.demo {
            subs.push(Subscription::run_with(paths, macro_listener));
        }
        if self.desktop == Desktop::Kde {
            subs.push(cosmic::iced::time::every(Duration::from_secs(2)).map(|_| Message::AppearanceTick));
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
                self.sync_board();
                let out = self.screen.on_caps_changed(self.caps().screen);
                return self.handle(out, Message::Screen);
            }
            Message::Settings(m) => {
                self.settings.update(&mut self.prefs, m);
                if let Some(l) = self.prefs.keyboard_layout {
                    self.keys.set_physical(l);
                }
                return self.apply_appearance();
            }
            Message::Macros(m) => {
                let physical = self.physical();
                match self.macros.update(m, self.keys.layout(), physical) {
                    pages::macros::Effect::None => {}
                    pages::macros::Effect::Out(out) => return self.handle(out, Message::Macros),
                    pages::macros::Effect::Test(m) => {
                        self.macros.status = Some(Ok(fl!("macro-testing", name = m.name.clone())));
                        return self.play_macro(&m, Some(Duration::from_secs(3)));
                    }
                }
            }
            Message::MacroTrigger(usage) => {
                if let Some(m) = self.macros.by_trigger(usage).cloned() {
                    return self.play_macro(&m, None);
                }
            }
            Message::MacroListener(r) => self.macros.status = Some(r),
            Message::MacroPlayed(name, r) => {
                self.macros.status = Some(match r {
                    Ok(()) => Ok(fl!("macro-played", name = name)),
                    Err(e) => Err(format!("{e} ({})", fl!("uinput-help"))),
                });
            }
            Message::LayoutChoice(l) => self.layout_choice = Some(l),
            Message::LayoutConfirm => {
                if let Some(l) = self.layout_choice.take() {
                    self.prefs.keyboard_layout = Some(l);
                    self.prefs.save();
                    self.keys.set_physical(l);
                }
            }
            Message::AppearanceTick => {
                let mtime = appearance::kdeglobals_mtime();
                if mtime != self.kde_mtime {
                    self.kde_mtime = mtime;
                    let kde = KdeScheme::load();
                    if kde != self.kde {
                        self.kde = kde;
                        return self.apply_appearance();
                    }
                }
            }
            Message::WindowMinimize => return self.core.minimize(None),
            Message::WindowMaximize => return self.core.toggle_maximize(None),
            Message::WindowClose => {
                if let Some(id) = self.core.main_window_id() {
                    return cosmic::iced::window::close(id);
                }
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
                self.sync_board();
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
                // A running game's FPS takes over the CPU gauge when asked to.
                let cpu = self.cpu.sample();
                if let Some(v) = self.screen.gauge_fps().or(cpu.filter(|_| self.screen.cpu_feed)) {
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
            Page::Macros => self.macros.view(&ctx, self.keys.layout(), self.physical()).map(Message::Macros),
            Page::Screen => self.screen.view(&ctx).map(Message::Screen),
            Page::Settings => self
                .settings
                .view(&self.prefs, self.frosted_effective(), self.desktop, &self.theme_source)
                .map(Message::Settings),
            Page::Device => pages::device::view(&self.devices, self.active, &ctx, self.scan_error.as_deref()).map(Message::Device),
        };

        let mut col = widget::column::with_capacity(4).spacing(space.space_s);
        if !connected && !matches!(page, Page::Device | Page::Settings) {
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

/// Reads the keyboard's key reports and reports macro trigger presses.
#[allow(clippy::ptr_arg)] // `Subscription::run_with` needs `fn(&D)`
fn macro_listener(paths: &Vec<String>) -> impl futures::Stream<Item = Message> + use<> {
    let paths = paths.clone();
    cosmic::iced::stream::channel(32, async move |mut tx: futures::channel::mpsc::Sender<Message>| {
        let cpaths: Vec<std::ffi::CString> = paths.iter().filter_map(|p| std::ffi::CString::new(p.as_str()).ok()).collect();
        let refs: Vec<&std::ffi::CStr> = cpaths.iter().map(|c| c.as_c_str()).collect();
        let nodes = rt85_device::input::keyboard_nodes(&refs);
        if nodes.is_empty() {
            let _ = tx.send(Message::MacroListener(Err(fl!("no-key-interface")))).await;
            return;
        }
        let _ = tx.send(Message::MacroListener(Ok(fl!("macros-listening")))).await;
        let mut handles = Vec::new();
        for (path, reports) in nodes {
            let mut press_tx = tx.clone();
            let alive_tx = tx.clone();
            handles.push(tokio::task::spawn_blocking(move || {
                rt85_device::input::watch(
                    &path,
                    &reports,
                    |u| u8::try_from(u).is_ok_and(|u| MACRO_TRIGGERS.contains(&u)),
                    |u| {
                        let _ = press_tx.try_send(Message::MacroTrigger(u));
                    },
                    || !alive_tx.is_closed(),
                )
            }));
        }
        for h in handles {
            if let Ok(Err(e)) = h.await {
                let _ = tx.send(Message::MacroListener(Err(e.to_string()))).await;
            }
        }
    })
}
