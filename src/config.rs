use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

/// `AccentColorMenu` DWORD `0xFF748501` is stored as `0xAABBGGRR`.
/// The low 24 bits are COLORREF `0x00748501`, which is RGB `#018574`.
pub const FALLBACK_ACCENT_BGR: u32 = 0x0074_8501;

pub const DWM_COLOR_DEFAULT: u32 = 0xFFFF_FFFF;
pub const DWM_COLOR_NONE: u32 = 0xFFFF_FFFE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    /// Let DWM use the system color (`DWMWA_COLOR_DEFAULT`).
    Default,
    /// Draw no color (`DWMWA_COLOR_NONE`).
    None,
    /// Current `AccentColorMenu` from the user's registry.
    Accent,
    Rgb {
        r: u8,
        g: u8,
        b: u8,
    },
}

impl Color {
    pub fn to_colorref(self, accent_bgr: u32) -> u32 {
        match self {
            Color::Default => DWM_COLOR_DEFAULT,
            Color::None => DWM_COLOR_NONE,
            Color::Accent => accent_bgr & 0x00FF_FFFF,
            Color::Rgb { r, g, b } => (b as u32) << 16 | (g as u32) << 8 | r as u32,
        }
    }

    pub fn components(self, accent_bgr: u32) -> Option<(u8, u8, u8)> {
        match self {
            Color::Rgb { r, g, b } => Some((r, g, b)),
            Color::Accent => {
                let bgr = accent_bgr & 0x00FF_FFFF;
                let r = (bgr & 0xFF) as u8;
                let g = ((bgr >> 8) & 0xFF) as u8;
                let b = ((bgr >> 16) & 0xFF) as u8;
                Some((r, g, b))
            }
            Color::Default | Color::None => None,
        }
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&match self {
            Color::Default => "default".to_string(),
            Color::None => "none".to_string(),
            Color::Accent => "accent".to_string(),
            Color::Rgb { r, g, b } => format!("#{r:02X}{g:02X}{b:02X}"),
        })
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        parse_color(&text).map_err(serde::de::Error::custom)
    }
}

pub fn parse_color(text: &str) -> Result<Color, String> {
    let value = text.trim();
    if value.is_empty() {
        return Err("color is empty".into());
    }
    match value.to_ascii_lowercase().as_str() {
        "default" => return Ok(Color::Default),
        "none" => return Ok(Color::None),
        "accent" => return Ok(Color::Accent),
        _ => {}
    }
    let hex = value
        .strip_prefix('#')
        .ok_or_else(|| format!("color '{value}' should be #RRGGBB, none, default, or accent"))?;
    let hex = hex.trim();
    let expanded = match hex.len() {
        3 => {
            let chars: Vec<char> = hex.chars().collect();
            format!("{0}{0}{1}{1}{2}{2}", chars[0], chars[1], chars[2])
        }
        6 => hex.to_string(),
        8 => hex[2..].to_string(),
        _ => {
            return Err(format!(
                "color '#{hex}' should be #RGB, #RRGGBB, or #AARRGGBB"
            ))
        }
    };
    if !expanded.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(format!("color '#{hex}' is not hexadecimal"));
    }
    let raw = u32::from_str_radix(&expanded, 16).map_err(|err| err.to_string())?;
    Ok(Color::Rgb {
        r: ((raw >> 16) & 0xFF) as u8,
        g: ((raw >> 8) & 0xFF) as u8,
        b: (raw & 0xFF) as u8,
    })
}

/// Values match `DWM_SYSTEMBACKDROP_TYPE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backdrop {
    Auto = 0,
    None = 1,
    Mica = 2,
    Acrylic = 3,
    MicaAlt = 4,
    /// `SetWindowCompositionAttribute` acrylic with its own tint. Not a DWM enum value.
    TintedAcrylic = 5,
}

impl Backdrop {
    /// DWM backdrop to set. Tinted acrylic uses `DWMSBT_NONE` so it does not stack
    /// with the composition accent. Callers override this if that accent call fails.
    pub fn dwm_value(self) -> i32 {
        match self {
            Backdrop::Auto => 0,
            Backdrop::None => 1,
            Backdrop::Mica => 2,
            Backdrop::Acrylic => 3,
            Backdrop::MicaAlt => 4,
            Backdrop::TintedAcrylic => 1,
        }
    }
}

impl Serialize for Backdrop {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Backdrop::Auto => "auto",
            Backdrop::None => "none",
            Backdrop::Mica => "mica",
            Backdrop::Acrylic => "acrylic",
            Backdrop::MicaAlt => "micaAlt",
            Backdrop::TintedAcrylic => "tintedAcrylic",
        })
    }
}

impl<'de> Deserialize<'de> for Backdrop {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        parse_backdrop(&text).map_err(serde::de::Error::custom)
    }
}

pub fn parse_backdrop(text: &str) -> Result<Backdrop, String> {
    match text.trim().to_ascii_lowercase().as_str() {
        "auto" | "default" => Ok(Backdrop::Auto),
        "none" | "off" => Ok(Backdrop::None),
        "mica" | "mainwindow" => Ok(Backdrop::Mica),
        "micaalt" | "mica-alt" | "mica_alt" | "tabbed" => Ok(Backdrop::MicaAlt),
        "acrylic" | "transient" => Ok(Backdrop::Acrylic),
        "tintedacrylic" | "tinted-acrylic" | "tinted_acrylic" => Ok(Backdrop::TintedAcrylic),
        other => Err(format!(
            "backdrop '{other}' should be none, mica, micaAlt, acrylic, tintedAcrylic, or auto"
        )),
    }
}

/// Values match `DWM_WINDOW_CORNER_PREFERENCE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corners {
    Default = 0,
    Square = 1,
    Round = 2,
    Small = 3,
}

impl Corners {
    pub fn dwm_value(self) -> u32 {
        self as u32
    }
}

impl Serialize for Corners {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Corners::Default => "default",
            Corners::Square => "square",
            Corners::Round => "round",
            Corners::Small => "small",
        })
    }
}

impl<'de> Deserialize<'de> for Corners {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        parse_corners(&text).map_err(serde::de::Error::custom)
    }
}

pub fn parse_corners(text: &str) -> Result<Corners, String> {
    match text.trim().to_ascii_lowercase().as_str() {
        "default" => Ok(Corners::Default),
        "square" | "donotround" | "do-not-round" | "none" => Ok(Corners::Square),
        "round" => Ok(Corners::Round),
        "small" | "roundsmall" | "round-small" => Ok(Corners::Small),
        other => Err(format!(
            "corners '{other}' should be default, round, small, or square"
        )),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    pub dark_mode: bool,
    pub backdrop: Backdrop,
    pub caption: Color,
    pub border: Color,
    pub text: Color,
    pub corners: Corners,
    pub acrylic_tint: Color,
    /// 0 = fully clear tint, 255 = opaque. `0.55` in JSON is 140.
    pub acrylic_opacity: u8,
    pub tinted_acrylic_on_move_fallback: bool,
}

pub fn base_style() -> Style {
    Style {
        dark_mode: true,
        backdrop: Backdrop::Mica,
        caption: Color::Rgb { r: 0, g: 0, b: 0 },
        border: Color::Accent,
        text: Color::Rgb {
            r: 255,
            g: 255,
            b: 255,
        },
        corners: Corners::Default,
        acrylic_tint: Color::Rgb { r: 0, g: 0, b: 0 },
        acrylic_opacity: opacity_byte(0.55).expect("default opacity"),
        tinted_acrylic_on_move_fallback: true,
    }
}

/// `None` and `0` mean "leave the system CaptionHeight alone".
/// Other values clamp to the slider range, 16–64 pixels.
pub fn normalize_caption_height(value: Option<u32>) -> Option<u32> {
    match value {
        None | Some(0) => None,
        Some(pixels) => Some(pixels.clamp(16, 64)),
    }
}

/// `acrylicOpacity` as the alpha byte of an `AABBGGRR` gradient.
pub fn opacity_byte(value: f64) -> Result<u8, String> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(format!(
            "acrylicOpacity {value} should be a number from 0.0 to 1.0"
        ));
    }
    Ok((value * 255.0).round() as u8)
}

/// Composition accent color: alpha from opacity, then B, G, R.
pub fn acrylic_gradient(tint: Color, opacity: u8, accent_bgr: u32) -> u32 {
    let (r, g, b) = tint.components(accent_bgr).unwrap_or((0, 0, 0));
    (u32::from(opacity) << 24) | (u32::from(b) << 16) | (u32::from(g) << 8) | u32::from(r)
}

/// JSON number 0.0–1.0 stored as the 0–255 alpha byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opacity(pub u8);

impl Serialize for Opacity {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let hundredths = (f64::from(self.0) / 255.0 * 100.0).round() / 100.0;
        serializer.serialize_f64(hundredths)
    }
}

impl<'de> Deserialize<'de> for Opacity {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = f64::deserialize(deserializer)?;
        opacity_byte(value)
            .map(Opacity)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PartialStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dark_mode: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backdrop: Option<Backdrop>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption_color: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_color: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_color: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corners: Option<Corners>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acrylic_tint: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acrylic_opacity: Option<Opacity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tinted_acrylic_on_move_fallback: Option<bool>,
}

impl PartialStyle {
    pub fn merge_onto(&self, mut style: Style) -> Style {
        if let Some(value) = self.dark_mode {
            style.dark_mode = value;
        }
        if let Some(value) = self.backdrop {
            style.backdrop = value;
        }
        if let Some(value) = self.caption_color {
            style.caption = value;
        }
        if let Some(value) = self.border_color {
            style.border = value;
        }
        if let Some(value) = self.text_color {
            style.text = value;
        }
        if let Some(value) = self.corners {
            style.corners = value;
        }
        if let Some(value) = self.acrylic_tint {
            style.acrylic_tint = value;
        }
        if let Some(Opacity(value)) = self.acrylic_opacity {
            style.acrylic_opacity = value;
        }
        if let Some(value) = self.tinted_acrylic_on_move_fallback {
            style.tinted_acrylic_on_move_fallback = value;
        }
        style
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ShellSettings {
    pub oled_taskbar: bool,
    pub black_shell: bool,
}

impl Default for ShellSettings {
    fn default() -> Self {
        Self {
            oled_taskbar: false,
            black_shell: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rule {
    pub process: String,
    #[serde(flatten, default)]
    pub style: PartialStyle,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub logging: bool,
    #[serde(default = "default_true")]
    pub reapply_on_foreground: bool,
    #[serde(default)]
    pub comment: String,
    #[serde(default, rename = "default")]
    pub defaults: PartialStyle,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub exclude_classes: Vec<String>,
    #[serde(default)]
    pub shell: ShellSettings,
    /// Legacy caption height in pixels. `None` leaves Windows `CaptionHeight` alone.
    /// Applied only after the user signs out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption_height: Option<u32>,
    /// Opt-in in-process recolor of classic dialogs. Off unless the user turns it on.
    #[serde(default)]
    pub experimental_dark_legacy: bool,
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        serde_json::from_str(bundled_default_json()).expect("bundled default config parses")
    }
}

pub fn bundled_default_json() -> &'static str {
    include_str!("../config.default.json")
}

pub fn parse_config(text: &str) -> Result<Config, String> {
    serde_json::from_str(text).map_err(|err| err.to_string())
}

pub fn set_shell_flag(text: &str, field: &str, enabled: bool) -> Result<String, String> {
    let mut value: Value = serde_json::from_str(text).map_err(|err| err.to_string())?;
    let obj = value
        .as_object_mut()
        .ok_or_else(|| "config root must be a JSON object".to_string())?;
    let shell = obj
        .entry("shell")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    let shell = shell
        .as_object_mut()
        .ok_or_else(|| "shell must be a JSON object".to_string())?;
    shell.insert(field.to_string(), Value::Bool(enabled));
    serde_json::to_string_pretty(&value).map_err(|err| err.to_string())
}

pub fn set_enabled_flag(text: &str, enabled: bool) -> Result<String, String> {
    let mut value: Value = serde_json::from_str(text).map_err(|err| err.to_string())?;
    let obj = value
        .as_object_mut()
        .ok_or_else(|| "config root must be a JSON object".to_string())?;
    obj.insert("enabled".into(), Value::Bool(enabled));
    serde_json::to_string_pretty(&value).map_err(|err| err.to_string())
}

pub fn resolve_style(config: &Config, exe: &str, accent_bgr: u32) -> Option<Style> {
    let rule = matching_rule(config, exe);
    let mut enabled = config.defaults.enabled.unwrap_or(true);
    let mut style = config.defaults.merge_onto(base_style());
    let mut text_explicit = config.defaults.text_color.is_some();
    if let Some(rule) = rule {
        if let Some(value) = rule.style.enabled {
            enabled = value;
        }
        if rule.style.text_color.is_some() {
            text_explicit = true;
        }
        style = rule.style.merge_onto(style);
    }
    if !enabled {
        return None;
    }
    if !text_explicit {
        style.text = auto_text(style.caption, accent_bgr);
    }
    Some(style)
}

fn matching_rule<'a>(config: &'a Config, exe: &str) -> Option<&'a Rule> {
    config
        .rules
        .iter()
        .find(|rule| exe_matches(&rule.process, exe))
}

pub fn exe_matches(pattern: &str, exe: &str) -> bool {
    let pattern = file_name(pattern);
    !pattern.is_empty() && pattern.eq_ignore_ascii_case(exe)
}

pub fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path).trim()
}

fn auto_text(caption: Color, accent_bgr: u32) -> Color {
    match caption.components(accent_bgr) {
        Some((r, g, b)) if is_dark(r, g, b) => Color::Rgb {
            r: 255,
            g: 255,
            b: 255,
        },
        Some(_) => Color::Rgb { r: 0, g: 0, b: 0 },
        None => Color::Default,
    }
}

fn is_dark(r: u8, g: u8, b: u8) -> bool {
    let y = 0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b);
    y < 150.0
}

#[cfg_attr(not(windows), allow(dead_code))]
pub fn summarize(config: &Config, accent_bgr: u32) -> String {
    let style = config.defaults.merge_onto(base_style());
    format!(
        "enabled={} logging={} reapplyOnForeground={} experimentalDarkLegacy={} captionHeight={}\ndefault darkMode={} backdrop={:?} caption={} border={} text={} corners={:?}\nrules={}\nexclude={}",
        config.enabled,
        config.logging,
        config.reapply_on_foreground,
        config.experimental_dark_legacy,
        config
            .caption_height
            .map(|px| px.to_string())
            .unwrap_or_else(|| "system".into()),
        style.dark_mode,
        style.backdrop,
        color_label(style.caption, accent_bgr),
        color_label(style.border, accent_bgr),
        color_label(style.text, accent_bgr),
        style.corners,
        config.rules.len(),
        config.exclude.len()
    )
}

#[cfg_attr(not(windows), allow(dead_code))]
fn color_label(color: Color, accent_bgr: u32) -> String {
    match color {
        Color::Default => "default".into(),
        Color::None => "none".into(),
        Color::Accent => format!("accent({})", hex_rgb(accent_bgr)),
        Color::Rgb { r, g, b } => format!("#{r:02X}{g:02X}{b:02X}"),
    }
}

pub fn hex_rgb(accent_bgr: u32) -> String {
    let r = (accent_bgr & 0xFF) as u8;
    let g = ((accent_bgr >> 8) & 0xFF) as u8;
    let b = ((accent_bgr >> 16) & 0xFF) as u8;
    format!("#{r:02X}{g:02X}{b:02X}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_default_parses_and_matches_teal() {
        let config = parse_config(bundled_default_json()).unwrap();
        assert!(config.enabled);
        assert!(config.logging);
        assert!(config
            .exclude
            .iter()
            .any(|name| name.eq_ignore_ascii_case("GeForceNOW.exe")));
        let style = resolve_style(&config, "notepad.exe", 0xFF74_8501).unwrap();
        assert!(style.dark_mode);
        assert_eq!(style.backdrop, Backdrop::Mica);
        assert_eq!(style.caption.to_colorref(0), 0x0000_0000);
        assert_eq!(style.border, Color::Accent);
        assert_eq!(style.border.to_colorref(0xFF74_8501), 0x0074_8501);
        assert_eq!(
            style.text,
            Color::Rgb {
                r: 255,
                g: 255,
                b: 255
            }
        );
        assert_eq!(hex_rgb(0xFF74_8501), "#018574");
    }

    #[test]
    fn empty_object_uses_builtin_base() {
        let config = parse_config("{}").unwrap();
        let style = resolve_style(&config, "notepad.exe", FALLBACK_ACCENT_BGR).unwrap();
        assert_eq!(style.caption.to_colorref(0), 0);
        assert_eq!(style.border, Color::Accent);
        assert_eq!(style.backdrop, Backdrop::Mica);
    }

    #[test]
    fn rule_overrides_and_disabled_rule_skips() {
        let config = parse_config(
            r#"{
                "default": { "borderColor": "accent" },
                "rules": [
                    { "process": "Notepad.exe", "backdrop": "micaAlt", "borderColor": "none" },
                    { "process": "calc.exe", "enabled": false }
                ]
            }"#,
        )
        .unwrap();
        let notepad = resolve_style(&config, "notepad.exe", FALLBACK_ACCENT_BGR).unwrap();
        assert_eq!(notepad.backdrop, Backdrop::MicaAlt);
        assert_eq!(notepad.border, Color::None);
        assert_eq!(notepad.caption.to_colorref(0), 0);
        assert!(resolve_style(&config, "calc.exe", 0).is_none());
    }

    #[test]
    fn allowlist_mode() {
        let config = parse_config(
            r##"{
                "default": { "enabled": false },
                "rules": [ { "process": "notepad.exe", "enabled": true, "captionColor": "#112233" } ]
            }"##,
        )
        .unwrap();
        assert!(resolve_style(&config, "calc.exe", 0).is_none());
        let notepad = resolve_style(&config, "notepad.exe", 0).unwrap();
        assert_eq!(
            notepad.caption,
            Color::Rgb {
                r: 0x11,
                g: 0x22,
                b: 0x33
            }
        );
        assert_eq!(notepad.caption.to_colorref(0), 0x0033_2211);
    }

    #[test]
    fn auto_text_tracks_a_light_caption_when_text_is_omitted() {
        let config =
            parse_config(r##"{ "rules": [ { "process": "a.exe", "captionColor": "#F0F0F0" } ] }"##)
                .unwrap();
        let style = resolve_style(&config, "a.exe", 0).unwrap();
        assert_eq!(style.text, Color::Rgb { r: 0, g: 0, b: 0 });
    }

    #[test]
    fn rejects_unknown_field_and_bad_color() {
        assert!(parse_config(r#"{ "backDrop": "mica" }"#).is_err());
        assert!(parse_color("blue").is_err());
        assert_eq!(
            parse_color("#018574").unwrap(),
            Color::Rgb {
                r: 0x01,
                g: 0x85,
                b: 0x74
            }
        );
        assert_eq!(
            parse_color("#abc").unwrap(),
            parse_color("#AABBCC").unwrap()
        );
    }

    #[test]
    fn set_enabled_preserves_other_keys() {
        let text = set_enabled_flag(r#"{ "enabled": true, "exclude": ["a.exe"] }"#, false).unwrap();
        let config = parse_config(&text).unwrap();
        assert!(!config.enabled);
        assert_eq!(config.exclude, vec!["a.exe".to_string()]);
    }

    #[test]
    fn backdrop_aliases() {
        assert_eq!(parse_backdrop("tabbed").unwrap(), Backdrop::MicaAlt);
        assert_eq!(parse_backdrop("Mica").unwrap(), Backdrop::Mica);
        assert_eq!(
            parse_backdrop("tintedAcrylic").unwrap(),
            Backdrop::TintedAcrylic
        );
        assert_eq!(Backdrop::Acrylic.dwm_value(), 3);
        assert_eq!(Backdrop::TintedAcrylic.dwm_value(), 1);
        assert_eq!(parse_corners("none").unwrap(), Corners::Square);
    }

    #[test]
    fn tinted_acrylic_defaults_and_rule_override() {
        let config = parse_config(bundled_default_json()).unwrap();
        let style = resolve_style(&config, "notepad.exe", 0).unwrap();
        assert_eq!(style.backdrop, Backdrop::Mica);
        assert_eq!(
            acrylic_gradient(style.acrylic_tint, style.acrylic_opacity, 0),
            0x8C00_0000
        );
        assert!(style.tinted_acrylic_on_move_fallback);
        assert!(!config.shell.oled_taskbar);
        assert!(!config.shell.black_shell);
        assert!(!config.experimental_dark_legacy);
        assert!(config.caption_height.is_none());

        let config = parse_config(
            r##"{
                "rules": [{
                    "process": "mmc.exe",
                    "backdrop": "tintedAcrylic",
                    "acrylicTint": "#010203",
                    "acrylicOpacity": 0.8,
                    "tintedAcrylicOnMoveFallback": false
                }]
            }"##,
        )
        .unwrap();
        let style = resolve_style(&config, "mmc.exe", 0).unwrap();
        assert_eq!(style.backdrop, Backdrop::TintedAcrylic);
        assert_eq!(style.acrylic_opacity, opacity_byte(0.8).unwrap());
        assert!(!style.tinted_acrylic_on_move_fallback);
        assert_eq!(
            acrylic_gradient(style.acrylic_tint, style.acrylic_opacity, 0) & 0x00FF_FFFF,
            0x0003_0201
        );
        assert!(parse_config(r#"{ "default": { "acrylicOpacity": 1.5 } }"#).is_err());
    }

    #[test]
    fn opacity_round_trips_through_two_decimals() {
        let text = serde_json::to_string(&Opacity(140)).unwrap();
        assert_eq!(text, "0.55");
        let text = serde_json::to_string(&Opacity(204)).unwrap();
        assert_eq!(text, "0.8");
    }

    #[test]
    fn new_v120_fields_default_off_and_round_trip() {
        let config = parse_config(bundled_default_json()).unwrap();
        assert!(!config.experimental_dark_legacy);
        assert!(config.caption_height.is_none());
        let text = serde_json::to_string(&config).unwrap();
        let again = parse_config(&text).unwrap();
        assert!(!again.experimental_dark_legacy);
        assert!(again.caption_height.is_none());
        let custom =
            parse_config(r#"{ "captionHeight": 28, "experimentalDarkLegacy": true }"#).unwrap();
        assert_eq!(custom.caption_height, Some(28));
        assert!(custom.experimental_dark_legacy);
        assert_eq!(normalize_caption_height(Some(0)), None);
        assert_eq!(normalize_caption_height(Some(8)), Some(16));
        assert_eq!(normalize_caption_height(Some(40)), Some(40));
        assert_eq!(normalize_caption_height(Some(200)), Some(64));
    }

    #[test]
    fn shell_flag_roundtrip() {
        let text = set_shell_flag(bundled_default_json(), "oledTaskbar", true).unwrap();
        let config = parse_config(&text).unwrap();
        assert!(config.shell.oled_taskbar);
        assert!(!config.shell.black_shell);
        assert_eq!(config.defaults.backdrop, Some(Backdrop::Mica));
    }
}
