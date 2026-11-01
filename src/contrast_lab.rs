//! Contrast Lab: measure the colors a look actually uses, say plainly what fails, and offer the
//! smallest change that fixes it.
//!
//! Pure. It measures what AllBlack11 chooses (title bar text on the title bar, the border on the
//! title bar, and every pair in the classic-dialog palette). It does not claim to measure what
//! Windows or an app draws by itself: hover, focus and selection colors of classic controls come
//! from Windows, so they are named as not measured instead of being guessed.
//!
//! Thresholds follow WCAG 2.1: 4.5:1 for text and 3:1 for non-text boundaries. The classic
//! palette's disabled text is held to 3:1, which is stricter than WCAG (it exempts disabled
//! controls) and is this product's own promise.

use crate::config::{Color, Style};
use crate::legacy_policy::{contrast_ratio, Palette};

pub const TEXT: f64 = 4.5;
pub const NON_TEXT: f64 = 3.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Pair {
    pub name: String,
    /// COLORREF (0x00BBGGRR).
    pub fg: u32,
    pub bg: u32,
    pub need: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub name: String,
    pub ratio: f64,
    pub need: f64,
    pub pass: bool,
    /// A corrected foreground that keeps the hue and passes, when the pair fails.
    pub suggestion: Option<u32>,
}

fn colorref(r: u8, g: u8, b: u8) -> u32 {
    u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16)
}

pub fn hex(color: u32) -> String {
    format!(
        "#{:02X}{:02X}{:02X}",
        color & 0xFF,
        (color >> 8) & 0xFF,
        (color >> 16) & 0xFF
    )
}

/// The pairs of a window look. Skips what cannot be measured: a title bar left to Windows
/// (`Default`) or drawn with no color (`None`) has no color of ours to compare.
pub fn style_pairs(style: &Style, accent_bgr: u32) -> Vec<Pair> {
    let component = |color: Color| {
        color
            .components(accent_bgr)
            .map(|(r, g, b)| colorref(r, g, b))
    };
    let mut pairs = Vec::new();
    let Some(caption) = component(style.caption) else {
        return pairs;
    };
    if let Some(text) = component(style.text) {
        pairs.push(Pair {
            name: "Title bar text on the title bar".into(),
            fg: text,
            bg: caption,
            need: TEXT,
        });
    }
    if let Some(border) = component(style.border) {
        pairs.push(Pair {
            name: "Border against the title bar".into(),
            fg: border,
            bg: caption,
            need: NON_TEXT,
        });
    }
    pairs
}

/// Every pair in the classic-dialog palette.
pub fn palette_pairs(palette: &Palette) -> Vec<Pair> {
    let p = |name: &str, fg: u32, bg: u32, need: f64| Pair {
        name: name.to_string(),
        fg,
        bg,
        need,
    };
    vec![
        p("Dialog text", palette.text, palette.bg, TEXT),
        p("Dialog headings", palette.heading, palette.bg, TEXT),
        p("Dialog links", palette.link, palette.bg, TEXT),
        p("Disabled text", palette.disabled_text, palette.bg, NON_TEXT),
        p("Text in edit boxes", palette.text, palette.edit_bg, TEXT),
        p(
            "Disabled text in edit boxes",
            palette.disabled_text,
            palette.edit_bg,
            NON_TEXT,
        ),
        p(
            "Text in read-only boxes",
            palette.text,
            palette.readonly_bg,
            TEXT,
        ),
    ]
}

pub fn check(pairs: &[Pair]) -> Vec<Finding> {
    pairs
        .iter()
        .map(|pair| {
            let ratio = contrast_ratio(pair.fg, pair.bg);
            let pass = ratio >= pair.need;
            Finding {
                name: pair.name.clone(),
                ratio,
                need: pair.need,
                pass,
                suggestion: if pass {
                    None
                } else {
                    suggest(pair.fg, pair.bg, pair.need)
                },
            }
        })
        .collect()
}

/// The closest color to `fg` that reaches `need` against `bg`, found by moving `fg` toward white
/// on a dark background and toward black on a light one, so its hue and character stay. None only
/// if even pure white or black cannot reach the target.
pub fn suggest(fg: u32, bg: u32, need: f64) -> Option<u32> {
    let toward: u32 = if crate::legacy_policy::luminance(bg) < 0.18 {
        0x00FF_FFFF
    } else {
        0
    };
    // Aim a little above the target so rounding to 8-bit channels cannot land just under it.
    let aim = need + 0.05;
    for step in 0..=50u32 {
        let t = f64::from(step) / 50.0;
        let mix = |shift: u32| {
            let a = f64::from((fg >> shift) & 0xFF);
            let b = f64::from((toward >> shift) & 0xFF);
            (a + (b - a) * t).round().clamp(0.0, 255.0) as u32
        };
        let candidate = mix(0) | (mix(8) << 8) | (mix(16) << 16);
        if contrast_ratio(candidate, bg) >= aim {
            return Some(candidate);
        }
    }
    None
}

/// Lines for the page: a word first (never color alone), then the numbers, then the fix.
pub fn lines(findings: &[Finding]) -> Vec<String> {
    findings
        .iter()
        .map(|f| {
            let head = if f.pass { "Passes" } else { "Fails" };
            let mut line = format!(
                "{head}: {} is {:.1}:1 (needs {:.1}:1).",
                f.name, f.ratio, f.need
            );
            if let Some(fix) = f.suggestion {
                line.push_str(&format!(
                    " Try {} instead, which is the nearest color that reaches it.",
                    hex(fix)
                ));
            } else if !f.pass {
                line.push_str(" No color reaches it on this background; change the background.");
            }
            line
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{base_style, Color};
    use crate::legacy_policy::PALETTES;

    fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color::Rgb { r, g, b }
    }

    #[test]
    fn the_shipped_look_and_both_classic_palettes_pass_their_own_thresholds() {
        let style = base_style();
        let findings = check(&style_pairs(&style, 0x00B0_8000));
        assert!(!findings.is_empty());
        assert!(findings.iter().all(|f| f.pass), "{findings:?}");
        for palette in PALETTES {
            let findings = check(&palette_pairs(&palette));
            assert_eq!(findings.len(), 7);
            assert!(
                findings.iter().all(|f| f.pass),
                "{}: {findings:?}",
                palette.name
            );
        }
    }

    #[test]
    fn low_contrast_text_fails_and_the_fix_passes_and_keeps_its_hue() {
        let mut style = base_style();
        style.caption = rgb(0x10, 0x10, 0x10);
        style.text = rgb(0x30, 0x40, 0x60); // a dim bluish text on near black
        let findings = check(&style_pairs(&style, 0));
        let text = &findings[0];
        assert!(!text.pass, "{text:?}");
        let fix = text.suggestion.expect("a fix exists");
        assert!(contrast_ratio(fix, colorref(0x10, 0x10, 0x10)) >= TEXT);
        // Moved toward white, still bluer than red (the intent survives).
        let (r, b) = (fix & 0xFF, (fix >> 16) & 0xFF);
        assert!(b > r, "{}", hex(fix));
        assert!(fix & 0xFF > 0x30, "it got lighter");
    }

    #[test]
    fn a_light_background_is_fixed_by_darkening() {
        let fix = suggest(0x00C0_C0C0, 0x00F0_F0F0, TEXT).unwrap();
        assert!(contrast_ratio(fix, 0x00F0_F0F0) >= TEXT);
        assert!(fix & 0xFF < 0xC0, "darker than the original");
    }

    #[test]
    fn colors_left_to_windows_are_not_measured() {
        let mut style = base_style();
        style.caption = Color::Default;
        assert!(style_pairs(&style, 0).is_empty());
        style.caption = rgb(0, 0, 0);
        style.border = Color::None;
        let pairs = style_pairs(&style, 0);
        assert_eq!(pairs.len(), 1, "text only; no border to measure");
    }

    #[test]
    fn the_accent_border_is_measured_with_the_users_accent() {
        let mut style = base_style();
        style.caption = rgb(0, 0, 0);
        style.border = Color::Accent;
        let dark_accent = 0x0010_0000; // almost black
        let findings = check(&style_pairs(&style, dark_accent));
        let border = findings
            .iter()
            .find(|f| f.name.starts_with("Border"))
            .unwrap();
        assert!(!border.pass);
        assert!(border.suggestion.is_some());
        assert!(lines(&findings)
            .iter()
            .any(|l| l.starts_with("Fails: Border")));
    }

    #[test]
    fn lines_always_start_with_a_word_and_carry_the_numbers() {
        let findings = check(&palette_pairs(&PALETTES[0]));
        for line in lines(&findings) {
            assert!(
                line.starts_with("Passes:") || line.starts_with("Fails:"),
                "{line}"
            );
            assert!(line.contains(":1 (needs"), "{line}");
        }
    }

    #[test]
    fn an_impossible_target_has_no_suggestion_instead_of_a_wrong_one() {
        // Mid gray background: neither white nor black reaches 21:1.
        assert_eq!(suggest(0x0080_8080, 0x0080_8080, 21.0), None);
    }
}
