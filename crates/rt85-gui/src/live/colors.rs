//! Album-art colour extraction for the live now-playing card.

use image::{DynamicImage, imageops};

/// A dark two-colour background for the card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scheme {
    pub top: [u8; 3],
    pub bottom: [u8; 3],
}

/// Used when the track has no artwork, or the artwork has no usable colour.
pub const FALLBACK: Scheme = Scheme {
    top: [30, 32, 40],
    bottom: [14, 15, 19],
};

const SAMPLE: u32 = 32;
const ITERATIONS: usize = 8;
/// Almost-black pixels are usually background; leave them out so the cover's
/// real colour gets a chance.
const MIN_LUMA: u8 = 24;
/// White title text needs a dark background. This is the simple (gamma-space)
/// luma ceiling the two gradient stops are scaled to.
const MAX_LUMA: f32 = 0.28;

/// Pick the two colours that best represent the cover, darkened for the card.
pub fn scheme(art: &DynamicImage) -> Scheme {
    let small = art.resize_exact(SAMPLE, SAMPLE, imageops::FilterType::Triangle).to_rgba8();
    let mut pixels = Vec::with_capacity((SAMPLE * SAMPLE) as usize);
    for p in small.pixels() {
        if p[3] < 128 {
            continue;
        }
        let max = p[0].max(p[1]).max(p[2]);
        if max < MIN_LUMA {
            continue;
        }
        let min = p[0].min(p[1]).min(p[2]);
        let sat = if max == 0 { 0.0 } else { (max - min) as f32 / max as f32 };
        // Keep neutral pixels, but let saturated ones decide the hue.
        let weight = 0.25 + sat;
        pixels.push(([p[0] as f32, p[1] as f32, p[2] as f32], weight));
    }
    let Some((a, b)) = clusters(&pixels) else { return FALLBACK };
    if dist(a, b) < 28.0 * 28.0 {
        let top = shade(a, 0.62);
        return Scheme { top, bottom: shade(a, 0.30) };
    }
    let (top_col, bottom_col) = if luma(a) >= luma(b) { (a, b) } else { (b, a) };
    let top = shade(top_col, 0.62);
    let bottom = shade(bottom_col, 0.30);
    Scheme { top, bottom }
}

/// Two-means over the sampled pixels. Deterministic, tiny, and good enough for
/// a background gradient.
fn clusters(pixels: &[([f32; 3], f32)]) -> Option<([f32; 3], [f32; 3])> {
    if pixels.is_empty() {
        return None;
    }
    let centre = mean(pixels);
    let mut first = pixels[0].0;
    let mut best = -1.0;
    for &(p, w) in pixels {
        let d = dist(p, centre) * w;
        if d > best {
            best = d;
            first = p;
        }
    }
    let mut second = pixels[0].0;
    best = -1.0;
    for &(p, w) in pixels {
        let d = dist(p, first) * w;
        if d > best {
            best = d;
            second = p;
        }
    }
    let (mut a, mut b) = (first, second);
    for _ in 0..ITERATIONS {
        let (mut sa, mut sb) = ([0.0; 3], [0.0; 3]);
        let (mut na, mut nb) = (0.0, 0.0);
        for &(p, w) in pixels {
            if dist(p, a) <= dist(p, b) {
                add(&mut sa, p, w);
                na += w;
            } else {
                add(&mut sb, p, w);
                nb += w;
            }
        }
        if na > 0.0 {
            a = scale(sa, 1.0 / na);
        }
        if nb > 0.0 {
            b = scale(sb, 1.0 / nb);
        }
    }
    Some((a, b))
}

fn mean(pixels: &[([f32; 3], f32)]) -> [f32; 3] {
    let mut sum = [0.0; 3];
    let mut n = 0.0;
    for &(p, w) in pixels {
        add(&mut sum, p, w);
        n += w;
    }
    scale(sum, 1.0 / n)
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    let (x, y, z) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    x * x + y * y + z * z
}

fn add(dst: &mut [f32; 3], src: [f32; 3], weight: f32) {
    for i in 0..3 {
        dst[i] += src[i] * weight;
    }
}

fn scale(c: [f32; 3], factor: f32) -> [f32; 3] {
    [c[0] * factor, c[1] * factor, c[2] * factor]
}

fn luma(c: [f32; 3]) -> f32 {
    (0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]) / 255.0
}

/// Mix with a dark neutral so dark covers keep a hint of their hue, then cap
/// the brightness so the white title stays readable.
fn shade(c: [f32; 3], factor: f32) -> [u8; 3] {
    let base = [12.0, 13.0, 16.0];
    let mut mixed = [
        base[0] + (c[0] - base[0]) * factor,
        base[1] + (c[1] - base[1]) * factor,
        base[2] + (c[2] - base[2]) * factor,
    ];
    let l = luma(mixed);
    if l > MAX_LUMA {
        let f = MAX_LUMA / l;
        for v in &mut mixed {
            *v *= f;
        }
    }
    mixed.map(|v| v.round().clamp(0.0, 255.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    fn image(colours: &[[u8; 3]]) -> DynamicImage {
        let mut img = RgbImage::new(32, 32);
        for (i, p) in img.pixels_mut().enumerate() {
            *p = Rgb(colours[i % colours.len()]);
        }
        DynamicImage::ImageRgb8(img)
    }

    #[test]
    fn picks_two_dark_colours_from_the_cover() {
        let s = scheme(&image(&[[220, 40, 40], [40, 40, 180]]));
        assert_ne!(s.top, s.bottom);
        assert!(luma([s.top[0] as f32, s.top[1] as f32, s.top[2] as f32]) <= MAX_LUMA + 0.01);
        assert!(luma([s.bottom[0] as f32, s.bottom[1] as f32, s.bottom[2] as f32]) <= MAX_LUMA + 0.01);
        // Red is lighter than the blue, so it becomes the top stop.
        assert!(s.top[0] > s.top[2]);
        assert!(s.bottom[2] > s.bottom[0]);
    }

    #[test]
    fn grayscale_and_black_covers_fall_back_gracefully() {
        let grey = scheme(&image(&[[128, 128, 128]]));
        assert_ne!(grey.top, grey.bottom);

        let black = scheme(&image(&[[0, 0, 0]]));
        assert_eq!(black, FALLBACK);
    }
}
