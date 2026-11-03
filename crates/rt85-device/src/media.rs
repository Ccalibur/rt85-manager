//! Load still images / GIFs from disk and fit them to the screen.

use image::{AnimationDecoder, DynamicImage, RgbaImage, imageops::FilterType};
use rt85_protocol::royuan::screen::{self, Animation, ScreenSpec};
use std::path::Path;

/// How a picture is fitted to the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fit {
    /// Scale to fill, cropping the overflow.
    #[default]
    Cover,
    /// Scale to fit, letterboxing with black.
    Contain,
    /// Ignore the aspect ratio.
    Stretch,
}

/// Target frame rate when downsampling long GIFs.
const GIF_FPS: f32 = 15.0;

pub fn fit(img: &DynamicImage, w: u32, h: u32, mode: Fit) -> RgbaImage {
    match mode {
        Fit::Stretch => img.resize_exact(w, h, FilterType::Lanczos3).to_rgba8(),
        Fit::Cover => img.resize_to_fill(w, h, FilterType::Lanczos3).to_rgba8(),
        Fit::Contain => {
            let scaled = img.resize(w, h, FilterType::Lanczos3).to_rgba8();
            let mut canvas = RgbaImage::from_pixel(w, h, image::Rgba([0, 0, 0, 255]));
            let x = (w - scaled.width()) / 2;
            let y = (h - scaled.height()) / 2;
            image::imageops::overlay(&mut canvas, &scaled, x.into(), y.into());
            canvas
        }
    }
}

/// Still image → one-frame [`Animation`] at the full screen size.
pub fn still(path: &Path, spec: ScreenSpec, mode: Fit) -> crate::Result<Animation> {
    let img = image::open(path)?;
    Ok(still_from(&img, spec, mode))
}

pub fn still_from(img: &DynamicImage, spec: ScreenSpec, mode: Fit) -> Animation {
    let rgba = fit(img, spec.width.into(), spec.height.into(), mode);
    Animation {
        width: spec.width,
        height: spec.height,
        delay_ms: 0,
        frames: vec![screen::encode_rgba(rgba.as_raw(), spec.width, spec.height)],
    }
}

/// Animation frame size. Boards with `animation_align` 1 (RT85) take
/// full-screen frames like stills; others get the largest aligned size that
/// fits the screen with the source's aspect ratio.
pub fn animation_size(src_w: u32, src_h: u32, spec: ScreenSpec) -> (u16, u16) {
    if spec.animation_align <= 1 {
        return (spec.width, spec.height);
    }
    let a = spec.animation_align as f32;
    let ratio = (spec.width as f32 / src_w as f32).min(spec.height as f32 / src_h as f32);
    let align = |v: f32| ((v / a).floor() * a).max(a) as u16;
    (align(src_w as f32 * ratio), align(src_h as f32 * ratio))
}

/// Animated GIF → [`Animation`]. Frames are composited by the decoder, and
/// long GIFs are downsampled to the screen's `max_frames`.
pub fn gif(path: &Path, spec: ScreenSpec, mode: Fit) -> crate::Result<Animation> {
    let file = std::io::BufReader::new(std::fs::File::open(path)?);
    let decoder = image::codecs::gif::GifDecoder::new(file)?;
    let frames = decoder.into_frames().collect_frames()?;
    if frames.len() <= 1 {
        let img = frames
            .into_iter()
            .next()
            .map(|f| DynamicImage::ImageRgba8(f.into_buffer()))
            .ok_or(rt85_protocol::ProtocolError::OutOfRange { what: "frame count", value: 0, max: 0 })?;
        return Ok(still_from(&img, spec, mode));
    }
    let (sw, sh) = frames[0].buffer().dimensions();
    let (w, h) = animation_size(sw, sh, spec);
    let step = (frames.len() as f32 / spec.max_frames.max(1) as f32).max(1.0);
    let count = ((frames.len() as f32 / step).floor() as usize).max(1);
    let delay_ms = (1000.0 / GIF_FPS * step).min(255.0) as u8;
    let encoded = (0..count)
        .map(|i| {
            let f = &frames[(i as f32 * step) as usize];
            let img = DynamicImage::ImageRgba8(f.buffer().clone());
            let rgba = fit(&img, w.into(), h.into(), mode);
            screen::encode_rgba(rgba.as_raw(), w, h)
        })
        .collect();
    Ok(Animation { width: w, height: h, delay_ms, frames: encoded })
}

/// Pick [`gif`] or [`still`] from the file extension.
pub fn load(path: &Path, spec: ScreenSpec, mode: Fit) -> crate::Result<Animation> {
    let is_gif = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("gif"));
    if is_gif { gif(path, spec, mode) } else { still(path, spec, mode) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rt85_protocol::royuan::screen::{RT85_SCREEN, RT100_SCREEN};

    #[test]
    fn animation_sizes_are_aligned() {
        assert_eq!(animation_size(480, 480, RT100_SCREEN), (128, 128));
        assert_eq!(animation_size(1920, 1080, RT100_SCREEN), (128, 64));
        let (w, h) = animation_size(10, 1000, RT100_SCREEN);
        assert_eq!((w % 64, h % 64), (0, 0));
        // RT85 animations are full-screen frames.
        assert_eq!(animation_size(480, 480, RT85_SCREEN), (320, 172));
    }
}
