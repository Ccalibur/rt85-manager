//! "Live screen": now-playing media and game FPS rendered onto the keyboard's
//! screen.

pub mod card;
pub mod fps;
pub mod mpris;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Off,
    Media,
    Fps,
    Both,
}

impl Mode {
    pub const ALL: [Mode; 4] = [Mode::Off, Mode::Media, Mode::Fps, Mode::Both];

    pub fn media(self) -> bool {
        matches!(self, Mode::Media | Mode::Both)
    }

    pub fn fps(self) -> bool {
        matches!(self, Mode::Fps | Mode::Both)
    }
}

/// One poll of the system.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub media: Option<mpris::NowPlaying>,
    pub media_error: Option<String>,
    pub fps: Option<fps::Reading>,
    pub fps_status: Option<fps::Status>,
}

pub async fn poll(want_media: bool, want_fps: bool) -> Snapshot {
    let mut snap = Snapshot::default();
    if want_media {
        match mpris::poll().await {
            Ok(m) => snap.media = m,
            Err(e) => snap.media_error = Some(e),
        }
    }
    if want_fps {
        let (reading, status) = tokio::task::spawn_blocking(|| (fps::read(), fps::status())).await.unwrap_or((None, fps::Status::NotInstalled));
        snap.fps = reading;
        snap.fps_status = Some(status);
    }
    snap
}
