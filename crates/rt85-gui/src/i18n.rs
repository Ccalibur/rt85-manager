//! Fluent localisation. Strings live in `i18n/<lang>/rt85_gui.ftl`.

use i18n_embed::{
    DefaultLocalizer, LanguageLoader, Localizer,
    fluent::{FluentLanguageLoader, fluent_language_loader},
    unic_langid::LanguageIdentifier,
};
use rust_embed::RustEmbed;
use std::sync::LazyLock;

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

pub static LANGUAGE_LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
    let loader: FluentLanguageLoader = fluent_language_loader!();
    loader.load_fallback_language(&Localizations).expect("fallback language loads");
    loader
});

pub fn init(requested: &[LanguageIdentifier]) {
    let localizer = DefaultLocalizer::new(&*LANGUAGE_LOADER, &Localizations);
    if let Err(why) = localizer.select(requested) {
        tracing::warn!("failed to load languages: {why}");
    }
}

#[macro_export]
macro_rules! fl {
    ($id:literal) => {{ i18n_embed_fl::fl!($crate::i18n::LANGUAGE_LOADER, $id) }};
    ($id:literal, $($args:expr),*) => {{ i18n_embed_fl::fl!($crate::i18n::LANGUAGE_LOADER, $id, $($args),*) }};
}
