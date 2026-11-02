//! Small RGB colour editor: swatch, hex field, three sliders and presets.

use cosmic::{
    Element,
    iced::{Color, Length, widget::container as iced_container},
    theme,
    widget::{self, container},
};

pub type Rgb = (u8, u8, u8);

const PRESETS: &[Rgb] = &[
    (255, 255, 255), (255, 0, 0), (255, 128, 0), (255, 220, 0), (0, 255, 0),
    (0, 255, 200), (0, 128, 255), (0, 0, 255), (160, 0, 255), (255, 0, 160),
];

#[derive(Debug, Clone)]
pub enum Message {
    R(u8),
    G(u8),
    B(u8),
    Hex(String),
    Preset(Rgb),
}

#[derive(Debug, Clone)]
pub struct ColorEditor {
    pub rgb: Rgb,
    hex: String,
}

impl ColorEditor {
    pub fn new(rgb: Rgb) -> Self {
        Self { rgb, hex: to_hex(rgb) }
    }

    pub fn update(&mut self, msg: Message) {
        match msg {
            Message::R(v) => self.rgb.0 = v,
            Message::G(v) => self.rgb.1 = v,
            Message::B(v) => self.rgb.2 = v,
            Message::Preset(c) => self.rgb = c,
            Message::Hex(s) => {
                if let Some(c) = parse_hex(&s) {
                    self.rgb = c;
                }
                self.hex = s;
                return;
            }
        }
        self.hex = to_hex(self.rgb);
    }

    pub fn view(&self) -> Element<'_, Message> {
        let space = theme::spacing();
        let slider = |label: &'static str, v: u8, f: fn(u8) -> Message| {
            widget::row::with_children(vec![
                widget::text::body(label).width(Length::Fixed(16.0)).into(),
                widget::slider(0..=255u8, v, f).into(),
                widget::text::monotext(format!("{v:>3}")).into(),
            ])
            .spacing(space.space_xs)
            .align_y(cosmic::iced::Alignment::Center)
        };
        let presets = widget::row::with_children(
            PRESETS
                .iter()
                .map(|&c| {
                    widget::button::custom(swatch(c, 24.0, 24.0))
                        .padding(2)
                        .on_press(Message::Preset(c))
                        .into()
                })
                .collect::<Vec<Element<Message>>>(),
        )
        .spacing(space.space_xxs);

        widget::column::with_children(vec![
            widget::row::with_children(vec![
                swatch(self.rgb, 48.0, 32.0),
                widget::text_input("#rrggbb", &self.hex).on_input(Message::Hex).width(Length::Fixed(120.0)).into(),
            ])
            .spacing(space.space_s)
            .align_y(cosmic::iced::Alignment::Center)
            .into(),
            slider("R", self.rgb.0, Message::R).into(),
            slider("G", self.rgb.1, Message::G).into(),
            slider("B", self.rgb.2, Message::B).into(),
            presets.into(),
        ])
        .spacing(space.space_xs)
        .into()
    }
}

/// A filled rounded rectangle.
pub fn swatch<'a, M: 'a>(c: Rgb, w: f32, h: f32) -> Element<'a, M> {
    container(widget::Space::new().width(Length::Fixed(w)).height(Length::Fixed(h)))
        .class(theme::Container::custom(move |t| iced_container::Style {
            background: Some(Color::from_rgb8(c.0, c.1, c.2).into()),
            border: cosmic::iced::Border {
                radius: 4.0.into(),
                width: 1.0,
                color: t.cosmic().bg_divider().into(),
            },
            ..Default::default()
        }))
        .into()
}

pub fn to_hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
}

pub fn parse_hex(s: &str) -> Option<Rgb> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some(((v >> 16) as u8, (v >> 8) as u8, v as u8))
}
