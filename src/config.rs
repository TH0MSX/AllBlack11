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

/// Options for the optional legacy-window layer. The switch itself is
/// `experimentalDarkLegacy`; these only matter while it is on.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct LegacySettings {
    /// `graphite` (default) or `black`.
    pub palette: String,
    /// ARM64 only: let the packaged x64 helper treat emulated x64 dialogs. Experimental.
    pub emulated_x64: bool,
}

impl Default for LegacySettings {
    fn default() -> Self {
        Self {
            palette: "graphite".into(),
            emulated_x64: false,
        }
    }
}

/// A per-app rule. At least one matcher (`process`, `class`, `path`, `package`, `title`) is
/// required. A rule overrides only the style fields it sets. See `rules.rs` for precedence:
/// higher `priority` wins, then the rule matching more criteria, then the earlier rule.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    /// Executable file name. Empty means "any executable".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub process: String,
    /// Window class, exact, case-insensitive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    /// Glob (`*`, `?`) against the full executable path, case-insensitive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Glob against the package family name of a packaged (MSIX/Store) app.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    /// Substring of the window title. Optional and off by default because titles change
    /// with the document and with the display language.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Higher wins. Default 0.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub priority: i32,
    /// A label for humans. Never used for matching.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten, default)]
    pub style: PartialStyle,
}

fn is_zero(value: &i32) -> bool {
    *value == 0
}

impl Rule {
    pub fn for_process(process: &str) -> Self {
        Rule {
            process: process.to_string(),
            class: None,
            path: None,
            package: None,
            title: None,
            priority: 0,
            name: None,
            style: PartialStyle::default(),
        }
    }

    pub fn has_matcher(&self) -> bool {
        !self.process.is_empty()
            || self.class.is_some()
            || self.path.is_some()
            || self.package.is_some()
            || self.title.is_some()
    }

    /// Short text for lists: the name if there is one, otherwise the matchers.
    pub fn label(&self) -> String {
        if let Some(name) = self.name.as_ref().filter(|name| !name.is_empty()) {
            return name.clone();
        }
        let mut parts: Vec<String> = Vec::new();
        if !self.process.is_empty() {
            parts.push(self.process.clone());
        }
        if let Some(class) = &self.class {
            parts.push(format!("class {class}"));
        }
        if let Some(package) = &self.package {
            parts.push(format!("package {package}"));
        }
        if let Some(path) = &self.path {
            parts.push(format!("path {path}"));
        }
        if let Some(title) = &self.title {
            parts.push(format!("title \"{title}\""));
        }
        parts.join(" + ")
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    /// 1 = files written by 1.x to 3.0. 2 = 3.5. Missing means 1.
    #[serde(default = "schema_v1")]
    pub schema_version: u32,
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
    #[serde(default)]
    pub legacy: LegacySettings,
    #[serde(default)]
    pub focus: FocusSettings,
    #[serde(default)]
    pub reduce_effects: ReduceEffects,
    /// Optional extras, each independently switchable and off by default. Written to the file
    /// only once something differs from the defaults, so an untouched config stays readable by
    /// 3.5.
    #[serde(default, skip_serializing_if = "Features::is_default")]
    pub features: Features,
    /// How AllBlack11's own windows look. Separate from the look it applies to other apps.
    #[serde(default, skip_serializing_if = "UiSettings::is_default")]
    pub ui: UiSettings,
    /// How AllBlack11 looks for its own updates. Off until the user turns it on.
    #[serde(default, skip_serializing_if = "UpdateSettings::is_default")]
    pub updates: UpdateSettings,
}

/// Settings for AllBlack11's own windows (Settings, palette, setup). Never the look applied to
/// other apps.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct UiSettings {
    /// `system` (follow Windows), `light` or `dark`.
    pub appearance: crate::uitheme::Appearance,
}

impl UiSettings {
    pub fn is_default(&self) -> bool {
        *self == UiSettings::default()
    }
}

/// Update checks. A check contacts GitHub, so the automatic one is off until the user turns it on;
/// a check the user asks for with a button always works. Nothing is ever downloaded or installed
/// without a button press.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct UpdateSettings {
    /// `auto` (a preview build follows previews, a stable build follows stable), `stable`, `preview`.
    pub channel: crate::update::Channel,
    /// Look once a day, in the background.
    pub auto_check: bool,
}

impl UpdateSettings {
    pub fn is_default(&self) -> bool {
        *self == UpdateSettings::default()
    }
}

/// Optional automation. Everything here is off until the user turns it on, and each switch
/// is independent of the others.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct Features {
    /// A stable border color per app, for telling windows apart at a glance.
    pub identity_accents: bool,
    /// Ctrl+Alt+Space opens the command palette from anywhere. The tray item always works.
    pub palette_hotkey: bool,
    pub day_night: DayNight,
}

impl Features {
    pub fn is_default(&self) -> bool {
        *self == Features::default()
    }
}

/// Switch scenes by local time of day. Evaluated when the clock crosses a boundary, when the
/// PC resumes, and when the time or time zone changes. It does not poll and cannot wake the PC.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct DayNight {
    pub enabled: bool,
    /// A scene id (such as `graphite`) or the id of a look in the Look Book.
    pub day_scene: String,
    pub night_scene: String,
    /// Local time, `HH:MM`, 24-hour.
    pub day_starts: String,
    pub night_starts: String,
}

impl Default for DayNight {
    fn default() -> Self {
        Self {
            enabled: false,
            day_scene: "graphite".into(),
            night_scene: "oled-black".into(),
            day_starts: "07:00".into(),
            night_starts: "20:00".into(),
        }
    }
}

fn default_true() -> bool {
    true
}

pub const SCHEMA_VERSION: u32 = 2;

fn schema_v1() -> u32 {
    1
}

/// Focus Look: a restrained active-window accent and calmer inactive windows. Event-driven
/// (foreground changes only), never animated. A rule that sets a color itself wins over it.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct FocusSettings {
    pub enabled: bool,
    pub active_border: Color,
    pub inactive_border: Color,
    pub inactive_text: Color,
}

impl Default for FocusSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            active_border: Color::Accent,
            inactive_border: Color::None,
            inactive_text: Color::Rgb {
                r: 0x9A,
                g: 0x9A,
                b: 0x9A,
            },
        }
    }
}

/// When to fall back to plain colors with no backdrop material. Each condition is a
/// documented system signal, not a guess.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct ReduceEffects {
    pub on_battery_saver: bool,
    pub on_remote_session: bool,
    pub on_transparency_off: bool,
}

impl Default for ReduceEffects {
    fn default() -> Self {
        Self {
            on_battery_saver: true,
            on_remote_session: true,
            on_transparency_off: true,
        }
    }
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
    let mut config: Config = serde_json::from_str(text).map_err(|err| err.to_string())?;
    for (index, rule) in config.rules.iter().enumerate() {
        if !rule.has_matcher() {
            return Err(format!(
                "rule {} has no matcher: set process, class, path, package, or title",
                index + 1
            ));
        }
        if rule.priority.unsigned_abs() > 1000 {
            return Err(format!(
                "rule {} priority should be between -1000 and 1000",
                index + 1
            ));
        }
    }
    crate::schedule::parse_times(
        &config.features.day_night.day_starts,
        &config.features.day_night.night_starts,
    )
    .map_err(|err| format!("features.dayNight: {err}"))?;
    // Older files (schemaVersion missing or 1) load unchanged. The new version is written
    // the next time the app saves, so downgrading before a save loses nothing.
    if config.schema_version > SCHEMA_VERSION {
        return Err(format!(
            "this config was written by a newer AllBlack11 (schemaVersion {}). Update AllBlack11 or restore config.lkg.json",
            config.schema_version
        ));
    }
    config.schema_version = SCHEMA_VERSION;
    Ok(config)
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

/// Resolve the look for a window, or `None` when theming is switched off for it.
/// The full story (which rule won, where each field came from) is `rules::resolve`.
pub fn resolve_style(
    config: &Config,
    identity: &crate::rules::WindowIdentity<'_>,
    accent_bgr: u32,
) -> Option<Style> {
    let resolved = crate::rules::resolve(config, identity, accent_bgr);
    if resolved.enabled {
        Some(resolved.style)
    } else {
        None
    }
}

pub fn exe_matches(pattern: &str, exe: &str) -> bool {
    let pattern = file_name(pattern);
    !pattern.is_empty() && pattern.eq_ignore_ascii_case(exe)
}

pub fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path).trim()
}

pub fn auto_text(caption: Color, accent_bgr: u32) -> Color {
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
        let style = resolve_style(
            &config,
            &crate::rules::WindowIdentity::exe("notepad.exe"),
            0xFF74_8501,
        )
        .unwrap();
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
        let style = resolve_style(
            &config,
            &crate::rules::WindowIdentity::exe("notepad.exe"),
            FALLBACK_ACCENT_BGR,
        )
        .unwrap();
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
        let notepad = resolve_style(
            &config,
            &crate::rules::WindowIdentity::exe("notepad.exe"),
            FALLBACK_ACCENT_BGR,
        )
        .unwrap();
        assert_eq!(notepad.backdrop, Backdrop::MicaAlt);
        assert_eq!(notepad.border, Color::None);
        assert_eq!(notepad.caption.to_colorref(0), 0);
        assert!(
            resolve_style(&config, &crate::rules::WindowIdentity::exe("calc.exe"), 0).is_none()
        );
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
        assert!(
            resolve_style(&config, &crate::rules::WindowIdentity::exe("calc.exe"), 0).is_none()
        );
        let notepad = resolve_style(
            &config,
            &crate::rules::WindowIdentity::exe("notepad.exe"),
            0,
        )
        .unwrap();
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
        let style = resolve_style(&config, &crate::rules::WindowIdentity::exe("a.exe"), 0).unwrap();
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
        let style = resolve_style(
            &config,
            &crate::rules::WindowIdentity::exe("notepad.exe"),
            0,
        )
        .unwrap();
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
        let style =
            resolve_style(&config, &crate::rules::WindowIdentity::exe("mmc.exe"), 0).unwrap();
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

    #[test]
    fn a_3_0_config_loads_unchanged_and_gets_the_new_sections_with_defaults() {
        let old = r##"{
          "comment": "x", "enabled": true, "logging": true, "reapplyOnForeground": true,
          "default": {"enabled": true, "darkMode": true, "backdrop": "mica",
                      "captionColor": "#000000", "borderColor": "accent", "textColor": "#FFFFFF"},
          "shell": {"oledTaskbar": true, "blackShell": false},
          "captionHeight": null, "experimentalDarkLegacy": false,
          "rules": [{"process": "mmc.exe", "backdrop": "tintedAcrylic", "acrylicOpacity": 0.7}],
          "exclude": ["GeForceNOW.exe"], "excludeClasses": []
        }"##;
        let config = parse_config(old).expect("3.0 config parses");
        assert_eq!(config.schema_version, SCHEMA_VERSION);
        assert_eq!(config.rules.len(), 1);
        assert_eq!(config.rules[0].process, "mmc.exe");
        assert!(config.shell.oled_taskbar);
        // New 3.5 sections arrive switched off or at their quiet defaults.
        assert!(!config.focus.enabled);
        assert!(config.reduce_effects.on_battery_saver);
        assert_eq!(config.legacy.palette, "graphite");
        assert!(!config.legacy.emulated_x64);
    }

    #[test]
    fn the_ui_section_is_optional_off_the_file_by_default_and_round_trips() {
        let config = parse_config("{}").unwrap();
        assert_eq!(config.ui.appearance, crate::uitheme::Appearance::System);
        // An untouched config does not grow a `ui` key, so 3.5/3.6 can still read it.
        let text = serde_json::to_string(&config).unwrap();
        assert!(!text.contains("\"ui\""), "{text}");
        let dark = parse_config(r#"{"ui":{"appearance":"dark"}}"#).unwrap();
        assert_eq!(dark.ui.appearance, crate::uitheme::Appearance::Dark);
        let back = parse_config(&serde_json::to_string(&dark).unwrap()).unwrap();
        assert_eq!(back.ui.appearance, crate::uitheme::Appearance::Dark);
        assert!(parse_config(r#"{"ui":{"appearance":"neon"}}"#).is_err());
        assert!(parse_config(r#"{"ui":{"tint":"red"}}"#).is_err());
    }

    #[test]
    fn saving_records_the_schema_version_and_round_trips() {
        let config =
            parse_config(r#"{"rules":[{"class":"Notepad","backdrop":"none","priority":3}]}"#)
                .unwrap();
        let text = serde_json::to_string_pretty(&config).unwrap();
        assert!(text.contains("\"schemaVersion\": 2"), "{text}");
        let again = parse_config(&text).unwrap();
        assert_eq!(again.rules[0].class.as_deref(), Some("Notepad"));
        assert_eq!(again.rules[0].priority, 3);
        // A rule that matches only by class has no empty "process" key in the file.
        assert!(!text.contains("\"process\""));
    }

    #[test]
    fn the_shipped_default_config_parses_and_has_no_rules_without_matchers() {
        let config = parse_config(bundled_default_json()).unwrap();
        assert!(config.rules.iter().all(|rule| rule.has_matcher()));
    }

    #[test]
    fn unknown_fields_and_future_versions_are_reported_not_ignored() {
        let err = parse_config(r#"{"colour":"red"}"#).unwrap_err();
        assert!(err.contains("colour"), "{err}");
        let err = parse_config(r#"{"schemaVersion":3}"#).unwrap_err();
        assert!(err.contains("newer AllBlack11"), "{err}");
        assert!(parse_config(r#"{"focus":{"enabled":true,"bogus":1}}"#).is_err());
        assert!(parse_config(r#"{"rules":[{"process":"a.exe","priority":5000}]}"#).is_err());
    }

    #[test]
    fn pre_3_5_files_are_recognised_so_they_can_be_backed_up_once() {
        assert!(crate::lkg::is_pre_schema_2(r#"{"enabled":true}"#));
        assert!(crate::lkg::is_pre_schema_2(r#"{"schemaVersion":1}"#));
        assert!(!crate::lkg::is_pre_schema_2(r#"{"schemaVersion":2}"#));
        assert!(!crate::lkg::is_pre_schema_2("{broken"));
    }

    #[test]
    fn the_updates_section_is_optional_closed_and_round_trips() {
        let config = parse_config("{}").unwrap();
        assert!(!config.updates.auto_check);
        assert_eq!(config.updates.channel, crate::update::Channel::Auto);
        let text = serde_json::to_string(&config).unwrap();
        assert!(
            !text.contains("updates"),
            "an untouched config does not grow a key: {text}"
        );
        let on = parse_config(r#"{"updates":{"channel":"preview","autoCheck":true}}"#).unwrap();
        assert!(on.updates.auto_check && on.updates.channel == crate::update::Channel::Preview);
        let back = parse_config(&serde_json::to_string(&on).unwrap()).unwrap();
        assert_eq!(back.updates, on.updates);
        assert!(parse_config(r#"{"updates":{"channel":"nightly"}}"#).is_err());
        assert!(parse_config(r#"{"updates":{"url":"https://evil.example"}}"#).is_err());
    }
}
