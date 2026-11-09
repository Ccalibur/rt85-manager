//! What's playing, from MPRIS players on the session bus (browsers, Spotify,
//! media players, ...).

use std::{collections::HashMap, sync::OnceLock};
use zbus::{Connection, proxy::CacheProperties, zvariant::OwnedValue};

#[derive(Debug, Clone, PartialEq)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: Option<String>,
    pub playing: bool,
}

impl NowPlaying {
    /// Identifies the track (title and artist); a change uploads straight
    /// away. Pausing or resuming on its own doesn't count.
    pub fn key(&self) -> (String, String) {
        (self.title.clone(), self.artist.clone())
    }
}

static CONN: OnceLock<Connection> = OnceLock::new();

async fn conn() -> zbus::Result<&'static Connection> {
    if let Some(c) = CONN.get() {
        return Ok(c);
    }
    let c = Connection::session().await?;
    Ok(CONN.get_or_init(|| c))
}

/// The most relevant player: playing beats paused, then the one with artwork
/// (Firefox and Plasma's browser integration both report the same video, but
/// only the latter has artwork).
pub async fn poll() -> Result<Option<NowPlaying>, String> {
    let conn = conn().await.map_err(|e| e.to_string())?;
    let dbus = zbus::fdo::DBusProxy::new(conn).await.map_err(|e| e.to_string())?;
    let names = dbus.list_names().await.map_err(|e| e.to_string())?;

    let mut best: Option<(u8, NowPlaying)> = None;
    for name in names.iter().filter(|n| n.starts_with("org.mpris.MediaPlayer2.")) {
        let Ok(Some(np)) = player(conn, name.as_str()).await else { continue };
        let score = (np.playing as u8) * 4 + np.art_url.is_some() as u8;
        if best.as_ref().is_none_or(|(s, _)| score > *s) {
            best = Some((score, np));
        }
    }
    Ok(best.map(|(_, np)| np))
}

async fn player(conn: &Connection, name: &str) -> zbus::Result<Option<NowPlaying>> {
    let proxy: zbus::Proxy = zbus::proxy::Builder::new(conn)
        .destination(name.to_owned())?
        .path("/org/mpris/MediaPlayer2")?
        .interface("org.mpris.MediaPlayer2.Player")?
        .cache_properties(CacheProperties::No)
        .build()
        .await?;
    let status: String = proxy.get_property("PlaybackStatus").await?;
    if status == "Stopped" {
        return Ok(None);
    }
    let meta: HashMap<String, OwnedValue> = proxy.get_property("Metadata").await?;
    let text = |k: &str| meta.get(k).and_then(|v| String::try_from(v.clone()).ok()).filter(|s| !s.is_empty());
    let Some(title) = text("xesam:title") else { return Ok(None) };
    let artist = meta
        .get("xesam:artist")
        .and_then(|v| Vec::<String>::try_from(v.clone()).ok())
        .map(|a| a.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", "))
        .unwrap_or_default();
    Ok(Some(NowPlaying {
        title,
        artist,
        album: text("xesam:album").unwrap_or_default(),
        art_url: text("mpris:artUrl"),
        playing: status == "Playing",
    }))
}
