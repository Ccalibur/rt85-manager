use super::{Ctx, Output};
use crate::{fl, widgets::color::{self, ColorEditor}, worker::Job};
use cosmic::{Element, widget};
use rt85_protocol::royuan::{
    Family,
    lighting::{self, Effect, MAX_BRIGHTNESS, Profile},
};

#[derive(Debug, Clone)]
pub enum Message {
    Effect(usize),
    Speed(u8),
    Brightness(u8),
    Dazzle(bool),
    Option(usize),
    Color(color::Message),
    Apply,
}

pub struct Page {
    profile: Profile,
    color: ColorEditor,
    /// Command set of the connected keyboard; the RT85's (gen2) when none is.
    family: Family,
    effect_names: Vec<String>,
}

fn names(family: Family) -> Vec<String> {
    lighting::effects(family).iter().map(|e| e.label.to_owned()).collect()
}

impl Page {
    pub fn new() -> Self {
        let profile = Profile::default();
        Self {
            color: ColorEditor::new(profile.rgb),
            profile,
            family: Family::Gen2,
            effect_names: names(Family::Gen2),
        }
    }

    /// Demo mode: a livelier starting point than plain white.
    pub fn seed_demo(&mut self) {
        self.profile.effect = 2; // breathing on both families
        self.profile.speed = 3;
        self.update(Message::Color(color::Message::Preset((43, 212, 196))));
    }

    /// The connected keyboard changed: offer its effects.
    pub fn set_family(&mut self, family: Option<Family>) {
        let family = family.unwrap_or(Family::Gen2);
        if family == self.family {
            return;
        }
        self.family = family;
        self.effect_names = names(family);
        if lighting::effect(family, self.profile.effect).is_none() {
            self.profile.effect = lighting::effects(family)[0].id;
            self.profile.option = 0;
        }
        self.profile.speed = self.profile.speed.min(lighting::max_speed(family));
    }

    fn effect(&self) -> &'static Effect {
        lighting::effect(self.family, self.profile.effect).unwrap_or(&lighting::effects(self.family)[0])
    }

    pub fn update(&mut self, msg: Message) -> Output<Message> {
        match msg {
            Message::Effect(i) => {
                if let Some(e) = lighting::effects(self.family).get(i) {
                    self.profile.effect = e.id;
                    if self.profile.option as usize >= e.options.len() {
                        self.profile.option = 0;
                    }
                }
            }
            Message::Speed(v) => self.profile.speed = v,
            Message::Brightness(v) => self.profile.brightness = v,
            Message::Dazzle(v) => self.profile.dazzle = v,
            Message::Option(i) => self.profile.option = i as u8,
            Message::Color(m) => {
                self.color.update(m);
                self.profile.rgb = self.color.rgb;
            }
            Message::Apply => {
                let p = Profile { effect: self.effect().id, ..self.profile };
                return Output::Job(Job::Profile(p), fl!("applying-lighting"));
            }
        }
        Output::None
    }

    pub fn view(&self, ctx: &Ctx) -> Element<'_, Message> {
        if ctx.connected && !ctx.caps.lighting_modes {
            return super::unsupported(fl!("unsupported-lighting"));
        }
        let effect = self.effect();
        let idx = lighting::effects(self.family).iter().position(|e| e.id == effect.id);
        let mut section = widget::settings::section()
            .title(fl!("effect"))
            .add(widget::settings::item(fl!("mode"), widget::dropdown(&self.effect_names, idx, Message::Effect)));
        if effect.speed {
            section = section.add(widget::settings::item(
                fl!("speed"),
                widget::slider(0..=lighting::max_speed(self.family), self.profile.speed, Message::Speed).width(240),
            ));
        }
        section = section.add(widget::settings::item(
            fl!("brightness"),
            widget::slider(0..=MAX_BRIGHTNESS, self.profile.brightness, Message::Brightness).width(240),
        ));
        if !effect.options.is_empty() {
            section = section.add(widget::settings::item(
                fl!("effect-option"),
                widget::dropdown(effect.options, Some(self.profile.option as usize), Message::Option),
            ));
        }
        if effect.color {
            section = section
                .add(widget::settings::item(fl!("rainbow"), widget::toggler(self.profile.dazzle).on_toggle(Message::Dazzle)));
        }

        let mut col: Vec<Element<Message>> = vec![section.into()];
        if effect.color && !self.profile.dazzle {
            col.push(widget::settings::section().title(fl!("color")).add(self.color.view().map(Message::Color)).into());
        }
        col.push(
            widget::button::suggested(fl!("apply"))
                .on_press_maybe((ctx.connected && !ctx.busy).then_some(Message::Apply))
                .into(),
        );
        widget::settings::view_column(col).into()
    }
}
