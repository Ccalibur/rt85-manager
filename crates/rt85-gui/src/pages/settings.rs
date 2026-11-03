use crate::{
    appearance::{Desktop, Prefs, Scheme, Source},
    fl,
};
use rt85_protocol::layout::Physical;
use cosmic::{Element, widget};

#[derive(Debug, Clone)]
pub enum Message {
    Scheme(usize),
    Frosted(bool),
    KeyboardLayout(usize),
}

pub struct Page {
    scheme_names: Vec<String>,
    layout_names: Vec<String>,
}

impl Page {
    pub fn new() -> Self {
        Self {
            scheme_names: Scheme::ALL
                .iter()
                .map(|s| match s {
                    Scheme::System => fl!("scheme-system"),
                    Scheme::Light => fl!("scheme-light"),
                    Scheme::Dark => fl!("scheme-dark"),
                })
                .collect(),
            layout_names: Physical::ALL.iter().map(|p| layout_name(*p)).collect(),
        }
    }

    /// Applies the message to `prefs`; the app re-resolves the theme after.
    pub fn update(&self, prefs: &mut Prefs, msg: Message) {
        match msg {
            Message::Scheme(i) => prefs.scheme = Scheme::ALL[i],
            Message::Frosted(on) => prefs.frosted = Some(on),
            Message::KeyboardLayout(i) => prefs.keyboard_layout = Some(Physical::ALL[i]),
        }
        prefs.save();
    }

    pub fn view<'a>(&'a self, prefs: &Prefs, frosted: bool, desktop: Desktop, source: &Source) -> Element<'a, Message> {
        let scheme_idx = Scheme::ALL.iter().position(|s| *s == prefs.scheme);
        let following = match source {
            Source::Cosmic => fl!("source-cosmic"),
            Source::Kde(name) => fl!("source-kde", name = name.clone()),
            Source::Portal => fl!("source-portal"),
            Source::Builtin => fl!("source-builtin"),
        };

        let frosted_help = if desktop == Desktop::Cosmic { fl!("frosted-help") } else { fl!("frosted-help-other") };

        let appearance = widget::settings::section()
            .title(fl!("appearance"))
            .add(
                widget::settings::item::builder(fl!("color-scheme"))
                    .description(following)
                    .control(widget::dropdown(&self.scheme_names, scheme_idx, Message::Scheme)),
            )
            .add(
                widget::settings::item::builder(fl!("frosted-glass"))
                    .description(frosted_help)
                    .toggler(frosted, Message::Frosted),
            );

        let keyboard = widget::settings::section().title(fl!("keyboard")).add(
            widget::settings::item::builder(fl!("keyboard-layout"))
                .description(fl!("keyboard-layout-help"))
                .control(widget::dropdown(
                    &self.layout_names,
                    prefs.keyboard_layout.and_then(|l| Physical::ALL.iter().position(|p| *p == l)),
                    Message::KeyboardLayout,
                )),
        );

        widget::settings::view_column(vec![keyboard.into(), appearance.into()]).into()
    }
}

pub fn layout_name(p: Physical) -> String {
    match p {
        Physical::Ansi => fl!("layout-ansi"),
        Physical::Iso => fl!("layout-iso"),
    }
}

pub fn layout_hint(p: Physical) -> String {
    match p {
        Physical::Ansi => fl!("layout-ansi-hint"),
        Physical::Iso => fl!("layout-iso-hint"),
    }
}
