//! Per-key colours and remapping on a visual layout.

use super::{Ctx, Output};
use crate::{
    fl,
    widgets::{color::{self, ColorEditor}, keyboard_layout::KeyView},
    worker::Job,
};
use cosmic::{Element, iced::Length, widget};
use rt85_protocol::{keycode, layout::Layout, royuan::keys::{MAX_KEYS, RgbFrame}};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Colors,
    Remap,
}

#[derive(Debug, Clone)]
pub enum Message {
    Tab(Tab),
    Toggle(u8),
    SelectAll,
    SelectNone,
    Color(color::Message),
    Paint,
    SendColors,
    RemapTarget(usize),
    ApplyRemap,
}

pub struct Page {
    layout: Layout,
    tab: Tab,
    selected: BTreeSet<u8>,
    colors: BTreeMap<u8, color::Rgb>,
    /// Remaps sent this session (the firmware can't be read back yet).
    remaps: BTreeMap<u8, u8>,
    color: ColorEditor,
    target: Option<usize>,
    keycode_names: Vec<String>,
}

impl Page {
    pub fn new() -> Self {
        Self {
            layout: Layout::rt100_uk_iso(),
            tab: Tab::Colors,
            selected: BTreeSet::new(),
            colors: BTreeMap::new(),
            remaps: BTreeMap::new(),
            color: ColorEditor::new((255, 0, 0)),
            target: None,
            keycode_names: keycode::KEYCODES.iter().map(|(_, n)| (*n).to_owned()).collect(),
        }
    }

    pub fn update(&mut self, msg: Message) -> Output<Message> {
        match msg {
            Message::Tab(t) => {
                self.tab = t;
                if t == Tab::Remap && self.selected.len() > 1 {
                    let first = self.selected.first().copied();
                    self.selected = first.into_iter().collect();
                }
            }
            Message::Toggle(i) => {
                if self.tab == Tab::Remap {
                    self.selected = BTreeSet::from([i]);
                } else if !self.selected.remove(&i) {
                    self.selected.insert(i);
                }
            }
            Message::SelectAll => self.selected = self.layout.keys.iter().map(|k| k.index).collect(),
            Message::SelectNone => self.selected.clear(),
            Message::Color(m) => self.color.update(m),
            Message::Paint => {
                for &i in &self.selected {
                    self.colors.insert(i, self.color.rgb);
                }
            }
            Message::SendColors => {
                let len = self.colors.keys().max().map_or(0, |m| *m as usize + 1).min(MAX_KEYS);
                let mut colors = vec![(0, 0, 0); len];
                for (&i, &c) in &self.colors {
                    if (i as usize) < len {
                        colors[i as usize] = c;
                    }
                }
                return Output::Job(Job::KeyColors(vec![RgbFrame { colors, time: 0 }]), fl!("sending-colors"));
            }
            Message::RemapTarget(i) => self.target = Some(i),
            Message::ApplyRemap => {
                if let (Some(&index), Some(t)) = (self.selected.first(), self.target) {
                    let code = keycode::KEYCODES[t].0;
                    self.remaps.insert(index, code);
                    return Output::Job(Job::Remap { index, code }, fl!("remapping"));
                }
            }
        }
        Output::None
    }

    pub fn view(&self, ctx: &Ctx) -> Element<'_, Message> {
        let space = cosmic::theme::spacing();
        if ctx.connected && !ctx.caps.per_key_rgb && !ctx.caps.remap {
            return super::unsupported(fl!("unsupported-keys"));
        }
        let colors = &self.colors;
        let remaps = &self.remaps;
        let selected = &self.selected;
        let tab = self.tab;
        let color_of = move |i: u8| if tab == Tab::Colors { colors.get(&i).copied() } else { None };
        let badge_of = move |i: u8| {
            if tab == Tab::Remap { remaps.get(&i).and_then(|c| keycode::name(*c)).map(|n| format!("→ {n}")) } else { None }
        };
        let is_selected = move |i: u8| selected.contains(&i);
        let board = KeyView { layout: &self.layout, unit: 46.0, color_of: &color_of, badge_of: &badge_of, is_selected: &is_selected }
            .view(Message::Toggle);

        let tabs = widget::row::with_children(vec![
            tab_button(fl!("tab-colors"), self.tab == Tab::Colors, Message::Tab(Tab::Colors)),
            tab_button(fl!("tab-remap"), self.tab == Tab::Remap, Message::Tab(Tab::Remap)),
        ])
        .spacing(space.space_xxs);

        let can_send = ctx.connected && !ctx.busy;
        let panel: Element<_> = match self.tab {
            Tab::Colors => widget::column::with_children(vec![
                widget::row::with_children(vec![
                    widget::button::standard(fl!("select-all")).on_press(Message::SelectAll).into(),
                    widget::button::standard(fl!("select-none")).on_press(Message::SelectNone).into(),
                ])
                .spacing(space.space_xs)
                .into(),
                self.color.view().map(Message::Color),
                widget::row::with_children(vec![
                    widget::button::standard(fl!("paint-selected"))
                        .on_press_maybe((!self.selected.is_empty()).then_some(Message::Paint))
                        .into(),
                    widget::button::suggested(fl!("send-colors"))
                        .on_press_maybe((can_send && ctx.caps.per_key_rgb).then_some(Message::SendColors))
                        .into(),
                ])
                .spacing(space.space_xs)
                .into(),
            ])
            .spacing(space.space_s)
            .into(),
            Tab::Remap => {
                let key_name = self
                    .selected
                    .first()
                    .and_then(|i| self.layout.keys.iter().find(|k| k.index == *i))
                    .map(|k| k.label.clone())
                    .unwrap_or_else(|| fl!("pick-a-key"));
                widget::column::with_children(vec![
                    widget::text::title4(key_name).into(),
                    widget::settings::item(
                        fl!("sends"),
                        widget::dropdown(&self.keycode_names, self.target, Message::RemapTarget),
                    )
                    .into(),
                    widget::button::suggested(fl!("apply-remap"))
                        .on_press_maybe(
                            (can_send && ctx.caps.remap && !self.selected.is_empty() && self.target.is_some())
                                .then_some(Message::ApplyRemap),
                        )
                        .into(),
                ])
                .spacing(space.space_s)
                .into()
            }
        };

        widget::column::with_children(vec![
            tabs.into(),
            widget::text::caption(fl!("layout-note", name = self.layout.name.clone())).into(),
            widget::scrollable::horizontal(board).width(Length::Fill).into(),
            widget::container(panel).max_width(520).into(),
        ])
        .spacing(space.space_m)
        .into()
    }
}

fn tab_button<'a>(label: String, active: bool, msg: Message) -> Element<'a, Message> {
    if active { widget::button::suggested(label).into() } else { widget::button::standard(label).on_press(msg).into() }
}
