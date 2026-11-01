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
    /// Kept as a diagnostic: the request is made and the evidence is shown, but no visible effect
    /// has been demonstrated, so it is never offered as a recommended template.
    Experimental,
    /// Nothing can be done from outside. No rule is added, whatever the look says.
    Unsupported,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Supported => "Supported",
            Verdict::Partial => "Partly supported",
            Verdict::Experimental => "Experimental (no visible effect shown)",
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
        Verdict::Partial | Verdict::Experimental => {
            if pack.reason.trim().is_empty() {
                return Err(
                    "a partial or experimental pack must say what cannot change in reason".into(),
                );
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
  "schema": 1, "id": "microsoft-store", "name": "Microsoft Store", "version": "2",
  "description": "Experimental test: asks Windows for a black title bar on the Microsoft Store window. It has not been shown to change anything you can see.",
  "author": "AllBlack11",
  "targets": [ { "package": "Microsoft.WindowsStore_*", "class": "ApplicationFrameWindow" } ],
  "verdict": "experimental",
  "reason": "On the PC this was first tried on, installing it produced no visible change. It can only ask for window-frame colors, and its values are close to the default look, so a change would be hard to see even if Windows accepts it. Use Check now to see where the request stops.",
  "look": { "darkMode": true, "captionColor": "#000000", "textColor": "#FFFFFF", "borderColor": "none" },
  "limits": [
    "Client content of the Store app is rendered by the app itself and cannot be recolored externally.",
    "This does not make the Store dark. Its pages are drawn by the app; Windows' app mode (Settings > Personalization > Colors) is what most Microsoft Store apps follow, and an app's own theme option, if it has one, is in that app's settings.",
    "No backdrop is requested: the Store paints an opaque surface over it."
  ],
  "notes": [ "The window is found by the app's package identity, read from the app inside the frame." ],
  "tested": [ "Installed on the owner's Windows PC: no visible change. Not yet investigated further on Windows hardware." ]
}"##;

const CALCULATOR: &str = r##"{
  "schema": 1, "id": "windows-calculator", "name": "Calculator", "version": "2",
  "description": "Experimental test: asks Windows for a black title bar on the Calculator window. It has not been shown to change anything you can see.",
  "author": "AllBlack11",
  "targets": [ { "package": "Microsoft.WindowsCalculator_*", "class": "ApplicationFrameWindow" } ],
  "verdict": "experimental",
  "reason": "On the PC this was first tried on, installing it produced no visible change. It can only ask for window-frame colors, and its values are close to the default look, so a change would be hard to see even if Windows accepts it. Use Check now to see where the request stops.",
  "look": { "darkMode": true, "captionColor": "#000000", "textColor": "#FFFFFF", "borderColor": "none" },
  "limits": [
    "Calculator's content is drawn by the app's own XAML and cannot be recolored from outside. Calculator being open source does not change that: this pack never touches its code.",
    "This does not make Calculator dark. Calculator's own settings (in its menu) have an app theme option; that is the supported way to change how it looks inside.",
    "No backdrop is requested: Calculator paints an opaque surface over it."
  ],
  "notes": [ "Calculator follows its own app theme setting, or the Windows app mode when it is set to follow the system." ],
  "tested": [ "Installed on the owner's Windows PC: no visible change. Not yet investigated further on Windows hardware." ]
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

// ---------------------------------------------------------------------------------------------
// Evidence: how far a pack's request got, and what is known about the result
// ---------------------------------------------------------------------------------------------

/// Where a pack's request stops, from the evidence in hand. The order of the checks is the order
/// of the chain: find the window, match the rule, win over the others, be applied, be accepted by
/// Windows, and only then what a person saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evidence {
    /// No open window matched the pack's target (app closed, or its package could not be read).
    TargetNotDetected,
    /// The window is one AllBlack11 never styles (protected or on your exclusion lists).
    Excluded,
    /// A window was found but the pack's rule is not among the rules that match it.
    RuleDidNotMatch,
    /// The pack's rule matches, and another rule (yours, normally) wins.
    OverriddenByOtherRule,
    /// Nothing has been asked of the window: styling is off, paused, reduced or skipped for it.
    NotAppliedYet,
    /// Windows rejected at least one request.
    OperationFailed,
    /// Windows accepted every request. What is on screen has not been checked.
    AcceptedUnverified,
    /// You looked and saw no change.
    UserVerifiedNoChange,
    /// You looked and saw a change.
    VerifiedVisibleChange,
}

impl Evidence {
    pub fn label(self) -> &'static str {
        match self {
            Evidence::TargetNotDetected => "Target not detected",
            Evidence::Excluded => "Window is excluded",
            Evidence::RuleDidNotMatch => "Rule did not match",
            Evidence::OverriddenByOtherRule => "Overridden by another rule",
            Evidence::NotAppliedYet => "Not applied right now",
            Evidence::OperationFailed => "Windows rejected a request",
            Evidence::AcceptedUnverified => "Accepted by Windows; visual result unverified",
            Evidence::UserVerifiedNoChange => "You verified no visible change",
            Evidence::VerifiedVisibleChange => "Verified visible change",
        }
    }
}

/// What the app could see about one open window that might be the pack's target.
#[derive(Debug, Clone, Default)]
pub struct Observed {
    pub exe: String,
    pub class: String,
    /// The package family read from the app inside the frame; empty when it could not be read.
    pub package: String,
    /// Why AllBlack11 leaves this window alone, if it does.
    pub excluded: Option<String>,
    /// The pack's own rule is among the rules matching this window.
    pub rule_matched: bool,
    /// ... and it is the winner.
    pub pack_wins: bool,
    /// Label of the winning rule, for the sentence.
    pub winner: String,
    /// The style engine decided to apply a look to this window.
    pub applied: bool,
    /// Requests Windows rejected, as "name (0x...)".
    pub rejected: Vec<String>,
}

/// What the person told us they saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Mark {
    SawChange,
    SawNothing,
}

/// The status and one plain sentence that says what to do about it.
pub fn assess(observed: Option<&Observed>, mark: Option<Mark>) -> (Evidence, String) {
    let Some(seen) = observed else {
        return (
            Evidence::TargetNotDetected,
            "No open window matched. Open the app, then press Check now. If it is open, AllBlack11 could not read its package name, so no rule can name it.".into(),
        );
    };
    if let Some(reason) = &seen.excluded {
        return (
            Evidence::Excluded,
            format!("AllBlack11 does not style this window. {reason}"),
        );
    }
    if !seen.rule_matched {
        return (
            Evidence::RuleDidNotMatch,
            format!(
                "Found {} (class {}, package {}), but the pack's rule is not among the rules matching it. Install the pack, or check that its package and class match.",
                seen.exe,
                seen.class,
                if seen.package.is_empty() { "unreadable" } else { &seen.package }
            ),
        );
    }
    if !seen.pack_wins {
        return (
            Evidence::OverriddenByOtherRule,
            format!(
                "The pack's rule matches, but \"{}\" wins. Your own rules always beat a pack.",
                seen.winner
            ),
        );
    }
    if !seen.applied {
        return (
            Evidence::NotAppliedYet,
            "The pack's rule wins, but no look is being applied to this window right now (styling is off, paused, reduced, or the app was skipped). See Customize an app for the reason.".into(),
        );
    }
    if !seen.rejected.is_empty() {
        return (
            Evidence::OperationFailed,
            format!("Windows rejected: {}.", seen.rejected.join(", ")),
        );
    }
    match mark {
        Some(Mark::SawNothing) => (
            Evidence::UserVerifiedNoChange,
            "Windows accepted every request and you saw no change. The app is probably drawing its own frame over the request. This pack will not help here.".into(),
        ),
        Some(Mark::SawChange) => (
            Evidence::VerifiedVisibleChange,
            "Windows accepted every request and you saw a change.".into(),
        ),
        None => (
            Evidence::AcceptedUnverified,
            "Windows accepted every request. Whether anything changed on screen is not checked: look at the window, then say what you see.".into(),
        ),
    }
}

/// What installing this pack would change compared with the default look, field by field. An empty
/// list means the pack asks for exactly what is already in force, so nothing visible could change.
pub fn delta(config: &Config, pack: &Pack) -> Vec<String> {
    let recipe = crate::recipes::Recipe {
        schema: crate::recipes::SCHEMA,
        id: "pack-delta".into(),
        name: pack.name.clone(),
        description: String::new(),
        author: String::new(),
        min_build: None,
        look: pack.look.clone(),
        matcher: None,
        notes: Vec::new(),
    };
    crate::recipes::preview(config, &recipe)
        .iter()
        .map(|change| format!("{}: {} → {}", change.field, change.from, change.to))
        .collect()
}

/// The marks, by pack id. Small, local, and damaged files are "nothing marked".
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct Marks {
    pub marks: Vec<(String, Mark)>,
}

impl Marks {
    pub fn parse(text: &str) -> Marks {
        let mut marks: Marks = serde_json::from_str(text).unwrap_or_default();
        marks.marks.truncate(32);
        marks
    }
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into()) + "\n"
    }
    pub fn get(&self, id: &str) -> Option<Mark> {
        self.marks
            .iter()
            .find(|(known, _)| known == id)
            .map(|(_, m)| *m)
    }
    pub fn set(&mut self, id: &str, mark: Option<Mark>) {
        self.marks.retain(|(known, _)| known != id);
        if let Some(mark) = mark {
            self.marks.push((id.to_string(), mark));
        }
    }
}

/// Whether `rule` belongs to the pack called `name`.
pub fn owns(rule: &Rule, name: &str) -> bool {
    owned_by(rule, name)
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
            assert_eq!(
                pack.verdict,
                Verdict::Experimental,
                "no demonstrated value yet: never recommended"
            );
            assert!(pack.reason.contains("no visible change"), "{}", pack.reason);
            let words = format!("{} {}", pack.description, pack.limits.join(" ")).to_lowercase();
            assert!(
                !words.contains("make the store darker") && !words.contains("dark calculator"),
                "{words}"
            );
            assert!(
                words.contains("does not make"),
                "says plainly what it does not do: {words}"
            );
            assert!(
                pack.look.backdrop.is_none(),
                "no backdrop is claimed for opaque XAML apps"
            );
            assert!(
                pack.tested.iter().any(|t| t.contains("no visible change")),
                "the owner's result is recorded, and no success is claimed"
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

    fn observed() -> Observed {
        Observed {
            exe: "ApplicationFrameHost.exe".into(),
            class: "ApplicationFrameWindow".into(),
            package: "Microsoft.WindowsCalculator_8wekyb3d8bbwe".into(),
            excluded: None,
            rule_matched: true,
            pack_wins: true,
            winner: "Pack: Calculator".into(),
            applied: true,
            rejected: vec![],
        }
    }

    #[test]
    fn each_stage_where_a_request_can_stop_has_its_own_status() {
        let (status, text) = assess(None, None);
        assert_eq!(status, Evidence::TargetNotDetected);
        assert!(text.contains("Open the app"));
        let mut o = observed();
        o.excluded = Some("Protected.".into());
        assert_eq!(assess(Some(&o), None).0, Evidence::Excluded);
        let mut o = observed();
        o.rule_matched = false;
        let (status, text) = assess(Some(&o), None);
        assert_eq!(status, Evidence::RuleDidNotMatch);
        assert!(
            text.contains("ApplicationFrameHost.exe") && text.contains("Calculator_8wekyb3d8bbwe")
        );
        let mut o = observed();
        o.pack_wins = false;
        o.winner = "my rule".into();
        let (status, text) = assess(Some(&o), None);
        assert_eq!(status, Evidence::OverriddenByOtherRule);
        assert!(text.contains("my rule"));
        let mut o = observed();
        o.applied = false;
        assert_eq!(assess(Some(&o), None).0, Evidence::NotAppliedYet);
        let mut o = observed();
        o.rejected = vec!["Caption color (0x80070057)".into()];
        let (status, text) = assess(Some(&o), None);
        assert_eq!(status, Evidence::OperationFailed);
        assert!(text.contains("0x80070057"));
    }

    #[test]
    fn accepted_is_never_called_verified_without_a_person_looking() {
        let o = observed();
        let (status, text) = assess(Some(&o), None);
        assert_eq!(status, Evidence::AcceptedUnverified);
        assert!(status.label().contains("unverified") && text.contains("not checked"));
        assert_eq!(
            assess(Some(&o), Some(Mark::SawNothing)).0,
            Evidence::UserVerifiedNoChange
        );
        assert_eq!(
            assess(Some(&o), Some(Mark::SawChange)).0,
            Evidence::VerifiedVisibleChange
        );
        // A mark never hides an earlier failure in the chain.
        let mut failed = observed();
        failed.rejected = vec!["x (0x1)".into()];
        assert_eq!(
            assess(Some(&failed), Some(Mark::SawChange)).0,
            Evidence::OperationFailed
        );
        let mut missing = observed();
        missing.rule_matched = false;
        assert_eq!(
            assess(Some(&missing), Some(Mark::SawChange)).0,
            Evidence::RuleDidNotMatch
        );
        // Seven plus two distinct labels.
        let labels = [
            Evidence::TargetNotDetected,
            Evidence::Excluded,
            Evidence::RuleDidNotMatch,
            Evidence::OverriddenByOtherRule,
            Evidence::NotAppliedYet,
            Evidence::OperationFailed,
            Evidence::AcceptedUnverified,
            Evidence::UserVerifiedNoChange,
            Evidence::VerifiedVisibleChange,
        ]
        .map(Evidence::label);
        let mut sorted = labels.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 9);
    }

    #[test]
    fn the_built_in_packs_barely_differ_from_the_default_look_which_explains_a_silent_result() {
        let config = parse_config("{}").unwrap();
        for pack in catalog() {
            let changes = delta(&config, &pack);
            assert!(changes.len() <= 3, "{changes:?}");
            assert!(
                changes.iter().all(|c| c.starts_with("title bar color")
                    || c.starts_with("text color")
                    || c.starts_with("border color")),
                "{changes:?}"
            );
        }
        // Against a default look that already matches the pack, nothing could change.
        let same = parse_config(
            r##"{"default":{"darkMode":true,"captionColor":"#000000","textColor":"#FFFFFF","borderColor":"none"}}"##,
        )
        .unwrap();
        assert!(delta(&same, &catalog()[0]).is_empty());
    }

    #[test]
    fn marks_round_trip_and_a_damaged_file_means_nothing_is_marked() {
        let mut marks = Marks::default();
        marks.set("windows-calculator", Some(Mark::SawNothing));
        marks.set("windows-calculator", Some(Mark::SawChange));
        assert_eq!(marks.marks.len(), 1);
        assert_eq!(
            Marks::parse(&marks.to_json()).get("windows-calculator"),
            Some(Mark::SawChange)
        );
        marks.set("windows-calculator", None);
        assert_eq!(marks.get("windows-calculator"), None);
        assert_eq!(Marks::parse("{ nope"), Marks::default());
    }

    #[test]
    fn experimental_packs_install_but_say_so_and_unsupported_ones_cannot() {
        let pack = &catalog()[0];
        assert_eq!(pack.verdict, Verdict::Experimental);
        assert!(
            unavailable(pack, 26_300).is_none(),
            "an experiment can still be run"
        );
        assert!(pack.verdict.label().contains("no visible effect"));
        let mut config = parse_config("{}").unwrap();
        assert!(install(&mut config, pack, 26_300).is_ok());
        // The rule name did not change, so rules installed by 3.8.0-preview.1 are still found.
        assert_eq!(installed(&config)[0].name, "Microsoft Store");
    }
}
