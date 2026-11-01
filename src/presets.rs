//! One-click looks. Each preset rewrites the default style and, where it says so,
//! the shell toggles. Rules, exclusions, and startup options are never touched.

use crate::config::{Backdrop, Color, Config, Corners, Opacity, PartialStyle};

pub struct Preset {
    pub name: &'static str,
    /// One line for the menu and Settings.
    pub blurb: &'static str,
}

pub const PRESETS: [Preset; 4] = [
    Preset {
        name: "Pure OLED",
        blurb: "Flat black windows with no border, plus a black taskbar, Start menu, and notification center.",
    },
    Preset {
        name: "Accent glass",
        blurb: "Black frosted glass behind windows with a thin accent-colored border. Shell is left as it is.",
    },
    Preset {
        name: "Classic Mica",
        blurb: "The original look: black title bar, Mica material, accent border. Shell is left as it is.",
    },
    Preset {
        name: "Factory defaults",
        blurb: "Back to the shipped config and every shell tweak off. Rules and exclusions are kept.",
    },
];

const BLACK: Color = Color::Rgb { r: 0, g: 0, b: 0 };
const WHITE: Color = Color::Rgb {
    r: 255,
    g: 255,
    b: 255,
};

fn look(backdrop: Backdrop, border: Color, tint_opacity: Option<f64>) -> PartialStyle {
    PartialStyle {
        enabled: Some(true),
        dark_mode: Some(true),
        backdrop: Some(backdrop),
        caption_color: Some(BLACK),
        border_color: Some(border),
        text_color: Some(WHITE),
        corners: Some(Corners::Default),
        acrylic_tint: Some(BLACK),
        acrylic_opacity: Some(Opacity(
            crate::config::opacity_byte(tint_opacity.unwrap_or(0.55)).unwrap_or(140),
        )),
        tinted_acrylic_on_move_fallback: Some(true),
    }
}

/// Apply preset `index` to `config`. Returns false for an unknown index.
pub fn apply(config: &mut Config, index: usize) -> bool {
    match index {
        0 => {
            config.enabled = true;
            config.defaults = look(Backdrop::None, Color::None, None);
            config.shell.oled_taskbar = true;
            config.shell.black_shell = true;
        }
        1 => {
            config.enabled = true;
            config.defaults = look(Backdrop::TintedAcrylic, Color::Accent, Some(0.70));
        }
        2 => {
            config.enabled = true;
            config.defaults = look(Backdrop::Mica, Color::Accent, None);
        }
        3 => {
            let fresh = Config::default();
            config.enabled = true;
            config.defaults = fresh.defaults;
            config.shell = fresh.shell;
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{base_style, parse_config};

    #[test]
    fn pure_oled_turns_the_shell_on_and_drops_the_border() {
        let mut config = Config::default();
        assert!(apply(&mut config, 0));
        let style = config.defaults.merge_onto(base_style());
        assert_eq!(style.backdrop, Backdrop::None);
        assert_eq!(style.border, Color::None);
        assert!(config.shell.oled_taskbar && config.shell.black_shell);
    }

    #[test]
    fn glass_keeps_the_shell_and_sets_tinted_acrylic() {
        let mut config = Config::default();
        config.shell.oled_taskbar = true;
        assert!(apply(&mut config, 1));
        let style = config.defaults.merge_onto(base_style());
        assert_eq!(style.backdrop, Backdrop::TintedAcrylic);
        assert_eq!(style.acrylic_opacity, 179);
        assert!(config.shell.oled_taskbar);
    }

    #[test]
    fn factory_keeps_rules_and_clears_the_shell() {
        let mut config = parse_config(
            r#"{"rules":[{"process":"mmc.exe","backdrop":"tintedAcrylic"}],"shell":{"oledTaskbar":true,"blackShell":true},"exclude":["x.exe"]}"#,
        )
        .unwrap();
        assert!(apply(&mut config, 3));
        assert_eq!(config.rules.len(), 1);
        assert_eq!(config.exclude, vec!["x.exe".to_string()]);
        assert!(!config.shell.oled_taskbar && !config.shell.black_shell);
    }

    #[test]
    fn every_preset_survives_a_json_round_trip() {
        for index in 0..PRESETS.len() {
            let mut config = Config::default();
            assert!(apply(&mut config, index));
            let text = serde_json::to_string_pretty(&config).unwrap();
            parse_config(&text).unwrap();
        }
        assert!(!apply(&mut Config::default(), 99));
    }
}
