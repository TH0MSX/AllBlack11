//! Pure decisions about *how much* effect to apply right now, and Focus Look colors.
//!
//! * Reduce effects: fall back to plain colors (no backdrop material) on documented system
//!   signals: battery saver, a remote desktop session, and Windows transparency being off.
//!   Each signal is separately configurable and the active reason is shown to the user.
//! * Focus Look: a restrained accent for the active window and calmer colors for inactive
//!   ones. A color that a rule sets explicitly always wins. Inactive text that would be
//!   hard to read is refused.

use crate::config::{Backdrop, Color, FocusSettings, ReduceEffects, Style};
use crate::legacy_policy::contrast_ratio;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SystemState {
    pub battery_saver: bool,
    pub remote_session: bool,
    pub transparency_enabled: bool,
}

/// Why effects are being reduced right now, or `None` for full effects.
pub fn reduce_reason(cfg: &ReduceEffects, system: &SystemState) -> Option<&'static str> {
    if cfg.on_remote_session && system.remote_session {
        Some("a remote desktop session")
    } else if cfg.on_battery_saver && system.battery_saver {
        Some("battery saver")
    } else if cfg.on_transparency_off && !system.transparency_enabled {
        Some("Windows transparency effects being off")
    } else {
        None
    }
}

fn is_material(backdrop: Backdrop) -> bool {
    matches!(
        backdrop,
        Backdrop::Mica | Backdrop::MicaAlt | Backdrop::Acrylic | Backdrop::TintedAcrylic
    )
}

/// The same look without translucency. Colors, corners and dark mode are untouched.
pub fn reduced(style: &Style, reason: Option<&str>) -> Style {
    let mut out = *style;
    if reason.is_some() && is_material(out.backdrop) {
        out.backdrop = Backdrop::None;
    }
    out
}

/// Which colors a rule set itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Explicit {
    pub caption: bool,
    pub border: bool,
    pub text: bool,
}

fn rgb_of(color: Color, accent_bgr: u32) -> Option<u32> {
    color
        .components(accent_bgr)
        .map(|(r, g, b)| u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16))
}

pub fn focus_style(
    style: &Style,
    focus: &FocusSettings,
    explicit: &Explicit,
    active: bool,
    accent_bgr: u32,
) -> Style {
    if !focus.enabled {
        return *style;
    }
    let mut out = *style;
    if active {
        if !explicit.border {
            out.border = focus.active_border;
        }
        return out;
    }
    if !explicit.border {
        out.border = focus.inactive_border;
    }
    if !explicit.text {
        // Only dim the text if it stays readable on the caption it sits on.
        let caption = rgb_of(out.caption, accent_bgr);
        let dim = rgb_of(focus.inactive_text, accent_bgr);
        let readable = match (caption, dim) {
            (Some(c), Some(d)) => contrast_ratio(c, d) >= 4.5,
            // Default or "none" caption: the system picks it, so a fixed gray is a gamble.
            _ => false,
        };
        if readable {
            out.text = focus.inactive_text;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{base_style, Corners};

    fn all_on() -> ReduceEffects {
        ReduceEffects::default()
    }

    fn full() -> SystemState {
        SystemState {
            battery_saver: false,
            remote_session: false,
            transparency_enabled: true,
        }
    }

    #[test]
    fn full_effects_when_nothing_is_wrong() {
        assert_eq!(reduce_reason(&all_on(), &full()), None);
    }

    #[test]
    fn each_documented_signal_reduces_effects_and_can_be_switched_off() {
        let mut system = full();
        system.battery_saver = true;
        assert_eq!(reduce_reason(&all_on(), &system), Some("battery saver"));
        let mut cfg = all_on();
        cfg.on_battery_saver = false;
        assert_eq!(reduce_reason(&cfg, &system), None);

        system = full();
        system.remote_session = true;
        assert_eq!(
            reduce_reason(&all_on(), &system),
            Some("a remote desktop session")
        );

        system = full();
        system.transparency_enabled = false;
        assert!(reduce_reason(&all_on(), &system)
            .unwrap()
            .contains("transparency"));
        let mut cfg = all_on();
        cfg.on_transparency_off = false;
        assert_eq!(reduce_reason(&cfg, &system), None);
    }

    #[test]
    fn reducing_removes_material_but_keeps_everything_else() {
        let mut style = base_style();
        style.corners = Corners::Square;
        for backdrop in [
            Backdrop::Mica,
            Backdrop::MicaAlt,
            Backdrop::Acrylic,
            Backdrop::TintedAcrylic,
        ] {
            style.backdrop = backdrop;
            let out = reduced(&style, Some("battery saver"));
            assert_eq!(out.backdrop, Backdrop::None);
            assert_eq!(out.caption, style.caption);
            assert_eq!(out.corners, Corners::Square);
        }
        style.backdrop = Backdrop::Auto;
        assert_eq!(reduced(&style, Some("x")).backdrop, Backdrop::Auto);
        style.backdrop = Backdrop::Mica;
        assert_eq!(reduced(&style, None).backdrop, Backdrop::Mica);
    }

    #[test]
    fn focus_look_marks_active_and_calms_inactive() {
        let focus = FocusSettings {
            enabled: true,
            ..FocusSettings::default()
        };
        let style = base_style(); // black caption, white text, accent border
        let active = focus_style(&style, &focus, &Explicit::default(), true, 0);
        assert_eq!(active.border, Color::Accent);
        let inactive = focus_style(&style, &focus, &Explicit::default(), false, 0);
        assert_eq!(inactive.border, Color::None);
        assert_eq!(inactive.text, focus.inactive_text);
        // Caption is never touched.
        assert_eq!(inactive.caption, style.caption);
    }

    #[test]
    fn a_rule_that_sets_a_color_always_wins_over_focus_look() {
        let focus = FocusSettings {
            enabled: true,
            ..FocusSettings::default()
        };
        let mut style = base_style();
        style.border = Color::Rgb { r: 255, g: 0, b: 0 };
        let explicit = Explicit {
            border: true,
            text: true,
            caption: false,
        };
        for active in [true, false] {
            let out = focus_style(&style, &focus, &explicit, active, 0);
            assert_eq!(out.border, style.border);
            assert_eq!(out.text, style.text);
        }
    }

    #[test]
    fn inactive_text_is_never_dimmed_below_readable_contrast() {
        let focus = FocusSettings {
            enabled: true,
            ..FocusSettings::default()
        };
        let mut style = base_style();
        style.caption = Color::Rgb {
            r: 0x9A,
            g: 0x9A,
            b: 0x9A,
        }; // same gray as the dim text
        let out = focus_style(&style, &focus, &Explicit::default(), false, 0);
        assert_eq!(
            out.text, style.text,
            "dim text on a gray caption would be unreadable"
        );
        // A system-chosen caption: contrast is unknowable, so do not gamble.
        style.caption = Color::Default;
        let out = focus_style(&style, &focus, &Explicit::default(), false, 0);
        assert_eq!(out.text, style.text);
    }

    #[test]
    fn disabled_focus_look_changes_nothing() {
        let focus = FocusSettings::default();
        assert!(!focus.enabled);
        let style = base_style();
        assert_eq!(
            focus_style(&style, &focus, &Explicit::default(), false, 0),
            style
        );
    }
}
