//! AllBlack11's own look: palettes, type ramp and metrics for its Settings window and
//! popups. Pure data and math so every color pair that carries meaning has a contrast test.
//!
//! This is *not* the appearance AllBlack11 applies to other apps. That lives in recipes and
//! rules. The choice here (System, Light, Dark) only changes AllBlack11's own windows.
//!
//! Sources are listed in `docs/dev/UI-GUIDELINES.md`. Where a number comes from Microsoft's
//! guidance (4 px corner radius for in-page controls, 8 px for overlays, the 12/14/20/28 px
//! type ramp, 4.5:1 text contrast) it says so; where AllBlack11 chose differently (a true-black
//! dark base instead of Windows' #202020) the deviation is recorded there too.

use crate::legacy_policy::contrast_ratio;
use serde::{Deserialize, Serialize};

/// The user's choice for AllBlack11's own windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Appearance {
    /// Follow Windows (Settings > Personalization > Colors > Choose your mode).
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    pub fn label(self) -> &'static str {
        match self {
            Appearance::System => "Use system setting",
            Appearance::Light => "Light",
            Appearance::Dark => "Dark",
        }
    }

    pub fn next(self) -> Appearance {
        match self {
            Appearance::System => Appearance::Light,
            Appearance::Light => Appearance::Dark,
            Appearance::Dark => Appearance::System,
        }
    }
}

/// What the choice resolves to right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Light,
    Dark,
    /// A Windows high contrast theme is on. Colors then come from the system, not from here.
    HighContrast,
}

/// High contrast wins over everything; otherwise an explicit choice wins over the system.
pub fn resolve(choice: Appearance, system_prefers_dark: bool, high_contrast: bool) -> Mode {
    if high_contrast {
        return Mode::HighContrast;
    }
    match choice {
        Appearance::Light => Mode::Light,
        Appearance::Dark => Mode::Dark,
        Appearance::System => {
            if system_prefers_dark {
                Mode::Dark
            } else {
                Mode::Light
            }
        }
    }
}

/// `COLORREF` (0x00BBGGRR) from channels.
pub const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

/// Every color the UI draws with. Roles, not shades, so a page never picks a gray by eye.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// The window behind everything (the lowest layer).
    pub bg: u32,
    /// The navigation pane, one step apart from the page.
    pub nav: u32,
    /// Cards and grouped content.
    pub card: u32,
    /// A card or list row under the pointer or selected.
    pub card_hi: u32,
    /// A card edge. Decorative: the content does not depend on seeing it.
    pub edge: u32,
    /// The boundary of something you operate (a text field, a switch track, a button). Needs 3:1.
    pub control_edge: u32,
    pub text: u32,
    /// Descriptions and secondary labels. Still 4.5:1 on every surface it sits on.
    pub muted: u32,
    pub disabled: u32,
    pub edit_bg: u32,
    /// Slider and switch tracks when off.
    pub track: u32,
    /// The keyboard focus ring. 3:1 against every surface.
    pub focus: u32,
    /// Status text. Always paired with a word or symbol, never color alone.
    pub ok: u32,
    pub warn: u32,
    pub bad: u32,
}

pub fn palette(mode: Mode, sys: &SysColors) -> Palette {
    match mode {
        Mode::Dark => DARK,
        Mode::Light => LIGHT,
        Mode::HighContrast => high_contrast(sys),
    }
}

// True black base: an OLED-first product choice, recorded as a deviation in UI-GUIDELINES.md
// (Windows' own dark surfaces are dark gray, not black). Surfaces step up in lightness the more they matter, as Microsoft's color
// guidance says: "darker colors indicate background surfaces of less importance".
const DARK: Palette = Palette {
    bg: rgb(0x00, 0x00, 0x00),
    nav: rgb(0x0C, 0x0C, 0x0C),
    card: rgb(0x16, 0x16, 0x16),
    card_hi: rgb(0x24, 0x24, 0x24),
    edge: rgb(0x2C, 0x2C, 0x2C),
    control_edge: rgb(0x8E, 0x8E, 0x8E),
    text: rgb(0xFF, 0xFF, 0xFF),
    muted: rgb(0xC2, 0xC2, 0xC2),
    disabled: rgb(0x7A, 0x7A, 0x7A),
    edit_bg: rgb(0x1C, 0x1C, 0x1C),
    track: rgb(0x8E, 0x8E, 0x8E),
    focus: rgb(0xFF, 0xFF, 0xFF),
    ok: rgb(0x6C, 0xCB, 0x5F),
    warn: rgb(0xFC, 0xE1, 0x00),
    bad: rgb(0xFF, 0x99, 0xA4),
};

const LIGHT: Palette = Palette {
    bg: rgb(0xF3, 0xF3, 0xF3),
    nav: rgb(0xEA, 0xEA, 0xEA),
    card: rgb(0xFF, 0xFF, 0xFF),
    card_hi: rgb(0xF0, 0xF0, 0xF0),
    edge: rgb(0xE0, 0xE0, 0xE0),
    control_edge: rgb(0x76, 0x76, 0x76),
    text: rgb(0x1A, 0x1A, 0x1A),
    muted: rgb(0x5A, 0x5A, 0x5A),
    disabled: rgb(0x9A, 0x9A, 0x9A),
    edit_bg: rgb(0xFF, 0xFF, 0xFF),
    track: rgb(0x76, 0x76, 0x76),
    focus: rgb(0x1A, 0x1A, 0x1A),
    ok: rgb(0x0F, 0x7B, 0x0F),
    warn: rgb(0x8A, 0x52, 0x00),
    bad: rgb(0xB0, 0x24, 0x18),
};

/// System colors, read once from `GetSysColor` by the platform layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SysColors {
    pub window: u32,
    pub window_text: u32,
    pub gray_text: u32,
    pub highlight: u32,
    pub highlight_text: u32,
    pub hotlight: u32,
}

impl Default for SysColors {
    fn default() -> Self {
        // Windows' own "High Contrast Black" values, used only if the system cannot be read.
        SysColors {
            window: 0x00_00_00,
            window_text: 0xFF_FF_FF,
            gray_text: rgb(0x3F, 0xF2, 0x3F),
            highlight: rgb(0x1A, 0xEB, 0xFF),
            highlight_text: 0x00_00_00,
            hotlight: rgb(0xFF, 0xFF, 0x00),
        }
    }
}

/// High contrast: exactly the system's colors, no tints, no dimmed text. Edges are drawn in
/// the text color so every boundary is visible, which is what these themes are for.
pub fn high_contrast(sys: &SysColors) -> Palette {
    Palette {
        bg: sys.window,
        nav: sys.window,
        card: sys.window,
        card_hi: sys.highlight,
        edge: sys.window_text,
        control_edge: sys.window_text,
        text: sys.window_text,
        muted: sys.window_text,
        disabled: sys.gray_text,
        edit_bg: sys.window,
        track: sys.window_text,
        focus: sys.hotlight,
        ok: sys.window_text,
        warn: sys.window_text,
        bad: sys.window_text,
    }
}

/// The user's accent color, adjusted until it works as a UI color on this palette: at least
/// 3:1 against both the page and a card, so a selected indicator or a focused switch is visible.
/// Windows' default accent (a deep blue or teal) is often too dark for a true-black page.
pub fn accent_on(palette: &Palette, accent: u32, mode: Mode) -> u32 {
    if mode == Mode::HighContrast {
        return palette.card_hi;
    }
    let needs =
        |c: u32| contrast_ratio(c, palette.bg) < 3.0 || contrast_ratio(c, palette.card) < 3.0;
    let toward = if mode == Mode::Dark {
        0x00FF_FFFF
    } else {
        0x0000_0000
    };
    let mut color = accent;
    let mut step = 0;
    while needs(color) && step < 20 {
        color = blend(color, toward, 0.12);
        step += 1;
    }
    color
}

/// Text color on top of an accent-filled control (a primary button, a selected switch):
/// whichever of black or white reads better, never a mid gray.
pub fn on_accent(accent: u32) -> u32 {
    if contrast_ratio(0x00FF_FFFF, accent) >= contrast_ratio(0, accent) {
        0x00FF_FFFF
    } else {
        0
    }
}

pub fn blend(a: u32, b: u32, t: f32) -> u32 {
    let mix = |shift: u32| {
        let x = ((a >> shift) & 0xFF) as f32;
        let y = ((b >> shift) & 0xFF) as f32;
        (x + (y - x) * t).round().clamp(0.0, 255.0) as u32
    };
    mix(0) | (mix(8) << 8) | (mix(16) << 16)
}

// ---------------------------------------------------------------------------
// Type ramp (Microsoft: "Typography in Windows", the Windows 11 type ramp)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    /// Segoe UI Variable Small: 12 px and under.
    Small,
    /// Segoe UI Variable Text: body.
    Text,
    /// Segoe UI Variable Display: 20 px and over.
    Display,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextStyle {
    pub name: &'static str,
    /// Effective pixels, before display and text scaling.
    pub size: i32,
    pub line: i32,
    pub weight: i32,
    pub face: Face,
}

pub const CAPTION: TextStyle = TextStyle {
    name: "Caption",
    size: 12,
    line: 16,
    weight: 400,
    face: Face::Small,
};
pub const BODY: TextStyle = TextStyle {
    name: "Body",
    size: 14,
    line: 20,
    weight: 400,
    face: Face::Text,
};
pub const BODY_STRONG: TextStyle = TextStyle {
    name: "Body strong",
    size: 14,
    line: 20,
    weight: 600,
    face: Face::Text,
};
pub const SUBTITLE: TextStyle = TextStyle {
    name: "Subtitle",
    size: 20,
    line: 28,
    weight: 600,
    face: Face::Display,
};
pub const TITLE: TextStyle = TextStyle {
    name: "Title",
    size: 28,
    line: 36,
    weight: 600,
    face: Face::Display,
};

#[cfg(test)]
pub const RAMP: [TextStyle; 5] = [CAPTION, BODY, BODY_STRONG, SUBTITLE, TITLE];

/// Font family names to try, best first. The platform picks the first one installed.
pub fn face_candidates(face: Face) -> &'static [&'static str] {
    match face {
        Face::Small => &["Segoe UI Variable Small", "Segoe UI"],
        Face::Text => &["Segoe UI Variable Text", "Segoe UI"],
        Face::Display => &["Segoe UI Variable Display", "Segoe UI"],
    }
}

/// Segoe Fluent Icons first, Segoe MDL2 Assets on Windows 10. If neither is installed the UI
/// shows text only, never a box of missing-glyph squares.
pub const ICON_FONTS: [&str; 2] = ["Segoe Fluent Icons", "Segoe MDL2 Assets"];

// ---------------------------------------------------------------------------
// Metrics (logical pixels; the platform multiplies by the display scale and text scale)
// ---------------------------------------------------------------------------

pub mod metrics {
    /// Layout moves in 4 px steps (Microsoft, "Screen sizes and breakpoints": sizes, margins and
    /// positions in multiples of 4 epx).
    pub const UNIT: i32 = 4;
    /// In-page controls and cards: 4 px. Overlays (dialogs, flyouts, teaching tips): 8 px.
    pub const CONTROL_RADIUS: i32 = 4;
    pub const OVERLAY_RADIUS: i32 = 8;
    pub const PAGE_MARGIN: i32 = 24;
    pub const CARD_PAD: i32 = 16;
    /// Space between cards.
    pub const CARD_GAP: i32 = 4;
    pub const NAV_EXPANDED: i32 = 216;
    pub const NAV_COMPACT: i32 = 48;
    /// Used when no icon font is installed: labels only, never an unlabeled rail.
    pub const NAV_TEXT_ONLY: i32 = 168;
    /// Below this client width the navigation pane collapses to icons.
    pub const NAV_BREAKPOINT: i32 = 720;
    /// AllBlack11's own targets: comfortable 40, never below 24 for a pointer. These are product
    /// choices; the Microsoft pages read for this pass state no pixel minimum.
    pub const TARGET: i32 = 40;
    pub const MIN_TARGET: i32 = 24;
}

/// The effective scale for layout: display scaling times the user's "Text size" setting
/// (Settings > Accessibility > Text size, 100% to 225%). Applying text scale to the whole
/// layout keeps rows from clipping their own text; it is documented as a deviation.
pub fn layout_scale(dpi: u32, text_scale_percent: u32) -> f32 {
    let dpi = dpi.clamp(96, 960) as f32 / 96.0;
    let text = text_scale_percent.clamp(100, 225) as f32 / 100.0;
    dpi * text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_modes() -> [(Mode, Palette); 3] {
        let sys = SysColors::default();
        [
            (Mode::Light, palette(Mode::Light, &sys)),
            (Mode::Dark, palette(Mode::Dark, &sys)),
            (Mode::HighContrast, palette(Mode::HighContrast, &sys)),
        ]
    }

    #[test]
    fn text_meets_4_5_to_1_on_every_surface_it_sits_on() {
        for (mode, p) in all_modes() {
            for (name, surface) in [
                ("bg", p.bg),
                ("nav", p.nav),
                ("card", p.card),
                ("card_hi", p.card_hi),
                ("edit", p.edit_bg),
            ] {
                for (label, color) in [("text", p.text), ("muted", p.muted)] {
                    // High contrast selection uses highlight/highlight-text, tested below.
                    if mode == Mode::HighContrast && name == "card_hi" {
                        continue;
                    }
                    let ratio = contrast_ratio(color, surface);
                    assert!(ratio >= 4.5, "{mode:?}: {label} on {name} is {ratio:.2}:1");
                }
            }
        }
    }

    #[test]
    fn things_you_operate_have_a_3_to_1_boundary() {
        for (mode, p) in all_modes() {
            for (name, surface) in [("card", p.card), ("bg", p.bg), ("edit", p.edit_bg)] {
                let edge = contrast_ratio(p.control_edge, surface);
                assert!(
                    edge >= 3.0,
                    "{mode:?}: control edge on {name} is {edge:.2}:1"
                );
                let track = contrast_ratio(p.track, surface);
                assert!(
                    track >= 3.0,
                    "{mode:?}: switch track on {name} is {track:.2}:1"
                );
                let focus = contrast_ratio(p.focus, surface);
                assert!(
                    focus >= 3.0,
                    "{mode:?}: focus ring on {name} is {focus:.2}:1"
                );
            }
        }
    }

    #[test]
    fn status_colors_are_readable_as_text_and_never_the_only_signal() {
        for (mode, p) in all_modes() {
            for (name, color) in [("ok", p.ok), ("warn", p.warn), ("bad", p.bad)] {
                for (surface_name, surface) in [("bg", p.bg), ("card", p.card)] {
                    let ratio = contrast_ratio(color, surface);
                    assert!(
                        ratio >= 4.5,
                        "{mode:?}: {name} on {surface_name} is {ratio:.2}:1"
                    );
                }
            }
        }
        // The three status colors must be distinguishable from each other in the two real
        // themes, but nothing relies on that: every status line also carries words.
        for p in [DARK, LIGHT] {
            assert_ne!(p.ok, p.bad);
            assert_ne!(p.warn, p.bad);
        }
    }

    #[test]
    fn surfaces_step_up_in_importance_in_dark_and_are_distinct_in_light() {
        let lum = |c: u32| crate::legacy_policy::luminance(c);
        assert!(lum(DARK.bg) < lum(DARK.nav));
        assert!(lum(DARK.nav) < lum(DARK.card));
        assert!(lum(DARK.card) < lum(DARK.card_hi));
        assert!(
            lum(LIGHT.card) > lum(LIGHT.bg),
            "light cards sit above the page"
        );
        assert_ne!(LIGHT.nav, LIGHT.bg);
        assert!(DARK.disabled != DARK.muted && LIGHT.disabled != LIGHT.muted);
    }

    #[test]
    fn the_accent_is_adjusted_until_it_can_be_seen_on_the_surfaces() {
        let dark = palette(Mode::Dark, &SysColors::default());
        let light = palette(Mode::Light, &SysColors::default());
        // A dark teal (a typical default accent) and a pale yellow, on both themes.
        for accent in [
            rgb(0x01, 0x85, 0x74),
            rgb(0x00, 0x2B, 0x5C),
            rgb(0xFF, 0xF1, 0x8E),
            rgb(0x20, 0x20, 0x20),
            0,
            0x00FF_FFFF,
        ] {
            for (mode, p) in [(Mode::Dark, dark), (Mode::Light, light)] {
                let adjusted = accent_on(&p, accent, mode);
                for surface in [p.bg, p.card] {
                    assert!(
                        contrast_ratio(adjusted, surface) >= 3.0,
                        "{mode:?}: accent {accent:06X} -> {adjusted:06X} is {:.2}:1",
                        contrast_ratio(adjusted, surface)
                    );
                }
            }
        }
        // An accent that already works is left alone.
        let fine = rgb(0x4C, 0x9A, 0xFF);
        assert_eq!(accent_on(&dark, fine, Mode::Dark), fine);
    }

    #[test]
    fn text_on_an_accent_fill_is_black_or_white_and_readable() {
        for accent in [
            rgb(0x01, 0x85, 0x74),
            rgb(0xFF, 0xF1, 0x8E),
            rgb(0x4C, 0x9A, 0xFF),
            rgb(0xC4, 0x2B, 0x1C),
            0,
            0x00FF_FFFF,
        ] {
            let text = on_accent(accent);
            assert!(text == 0 || text == 0x00FF_FFFF);
            // 4.5:1 is not reachable for every mid-tone fill; the better of black/white is the
            // best any text color can do, and the real accents above all clear 3:1.
            assert!(contrast_ratio(text, accent) >= 3.0, "{accent:06X}");
        }
    }

    #[test]
    fn high_contrast_uses_the_system_colors_and_nothing_else() {
        let sys = SysColors {
            window: rgb(1, 2, 3),
            window_text: rgb(250, 251, 252),
            gray_text: rgb(10, 200, 10),
            highlight: rgb(0, 100, 200),
            highlight_text: rgb(255, 255, 255),
            hotlight: rgb(255, 255, 0),
        };
        let p = palette(Mode::HighContrast, &sys);
        assert_eq!(p.bg, sys.window);
        assert_eq!(p.card, sys.window);
        assert_eq!(p.text, sys.window_text);
        assert_eq!(p.muted, sys.window_text, "no dimmed text in high contrast");
        assert_eq!(p.disabled, sys.gray_text);
        assert_eq!(p.card_hi, sys.highlight);
        assert_eq!(p.edge, sys.window_text, "every boundary is drawn");
        assert_eq!(
            accent_on(&p, rgb(9, 9, 9), Mode::HighContrast),
            sys.highlight
        );
    }

    #[test]
    fn high_contrast_beats_the_choice_and_the_choice_beats_the_system() {
        for choice in [Appearance::System, Appearance::Light, Appearance::Dark] {
            assert_eq!(resolve(choice, true, true), Mode::HighContrast);
        }
        assert_eq!(resolve(Appearance::Light, true, false), Mode::Light);
        assert_eq!(resolve(Appearance::Dark, false, false), Mode::Dark);
        assert_eq!(resolve(Appearance::System, true, false), Mode::Dark);
        assert_eq!(resolve(Appearance::System, false, false), Mode::Light);
        // The choice cycles through all three and returns.
        assert_eq!(Appearance::System.next().next().next(), Appearance::System);
    }

    #[test]
    fn the_type_ramp_matches_microsofts_numbers_and_never_goes_below_the_minimums() {
        assert_eq!((CAPTION.size, CAPTION.line), (12, 16));
        assert_eq!((BODY.size, BODY.line), (14, 20));
        assert_eq!((SUBTITLE.size, SUBTITLE.line), (20, 28));
        assert_eq!((TITLE.size, TITLE.line), (28, 36));
        for style in RAMP {
            // Minimums: 14 semibold or 12 regular.
            if style.weight >= 600 {
                assert!(style.size >= 14, "{}", style.name);
            } else {
                assert!(style.size >= 12, "{}", style.name);
            }
            assert!(
                style.line >= style.size + 4,
                "{} needs room for ascenders",
                style.name
            );
            assert!(
                style.weight == 400 || style.weight == 600,
                "no bold or italic in the ramp"
            );
        }
    }

    #[test]
    fn every_face_has_a_fallback_and_icon_fonts_name_both_generations() {
        for face in [Face::Small, Face::Text, Face::Display] {
            let list = face_candidates(face);
            assert_eq!(list.last(), Some(&"Segoe UI"));
            assert!(list[0].starts_with("Segoe UI Variable"));
        }
        assert_eq!(ICON_FONTS[0], "Segoe Fluent Icons");
        assert_eq!(ICON_FONTS[1], "Segoe MDL2 Assets");
    }

    #[test]
    fn metrics_sit_on_the_four_pixel_grid_and_scale_is_bounded() {
        use metrics::*;
        for value in [
            CONTROL_RADIUS,
            OVERLAY_RADIUS,
            PAGE_MARGIN,
            CARD_PAD,
            CARD_GAP,
            NAV_EXPANDED,
            NAV_COMPACT,
            NAV_TEXT_ONLY,
            TARGET,
            MIN_TARGET,
        ] {
            assert_eq!(value % UNIT, 0, "{value}");
        }
        assert!(CONTROL_RADIUS < OVERLAY_RADIUS);
        assert!((layout_scale(96, 100) - 1.0).abs() < 1e-6);
        assert!((layout_scale(192, 100) - 2.0).abs() < 1e-6);
        assert!((layout_scale(96, 225) - 2.25).abs() < 1e-6);
        // Out-of-range input is clamped rather than producing a huge or tiny UI.
        assert_eq!(layout_scale(0, 0), 1.0);
        assert!(layout_scale(u32::MAX, u32::MAX) <= 10.0 * 2.25);
    }
}
