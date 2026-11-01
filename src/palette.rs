//! Palette from an image: turn a handful of sampled pixels into a window-frame look.
//!
//! Local and bounded by construction. The platform layer reads at most [`GRID`] x [`GRID`]
//! pixels from an image that passed [`check_image`]; nothing here sees a file, a path or the
//! network. The result is a *frame* look (title bar, border, text): it is built to stay
//! readable, not to reproduce the picture, and `derive` guarantees the contrast numbers below
//! for any input it accepts.
//!
//! Guarantees (each covered by a test, including a pseudo-random sweep):
//!  * text on title bar is at least 7:1,
//!  * the border is at least 3:1 against the title bar (so it is visible),
//!  * the title bar is dark (near-black with a hint of the image's hue),
//!  * empty input is an error, never a made-up palette.

use crate::legacy_policy::contrast_ratio;
use crate::recipes::{parse_recipe, Recipe};
use crate::scenes::Scene;

/// Samples are taken on at most this many points per side.
pub const GRID: u32 = 64;
/// Hard cap on samples that `derive` will look at.
pub const MAX_SAMPLES: usize = (GRID * GRID) as usize;
pub const MAX_FILE_BYTES: u64 = 25 * 1024 * 1024;
pub const MAX_SIDE: u32 = 16_384;
/// Decoding a bitmap costs about four bytes per pixel; this keeps that near 100 MB.
pub const MAX_PIXELS: u64 = 24_000_000;

/// Refuse images that would be slow or large to decode, before decoding anything.
pub fn check_image(width: u32, height: u32, file_bytes: u64) -> Result<(), String> {
    if width == 0 || height == 0 {
        return Err("The image has no size.".into());
    }
    if file_bytes > MAX_FILE_BYTES {
        return Err(format!(
            "The image file is larger than {} MB.",
            MAX_FILE_BYTES / 1024 / 1024
        ));
    }
    if width > MAX_SIDE || height > MAX_SIDE {
        return Err(format!(
            "The image is wider or taller than {MAX_SIDE} pixels."
        ));
    }
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err("The image has too many pixels to read safely. Use a smaller copy.".into());
    }
    Ok(())
}

/// Evenly spaced pixel positions inside the image, at most `GRID` per side.
pub fn sample_points(width: u32, height: u32) -> Vec<(u32, u32)> {
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let across = width.min(GRID);
    let down = height.min(GRID);
    let mut points = Vec::with_capacity((across * down) as usize);
    for row in 0..down {
        let y = ((u64::from(row) * 2 + 1) * u64::from(height) / (u64::from(down) * 2)) as u32;
        for column in 0..across {
            let x =
                ((u64::from(column) * 2 + 1) * u64::from(width) / (u64::from(across) * 2)) as u32;
            points.push((x.min(width - 1), y.min(height - 1)));
        }
    }
    points
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Hsv {
    h: f64,
    s: f64,
    v: f64,
}

fn split(color: u32) -> (f64, f64, f64) {
    (
        f64::from(color & 0xFF),
        f64::from((color >> 8) & 0xFF),
        f64::from((color >> 16) & 0xFF),
    )
}

fn to_hsv(color: u32) -> Hsv {
    let (r, g, b) = split(color);
    let (r, g, b) = (r / 255.0, g / 255.0, b / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    Hsv {
        h: if h < 0.0 { h + 360.0 } else { h },
        s: if max == 0.0 { 0.0 } else { delta / max },
        v: max,
    }
}

fn from_hsv(hsv: Hsv) -> u32 {
    let s = hsv.s.clamp(0.0, 1.0);
    let v = hsv.v.clamp(0.0, 1.0);
    let c = v * s;
    let h = (hsv.h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let channel = |value: f64| ((value + m) * 255.0).round().clamp(0.0, 255.0) as u32;
    channel(r1) | (channel(g1) << 8) | (channel(b1) << 16)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    /// COLORREF values (0x00BBGGRR).
    pub caption: u32,
    pub border: u32,
    pub text: u32,
    /// The color taken from the image that the others were built around.
    pub source: u32,
    /// Set when the image offered little to work with.
    pub note: Option<&'static str>,
}

pub fn hex(color: u32) -> String {
    let (r, g, b) = split(color);
    format!("#{:02X}{:02X}{:02X}", r as u32, g as u32, b as u32)
}

const NEUTRAL_ACCENT: u32 = 0x009A_9A9A;

/// Pick the most characteristic strong color: buckets weighted by how common and how colorful.
fn pick_source(samples: &[u32]) -> Option<u32> {
    // 4 bits per channel -> 4096 buckets.
    let mut counts = vec![0u32; 4096];
    let mut sums = vec![[0u64; 3]; 4096];
    for &color in samples {
        let (r, g, b) = split(color);
        let index = (((r as usize) >> 4) << 8) | (((g as usize) >> 4) << 4) | ((b as usize) >> 4);
        counts[index] += 1;
        sums[index][0] += r as u64;
        sums[index][1] += g as u64;
        sums[index][2] += b as u64;
    }
    let total = samples.len() as f64;
    let mut best: Option<(f64, u32)> = None;
    for (index, &count) in counts.iter().enumerate() {
        if count == 0 {
            continue;
        }
        let n = u64::from(count);
        let mean = ((sums[index][0] / n) as u32)
            | (((sums[index][1] / n) as u32) << 8)
            | (((sums[index][2] / n) as u32) << 16);
        let hsv = to_hsv(mean);
        // Needs to be visibly colored and not nearly black.
        if hsv.s < 0.25 || hsv.v < 0.25 {
            continue;
        }
        let score = (f64::from(count) / total) * (0.25 + hsv.s) * (0.5 + hsv.v);
        // Ties go to the lower bucket index so the result never depends on hash order.
        if best.map_or(true, |(top, _)| score > top) {
            best = Some((score, mean));
        }
    }
    best.map(|(_, color)| color)
}

/// Build a readable frame palette from sampled pixels (COLORREF each).
pub fn derive(samples: &[u32]) -> Result<Palette, String> {
    if samples.is_empty() {
        return Err("No pixels could be read from that image.".into());
    }
    let samples = &samples[..samples.len().min(MAX_SAMPLES)];
    let (source, note) = match pick_source(samples) {
        Some(color) => (color, None),
        None => (
            NEUTRAL_ACCENT,
            Some("The image has little color, so the border is neutral gray."),
        ),
    };
    let hsv = to_hsv(source);
    // Title bar: near-black with a hint of the image's hue.
    let caption = from_hsv(Hsv {
        h: hsv.h,
        s: (hsv.s * 0.6).min(0.55),
        v: 0.07,
    });
    // Border: the image color, lifted until it stands out from the title bar.
    let mut border_hsv = Hsv {
        h: hsv.h,
        s: hsv.s.max(0.35).min(1.0),
        v: hsv.v.max(0.6),
    };
    if note.is_some() {
        border_hsv.s = 0.0;
    }
    let mut border = from_hsv(border_hsv);
    while contrast_ratio(border, caption) < 3.0 && border_hsv.v < 1.0 {
        border_hsv.v = (border_hsv.v + 0.05).min(1.0);
        border = from_hsv(border_hsv);
    }
    if contrast_ratio(border, caption) < 3.0 {
        border = 0x00FF_FFFF;
    }
    // Text: a very light tint of the hue, white if that ever fails the bar.
    let mut text = from_hsv(Hsv {
        h: hsv.h,
        s: if note.is_some() { 0.0 } else { 0.06 },
        v: 0.96,
    });
    if contrast_ratio(text, caption) < 7.0 {
        text = 0x00FF_FFFF;
    }
    Ok(Palette {
        caption,
        border,
        text,
        source,
        note,
    })
}

/// A palette as a scene, validated by the same parser that checks imported recipes.
pub fn to_scene(palette: &Palette, name: &str) -> Result<Scene, String> {
    let json = format!(
        r##"{{"schema":1,"id":"from-image","name":{name},"description":"Built from a picture on this PC.","author":"AllBlack11","look":{{"darkMode":true,"backdrop":"none","captionColor":"{c}","borderColor":"{b}","textColor":"{t}","corners":"default"}},"notes":["Made from {s}. Frames only."]}}"##,
        name = serde_json::to_string(name).map_err(|e| e.to_string())?,
        c = hex(palette.caption),
        b = hex(palette.border),
        t = hex(palette.text),
        s = hex(palette.source),
    );
    let recipe: Recipe = parse_recipe(&json)?;
    Ok(Scene {
        id: "from-image".to_string(),
        name: name.to_string(),
        blurb: "A frame look built from a picture. Not saved until you save it.".to_string(),
        limit: "Frames only. The picture is not kept.".to_string(),
        recipe,
        focus: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(r: u32, g: u32, b: u32) -> u32 {
        r | (g << 8) | (b << 16)
    }

    fn check(palette: &Palette) {
        assert!(
            contrast_ratio(palette.text, palette.caption) >= 7.0,
            "text {} on {}",
            hex(palette.text),
            hex(palette.caption)
        );
        assert!(
            contrast_ratio(palette.border, palette.caption) >= 3.0,
            "border {} on {}",
            hex(palette.border),
            hex(palette.caption)
        );
        assert!(
            to_hsv(palette.caption).v <= 0.10,
            "title bar must stay dark"
        );
    }

    #[test]
    fn a_blue_picture_gives_a_dark_blue_tinted_bar_and_readable_text() {
        let mut samples = vec![rgb(10, 60, 200); 300];
        samples.extend(vec![rgb(5, 5, 20); 100]);
        let palette = derive(&samples).unwrap();
        check(&palette);
        let source = to_hsv(palette.source);
        assert!((source.h - 224.0).abs() < 20.0, "hue {}", source.h);
        assert!(palette.note.is_none());
    }

    #[test]
    fn bright_and_pale_pictures_still_come_out_readable() {
        for color in [
            rgb(255, 255, 0),
            rgb(250, 250, 250),
            rgb(255, 200, 200),
            rgb(0, 255, 255),
        ] {
            check(&derive(&vec![color; 64]).unwrap());
        }
    }

    #[test]
    fn a_gray_or_black_picture_gets_a_neutral_border_and_says_so() {
        for color in [rgb(0, 0, 0), rgb(128, 128, 128), rgb(255, 255, 255)] {
            let palette = derive(&vec![color; 100]).unwrap();
            check(&palette);
            assert!(palette.note.is_some());
            assert_eq!(to_hsv(palette.border).s, 0.0);
        }
    }

    #[test]
    fn nothing_in_means_an_error_not_an_invented_palette() {
        assert!(derive(&[]).is_err());
    }

    #[test]
    fn only_a_bounded_number_of_samples_is_used() {
        let mut samples = vec![rgb(200, 30, 30); MAX_SAMPLES];
        // Everything after the cap is ignored, so a flood of green changes nothing.
        samples.extend(vec![rgb(30, 200, 30); MAX_SAMPLES * 10]);
        let palette = derive(&samples).unwrap();
        assert!(to_hsv(palette.source).h < 20.0 || to_hsv(palette.source).h > 340.0);
    }

    #[test]
    fn the_same_pixels_always_give_the_same_palette() {
        let samples: Vec<u32> = (0..500)
            .map(|i| rgb(i % 255, (i * 7) % 255, (i * 13) % 255))
            .collect();
        assert_eq!(derive(&samples).unwrap(), derive(&samples).unwrap());
    }

    #[test]
    fn a_pseudo_random_sweep_never_breaks_the_contrast_guarantees() {
        let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as u32
        };
        for _ in 0..400 {
            let count = 1 + (next() % 60) as usize;
            let samples: Vec<u32> = (0..count).map(|_| next() & 0x00FF_FFFF).collect();
            let palette = derive(&samples).unwrap();
            check(&palette);
        }
    }

    #[test]
    fn image_limits_refuse_what_would_be_slow_or_large() {
        assert!(check_image(1920, 1080, 500_000).is_ok());
        assert!(check_image(0, 10, 1).is_err());
        assert!(check_image(10, 0, 1).is_err());
        assert!(check_image(1000, 1000, MAX_FILE_BYTES + 1).is_err());
        assert!(check_image(MAX_SIDE + 1, 10, 1).is_err());
        assert!(
            check_image(8000, 6000, 1_000_000).is_err(),
            "48 MP is over the pixel cap"
        );
        assert!(
            check_image(6000, 4000, 1_000_000).is_ok(),
            "24 MP is allowed"
        );
    }

    #[test]
    fn sample_points_stay_inside_the_image_and_are_bounded() {
        for (w, h) in [
            (1, 1),
            (1, 5000),
            (5000, 1),
            (63, 63),
            (64, 64),
            (4000, 3000),
            (7, 3),
        ] {
            let points = sample_points(w, h);
            assert!(!points.is_empty() && points.len() <= MAX_SAMPLES, "{w}x{h}");
            assert!(points.iter().all(|&(x, y)| x < w && y < h), "{w}x{h}");
        }
        assert!(sample_points(0, 10).is_empty());
        assert_eq!(sample_points(1, 1), vec![(0, 0)]);
    }

    #[test]
    fn the_result_becomes_a_valid_scene_that_the_recipe_parser_accepts() {
        let palette = derive(&vec![rgb(200, 80, 20); 50]).unwrap();
        let scene = to_scene(&palette, "Sunset \"quoted\"").unwrap();
        assert_eq!(scene.name, "Sunset \"quoted\"");
        assert!(scene.recipe.matcher.is_none());
        // A name that would break the recipe is refused rather than mangled.
        assert!(to_scene(&palette, &"x".repeat(200)).is_err());
        assert!(to_scene(&palette, "bad\u{7}name").is_err());
    }
}
