//! Draws the "live" screen image: now-playing card (cover art and song name on
//! a background taken from the cover) and/or game FPS, at the keyboard
//! screen's resolution.

use super::{colors, fps::Reading, mpris::NowPlaying};
use cosmic_text::{Align, Attrs, Buffer, Color, Ellipsize, EllipsizeHeightLimit, Family, FontSystem, Metrics, Shaping, SwashCache, Weight, Wrap};
use image::{DynamicImage, Rgba, RgbaImage, imageops};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::{LazyLock, Mutex},
};

const WHITE: Rgba<u8> = Rgba([255, 255, 255, 255]);
const DIM: Rgba<u8> = Rgba([200, 200, 210, 255]);

static FONTS: LazyLock<Mutex<(FontSystem, SwashCache)>> =
    LazyLock::new(|| Mutex::new((FontSystem::new(), SwashCache::new())));

/// Last cover art, keyed by URL, with the palette picked from it.
static ART: Mutex<Option<(String, Option<Art>)>> = Mutex::new(None);

#[derive(Clone)]
struct Art {
    image: DynamicImage,
    scheme: colors::Scheme,
}

#[derive(Debug)]
pub struct Card {
    pub rgba: RgbaImage,
    /// Hash of the pixels, to skip uploading an identical image.
    pub hash: u64,
}

pub fn render(w: u32, h: u32, media: Option<&NowPlaying>, fps: Option<&Reading>, accent: [u8; 3]) -> Card {
    let mut img = RgbaImage::from_pixel(w, h, Rgba([16, 17, 20, 255]));
    let accent = Rgba([accent[0], accent[1], accent[2], 255]);
    let mut fonts = FONTS.lock().unwrap_or_else(|e| e.into_inner());
    let (fs, cache) = &mut *fonts;
    let u = |v: f32| (v * w.min(h) as f32 / 162.0).round().max(1.0); // sizes are in RT100-screen pixels
    let pad = u(8.0);

    match (media, fps) {
        (Some(m), fps) => {
            let art = m.art_url.as_deref().and_then(art);
            let scheme = art.as_ref().map_or(colors::FALLBACK, |a| a.scheme);
            gradient(&mut img, scheme.top, scheme.bottom);

            // Cover art plus title: stacked on squarish screens (RT100), side
            // by side on wide ones (RT85, 320x172).
            let wide = w as f32 > h as f32 * 1.3;
            let (x0, y0, side) = if wide {
                let side = (h as f32 - pad * 2.0).max(0.0) as u32;
                (pad as u32, pad as u32, side)
            } else {
                let text_h = u(30.0);
                let side = ((h as f32 - text_h - pad * 2.0).min(w as f32 - pad * 2.0)).max(0.0) as u32;
                ((w - side) / 2, pad as u32, side)
            };
            match &art {
                Some(a) => {
                    let cover = a.image.resize_to_fill(side, side, imageops::FilterType::Lanczos3).to_rgba8();
                    imageops::overlay(&mut img, &cover, x0 as i64, y0 as i64);
                }
                None => {
                    fill(&mut img, x0 as i32, y0 as i32, side, side, Rgba([0, 0, 0, 90]));
                    text(fs, cache, &mut img, "♪", x0 as f32, y0 as f32 + side as f32 * 0.25, side as f32, side as f32 * 0.5, true, DIM, Align::Center);
                }
            }
            if let Some(f) = fps {
                fps_badge(fs, cache, &mut img, f, x0 as f32 + u(4.0), y0 as f32 + u(4.0), u(1.0), accent);
            }

            // Song name only: the artist, play state and progress stay off the
            // card, so the artwork and its colours carry the design.
            if wide {
                let tx = (x0 + side) as f32 + pad * 1.5;
                let px = u(15.0);
                let y = (h as f32 - px * 1.2) / 2.0;
                text(fs, cache, &mut img, &m.title, tx, y, w as f32 - tx - pad, px, true, WHITE, Align::Left);
            } else {
                let px = u(13.0);
                let y = y0 as f32 + side as f32 + u(4.0);
                text(fs, cache, &mut img, &m.title, pad, y, w as f32 - pad * 2.0, px, true, WHITE, Align::Center);
            }
        }
        (None, Some(f)) => {
            let big = u(64.0);
            let y = h as f32 / 2.0 - big * 0.85;
            text(fs, cache, &mut img, &f.fps.to_string(), 0.0, y, w as f32, big, true, accent, Align::Center);
            text(fs, cache, &mut img, "FPS", 0.0, y + big * 1.05, w as f32, u(16.0), true, WHITE, Align::Center);
            text(fs, cache, &mut img, &f.game, pad, y + big * 1.05 + u(22.0), w as f32 - pad * 2.0, u(10.0), false, DIM, Align::Center);
        }
        (None, None) => {
            text(fs, cache, &mut img, "Nothing playing", pad, h as f32 / 2.0 - u(8.0), w as f32 - pad * 2.0, u(12.0), false, DIM, Align::Center);
        }
    }

    let mut hasher = DefaultHasher::new();
    img.as_raw().hash(&mut hasher);
    Card { hash: hasher.finish(), rgba: img }
}

#[allow(clippy::too_many_arguments)]
fn fps_badge(fs: &mut FontSystem, cache: &mut SwashCache, img: &mut RgbaImage, f: &Reading, x: f32, y: f32, u: f32, accent: Rgba<u8>) {
    let label = format!("{} FPS", f.fps);
    let (bw, bh) = ((label.len() as f32 * 6.5 + 10.0) * u, 16.0 * u);
    fill(img, x as i32, y as i32, bw as u32, bh as u32, Rgba([0, 0, 0, 190]));
    text(fs, cache, img, &label, x, y + 2.0 * u, bw, 11.0 * u, true, accent, Align::Center);
}

/// Two-stop vertical gradient over the whole card.
fn gradient(img: &mut RgbaImage, top: [u8; 3], bottom: [u8; 3]) {
    let height = img.height();
    for (y, row) in img.rows_mut().enumerate() {
        let t = if height > 1 { y as f32 / (height - 1) as f32 } else { 0.0 };
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        let c = Rgba([mix(top[0], bottom[0]), mix(top[1], bottom[1]), mix(top[2], bottom[2]), 255]);
        for p in row {
            *p = c;
        }
    }
}

/// Alpha-blended rectangle.
fn fill(img: &mut RgbaImage, x: i32, y: i32, w: u32, h: u32, c: Rgba<u8>) {
    for yy in y.max(0)..(y + h as i32).min(img.height() as i32) {
        for xx in x.max(0)..(x + w as i32).min(img.width() as i32) {
            blend(img.get_pixel_mut(xx as u32, yy as u32), c, c[3]);
        }
    }
}

fn blend(d: &mut Rgba<u8>, c: Rgba<u8>, a: u8) {
    let a = a as u16;
    for i in 0..3 {
        d[i] = ((c[i] as u16 * a + d[i] as u16 * (255 - a)) / 255) as u8;
    }
}

/// One line of text, ellipsized to `width`.
#[allow(clippy::too_many_arguments)]
fn text(
    fs: &mut FontSystem,
    cache: &mut SwashCache,
    img: &mut RgbaImage,
    s: &str,
    x: f32,
    y: f32,
    width: f32,
    size: f32,
    bold: bool,
    color: Rgba<u8>,
    align: Align,
) {
    let mut buf = Buffer::new(fs, Metrics::new(size, size * 1.2));
    let mut b = buf.borrow_with(fs);
    b.set_wrap(Wrap::None);
    b.set_ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1)));
    b.set_size(Some(width), None);
    let attrs = Attrs::new().family(Family::SansSerif).weight(if bold { Weight::BOLD } else { Weight::NORMAL });
    b.set_text(s, &attrs, Shaping::Advanced, Some(align));
    let base = Color::rgba(color[0], color[1], color[2], 255);
    b.draw(cache, base, |px, py, w, h, c| {
        let (gx, gy) = (x as i32 + px, y as i32 + py);
        for yy in gy.max(0)..(gy + h as i32).min(img.height() as i32) {
            for xx in gx.max(0)..(gx + w as i32).min(img.width() as i32) {
                blend(img.get_pixel_mut(xx as u32, yy as u32), Rgba([c.r(), c.g(), c.b(), 255]), c.a());
            }
        }
    });
}

/// Loads cover art (file://, or http(s) through `curl`), caching the last one.
fn art(url: &str) -> Option<Art> {
    let mut slot = ART.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((u, a)) = slot.as_ref()
        && u == url
    {
        return a.clone();
    }
    let loaded = load_art(url).map(|image| {
        let scheme = colors::scheme(&image);
        Art { image, scheme }
    });
    *slot = Some((url.to_owned(), loaded.clone()));
    loaded
}

fn load_art(url: &str) -> Option<DynamicImage> {
    let bytes = if let Some(path) = url.strip_prefix("file://") {
        std::fs::read(percent_decode(path)).ok()?
    } else if url.starts_with("http://") || url.starts_with("https://") {
        let out = std::process::Command::new("curl")
            .args(["-sfL", "--max-time", "5", "--max-filesize", "8000000", url])
            .output()
            .ok()?;
        out.status.success().then_some(out.stdout)?
    } else if url.starts_with('/') {
        std::fs::read(PathBuf::from(url)).ok()?
    } else {
        return None;
    };
    image::load_from_memory(&bytes).map_err(|e| tracing::debug!("cover art {url}: {e}")).ok()
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or(""), 16)
        {
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `RT85_CARD_DIR=dir cargo test -p rt85-gui render_samples -- --ignored`
    /// writes sample cards for a visual check.
    #[test]
    #[ignore]
    fn render_samples() {
        let dir = PathBuf::from(std::env::var("RT85_CARD_DIR").unwrap_or_else(|_| "target/cards".into()));
        std::fs::create_dir_all(&dir).unwrap();
        let media = NowPlaying {
            title: "Lagtrain".into(),
            artist: "inabakumori".into(),
            art_url: std::env::var("RT85_CARD_ART").ok(),
            playing: true,
        };
        let paused = NowPlaying { playing: false, art_url: None, ..media.clone() };
        let fps = Reading { fps: 144, game: "Cyberpunk2077".into() };
        let accent = [99, 208, 223];
        for (name, m, f) in [
            ("media", Some(&media), None),
            ("noart", Some(&paused), None),
            ("both", Some(&media), Some(&fps)),
            ("fps", None, Some(&fps)),
            ("idle", None, None),
        ] {
            let rt85_protocol::royuan::screen::ScreenSpec { width, height, .. } = rt85_protocol::royuan::screen::RT85_SCREEN;
            render(width.into(), height.into(), m, f, accent).rgba.save(dir.join(format!("{name}.png"))).unwrap();
            render(162, 173, m, f, accent).rgba.save(dir.join(format!("{name}-rt100.png"))).unwrap();
        }
    }
}
