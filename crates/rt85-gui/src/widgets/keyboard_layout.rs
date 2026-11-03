//! Clickable keyboard drawn from a [`Layout`]: one row per distinct `y`,
//! with spacers for the gaps. Keys taller than 1u are drawn 1u high.

use super::color::{Rgb, swatch};
use cosmic::{
    Element,
    iced::{Alignment, Length},
    widget,
};
use rt85_protocol::layout::Layout;

pub struct KeyView<'a> {
    pub layout: &'a Layout,
    /// Key unit in pixels.
    pub unit: f32,
    pub color_of: &'a dyn Fn(u8) -> Option<Rgb>,
    pub badge_of: &'a dyn Fn(u8) -> Option<String>,
    pub is_selected: &'a dyn Fn(u8) -> bool,
}

impl<'a> KeyView<'a> {
    pub fn view<'b, M: Clone + 'static>(&self, on_press: impl Fn(u8) -> M) -> Element<'b, M> {
        let gap = 3.0;
        let mut rows: Vec<(f32, Vec<&rt85_protocol::layout::PlacedKey>)> = Vec::new();
        for k in &self.layout.keys {
            // Rows are snapped to quarter units.
            let y = (k.y * 4.0).round() / 4.0;
            match rows.iter_mut().find(|(ry, _)| (*ry - y).abs() < f32::EPSILON) {
                Some((_, keys)) => keys.push(k),
                None => rows.push((y, vec![k])),
            }
        }
        rows.sort_by(|a, b| a.0.total_cmp(&b.0));

        let mut column = widget::column::with_capacity(rows.len()).spacing(gap);
        let mut last_y = 0.0;
        for (y, mut keys) in rows {
            if y - last_y > 1.0 {
                column = column.push(widget::Space::new().height(Length::Fixed((y - last_y - 1.0) * self.unit)));
            }
            last_y = y;
            keys.sort_by(|a, b| a.x.total_cmp(&b.x));
            let mut row = widget::row::with_capacity(keys.len() * 2).spacing(0).align_y(Alignment::Start);
            let mut cursor = 0.0;
            for k in keys {
                if k.x > cursor {
                    row = row.push(widget::Space::new().width(Length::Fixed((k.x - cursor) * self.unit)));
                }
                let w = k.w * self.unit - gap;
                let mut content = widget::column::with_capacity(3)
                    .push(widget::text::caption(k.label.clone()))
                    .align_x(Alignment::Center)
                    .spacing(2)
                    .width(Length::Fill);
                if let Some(c) = (self.color_of)(k.index) {
                    content = content.push(swatch(c, (w - 16.0).max(8.0), 4.0));
                }
                if let Some(b) = (self.badge_of)(k.index) {
                    content = content.push(widget::text::caption(b));
                }
                let button = widget::button::custom(content)
                    .width(Length::Fixed(w))
                    .height(Length::Fixed(self.unit - gap))
                    .padding(2)
                    .selected((self.is_selected)(k.index))
                    .class(keycap())
                    .on_press(on_press(k.index));
                row = row.push(button);
                row = row.push(widget::Space::new().width(Length::Fixed(gap)));
                cursor = k.x + k.w;
            }
            column = column.push(row);
        }
        column.into()
    }
}

/// Square keycap: small corner radius, a thin edge, accent outline when selected.
fn keycap() -> cosmic::theme::Button {
    use cosmic::{
        iced::{Background, Color},
        widget::button::Style,
    };
    fn style(theme: &cosmic::Theme, selected: bool, fill: impl Fn(&cosmic::cosmic_theme::Component) -> cosmic::cosmic_theme::palette::Srgba) -> Style {
        let c = theme.cosmic();
        let mut s = Style::new();
        s.background = Some(Background::Color(fill(&c.button).into()));
        s.border_radius = 4.0.into();
        s.border_width = if selected { 2.0 } else { 1.0 };
        s.border_color = if selected { c.accent_color().into() } else { c.button.divider.into() };
        s.text_color = Some(c.button.on.into());
        s.icon_color = Some(c.button.on.into());
        if selected {
            let mut a: Color = c.accent_color().into();
            a.a = 0.18;
            s.overlay = Some(Background::Color(a));
        }
        s
    }
    cosmic::theme::Button::Custom {
        active: Box::new(|selected, t| style(t, selected, |b| b.base)),
        disabled: Box::new(|t| style(t, false, |b| b.disabled)),
        hovered: Box::new(|selected, t| style(t, selected, |b| b.hover)),
        pressed: Box::new(|selected, t| style(t, selected, |b| b.pressed)),
    }
}
