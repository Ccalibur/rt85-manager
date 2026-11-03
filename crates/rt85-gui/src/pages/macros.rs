//! Macros: a list of key/text/delay sequences, each fired by a keyboard key
//! that is remapped to an unused F13–F24 code.

use super::{Ctx, Output};
use crate::{fl, widgets::keyboard_layout::KeyView, worker::Job};
use cosmic::{
    Element,
    iced::{Alignment, Length},
    widget,
};
use rt85_protocol::{
    keycode::MACRO_TRIGGERS,
    layout::{Layout, Physical, default_usage},
    macros::{Macro, Mods, Step},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum Message {
    Enabled(bool),
    New,
    Delete,
    Select(usize),
    Name(String),
    Trigger(usize),
    NewKind(usize),
    NewKey(usize),
    Ctrl(bool),
    Shift(bool),
    Alt(bool),
    Super(bool),
    NewText(String),
    NewDelay(String),
    AddStep,
    RemoveStep(usize),
    MoveStep(usize, bool),
    Assigning(bool),
    PickKey(u8),
    Restore,
    Test,
}

/// What the app should do after a macros message.
pub enum Effect {
    None,
    Out(Output<Message>),
    /// Play this macro after a short countdown (Test button).
    Test(Macro),
}

/// Saved to `~/.config/rt85-manager/macros.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Saved {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    macros: Vec<Macro>,
}

pub struct Page {
    pub enabled: bool,
    pub macros: Vec<Macro>,
    selected: Option<usize>,
    assigning: bool,
    kind: usize,
    kind_names: Vec<String>,
    trigger_names: Vec<String>,
    key: Option<usize>,
    mods: Mods,
    text: String,
    delay: String,
    /// Listener / player state, set by the app.
    pub status: Option<Result<String, String>>,
    load_error: Option<String>,
}

fn path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("rt85-manager/macros.json"))
}

impl Page {
    pub fn new() -> Self {
        let (saved, load_error) = match path().map(std::fs::read_to_string) {
            Some(Ok(text)) => match serde_json::from_str::<Saved>(&text) {
                Ok(s) => (s, None),
                Err(e) => (Saved::default(), Some(e.to_string())),
            },
            _ => (Saved::default(), None),
        };
        Self {
            enabled: saved.enabled,
            selected: (!saved.macros.is_empty()).then_some(0),
            macros: saved.macros,
            assigning: false,
            kind: 0,
            kind_names: vec![fl!("step-key"), fl!("step-text"), fl!("step-delay")],
            trigger_names: MACRO_TRIGGERS.map(|u| format!("F{}", u - 0x68 + 13)).collect(),
            key: None,
            mods: Mods::default(),
            text: String::new(),
            delay: "100".into(),
            status: None,
            load_error,
        }
    }

    fn save(&self) {
        let Some(p) = path() else { return };
        let saved = Saved { enabled: self.enabled, macros: self.macros.clone() };
        let r = p
            .parent()
            .map(std::fs::create_dir_all)
            .transpose()
            .and_then(|_| std::fs::write(&p, serde_json::to_string_pretty(&saved).unwrap_or_default()));
        if let Err(e) = r {
            tracing::warn!("saving macros: {e}");
        }
    }

    pub fn by_trigger(&self, usage: u16) -> Option<&Macro> {
        self.macros.iter().find(|m| u16::from(m.trigger) == usage)
    }

    fn current(&mut self) -> Option<&mut Macro> {
        self.selected.and_then(|i| self.macros.get_mut(i))
    }

    fn free_trigger(&self) -> Option<u8> {
        MACRO_TRIGGERS.clone().find(|t| !self.macros.iter().any(|m| m.trigger == *t))
    }

    pub fn update(&mut self, msg: Message, layout: &Layout, physical: Physical) -> Effect {
        let keycodes = physical.keycodes();
        match msg {
            Message::Enabled(v) => {
                self.enabled = v;
                self.save();
            }
            Message::New => {
                let Some(trigger) = self.free_trigger() else { return Effect::None };
                self.macros.push(Macro {
                    name: fl!("macro-default-name", n = ((self.macros.len() + 1) as u64)),
                    trigger,
                    key: None,
                    steps: Vec::new(),
                });
                self.selected = Some(self.macros.len() - 1);
                self.assigning = false;
                self.save();
            }
            Message::Delete => {
                if let Some(i) = self.selected {
                    // Put the key back first so it isn't left sending F13–F24.
                    let restore = self.restore_job(i, layout);
                    self.macros.remove(i);
                    self.selected = if self.macros.is_empty() { None } else { Some(i.min(self.macros.len() - 1)) };
                    self.save();
                    if let Some(job) = restore {
                        return Effect::Out(Output::Job(job, fl!("remapping")));
                    }
                }
            }
            Message::Select(i) => {
                self.selected = Some(i);
                self.assigning = false;
            }
            Message::Name(n) => {
                if let Some(m) = self.current() {
                    m.name = n;
                }
                self.save();
            }
            Message::Trigger(i) => {
                let t = *MACRO_TRIGGERS.start() + i as u8;
                if self.macros.iter().enumerate().any(|(j, m)| m.trigger == t && Some(j) != self.selected) {
                    return Effect::None;
                }
                let mut job = None;
                if let Some(m) = self.current() {
                    m.trigger = t;
                    // Re-point an assigned key at the new code.
                    job = m.key.map(|index| Job::Remap { index, code: t });
                }
                self.save();
                if let Some(job) = job {
                    return Effect::Out(Output::Job(job, fl!("remapping")));
                }
            }
            Message::NewKind(i) => self.kind = i,
            Message::NewKey(i) => self.key = Some(i),
            Message::Ctrl(v) => self.mods.ctrl = v,
            Message::Shift(v) => self.mods.shift = v,
            Message::Alt(v) => self.mods.alt = v,
            Message::Super(v) => self.mods.meta = v,
            Message::NewText(t) => self.text = t,
            Message::NewDelay(d) => self.delay = d.chars().filter(char::is_ascii_digit).take(6).collect(),
            Message::AddStep => {
                let step = match self.kind {
                    0 => self.key.and_then(|k| keycodes.get(k)).map(|(usage, _)| Step::Key { usage: *usage, mods: self.mods }),
                    1 => (!self.text.is_empty()).then(|| Step::Text { text: self.text.clone() }),
                    _ => self.delay.parse().ok().map(|ms| Step::Delay { ms }),
                };
                if let (Some(step), Some(m)) = (step, self.current()) {
                    m.steps.push(step);
                    self.text.clear();
                    self.save();
                }
            }
            Message::RemoveStep(i) => {
                if let Some(m) = self.current()
                    && i < m.steps.len()
                {
                    m.steps.remove(i);
                    self.save();
                }
            }
            Message::MoveStep(i, up) => {
                if let Some(m) = self.current() {
                    let j = if up { i.checked_sub(1) } else { Some(i + 1) };
                    if let Some(j) = j.filter(|j| *j < m.steps.len()) {
                        m.steps.swap(i, j);
                        self.save();
                    }
                }
            }
            Message::Assigning(v) => self.assigning = v,
            Message::PickKey(index) => {
                self.assigning = false;
                let Some(i) = self.selected else { return Effect::None };
                let previous = self.restore_job(i, layout).filter(|_| self.macros[i].key != Some(index));
                let trigger = self.macros[i].trigger;
                // One macro per key.
                for (j, m) in self.macros.iter_mut().enumerate() {
                    if j != i && m.key == Some(index) {
                        m.key = None;
                    }
                }
                self.macros[i].key = Some(index);
                self.save();
                let job = Job::Remap { index, code: trigger };
                return Effect::Out(match previous {
                    Some(p) => Output::Job(Job::Batch(vec![p, job]), fl!("remapping")),
                    None => Output::Job(job, fl!("remapping")),
                });
            }
            Message::Restore => {
                if let Some(i) = self.selected {
                    let job = self.restore_job(i, layout);
                    self.macros[i].key = None;
                    self.save();
                    if let Some(job) = job {
                        return Effect::Out(Output::Job(job, fl!("remapping")));
                    }
                }
            }
            Message::Test => {
                if let Some(m) = self.selected.and_then(|i| self.macros.get(i)) {
                    return Effect::Test(m.clone());
                }
            }
        }
        Effect::None
    }

    /// The remap that gives macro `i`'s key its normal code back.
    fn restore_job(&self, i: usize, layout: &Layout) -> Option<Job> {
        let index = self.macros.get(i)?.key?;
        let key = layout.keys.iter().find(|k| k.index == index)?;
        Some(Job::Remap { index, code: default_usage(&key.name)? })
    }

    pub fn view<'a>(&'a self, ctx: &Ctx, layout: &'a Layout, physical: Physical) -> Element<'a, Message> {
        let space = cosmic::theme::spacing();
        let idle = ctx.connected && !ctx.busy && ctx.caps.remap;
        let mut col: Vec<Element<Message>> = Vec::new();

        if let Some(e) = &self.load_error {
            col.push(widget::warning(format!("macros.json: {e}")).into());
        }
        if ctx.connected && !ctx.caps.remap {
            col.push(widget::warning(fl!("macros-unsupported")).into());
        }

        let mut general = widget::settings::section().add(
            widget::settings::item::builder(fl!("macros-enabled"))
                .description(fl!("macros-enabled-help"))
                .toggler(self.enabled, Message::Enabled),
        );
        if let Some(s) = &self.status {
            let text = match s {
                Ok(t) => t.clone(),
                Err(e) => format!("{}: {e}", fl!("error")),
            };
            general = general.add(widget::settings::item(fl!("macros-status"), widget::text::body(text)));
        }
        col.push(general.into());

        // Macro list
        let mut list = widget::settings::section().title(fl!("macros"));
        for (i, m) in self.macros.iter().enumerate() {
            let key = m
                .key
                .and_then(|idx| layout.keys.iter().find(|k| k.index == idx))
                .map(|k| k.label.clone())
                .unwrap_or_else(|| fl!("not-assigned"));
            let label = widget::column::with_children(vec![
                widget::text::body(m.name.clone()).into(),
                widget::text::caption(format!(
                    "{} · {} · {}",
                    key,
                    self.trigger_names[(m.trigger - MACRO_TRIGGERS.start()) as usize],
                    fl!("step-count", n = (m.steps.len() as u64))
                ))
                .into(),
            ])
            .spacing(2)
            .width(Length::Fill);
            let selected = self.selected == Some(i);
            list = list.add(
                widget::button::custom(label)
                    .width(Length::Fill)
                    .padding(space.space_xs)
                    .class(if selected { cosmic::theme::Button::Suggested } else { cosmic::theme::Button::Text })
                    .on_press(Message::Select(i)),
            );
        }
        if self.macros.is_empty() {
            list = list.add(widget::text::body(fl!("no-macros")));
        }
        col.push(list.into());
        col.push(
            widget::row::with_children(vec![
                widget::button::standard(fl!("new-macro"))
                    .on_press_maybe(self.free_trigger().map(|_| Message::New))
                    .into(),
                widget::button::destructive(fl!("delete-macro"))
                    .on_press_maybe(self.selected.map(|_| Message::Delete))
                    .into(),
            ])
            .spacing(space.space_xs)
            .into(),
        );

        if let Some(m) = self.selected.and_then(|i| self.macros.get(i)) {
            col.push(self.editor(m, ctx, layout, physical, idle));
        }

        widget::settings::view_column(col).spacing(space.space_m).into()
    }

    fn editor<'a>(&'a self, m: &'a Macro, ctx: &Ctx, layout: &'a Layout, physical: Physical, idle: bool) -> Element<'a, Message> {
        let space = cosmic::theme::spacing();
        let keycodes = physical.keycodes();
        let key_names: Vec<String> = keycodes.iter().map(|(_, n)| (*n).to_owned()).collect();

        let assigned = m.key.and_then(|idx| layout.keys.iter().find(|k| k.index == idx)).map(|k| k.label.clone());
        let mut settings = widget::settings::section()
            .title(fl!("macro-edit"))
            .add(widget::settings::item(fl!("macro-name"), widget::text_input("", &m.name).on_input(Message::Name).width(240)))
            .add(
                widget::settings::item::builder(fl!("macro-trigger"))
                    .description(fl!("macro-trigger-help"))
                    .control(widget::dropdown(
                        &self.trigger_names,
                        Some((m.trigger - MACRO_TRIGGERS.start()) as usize),
                        Message::Trigger,
                    )),
            )
            .add(
                widget::settings::item::builder(fl!("macro-key"))
                    .description(assigned.clone().unwrap_or_else(|| fl!("not-assigned")))
                    .control(
                        widget::row::with_children(vec![
                            widget::button::standard(if self.assigning { fl!("cancel") } else { fl!("assign-key") })
                                .on_press_maybe((idle || self.assigning).then_some(Message::Assigning(!self.assigning)))
                                .into(),
                            widget::button::text(fl!("restore-key"))
                                .on_press_maybe((idle && m.key.is_some()).then_some(Message::Restore))
                                .into(),
                        ])
                        .spacing(space.space_xs),
                    ),
            );
        if !ctx.connected {
            settings = settings.add(widget::text::caption(fl!("macro-connect")));
        }

        let mut col: Vec<Element<Message>> = vec![settings.into()];

        if self.assigning {
            let key = m.key;
            let is_selected = move |i: u8| Some(i) == key;
            let none_c = |_: u8| None;
            let none_b = |_: u8| None;
            let board = KeyView { layout, unit: 40.0, color_of: &none_c, badge_of: &none_b, is_selected: &is_selected }
                .view(Message::PickKey);
            col.push(widget::text::body(fl!("pick-macro-key")).into());
            col.push(widget::scrollable::horizontal(board).width(Length::Fill).into());
        }

        // Steps
        let mut steps = widget::settings::section().title(fl!("macro-steps"));
        for (i, s) in m.steps.iter().enumerate() {
            let text = match s {
                Step::Key { usage, mods } => {
                    let name = keycodes.iter().find(|(c, _)| c == usage).map(|(_, n)| *n).unwrap_or("?");
                    format!("{} {}{}", fl!("step-press"), mods.label(), name)
                }
                Step::Text { text } => format!("{} “{}”", fl!("step-type"), text),
                Step::Delay { ms } => format!("{} {ms} ms", fl!("step-wait")),
            };
            let row = widget::row::with_children(vec![
                widget::text::body(format!("{}. {text}", i + 1)).width(Length::Fill).into(),
                widget::button::icon(widget::icon::from_name("go-up-symbolic"))
                    .on_press_maybe((i > 0).then_some(Message::MoveStep(i, true)))
                    .into(),
                widget::button::icon(widget::icon::from_name("go-down-symbolic"))
                    .on_press_maybe((i + 1 < m.steps.len()).then_some(Message::MoveStep(i, false)))
                    .into(),
                widget::button::icon(widget::icon::from_name("edit-delete-symbolic"))
                    .on_press(Message::RemoveStep(i))
                    .into(),
            ])
            .align_y(Alignment::Center)
            .spacing(space.space_xxs);
            steps = steps.add(row);
        }
        if m.steps.is_empty() {
            steps = steps.add(widget::text::body(fl!("no-steps")));
        }
        col.push(steps.into());

        // Add a step
        let input: Element<Message> = match self.kind {
            0 => widget::row::with_children(vec![
                widget::dropdown(key_names, self.key, Message::NewKey).into(),
                widget::checkbox(self.mods.ctrl).label("Ctrl").on_toggle(Message::Ctrl).into(),
                widget::checkbox(self.mods.shift).label("Shift").on_toggle(Message::Shift).into(),
                widget::checkbox(self.mods.alt).label("Alt").on_toggle(Message::Alt).into(),
                widget::checkbox(self.mods.meta).label("Super").on_toggle(Message::Super).into(),
            ])
            .spacing(space.space_s)
            .align_y(Alignment::Center)
            .into(),
            1 => widget::text_input(fl!("text-to-type"), &self.text).on_input(Message::NewText).width(320).into(),
            _ => widget::row::with_children(vec![
                widget::text_input("100", &self.delay).on_input(Message::NewDelay).width(100).into(),
                widget::text::body("ms").into(),
            ])
            .spacing(space.space_xs)
            .align_y(Alignment::Center)
            .into(),
        };
        let can_add = match self.kind {
            0 => self.key.is_some(),
            1 => !self.text.is_empty(),
            _ => self.delay.parse::<u32>().is_ok(),
        };
        col.push(
            widget::row::with_children(vec![
                widget::dropdown(&self.kind_names, Some(self.kind), Message::NewKind).into(),
                input,
                widget::button::standard(fl!("add-step")).on_press_maybe(can_add.then_some(Message::AddStep)).into(),
            ])
            .spacing(space.space_s)
            .align_y(Alignment::Center)
            .into(),
        );
        if self.kind == 1 {
            col.push(widget::text::caption(fl!("text-layout-note", layout = physical.label())).into());
        }

        col.push(
            widget::button::standard(fl!("test-macro"))
                .on_press_maybe((!m.steps.is_empty()).then_some(Message::Test))
                .into(),
        );

        widget::column::with_children(col).spacing(space.space_s).into()
    }
}
