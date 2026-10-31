//! Scenes and the Look Book.
//!
//! A *scene* is a bundled recipe plus an optional Focus Look setting. A *look* is the same
//! thing saved by the user under a name. Both are applied the same way, and both touch only
//! the **default look and Focus Look**: per-app rules, exclusions, the shell, the legacy layer
//! and startup are never changed, so switching scenes cannot lose a per-app override. A rule
//! that sets a field keeps it whatever the scene says (that is rule precedence, tested in
//! `rules.rs`; the tests here check that a scene switch leaves the rules alone).
//!
//! Honesty: a scene is a *window frame* look. It does not recolor app content and it does not
//! make Windows dark system-wide. The notes say so.

use serde::{Deserialize, Serialize};

use crate::config::{base_style, Color, Config, FocusSettings, Style};
use crate::recipes::{self, Change, Recipe};

pub const MAX_LOOKS: usize = 40;
pub const MAX_NAME: usize = 40;
const MAX_FILE: usize = 256 * 1024;
pub const BOOK_VERSION: u32 = 1;

/// How a scene is described in the gallery.
pub struct SceneDef {
    pub id: &'static str,
    pub name: &'static str,
    pub blurb: &'static str,
    /// What this scene does not do, in the gallery's words.
    pub limit: &'static str,
    recipe_id: &'static str,
    focus: bool,
}

pub const SCENE_COUNT: usize = 5;

pub const SCENES: [SceneDef; SCENE_COUNT] = [
    SceneDef {
        id: "oled-black",
        name: "OLED Black",
        blurb: "True-black title bars, no border, no material.",
        limit: "Frames only. The inside of each window is drawn by the app.",
        recipe_id: "black-frames",
        focus: false,
    },
    SceneDef {
        id: "graphite",
        name: "Graphite",
        blurb: "Dark gray frames on a soft Mica surface.",
        limit: "Mica needs Windows 11 22H2 and is only visible where an app is transparent.",
        recipe_id: "graphite",
        focus: false,
    },
    SceneDef {
        id: "midnight-accent",
        name: "Midnight Accent",
        blurb: "Near-black frames with an accent-colored border.",
        limit: "The border follows your Windows accent color.",
        recipe_id: "midnight-accent",
        focus: false,
    },
    SceneDef {
        id: "focus",
        name: "Focus",
        blurb: "Quiet frames; the active window gets an accent border.",
        limit: "Changes frames when focus moves. Never dims or covers windows.",
        recipe_id: "focus",
        focus: true,
    },
    SceneDef {
        id: "living-room",
        name: "Living Room",
        blurb: "Maximum-contrast frames that read from a distance.",
        limit: "Frames only. It does not enlarge text or buttons inside apps.",
        recipe_id: "living-room",
        focus: false,
    },
];

/// The Focus Look a scene carries. Colors are the documented defaults for Focus Look.
fn scene_focus() -> FocusSettings {
    FocusSettings {
        enabled: true,
        ..FocusSettings::default()
    }
}

/// A scene resolved to its recipe.
#[derive(Debug, Clone)]
pub struct Scene {
    pub id: String,
    pub name: String,
    pub blurb: String,
    pub limit: String,
    pub recipe: Recipe,
    pub focus: Option<FocusSettings>,
}

pub fn catalog() -> Vec<Scene> {
    let recipes = recipes::bundled();
    SCENES
        .iter()
        .filter_map(|def| {
            let recipe = recipes.iter().find(|r| r.id == def.recipe_id)?.clone();
            Some(Scene {
                id: def.id.to_string(),
                name: def.name.to_string(),
                blurb: def.blurb.to_string(),
                limit: def.limit.to_string(),
                recipe,
                focus: def.focus.then(scene_focus),
            })
        })
        .collect()
}

/// A look as the apply/preview functions see it, whether it came from a scene or the book.
pub trait Applicable {
    fn recipe(&self) -> &Recipe;
    fn focus(&self) -> Option<&FocusSettings>;
}

impl Applicable for Scene {
    fn recipe(&self) -> &Recipe {
        &self.recipe
    }
    fn focus(&self) -> Option<&FocusSettings> {
        self.focus.as_ref()
    }
}

fn focus_after(config: &Config, focus: Option<&FocusSettings>) -> FocusSettings {
    match focus {
        Some(settings) => settings.clone(),
        None => FocusSettings {
            enabled: false,
            ..config.focus.clone()
        },
    }
}

/// What applying would change, as lines for a diff. Includes Focus Look.
pub fn preview(config: &Config, item: &dyn Applicable) -> Vec<Change> {
    let mut changes = recipes::preview(config, item.recipe());
    let after = focus_after(config, item.focus());
    if after != config.focus {
        changes.push(Change {
            field: "Focus Look",
            from: if config.focus.enabled { "on" } else { "off" }.to_string(),
            to: if after.enabled { "on" } else { "off" }.to_string(),
        });
    }
    changes
}

/// Apply in memory. Only the default look and Focus Look change; the caller saves.
pub fn apply(config: &mut Config, item: &dyn Applicable) -> Vec<Change> {
    let changes = preview(config, item);
    let recipe = Recipe {
        matcher: None,
        ..item.recipe().clone()
    };
    recipes::apply(config, &recipe);
    config.focus = focus_after(config, item.focus());
    changes
}

/// The id of the scene the current default look and Focus Look already match, if any.
pub fn active(config: &Config) -> Option<String> {
    catalog()
        .into_iter()
        .find(|scene| preview(config, scene).is_empty())
        .map(|scene| scene.id)
}

// ---------------------------------------------------------------------------
// Swatches (readable previews without capturing anything)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Swatch {
    /// COLORREF (0x00BBGGRR), or `None` when the look draws nothing there.
    pub caption: Option<u32>,
    pub border: Option<u32>,
    pub text: Option<u32>,
}

fn colorref(color: Color, accent_bgr: u32) -> Option<u32> {
    color
        .components(accent_bgr)
        .map(|(r, g, b)| u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16))
}

/// The colors a look gives frames, for painting a preview chip. Computed from the recipe, not
/// from the screen.
pub fn swatch(item: &dyn Applicable, accent_bgr: u32) -> Swatch {
    let style: Style = item.recipe().look.merge_onto(base_style());
    Swatch {
        caption: colorref(style.caption, accent_bgr),
        border: colorref(style.border, accent_bgr),
        text: colorref(style.text, accent_bgr),
    }
}

// ---------------------------------------------------------------------------
// Look Book
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Look {
    pub id: String,
    pub name: String,
    /// Unix seconds.
    pub created: u64,
    pub recipe: Recipe,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<FocusSettings>,
}

impl PartialEq for Recipe {
    fn eq(&self, other: &Self) -> bool {
        self.to_json() == other.to_json()
    }
}

impl Applicable for Look {
    fn recipe(&self) -> &Recipe {
        &self.recipe
    }
    fn focus(&self) -> Option<&FocusSettings> {
        self.focus.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LookBook {
    pub version: u32,
    #[serde(default)]
    pub looks: Vec<Look>,
}

impl Default for LookBook {
    fn default() -> Self {
        Self {
            version: BOOK_VERSION,
            looks: Vec::new(),
        }
    }
}

fn clean_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Give the look a name.".into());
    }
    if name.chars().count() > MAX_NAME {
        return Err(format!("Names can be at most {MAX_NAME} characters."));
    }
    if name.chars().any(char::is_control) {
        return Err("Names cannot contain control characters.".into());
    }
    Ok(name.to_string())
}

fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-');
    let base = if trimmed.is_empty() { "look" } else { trimmed };
    base.chars().take(30).collect()
}

impl LookBook {
    pub fn parse(text: &str) -> Result<LookBook, String> {
        if text.trim().is_empty() {
            return Ok(LookBook::default());
        }
        if text.len() > MAX_FILE {
            return Err("the look book is larger than 256 KB".into());
        }
        // Check each recipe as raw JSON first: the typed look drops keys it does not know, so
        // validating only after deserializing would let a foreign field through unnoticed.
        let raw: serde_json::Value = serde_json::from_str(text).map_err(|err| err.to_string())?;
        if let Some(items) = raw.get("looks").and_then(serde_json::Value::as_array) {
            if items.len() > MAX_LOOKS {
                return Err(format!("at most {MAX_LOOKS} looks"));
            }
            for item in items {
                let recipe = item.get("recipe").ok_or("a look has no recipe")?;
                recipes::parse_recipe(&recipe.to_string())
                    .map_err(|err| format!("a saved look is not valid: {err}"))?;
            }
        }
        let book: LookBook = serde_json::from_str(text).map_err(|err| err.to_string())?;
        if book.version != BOOK_VERSION {
            return Err(format!(
                "look book version {} is not supported",
                book.version
            ));
        }
        if book.looks.len() > MAX_LOOKS {
            return Err(format!("at most {MAX_LOOKS} looks"));
        }
        let mut seen: Vec<&str> = Vec::new();
        for look in &book.looks {
            clean_name(&look.name)?;
            if seen.contains(&look.id.as_str()) {
                return Err(format!("the id {} is used twice", look.id));
            }
            seen.push(&look.id);
            // The saved recipe passes the same checks as an imported one.
            recipes::parse_recipe(&look.recipe.to_json())
                .map_err(|err| format!("look “{}”: {err}", look.name))?;
        }
        Ok(book)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }

    /// Save what the default look and Focus Look are right now under a name.
    pub fn add(&mut self, name: &str, config: &Config, now: u64) -> Result<String, String> {
        let name = clean_name(name)?;
        if self.looks.len() >= MAX_LOOKS {
            return Err(format!(
                "The Look Book holds {MAX_LOOKS} looks. Delete one first."
            ));
        }
        let base = slug(&name);
        let mut id = base.clone();
        let mut counter = 2;
        while self.looks.iter().any(|look| look.id == id) {
            id = format!("{base}-{counter}");
            counter += 1;
        }
        let mut recipe = recipes::export_current(config, &id, &name);
        recipe.description = "Saved from the Look Book.".to_string();
        recipe.notes = vec!["Saved on this PC. Frames only.".to_string()];
        recipes::parse_recipe(&recipe.to_json())?;
        self.looks.push(Look {
            id: id.clone(),
            name,
            created: now,
            recipe,
            focus: config.focus.enabled.then(|| config.focus.clone()),
        });
        Ok(id)
    }

    pub fn rename(&mut self, id: &str, name: &str) -> Result<(), String> {
        let name = clean_name(name)?;
        let look = self
            .looks
            .iter_mut()
            .find(|look| look.id == id)
            .ok_or("That look no longer exists.")?;
        look.name = name.clone();
        look.recipe.name = name;
        Ok(())
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.looks.len();
        self.looks.retain(|look| look.id != id);
        self.looks.len() != before
    }

    pub fn get(&self, id: &str) -> Option<&Look> {
        self.looks.iter().find(|look| look.id == id)
    }
}

/// What switching from look `a` to look `b` would change, in the same terms as a scene diff.
pub fn compare(a: &dyn Applicable, b: &dyn Applicable) -> Vec<Change> {
    let mut config = Config::default();
    apply(&mut config, a);
    preview(&config, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{parse_config, Rule};
    use crate::rules::{resolve, WindowIdentity};

    fn config_with_rules() -> Config {
        let mut config = Config::default();
        let mut rule = Rule::for_process("notepad.exe");
        rule.style.enabled = Some(true);
        rule.style.caption_color = Some(Color::Rgb {
            r: 0x20,
            g: 0x00,
            b: 0x40,
        });
        config.rules.push(rule);
        config.exclude.push("steam.exe".to_string());
        config.shell.oled_taskbar = true;
        config.experimental_dark_legacy = true;
        config.legacy.palette = "black".into();
        config
    }

    fn style_for(config: &Config, exe: &str) -> Style {
        resolve(
            config,
            &WindowIdentity {
                exe,
                class: "",
                path: "",
                package: "",
                title: None,
            },
            0x00FF_8000,
        )
        .style
    }

    #[test]
    fn the_five_scenes_exist_and_resolve_to_real_recipes() {
        let scenes = catalog();
        let names: Vec<&str> = scenes.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "OLED Black",
                "Graphite",
                "Midnight Accent",
                "Focus",
                "Living Room"
            ]
        );
        let mut ids: Vec<&str> = scenes.iter().map(|s| s.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 5);
        for scene in &scenes {
            assert!(
                scene.recipe.matcher.is_none(),
                "{} must target the default look",
                scene.id
            );
            assert!(
                !scene.limit.is_empty(),
                "{} should say what it is not",
                scene.id
            );
        }
        // No scene claims more than frames: the words "system-wide" and "everything" are out.
        for scene in &scenes {
            let text = format!(
                "{} {} {}",
                scene.blurb, scene.limit, scene.recipe.description
            )
            .to_lowercase();
            for banned in ["system-wide", "everything", "every app", "all apps"] {
                assert!(!text.contains(banned), "{}: {banned}", scene.id);
            }
        }
    }

    #[test]
    fn switching_scenes_leaves_rules_exclusions_shell_and_legacy_alone() {
        let before = config_with_rules();
        for scene in catalog() {
            let mut config = before.clone();
            apply(&mut config, &scene);
            assert_eq!(config.rules.len(), before.rules.len(), "{}", scene.id);
            assert_eq!(config.rules[0].process, "notepad.exe");
            assert_eq!(
                config.rules[0].style.caption_color,
                before.rules[0].style.caption_color
            );
            assert_eq!(config.exclude, before.exclude);
            assert_eq!(config.shell, before.shell);
            assert_eq!(
                config.experimental_dark_legacy,
                before.experimental_dark_legacy
            );
            assert_eq!(config.legacy, before.legacy);
        }
    }

    #[test]
    fn a_per_app_override_still_wins_after_a_scene_is_applied() {
        let mut config = config_with_rules();
        let graphite = catalog().into_iter().find(|s| s.id == "graphite").unwrap();
        apply(&mut config, &graphite);
        let notepad = style_for(&config, "notepad.exe");
        assert_eq!(
            notepad.caption,
            Color::Rgb {
                r: 0x20,
                g: 0x00,
                b: 0x40
            },
            "the rule's own caption color must survive the scene"
        );
        // An app with no rule follows the scene.
        let other = style_for(&config, "calc.exe");
        assert_eq!(
            other.caption,
            Color::Rgb {
                r: 0x1A,
                g: 0x1A,
                b: 0x1A
            }
        );
    }

    #[test]
    fn switching_back_and_forth_is_stable_and_detected() {
        let mut config = Config::default();
        let scenes = catalog();
        let oled = &scenes[0];
        let graphite = &scenes[1];
        apply(&mut config, oled);
        assert_eq!(active(&config).as_deref(), Some("oled-black"));
        assert!(
            preview(&config, oled).is_empty(),
            "re-applying changes nothing"
        );
        apply(&mut config, graphite);
        assert_eq!(active(&config).as_deref(), Some("graphite"));
        apply(&mut config, oled);
        assert_eq!(active(&config).as_deref(), Some("oled-black"));
        // A manual change means no scene is active any more.
        config.defaults.caption_color = Some(Color::Rgb { r: 1, g: 2, b: 3 });
        assert_eq!(active(&config), None);
    }

    #[test]
    fn the_focus_scene_owns_focus_look_and_others_turn_it_off() {
        let mut config = Config::default();
        let scenes = catalog();
        let focus = scenes.iter().find(|s| s.id == "focus").unwrap();
        let changes = apply(&mut config, focus);
        assert!(config.focus.enabled);
        assert!(changes
            .iter()
            .any(|c| c.field == "Focus Look" && c.to == "on"));
        let graphite = scenes.iter().find(|s| s.id == "graphite").unwrap();
        let changes = apply(&mut config, graphite);
        assert!(!config.focus.enabled);
        assert!(changes
            .iter()
            .any(|c| c.field == "Focus Look" && c.to == "off"));
        // The user's chosen Focus colors are kept when it is turned off.
        config.focus.inactive_text = Color::Rgb { r: 9, g: 9, b: 9 };
        apply(&mut config, graphite);
        assert_eq!(config.focus.inactive_text, Color::Rgb { r: 9, g: 9, b: 9 });
    }

    #[test]
    fn swatches_come_from_the_recipe_and_follow_the_accent() {
        let scenes = catalog();
        let midnight = scenes.iter().find(|s| s.id == "midnight-accent").unwrap();
        let chip = swatch(midnight, 0x0000_FF00);
        assert_eq!(
            chip.border,
            Some(0x0000_FF00),
            "accent border follows the accent"
        );
        assert_eq!(chip.caption, Some(0x000D_0705));
        let oled = swatch(&scenes[0], 0);
        assert_eq!(oled.border, None, "no border is drawn");
        assert_eq!(oled.caption, Some(0));
    }

    #[test]
    fn the_look_book_saves_renames_deletes_and_applies() {
        let mut config = Config::default();
        apply(&mut config, &catalog()[2]);
        let mut book = LookBook::default();
        let id = book.add("  Desk setup ", &config, 100).unwrap();
        assert_eq!(id, "desk-setup");
        assert_eq!(book.get(&id).unwrap().name, "Desk setup");
        let again = book.add("Desk setup", &config, 101).unwrap();
        assert_eq!(again, "desk-setup-2");
        book.rename(&id, "Night desk").unwrap();
        assert_eq!(book.get(&id).unwrap().recipe.name, "Night desk");
        // Round trip through the file format.
        let loaded = LookBook::parse(&book.to_json()).unwrap();
        assert_eq!(loaded, book);
        // Applying a saved look reproduces the saved default look.
        let mut elsewhere = Config::default();
        apply(&mut elsewhere, book.get(&id).unwrap());
        assert_eq!(
            style_for(&elsewhere, "x.exe").caption,
            style_for(&config, "x.exe").caption
        );
        assert!(book.remove(&id));
        assert!(!book.remove(&id), "deleting twice is harmless");
        assert!(book.get(&id).is_none());
    }

    #[test]
    fn names_are_checked_and_the_book_is_bounded() {
        let config = Config::default();
        let mut book = LookBook::default();
        assert!(book.add("   ", &config, 1).is_err());
        assert!(book.add("bad\nname", &config, 1).is_err());
        assert!(book.add(&"x".repeat(MAX_NAME + 1), &config, 1).is_err());
        assert!(book.rename("nope", "Fine").is_err());
        for n in 0..MAX_LOOKS {
            book.add(&format!("Look {n}"), &config, 1).unwrap();
        }
        let err = book.add("One too many", &config, 1).unwrap_err();
        assert!(err.contains("Delete one first"));
        assert_eq!(book.looks.len(), MAX_LOOKS);
    }

    #[test]
    fn a_damaged_or_hostile_book_is_refused_not_trusted() {
        assert!(LookBook::parse("").unwrap().looks.is_empty());
        for bad in [
            "not json",
            "[]",
            r#"{"version":9,"looks":[]}"#,
            r#"{"version":1,"looks":[],"run":"calc"}"#,
            // a look whose recipe carries a field outside the closed look schema
            r#"{"version":1,"looks":[{"id":"a","name":"A","created":1,"recipe":{"schema":1,"id":"a","name":"A","look":{"exclude":["x.exe"]}}}]}"#,
            // an unknown field on the look itself
            r#"{"version":1,"looks":[{"id":"a","name":"A","created":1,"command":"x","recipe":{"schema":1,"id":"a","name":"A","look":{}}}]}"#,
        ] {
            assert!(LookBook::parse(bad).is_err(), "{bad}");
        }
        let dup = r#"{"version":1,"looks":[
            {"id":"a","name":"A","created":1,"recipe":{"schema":1,"id":"a","name":"A","look":{}}},
            {"id":"a","name":"B","created":2,"recipe":{"schema":1,"id":"a","name":"B","look":{}}}]}"#;
        assert!(LookBook::parse(dup).unwrap_err().contains("twice"));
        assert!(LookBook::parse(&" ".repeat(MAX_FILE + 1).replace(' ', "{")).is_err());
    }

    #[test]
    fn comparing_two_looks_lists_only_what_differs() {
        let scenes = catalog();
        let diff = compare(&scenes[0], &scenes[2]);
        let fields: Vec<&str> = diff.iter().map(|c| c.field).collect();
        assert!(fields.contains(&"title bar color"));
        assert!(fields.contains(&"border color"));
        assert!(compare(&scenes[0], &scenes[0]).is_empty());
    }

    #[test]
    fn the_default_config_file_still_parses_after_a_scene_is_saved() {
        let mut config = Config::default();
        apply(&mut config, &catalog()[3]);
        let text = serde_json::to_string_pretty(&config).unwrap();
        let back = parse_config(&text).unwrap();
        assert!(back.focus.enabled);
    }
}
