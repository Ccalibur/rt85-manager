mod app;
mod demo;
mod i18n;
mod pages;
mod telemetry;
mod widgets;
mod worker;

fn main() -> cosmic::iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,rt85=info".into()),
        )
        .init();

    let requested = i18n_embed::DesktopLanguageRequester::requested_languages();
    i18n::init(&requested);

    let size = if demo::screenshot_path().is_some() {
        cosmic::iced::Size::new(1500.0, 900.0)
    } else {
        cosmic::iced::Size::new(1100.0, 760.0)
    };
    let settings = cosmic::app::Settings::default()
        .size(size)
        .size_limits(cosmic::iced::Limits::NONE.min_width(640.0).min_height(480.0));
    cosmic::app::run::<app::App>(settings, ())
}
