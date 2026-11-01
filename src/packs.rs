//! Compatibility packs: small declarative files that say how AllBlack11 should treat one app that
//! needs special handling, and, just as important, what it cannot do for that app.
//!
//! A pack is data only. There is no field that runs anything, writes the registry, opens a path or
//! changes the shell; the schema is closed (`deny_unknown_fields` plus the same look allow-list as
//! recipes); every string is length-limited; the whole file is size-bounded.
//!
//! What a pack can do is exactly what a per-app rule can do: window-level appearance (title bar
//! and border colors, dark title bar, a backdrop where Windows accepts it) for windows that match
//! the pack's targets. It cannot recolor what an app draws itself, and every pack must say so:
//! `limits` is required and `verdict` says `supported`, `partial` or `unsupported`.
//!
//! Installing a pack adds ordinary rules named `Pack: <name>` at priority [`PACK_PRIORITY`], which
//! is below the default 0, so **a rule of the user's that matches the same window always wins**;
//! the user can raise the priority of a pack rule to make it win. Removing a pack deletes exactly
//! those rules. Both go through the normal config path, so look history and Undo see them.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::{Config, PartialStyle, Rule};
use crate::recipes::{plain, valid_id, LOOK_KEYS};

pub const SCHEMA: u32 = 1;
pub const MAX_BYTES: usize = 24 * 1024;
pub const PACK_PRIORITY: i32 = -10;
pub const RULE_PREFIX: &str = "Pack: ";
const MAX_NAME: usize = 60;
const MAX_TEXT: usize = 400;
const MAX_TARGETS: usize = 4;
const MAX_LINES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    /// Window-level styling is expected to work for this app.
    Supported,
    /// Only part of the window can change; `reason` says which part cannot.
    Partial,
    /// Nothing can be done from outside. No rule is added, whatever the look says.
    Unsupported,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Supported => "Supported",
            Verdict::Partial => "Partly supported",
            Verdict::Unsupported => "Not supported",
        }
    }
}

/// What a pack's rule matches. At least one of `package`, `process` must be set; `class` narrows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Target {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pack {
    pub schema: u32,
    pub id: String,
    pub name: String,
    /// The pack's own revision, shown to people ("1", "1.1").
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    pub targets: Vec<Target>,
    pub verdict: Verdict,
    /// Why the verdict is not "supported". Required for partial and unsupported.
    #[serde(default)]
    pub reason: String,
    /// The look applied to the targets. Must be empty for an unsupported pack.
    #[serde(default)]
    pub look: PartialStyle,
    /// What this pack cannot do, in the words the user will read. At least one.
    pub limits: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
    /// What was actually tried, and where. "Not tested" is a valid, honest entry.
    #[serde(default)]
    pub tested: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_build: Option<u32>,
}

fn pattern_ok(text: &str, what: &str) -> Result<(), String> {
    plain(text, 120, what)?;
    if text.is_empty()
        || !text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '*' | '?'))
    {
        return Err(format!(
            "{what} may use only letters, digits and . _ - * ? (no paths, no spaces)"
        ));
    }
    Ok(())
}

fn look_is_empty(look: &PartialStyle) -> bool {
    matches!(serde_json::to_value(look), Ok(Value::Object(map)) if map.is_empty())
}

/// Parse and validate. A pack is exactly what the schema says or it is refused.
pub fn parse_pack(text: &str) -> Result<Pack, String> {
    if text.len() > MAX_BYTES {
        return Err(format!("the pack is larger than {} KB", MAX_BYTES / 1024));
    }
    let value: Value =
        serde_json::from_str(text).map_err(|err| format!("not valid JSON: {err}"))?;
    let object = value.as_object().ok_or("a pack must be a JSON object")?;
    if let Some(look) = object.get("look") {
        let look = look.as_object().ok_or("look must be an object")?;
        for key in look.keys() {
            if !LOOK_KEYS.contains(&key.as_str()) {
                return Err(format!(
                    "look field \"{key}\" is not allowed. Allowed: {}",
                    LOOK_KEYS.join(", ")
                ));
            }
        }
    }
    let pack: Pack = serde_json::from_value(value).map_err(|err| err.to_string())?;
    if pack.schema != SCHEMA {
        return Err(format!(
            "unsupported pack schema {} (this build reads schema {SCHEMA})",
            pack.schema
        ));
    }
    if !valid_id(&pack.id) {
        return Err("id must be 1-48 characters of a-z, 0-9, '-', '_' or '.'".into());
    }
    if pack.name.trim().is_empty() {
        return Err("name is required".into());
    }
    plain(&pack.name, MAX_NAME, "name")?;
    if pack.name.contains(['(', ')']) {
        return Err("name may not contain parentheses".into());
    }
    plain(&pack.version, 16, "version")?;
    if pack.version.trim().is_empty() {
        return Err("version is required".into());
    }
    plain(&pack.description, MAX_TEXT, "description")?;
    plain(&pack.author, MAX_NAME, "author")?;
    plain(&pack.reason, MAX_TEXT, "reason")?;
    if pack.targets.is_empty() || pack.targets.len() > MAX_TARGETS {
        return Err(format!("a pack needs 1 to {MAX_TARGETS} targets"));
    }
    for target in &pack.targets {
        if target.package.is_none() && target.process.is_none() {
            return Err(
                "every target needs a package or a process (a class alone is too broad)".into(),
            );
        }
        for (value, what) in [
            (&target.package, "package"),
            (&target.process, "process"),
            (&target.class, "class"),
        ] {
            if let Some(value) = value {
                pattern_ok(value, what)?;
            }
        }
    }
    if pack.limits.is_empty() {
        return Err("limits is required: say what this pack cannot do".into());
    }
    for (list, what) in [
        (&pack.limits, "limits"),
        (&pack.notes, "notes"),
        (&pack.tested, "tested"),
    ] {
        if list.len() > MAX_LINES {
            return Err(format!("at most {MAX_LINES} lines in {what}"));
        }
        for line in list {
            plain(line, MAX_TEXT, what)?;
        }
    }
    let empty_look = look_is_empty(&pack.look);
    match pack.verdict {
        Verdict::Unsupported => {
            if !empty_look {
                return Err(
                    "an unsupported pack must have an empty look: it may not force a change".into(),
                );
            }
            if pack.reason.trim().is_empty() {
                return Err("an unsupported pack must say why in reason".into());
            }
        }
        Verdict::Partial => {
            if pack.reason.trim().is_empty() {
                return Err("a partial pack must say what cannot change in reason".into());
            }
            if empty_look {
                return Err("a partial pack needs a look".into());
            }
        }
        Verdict::Supported => {
            if empty_look {
                return Err("a supported pack needs a look".into());
            }
        }
    }
    Ok(pack)
}

// ---------------------------------------------------------------------------------------------
// Built-in catalog
// ---------------------------------------------------------------------------------------------

const STORE: &str = r##"{
  "schema": 1, "id": "microsoft-store", "name": "Microsoft Store", "version": "1",
  "description": "Dark title bar and border for the Microsoft Store window.",
  "author": "AllBlack11",
  "targets": [ { "package": "Microsoft.WindowsStore_*", "class": "ApplicationFrameWindow" } ],
  "verdict": "partial",
  "reason": "Only the window frame (title bar and border) can change. Everything inside the window is drawn by the Store itself.",
  "look": { "darkMode": true, "captionColor": "#000000", "textColor": "#FFFFFF", "borderColor": "none" },
  "limits": [
    "Client content of the Store app is rendered by the app itself and cannot be recolored externally.",
    "No backdrop is requested: the Store paints an opaque surface over it, so a backdrop would not show."
  ],
  "notes": [ "The window is found by the app's package identity, read from the app inside the frame." ],
  "tested": [ "Not yet tested on Windows hardware. If caption colors turn out not to apply to this window, this pack will be changed to Not supported with the observed reason." ]
}"##;

const CALCULATOR: &str = r##"{
  "schema": 1, "id": "windows-calculator", "name": "Calculator", "version": "1",
  "description": "Dark title bar and border for Windows Calculator.",
  "author": "AllBlack11",
  "targets": [ { "package": "Microsoft.WindowsCalculator_*", "class": "ApplicationFrameWindow" } ],
  "verdict": "partial",
  "reason": "Only the window frame (title bar and border) can change. The keys and display are drawn by Calculator itself.",
  "look": { "darkMode": true, "captionColor": "#000000", "textColor": "#FFFFFF", "borderColor": "none" },
  "limits": [
    "Calculator's content is drawn by the app's own XAML and cannot be recolored from outside. Calculator being open source does not change that: this pack never touches its code.",
    "No backdrop is requested: Calculator paints an opaque surface over it."
  ],
  "notes": [ "Calculator follows the Windows app theme for its own content; set the app theme in Windows if you want that dark too." ],
  "tested": [ "Not yet tested on Windows hardware. If caption colors turn out not to apply to this window, this pack will be changed to Not supported with the observed reason." ]
}"##;

const CATALOG_JSON: [&str; 2] = [STORE, CALCULATOR];

/// The packs that ship inside the exe. They are trusted only as far as any pack is: they go through
/// the same validation, and a pack that fails it is left out rather than half-used.
pub fn catalog() -> Vec<Pack> {
    CATALOG_JSON
        .iter()
        .filter_map(|text| parse_pack(text).ok())
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Install, remove, inspect
// ---------------------------------------------------------------------------------------------

/// The name given to the rule for target `index` of `pack`.
fn rule_name(pack: &Pack, index: usize) -> String {
    if index == 0 {
        format!("{RULE_PREFIX}{}", pack.name)
    } else {
        format!("{RULE_PREFIX}{} ({})", pack.name, index + 1)
    }
}

/// Whether `rule` was added by the pack called `name` (exact name, or `name (n)`).
fn owned_by(rule: &Rule, name: &str) -> bool {
    let Some(label) = rule
        .name
        .as_deref()
        .and_then(|n| n.strip_prefix(RULE_PREFIX))
    else {
        return false;
    };
    if label == name {
        return true;
    }
    label
        .strip_prefix(name)
        .and_then(|rest| rest.strip_prefix(" ("))
        .and_then(|rest| rest.strip_suffix(')'))
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub name: String,
    /// Indexes into `config.rules`.
    pub rules: Vec<usize>,
}

/// Packs present in the config, found by the rules they own.
pub fn installed(config: &Config) -> Vec<Installed> {
    let mut found: Vec<Installed> = Vec::new();
    for (index, rule) in config.rules.iter().enumerate() {
        let Some(label) = rule
            .name
            .as_deref()
            .and_then(|n| n.strip_prefix(RULE_PREFIX))
        else {
            continue;
        };
        let base = match label.rfind(" (") {
            Some(at)
                if label.ends_with(')')
                    && label[at + 2..label.len() - 1]
                        .chars()
                        .all(|c| c.is_ascii_digit()) =>
            {
                &label[..at]
            }
            _ => label,
        };
        match found.iter_mut().find(|entry| entry.name == base) {
            Some(entry) => entry.rules.push(index),
            None => found.push(Installed {
                name: base.to_string(),
                rules: vec![index],
            }),
        }
    }
    found
}

pub fn is_installed(config: &Config, pack: &Pack) -> bool {
    config.rules.iter().any(|rule| owned_by(rule, &pack.name))
}

/// Why this pack cannot be installed here, if it cannot.
pub fn unavailable(pack: &Pack, windows_build: u32) -> Option<String> {
    if pack.verdict == Verdict::Unsupported {
        return Some(format!("Not supported: {}", pack.reason));
    }
    match pack.min_build {
        Some(min) if windows_build < min => Some(format!(
            "Needs Windows build {min} or newer; this PC is on build {windows_build}."
        )),
        _ => None,
    }
}

/// The words for what a look sets, for the page.
pub fn look_actions(look: &PartialStyle) -> Vec<String> {
    let Ok(Value::Object(map)) = serde_json::to_value(look) else {
        return Vec::new();
    };
    let mut actions = Vec::new();
    for key in LOOK_KEYS {
        let Some(value) = map.get(key) else { continue };
        let name = match key {
            "darkMode" => "dark title bar",
            "backdrop" => "backdrop",
            "captionColor" => "title bar color",
            "borderColor" => "border color",
            "textColor" => "title text color",
            "corners" => "corners",
            "acrylicTint" => "acrylic tint",
            "acrylicOpacity" => "acrylic opacity",
            _ => "blur while dragging",
        };
        let shown = match value {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        };
        actions.push(format!("{name}: {shown}"));
    }
    actions
}

/// Who a pack's targets are, in words.
pub fn scope_text(pack: &Pack) -> String {
    pack.targets
        .iter()
        .map(|t| {
            let mut parts = Vec::new();
            if let Some(package) = &t.package {
                parts.push(format!("package {package}"));
            }
            if let Some(process) = &t.process {
                parts.push(format!("program {process}"));
            }
            if let Some(class) = &t.class {
                parts.push(format!("window class {class}"));
            }
            parts.join(", ")
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Remove every rule this pack owns. Returns how many were removed.
pub fn remove(config: &mut Config, pack_name: &str) -> usize {
    let before = config.rules.len();
    config.rules.retain(|rule| !owned_by(rule, pack_name));
    before - config.rules.len()
}

/// Add the pack's rules (replacing any earlier install of the same pack). Returns plain lines
/// saying what was added and what, if anything, of the user's would win over it.
pub fn install(
    config: &mut Config,
    pack: &Pack,
    windows_build: u32,
) -> Result<Vec<String>, String> {
    if let Some(reason) = unavailable(pack, windows_build) {
        return Err(reason);
    }
    let replaced = remove(config, &pack.name);
    let mut lines = Vec::new();
    if replaced > 0 {
        lines.push(format!(
            "Replaced the {replaced} rule(s) of the earlier install."
        ));
    }
    let first = config.rules.len();
    for (index, target) in pack.targets.iter().enumerate() {
        let mut rule = Rule::for_process(target.process.as_deref().unwrap_or(""));
        rule.package = target.package.clone();
        rule.class = target.class.clone();
        rule.priority = PACK_PRIORITY;
        rule.name = Some(rule_name(pack, index));
        rule.style = pack.look.clone();
        rule.style.enabled = Some(true);
        lines.push(format!(
            "Added rule \"{}\" for {} (priority {PACK_PRIORITY}, below your own rules).",
            rule_name(pack, index),
            scope_text(&Pack {
                targets: vec![target.clone()],
                ..pack.clone()
            })
        ));
        config.rules.push(rule);
    }
    // A rule of the user's that matches the same window wins; say so instead of hiding it.
    for (index, target) in pack.targets.iter().enumerate() {
        let identity = crate::rules::WindowIdentity {
            exe: target.process.as_deref().unwrap_or(""),
            class: target.class.as_deref().unwrap_or(""),
            path: "",
            package: target.package.as_deref().unwrap_or(""),
            title: None,
        };
        if let Some(winner) = crate::rules::rank(&config.rules, &identity).first() {
            if winner.index < first || winner.index >= first + pack.targets.len() {
                lines.push(format!(
                    "Your rule {} also matches target {} and wins, so this pack does nothing there until you change it.",
                    winner.index + 1,
                    index + 1
                ));
            }
        }
    }
    Ok(lines)
}

/// What installing would do, without doing it.
pub fn preview(config: &Config, pack: &Pack, windows_build: u32) -> Vec<String> {
    let mut copy = config.clone();
    match install(&mut copy, pack, windows_build) {
        Ok(lines) => lines,
        Err(reason) => vec![reason],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse_config;
    use crate::rules::{rank, WindowIdentity};

    fn base(extra: &str) -> String {
        format!(
            r##"{{"schema":1,"id":"t","name":"T","version":"1","targets":[{{"package":"Pkg_*"}}],
               "verdict":"supported","limits":["It cannot recolor the content."],
               "look":{{"darkMode":true,"captionColor":"#000000"}}{extra}}}"##
        )
    }

    #[test]
    fn the_built_in_catalog_parses_and_is_honest() {
        let all = catalog();
        assert_eq!(
            all.len(),
            CATALOG_JSON.len(),
            "a built-in pack failed validation"
        );
        for pack in &all {
            assert!(!pack.limits.is_empty());
            let limits = pack.limits.join(" ");
            assert!(
                limits.contains("cannot be recolored")
                    || limits.contains("cannot be recolored from outside"),
                "{limits}"
            );
            assert!(pack.reason.contains("drawn by"), "{}", pack.reason);
            assert!(
                pack.look.backdrop.is_none(),
                "no backdrop is claimed for opaque XAML apps"
            );
            assert!(
                pack.tested.iter().any(|t| t.contains("Not yet tested")),
                "must not claim a test"
            );
            assert!(pack.targets.iter().all(
                |t| t.package.is_some() && t.class.as_deref() == Some("ApplicationFrameWindow")
            ));
        }
        assert!(
            all.iter().any(|p| p.id == "microsoft-store")
                && all.iter().any(|p| p.id == "windows-calculator")
        );
        let calculator = all.iter().find(|p| p.id == "windows-calculator").unwrap();
        assert!(
            calculator.limits.join(" ").contains("open source"),
            "the open-source note is there and does not overpromise"
        );
    }

    #[test]
    fn a_pack_is_data_only_and_the_schema_is_closed() {
        assert!(parse_pack(&base("")).is_ok());
        for (extra, why) in [
            (r#","script":"calc.exe""#, "unknown field"),
            (r#","run":["x"]"#, "unknown field"),
        ] {
            assert!(parse_pack(&base(extra)).is_err(), "{why}");
        }
        // The look is the recipe allow-list: nothing outside it, such as a shell or registry field.
        let bad_look = base("").replace("\"darkMode\":true", "\"shell\":{\"oled\":true}");
        assert!(parse_pack(&bad_look).unwrap_err().contains("not allowed"));
        assert!(parse_pack(&"x".repeat(MAX_BYTES + 1)).is_err());
        assert!(parse_pack("[]").is_err() && parse_pack("nope").is_err());
        let wrong_schema = base("").replace("\"schema\":1", "\"schema\":2");
        assert!(parse_pack(&wrong_schema).unwrap_err().contains("schema"));
    }

    #[test]
    fn targets_must_be_specific_and_plain() {
        let class_only = base("").replace(r#"{"package":"Pkg_*"}"#, r#"{"class":"Foo"}"#);
        assert!(parse_pack(&class_only).unwrap_err().contains("too broad"));
        for bad in [
            r#"{"package":"C:\\evil"}"#,
            r#"{"process":"a b.exe"}"#,
            r#"{"package":""}"#,
            r#"{"process":"../x.exe"}"#,
        ] {
            let text = base("").replace(r#"{"package":"Pkg_*"}"#, bad);
            assert!(parse_pack(&text).is_err(), "{bad}");
        }
        let none = base("").replace(r#"[{"package":"Pkg_*"}]"#, "[]");
        assert!(parse_pack(&none).is_err());
        let many = base("").replace(
            r#"[{"package":"Pkg_*"}]"#,
            &format!("[{}]", vec![r#"{"package":"P"}"#; 5].join(",")),
        );
        assert!(parse_pack(&many).is_err());
    }

    #[test]
    fn limits_and_reasons_are_required_where_honesty_needs_them() {
        let no_limits = base("").replace(
            r#""limits":["It cannot recolor the content."]"#,
            r#""limits":[]"#,
        );
        assert!(parse_pack(&no_limits).unwrap_err().contains("limits"));
        let partial = base("").replace("\"supported\"", "\"partial\"");
        assert!(parse_pack(&partial).unwrap_err().contains("reason"));
        assert!(parse_pack(
            &base(r#","reason":"Only the frame.""#).replace("\"supported\"", "\"partial\"")
        )
        .is_ok());
    }

    #[test]
    fn an_unsupported_pack_cannot_force_a_change() {
        let forced = base(r#","reason":"Colors never applied on build 26300.""#)
            .replace("\"supported\"", "\"unsupported\"");
        assert!(parse_pack(&forced).unwrap_err().contains("empty look"));
        let ok = r##"{"schema":1,"id":"t","name":"T","version":"1","targets":[{"package":"P_*"}],
            "verdict":"unsupported","reason":"Caption colors were not applied (observed on build 26300).",
            "limits":["Nothing can be changed from outside."]}"##;
        let pack = parse_pack(ok).unwrap();
        let mut config = parse_config("{}").unwrap();
        let err = install(&mut config, &pack, 26_300).unwrap_err();
        assert!(err.starts_with("Not supported:") && err.contains("observed"));
        assert!(config.rules.is_empty(), "nothing was added");
    }

    #[test]
    fn install_adds_named_low_priority_rules_and_remove_deletes_exactly_those() {
        let mut config =
            parse_config(r#"{"rules":[{"process":"keep.exe","backdrop":"none"}]}"#).unwrap();
        let pack = &catalog()[0];
        let lines = install(&mut config, pack, 26_300).unwrap();
        assert!(lines[0].starts_with("Added rule \"Pack: Microsoft Store\""));
        assert_eq!(config.rules.len(), 2);
        let rule = &config.rules[1];
        assert_eq!(rule.priority, PACK_PRIORITY);
        assert_eq!(rule.package.as_deref(), Some("Microsoft.WindowsStore_*"));
        assert_eq!(rule.style.enabled, Some(true));
        assert!(is_installed(&config, pack));
        assert_eq!(
            installed(&config),
            vec![Installed {
                name: "Microsoft Store".into(),
                rules: vec![1]
            }]
        );
        // Installing again replaces instead of duplicating.
        let again = install(&mut config, pack, 26_300).unwrap();
        assert!(again[0].starts_with("Replaced"));
        assert_eq!(config.rules.len(), 2);
        assert_eq!(remove(&mut config, "Microsoft Store"), 1);
        assert_eq!(config.rules.len(), 1);
        assert_eq!(
            config.rules[0].process, "keep.exe",
            "the user's rule is untouched"
        );
        assert_eq!(remove(&mut config, "Microsoft Store"), 0);
    }

    #[test]
    fn a_users_rule_for_the_same_app_always_wins_until_they_say_otherwise() {
        let mut config = parse_config(
            r#"{"rules":[{"name":"mine","package":"Microsoft.WindowsCalculator_*","backdrop":"none"}]}"#,
        )
        .unwrap();
        let calc = &catalog()[1];
        let lines = install(&mut config, calc, 26_300).unwrap();
        assert!(
            lines
                .iter()
                .any(|l| l.contains("Your rule 1 also matches") && l.contains("wins")),
            "{lines:?}"
        );
        let identity = WindowIdentity {
            exe: "ApplicationFrameHost.exe",
            class: "ApplicationFrameWindow",
            path: "",
            package: "Microsoft.WindowsCalculator_8wekyb3d8bbwe",
            title: None,
        };
        assert_eq!(
            rank(&config.rules, &identity)[0].index,
            0,
            "the user's rule wins"
        );
        // Raising the pack rule's priority is the user's explicit choice.
        config.rules[1].priority = 50;
        assert_eq!(rank(&config.rules, &identity)[0].index, 1);
    }

    #[test]
    fn without_a_user_rule_the_pack_rule_matches_the_hosted_frame_and_nothing_else() {
        let mut config = parse_config("{}").unwrap();
        install(&mut config, &catalog()[1], 26_300).unwrap();
        let frame = |package: &'static str, class: &'static str| WindowIdentity {
            exe: "ApplicationFrameHost.exe",
            class,
            path: "",
            package,
            title: None,
        };
        assert_eq!(
            rank(
                &config.rules,
                &frame(
                    "Microsoft.WindowsCalculator_8wekyb3d8bbwe",
                    "ApplicationFrameWindow"
                )
            )
            .len(),
            1
        );
        assert!(rank(
            &config.rules,
            &frame(
                "Microsoft.WindowsStore_8wekyb3d8bbwe",
                "ApplicationFrameWindow"
            )
        )
        .is_empty());
        assert!(rank(
            &config.rules,
            &frame("Microsoft.WindowsCalculator_8wekyb3d8bbwe", "Other")
        )
        .is_empty());
        assert!(
            rank(&config.rules, &frame("", "ApplicationFrameWindow")).is_empty(),
            "an unknown package never matches"
        );
    }

    #[test]
    fn multi_target_packs_name_and_remove_every_rule() {
        let text = base("").replace(
            r#"[{"package":"Pkg_*"}]"#,
            r#"[{"package":"A_*"},{"process":"b.exe"},{"package":"C_*"}]"#,
        );
        let pack = parse_pack(&text).unwrap();
        let mut config = parse_config("{}").unwrap();
        install(&mut config, &pack, 22_621).unwrap();
        let names: Vec<_> = config
            .rules
            .iter()
            .map(|r| r.name.clone().unwrap())
            .collect();
        assert_eq!(names, ["Pack: T", "Pack: T (2)", "Pack: T (3)"]);
        assert_eq!(installed(&config).len(), 1);
        assert_eq!(installed(&config)[0].rules, vec![0, 1, 2]);
        // A user's rule with a similar name is not owned by the pack.
        config.rules.push(Rule {
            name: Some("Pack: T extra".into()),
            ..Rule::for_process("x.exe")
        });
        assert_eq!(remove(&mut config, "T"), 3);
        assert_eq!(config.rules.len(), 1);
    }

    #[test]
    fn unavailable_says_why() {
        let mut pack = catalog()[0].clone();
        assert!(unavailable(&pack, 26_300).is_none());
        pack.min_build = Some(26_100);
        assert!(unavailable(&pack, 22_631).unwrap().contains("26100"));
        let text = look_actions(&pack.look).join("; ");
        assert!(
            text.contains("dark title bar: true") && text.contains("title bar color: #000000"),
            "{text}"
        );
        assert!(
            scope_text(&pack).contains("package Microsoft.WindowsStore_*")
                && scope_text(&pack).contains("ApplicationFrameWindow")
        );
    }

    #[test]
    fn what_install_writes_is_a_config_the_app_will_load_again() {
        let mut config = parse_config("{}").unwrap();
        for pack in catalog() {
            install(&mut config, &pack, 26_300).unwrap();
        }
        let text = serde_json::to_string_pretty(&config).unwrap();
        let again = parse_config(&text).expect("a config with pack rules must reload");
        assert_eq!(again.rules.len(), 2);
        assert!(again
            .rules
            .iter()
            .all(|r| r.priority == PACK_PRIORITY && r.has_matcher()));
        assert!(
            !text.contains("\"process\": \"\""),
            "no empty process is written"
        );
        assert_eq!(installed(&again).len(), 2);
    }
}
