//! Appearance recipes: small, versioned, local JSON files that describe a look.
//!
//! A recipe is *data only*. There is no field that runs anything, no script, no path to open,
//! no registry key to write, and no shell setting: the schema is closed (`deny_unknown_fields`
//! plus an explicit key allow-list for the look) and every string is length-limited. Bundled
//! recipes ship inside the exe and are trusted; anything imported is validated the same way
//! and is shown as a diff before it is applied.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::{base_style, Config, PartialStyle, Rule, Style};

pub const SCHEMA: u32 = 1;
pub const MAX_BYTES: usize = 32 * 1024;
const MAX_NAME: usize = 60;
const MAX_TEXT: usize = 240;
const MAX_NOTES: usize = 8;

const LOOK_KEYS: [&str; 9] = [
    "darkMode",
    "backdrop",
    "captionColor",
    "borderColor",
    "textColor",
    "corners",
    "acrylicTint",
    "acrylicOpacity",
    "tintedAcrylicOnMoveFallback",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trust {
    /// Ships inside the exe.
    Bundled,
    /// Came from a file the user picked. Validated, previewed, never auto-applied.
    Imported,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecipeMatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Recipe {
    pub schema: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    /// Lowest Windows build this look is meant for. Informational: applying never fails on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_build: Option<u32>,
    pub look: PartialStyle,
    /// Present: the look becomes a per-app rule. Absent: it becomes the default look.
    #[serde(default, rename = "match", skip_serializing_if = "Option::is_none")]
    pub matcher: Option<RecipeMatch>,
    #[serde(default)]
    pub notes: Vec<String>,
}

fn plain(text: &str, max: usize, what: &str) -> Result<(), String> {
    if text.chars().count() > max {
        return Err(format!("{what} is longer than {max} characters"));
    }
    if text.chars().any(|c| c.is_control() && c != '\n') {
        return Err(format!("{what} contains control characters"));
    }
    Ok(())
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 48
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.'))
}

/// Parse and validate. Anything unexpected is an error: a recipe either is exactly what the
/// schema says or it is refused.
pub fn parse_recipe(text: &str) -> Result<Recipe, String> {
    if text.len() > MAX_BYTES {
        return Err(format!("recipe is larger than {} KB", MAX_BYTES / 1024));
    }
    let value: Value =
        serde_json::from_str(text).map_err(|err| format!("not valid JSON: {err}"))?;
    let object = value.as_object().ok_or("recipe must be a JSON object")?;
    let look = object
        .get("look")
        .and_then(Value::as_object)
        .ok_or("recipe needs a \"look\" object")?;
    for key in look.keys() {
        if !LOOK_KEYS.contains(&key.as_str()) {
            return Err(format!(
                "look field \"{key}\" is not allowed. Allowed: {}",
                LOOK_KEYS.join(", ")
            ));
        }
    }
    let recipe: Recipe = serde_json::from_value(value).map_err(|err| err.to_string())?;
    if recipe.schema != SCHEMA {
        return Err(format!(
            "unsupported schema {} (this build reads schema {SCHEMA})",
            recipe.schema
        ));
    }
    if !valid_id(&recipe.id) {
        return Err("id must be 1-48 characters of a-z, 0-9, '-', '_' or '.'".into());
    }
    if recipe.name.trim().is_empty() {
        return Err("name is required".into());
    }
    plain(&recipe.name, MAX_NAME, "name")?;
    plain(&recipe.description, MAX_TEXT, "description")?;
    plain(&recipe.author, MAX_NAME, "author")?;
    if recipe.notes.len() > MAX_NOTES {
        return Err(format!("at most {MAX_NOTES} notes"));
    }
    for note in &recipe.notes {
        plain(note, MAX_TEXT, "a note")?;
    }
    if let Some(matcher) = &recipe.matcher {
        let any = matcher.process.is_some() || matcher.class.is_some() || matcher.package.is_some();
        if !any {
            return Err("\"match\" needs at least one of process, class, package".into());
        }
        for (what, value) in [
            ("match.process", &matcher.process),
            ("match.class", &matcher.class),
            ("match.package", &matcher.package),
        ] {
            if let Some(value) = value {
                plain(value, 128, what)?;
                if value.is_empty() {
                    return Err(format!("{what} is empty"));
                }
            }
        }
        if let Some(process) = &matcher.process {
            if process.contains(['\\', '/', ':']) {
                return Err("match.process is a file name, not a path".into());
            }
        }
    }
    Ok(recipe)
}

impl Recipe {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }

    pub fn target_label(&self) -> String {
        match &self.matcher {
            None => "all apps (default look)".into(),
            Some(m) => {
                let mut parts = Vec::new();
                if let Some(p) = &m.process {
                    parts.push(p.clone());
                }
                if let Some(c) = &m.class {
                    parts.push(format!("class {c}"));
                }
                if let Some(p) = &m.package {
                    parts.push(format!("package {p}"));
                }
                parts.join(" + ")
            }
        }
    }
}

const BUNDLED_JSON: [&str; 7] = [
    include_str!("../recipes/black-frames.json"),
    include_str!("../recipes/graphite.json"),
    include_str!("../recipes/midnight-accent.json"),
    include_str!("../recipes/focus.json"),
    include_str!("../recipes/living-room.json"),
    include_str!("../recipes/teal-glass.json"),
    include_str!("../recipes/low-distraction.json"),
];

/// The recipes that ship with AllBlack11. A failure to parse one is a build bug, caught by a
/// unit test; at runtime a bad one is simply skipped.
pub fn bundled() -> Vec<Recipe> {
    BUNDLED_JSON
        .iter()
        .filter_map(|text| parse_recipe(text).ok())
        .collect()
}

// ---------------------------------------------------------------------------
// Preview and apply
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub field: &'static str,
    pub from: String,
    pub to: String,
}

fn show<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(Value::String(text)) => text,
        Ok(other) => other.to_string(),
        Err(_) => "?".into(),
    }
}

fn fields(style: &Style) -> [(&'static str, String); 9] {
    [
        ("dark title bar", style.dark_mode.to_string()),
        ("backdrop", show(&style.backdrop)),
        ("title bar color", show(&style.caption)),
        ("border color", show(&style.border)),
        ("text color", show(&style.text)),
        ("corners", show(&style.corners)),
        ("acrylic tint", show(&style.acrylic_tint)),
        (
            "acrylic opacity",
            format!("{:.0}%", f64::from(style.acrylic_opacity) / 255.0 * 100.0),
        ),
        (
            "blur while dragging",
            style.tinted_acrylic_on_move_fallback.to_string(),
        ),
    ]
}

fn target_partial<'a>(config: &'a Config, recipe: &Recipe) -> Option<&'a PartialStyle> {
    match &recipe.matcher {
        None => Some(&config.defaults),
        Some(m) => config
            .rules
            .iter()
            .find(|rule| rule_matches_recipe(rule, m))
            .map(|rule| &rule.style),
    }
}

fn rule_matches_recipe(rule: &Rule, m: &RecipeMatch) -> bool {
    let eq = |a: &str, b: &Option<String>| match b {
        Some(b) => a.eq_ignore_ascii_case(b),
        None => a.is_empty(),
    };
    let opt = |a: &Option<String>, b: &Option<String>| match (a, b) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        (None, None) => true,
        _ => false,
    };
    eq(&rule.process, &m.process)
        && opt(&rule.class, &m.class)
        && opt(&rule.package, &m.package)
        && rule.path.is_none()
        && rule.title.is_none()
}

/// What applying the recipe would change, field by field, in terms of the effective look.
pub fn preview(config: &Config, recipe: &Recipe) -> Vec<Change> {
    let before_partial = target_partial(config, recipe).cloned().unwrap_or_default();
    let base = if recipe.matcher.is_some() {
        config.defaults.merge_onto(base_style())
    } else {
        base_style()
    };
    let before = before_partial.merge_onto(base.clone());
    let after = recipe.look.merge_onto(before.clone());
    fields(&before)
        .into_iter()
        .zip(fields(&after))
        .filter(|((_, a), (_, b))| a != b)
        .map(|((field, from), (_, to))| Change { field, from, to })
        .collect()
}

fn overlay(target: &mut PartialStyle, look: &PartialStyle) {
    macro_rules! take {
        ($($field:ident),*) => {$(
            if look.$field.is_some() {
                target.$field = look.$field;
            }
        )*};
    }
    take!(
        dark_mode,
        backdrop,
        caption_color,
        border_color,
        text_color,
        corners,
        acrylic_tint,
        acrylic_opacity,
        tinted_acrylic_on_move_fallback
    );
}

/// Apply the recipe to the config in memory. The caller decides whether to save. Shell
/// settings, startup, exclusions and legacy options are never touched by a recipe.
pub fn apply(config: &mut Config, recipe: &Recipe) -> Vec<Change> {
    let changes = preview(config, recipe);
    match &recipe.matcher {
        None => overlay(&mut config.defaults, &recipe.look),
        Some(m) => {
            let index = config
                .rules
                .iter()
                .position(|rule| rule_matches_recipe(rule, m));
            let index = index.unwrap_or_else(|| {
                let mut rule = Rule::for_process(m.process.as_deref().unwrap_or(""));
                rule.class = m.class.clone();
                rule.package = m.package.clone();
                rule.name = Some(recipe.name.clone());
                rule.style.enabled = Some(true);
                config.rules.push(rule);
                config.rules.len() - 1
            });
            overlay(&mut config.rules[index].style, &recipe.look);
        }
    }
    changes
}

/// A recipe describing the current default look, for export.
pub fn export_current(config: &Config, id: &str, name: &str) -> Recipe {
    let mut look = PartialStyle::default();
    overlay(&mut look, &config.defaults);
    Recipe {
        schema: SCHEMA,
        id: id.to_string(),
        name: name.to_string(),
        description: String::new(),
        author: String::new(),
        min_build: None,
        look,
        matcher: None,
        notes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Backdrop, Color};
    use crate::legacy_policy::contrast_ratio;

    fn minimal(extra_look: &str) -> String {
        format!(r#"{{"schema":1,"id":"x","name":"X","look":{{{extra_look}}}}}"#)
    }

    #[test]
    fn every_bundled_recipe_parses_and_keeps_text_readable() {
        let all = bundled();
        assert_eq!(
            all.len(),
            BUNDLED_JSON.len(),
            "a bundled recipe failed to parse"
        );
        for recipe in &all {
            let look = recipe.look.merge_onto(base_style());
            if let (Some(caption), Some(text)) =
                (look.caption.components(0), look.text.components(0))
            {
                let cr = |(r, g, b): (u8, u8, u8)| {
                    u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16)
                };
                let ratio = contrast_ratio(cr(caption), cr(text));
                assert!(
                    ratio >= 7.0,
                    "{} caption/text contrast {ratio:.1}",
                    recipe.id
                );
            }
            assert!(
                !recipe.notes.is_empty(),
                "{} should say what it is not",
                recipe.id
            );
        }
        // IDs are unique.
        let mut ids: Vec<&str> = all.iter().map(|r| r.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), all.len());
    }

    #[test]
    fn the_schema_is_closed_so_a_recipe_cannot_carry_behavior() {
        for bad in [
            // extra top-level field
            r#"{"schema":1,"id":"x","name":"X","look":{},"run":"cmd.exe"}"#,
            // fields that are not part of a look
            &minimal(r#""enabled":false"#),
            &minimal(r#""exclude":["a.exe"]"#),
            &minimal(r#""shell":{"blackShell":true}"#),
            &minimal(r#""path":"C:\\x""#),
            // values that are not what the field accepts
            &minimal(r#""captionColor":"javascript:alert(1)""#),
            &minimal(r#""backdrop":"cmd /c calc""#),
            &minimal(r#""acrylicOpacity":9"#),
        ] {
            assert!(
                parse_recipe(bad).is_err(),
                "should have been refused: {bad}"
            );
        }
        assert!(parse_recipe(&minimal(r##""captionColor":"#101010""##)).is_ok());
    }

    #[test]
    fn identity_and_text_fields_are_validated() {
        let with = |id: &str, name: &str| {
            format!(r#"{{"schema":1,"id":"{id}","name":"{name}","look":{{}}}}"#)
        };
        assert!(parse_recipe(&with("Bad Id!", "X")).is_err());
        assert!(parse_recipe(&with("", "X")).is_err());
        assert!(parse_recipe(&with("ok", "  ")).is_err());
        assert!(
            parse_recipe(&with("ok", "bell\u{7}")).is_err()
                || parse_recipe(&with("ok", "bell\\u0007")).is_err()
        );
        assert!(parse_recipe(r#"{"schema":2,"id":"x","name":"X","look":{}}"#).is_err());
        assert!(parse_recipe("{not json").is_err());
        assert!(parse_recipe("[]").is_err());
        let huge = format!(
            r#"{{"schema":1,"id":"x","name":"X","look":{{}},"description":"{}"}}"#,
            "a".repeat(MAX_BYTES)
        );
        assert!(parse_recipe(&huge).is_err());
        let long_note = format!(
            r#"{{"schema":1,"id":"x","name":"X","look":{{}},"notes":["{}"]}}"#,
            "n".repeat(300)
        );
        assert!(parse_recipe(&long_note).is_err());
    }

    #[test]
    fn matchers_must_name_something_and_process_is_a_file_name() {
        let base =
            |m: &str| format!(r#"{{"schema":1,"id":"x","name":"X","look":{{}},"match":{m}}}"#);
        assert!(parse_recipe(&base("{}")).is_err());
        assert!(parse_recipe(&base(r#"{"process":"C:\\Windows\\notepad.exe"}"#)).is_err());
        assert!(parse_recipe(&base(r#"{"process":"notepad.exe"}"#)).is_ok());
        assert!(parse_recipe(&base(r#"{"class":"X","run":"y"}"#)).is_err());
    }

    #[test]
    fn preview_lists_only_what_would_change_and_apply_matches_it() {
        let mut config = crate::Config::default();
        let recipe = parse_recipe(
            r##"{"schema":1,"id":"quiet","name":"Quiet","look":{"backdrop":"none","borderColor":"none","textColor":"#BDBDBD"}}"##,
        )
        .unwrap();
        let changes = preview(&config, &recipe);
        let fields: Vec<&str> = changes.iter().map(|c| c.field).collect();
        assert_eq!(fields, vec!["backdrop", "border color", "text color"]);
        assert_eq!(changes[0].from, "mica");
        assert_eq!(changes[0].to, "none");
        let applied = apply(&mut config, &recipe);
        assert_eq!(applied, changes);
        let look = config.defaults.merge_onto(base_style());
        assert_eq!(look.backdrop, Backdrop::None);
        assert_eq!(look.border, Color::None);
        // Applying again changes nothing.
        assert!(preview(&config, &recipe).is_empty());
    }

    #[test]
    fn a_recipe_with_a_matcher_creates_or_updates_one_rule_and_nothing_else() {
        let mut config = crate::Config::default();
        config.shell.black_shell = true;
        config.exclude.push("keep.exe".into());
        let recipe = parse_recipe(
            r#"{"schema":1,"id":"term","name":"Terminal glass","match":{"class":"CASCADIA_HOSTING_WINDOW_CLASS"},"look":{"backdrop":"acrylic"}}"#,
        )
        .unwrap();
        apply(&mut config, &recipe);
        apply(&mut config, &recipe);
        assert_eq!(
            config.rules.len(),
            1,
            "applying twice must not duplicate the rule"
        );
        assert_eq!(
            config.rules[0].class.as_deref(),
            Some("CASCADIA_HOSTING_WINDOW_CLASS")
        );
        assert_eq!(config.rules[0].style.backdrop, Some(Backdrop::Acrylic));
        // Default look, shell and exclusions are untouched.
        assert_eq!(
            config.defaults.backdrop,
            crate::Config::default().defaults.backdrop
        );
        assert!(config.shell.black_shell);
        assert_eq!(config.exclude.last().map(String::as_str), Some("keep.exe"));
    }

    #[test]
    fn export_round_trips_through_the_validator() {
        let mut config = crate::Config::default();
        apply(&mut config, &bundled()[1]);
        let exported = export_current(&config, "mine", "My look");
        let text = exported.to_json();
        let back = parse_recipe(&text).expect("an exported recipe must import");
        assert_eq!(back.id, "mine");
        assert_eq!(back.look.backdrop, exported.look.backdrop);
    }
}
