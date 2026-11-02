use super::{Ctx, Output};
use crate::{fl, widgets::color::{self, ColorEditor}, worker::Job};
use cosmic::{Element, widget};
use rt85_protocol::royuan::lighting::{Direction, MAX_BRIGHTNESS, MAX_SPEED, Mode, Profile};

#[derive(Debug, Clone)]
pub enum Message {
    Mode(usize),
    Speed(u8),
    Brightness(u8),
    Dazzle(bool),
    Direction(usize),
    Color(color::Message),
    Apply,
}

pub struct Page {
    profile: Profile,
    color: ColorEditor,
    mode_names: Vec<String>,
    directions: Vec<String>,
}

impl Page {
    pub fn new() -> Self {
        let profile = Profile::default();
        Self {
            color: ColorEditor::new(profile.rgb),
            profile,
            mode_names: Mode::ALL.iter().map(|m| m.label().to_owned()).collect(),
            directions: (1..=4).map(|i| format!("{} {i}", fl!("direction"))).collect(),
        }
    }

    pub fn update(&mut self, msg: Message) -> Output<Message> {
        match msg {
            Message::Mode(i) => self.profile.mode = Mode::ALL[i],
            Message::Speed(v) => self.profile.speed = v,
            Message::Brightness(v) => self.profile.brightness = v,
            Message::Dazzle(v) => self.profile.dazzle = v,
            Message::Direction(i) => {
                self.profile.direction = [Direction::D0, Direction::D1, Direction::D2, Direction::D3][i]
            }
            Message::Color(m) => {
                self.color.update(m);
                self.profile.rgb = self.color.rgb;
            }
            Message::Apply => return Output::Job(Job::Profile(self.profile), fl!("applying-lighting")),
        }
        Output::None
    }

    pub fn view(&self, ctx: &Ctx) -> Element<'_, Message> {
        if ctx.connected && !ctx.caps.lighting_modes {
            return super::unsupported(fl!("unsupported-lighting"));
        }
        let mode_idx = Mode::ALL.iter().position(|m| *m == self.profile.mode);
        let effect = widget::settings::section()
            .title(fl!("effect"))
            .add(widget::settings::item(fl!("mode"), widget::dropdown(&self.mode_names, mode_idx, Message::Mode)))
            .add(widget::settings::item(
                fl!("speed"),
                widget::slider(0..=MAX_SPEED, self.profile.speed, Message::Speed).width(240),
            ))
            .add(widget::settings::item(
                fl!("brightness"),
                widget::slider(0..=MAX_BRIGHTNESS, self.profile.brightness, Message::Brightness).width(240),
            ))
            .add(widget::settings::item(
                fl!("direction"),
                widget::dropdown(&self.directions, Some(self.profile.direction as usize), Message::Direction),
            ))
            .add(widget::settings::item(fl!("rainbow"), widget::toggler(self.profile.dazzle).on_toggle(Message::Dazzle)));

        let colour = widget::settings::section()
            .title(fl!("color"))
            .add(self.color.view().map(Message::Color));

        let apply = widget::button::suggested(fl!("apply"))
            .on_press_maybe((ctx.connected && !ctx.busy).then_some(Message::Apply));

        widget::settings::view_column(vec![effect.into(), colour.into(), apply.into()]).into()
    }
}
