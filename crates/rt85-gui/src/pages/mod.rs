pub mod device;
pub mod keys;
pub mod lighting;
pub mod screen;

use crate::worker::Job;

/// What a page asks the app to do after handling a message.
pub enum Output<M> {
    None,
    /// Run on the keyboard; the label is shown while it is busy.
    Job(Job, String),
    /// Run an async task whose result comes back to the page.
    Task(cosmic::Task<M>),
}

/// Shared view context.
pub struct Ctx<'a> {
    pub caps: &'a rt85_device::Capabilities,
    pub connected: bool,
    pub busy: bool,
}

/// "Not supported" banner for a section.
pub fn unsupported<'a, M: Clone + 'static>(what: String) -> cosmic::Element<'a, M> {
    cosmic::widget::warning(what).into()
}
