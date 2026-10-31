//! The rule engine. Pure, deterministic, and explainable.
//!
//! A rule can match on executable name, window class, executable path, package family name,
//! and (optionally) window title. When several rules match one window, exactly one wins:
//!
//!  1. higher `priority`
//!  2. then the rule that matched more criteria, weighted by how specific each is
//!  3. then the earlier rule in the file
//!
//! The same code answers "which look does this window get?" and "why?", so the Settings
//! inspector can never disagree with what the theming pass actually did.
//!
//! Titles are deliberately last-resort: they change with the open document and with the
//! display language, and reading another app's title is a privacy cost. A title matcher only
//! ever matches when the caller supplied a title, and the caller only reads titles when some
//! rule asks for one.

use crate::config::{auto_text, base_style, Config, Rule, Style};

/// What is known about a window, borrowed from the caller. Empty strings mean "unknown".
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowIdentity<'a> {
    pub exe: &'a str,
    pub class: &'a str,
    pub path: &'a str,
    pub package: &'a str,
    /// Only present when a rule needs it. See the module comment.
    pub title: Option<&'a str>,
}

impl<'a> WindowIdentity<'a> {
    pub fn exe(exe: &'a str) -> Self {
        Self {
            exe,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Criterion {
    Exe,
    Class,
    Path,
    Package,
    Title,
}

impl Criterion {
    /// How much a matched criterion says about *which* app this is.
    pub fn weight(self) -> u32 {
        match self {
            Criterion::Exe => 10,
            Criterion::Path => 15,
            Criterion::Class => 20,
            Criterion::Title => 25,
            Criterion::Package => 30,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Criterion::Exe => "executable",
            Criterion::Class => "window class",
            Criterion::Path => "path",
            Criterion::Package => "package",
            Criterion::Title => "title",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleMatch {
    pub index: usize,
    pub priority: i32,
    pub specificity: u32,
    pub criteria: Vec<Criterion>,
}

/// Case-insensitive glob: `*` any run, `?` any one character. Backslashes and slashes are
/// the same, so a path written either way matches.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    fn norm(c: char) -> char {
        if c == '/' {
            '\\'
        } else {
            c.to_ascii_lowercase()
        }
    }
    let pattern: Vec<char> = pattern.chars().map(norm).collect();
    let text: Vec<char> = text.chars().map(norm).collect();
    let (mut p, mut t) = (0usize, 0usize);
    let (mut star, mut mark) = (None::<usize>, 0usize);
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            mark = t;
            p += 1;
        } else if let Some(s) = star {
            p = s + 1;
            mark += 1;
            t = mark;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

fn exe_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

/// Every criterion the rule sets must match. Returns the criteria it matched, or `None`.
pub fn match_rule(rule: &Rule, identity: &WindowIdentity<'_>) -> Option<Vec<Criterion>> {
    let mut matched = Vec::new();
    if !rule.process.is_empty() {
        let pattern = exe_name(&rule.process);
        if pattern.is_empty() || !pattern.eq_ignore_ascii_case(identity.exe) {
            return None;
        }
        matched.push(Criterion::Exe);
    }
    if let Some(class) = &rule.class {
        if class.is_empty() || !class.eq_ignore_ascii_case(identity.class) {
            return None;
        }
        matched.push(Criterion::Class);
    }
    if let Some(path) = &rule.path {
        if path.is_empty() || identity.path.is_empty() || !glob_match(path, identity.path) {
            return None;
        }
        matched.push(Criterion::Path);
    }
    if let Some(package) = &rule.package {
        if package.is_empty()
            || identity.package.is_empty()
            || !glob_match(package, identity.package)
        {
            return None;
        }
        matched.push(Criterion::Package);
    }
    if let Some(title) = &rule.title {
        let hay = identity.title?;
        if title.is_empty()
            || !hay
                .to_ascii_lowercase()
                .contains(&title.to_ascii_lowercase())
        {
            return None;
        }
        matched.push(Criterion::Title);
    }
    if matched.is_empty() {
        None
    } else {
        Some(matched)
    }
}

/// All matching rules, best first. `result[0]` wins.
pub fn rank(rules: &[Rule], identity: &WindowIdentity<'_>) -> Vec<RuleMatch> {
    let mut found: Vec<RuleMatch> = rules
        .iter()
        .enumerate()
        .filter_map(|(index, rule)| {
            let criteria = match_rule(rule, identity)?;
            Some(RuleMatch {
                index,
                priority: rule.priority,
                specificity: criteria.iter().map(|c| c.weight()).sum(),
                criteria,
            })
        })
        .collect();
    found.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then(b.specificity.cmp(&a.specificity))
            .then(a.index.cmp(&b.index))
    });
    found
}

/// Why `winner` beat `loser`, in words a person can check against their own rules.
pub fn why_it_won(winner: &RuleMatch, loser: &RuleMatch) -> String {
    if winner.priority != loser.priority {
        format!(
            "priority {} beats priority {}",
            winner.priority, loser.priority
        )
    } else if winner.specificity != loser.specificity {
        format!(
            "it matched more specifically ({} points against {})",
            winner.specificity, loser.specificity
        )
    } else {
        "same priority and specificity, so the earlier rule in the file wins".to_string()
    }
}

// ---------------------------------------------------------------------------
// Resolution with provenance
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// AllBlack11's built-in base look.
    BuiltIn,
    /// The `default` block of the config.
    Default,
    /// The winning rule, by index.
    Rule(usize),
    /// Computed from other settings (text color picked for contrast).
    Derived,
}

#[derive(Debug, Clone)]
pub struct Resolved {
    pub style: Style,
    /// False when theming is off for this window (a rule or the default says so).
    pub enabled: bool,
    pub enabled_source: Source,
    pub winner: Option<RuleMatch>,
    pub others: Vec<RuleMatch>,
    /// One entry per style field: (field name, where its value came from).
    pub sources: Vec<(&'static str, Source)>,
}

pub fn resolve(config: &Config, identity: &WindowIdentity<'_>, accent_bgr: u32) -> Resolved {
    let ranked = rank(&config.rules, identity);
    let (winner, others) = match ranked.split_first() {
        Some((first, rest)) => (Some(first.clone()), rest.to_vec()),
        None => (None, Vec::new()),
    };
    let rule: Option<&Rule> = winner.as_ref().map(|win| &config.rules[win.index]);

    let mut enabled = config.defaults.enabled.unwrap_or(true);
    let mut enabled_source = if config.defaults.enabled.is_some() {
        Source::Default
    } else {
        Source::BuiltIn
    };
    let mut style = config.defaults.merge_onto(base_style());
    let mut sources: Vec<(&'static str, Source)> = FIELDS
        .iter()
        .map(|(name, getter)| {
            let source = if getter(&config.defaults) {
                Source::Default
            } else {
                Source::BuiltIn
            };
            (*name, source)
        })
        .collect();
    let mut text_explicit = config.defaults.text_color.is_some();
    if let (Some(rule), Some(win)) = (rule, winner.as_ref()) {
        if let Some(value) = rule.style.enabled {
            enabled = value;
            enabled_source = Source::Rule(win.index);
        }
        if rule.style.text_color.is_some() {
            text_explicit = true;
        }
        for (slot, (_, getter)) in sources.iter_mut().zip(FIELDS.iter()) {
            if getter(&rule.style) {
                slot.1 = Source::Rule(win.index);
            }
        }
        style = rule.style.merge_onto(style);
    }
    if !text_explicit {
        style.text = auto_text(style.caption, accent_bgr);
        if let Some(slot) = sources.iter_mut().find(|slot| slot.0 == "text color") {
            slot.1 = Source::Derived;
        }
    }
    Resolved {
        style,
        enabled,
        enabled_source,
        winner,
        others,
        sources,
    }
}

type Getter = fn(&crate::config::PartialStyle) -> bool;

const FIELDS: [(&str, Getter); 9] = [
    ("dark title bar", |s| s.dark_mode.is_some()),
    ("backdrop", |s| s.backdrop.is_some()),
    ("title bar color", |s| s.caption_color.is_some()),
    ("border color", |s| s.border_color.is_some()),
    ("text color", |s| s.text_color.is_some()),
    ("corners", |s| s.corners.is_some()),
    ("acrylic tint", |s| s.acrylic_tint.is_some()),
    ("acrylic opacity", |s| s.acrylic_opacity.is_some()),
    ("blur while dragging", |s| {
        s.tinted_acrylic_on_move_fallback.is_some()
    }),
];

// ---------------------------------------------------------------------------
// Conflict detection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictKind {
    /// Same matchers, same priority: the later rule can never win.
    Duplicate,
    /// Can match the same window, same priority, same specificity: only file order decides.
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub first: usize,
    pub second: usize,
    pub kind: ConflictKind,
}

fn matchers(rule: &Rule) -> [Option<String>; 5] {
    let lower = |text: &str| text.to_ascii_lowercase();
    [
        (!rule.process.is_empty()).then(|| lower(exe_name(&rule.process))),
        rule.class.as_deref().map(lower),
        rule.path.as_deref().map(lower),
        rule.package.as_deref().map(lower),
        rule.title.as_deref().map(lower),
    ]
}

fn specificity(rule: &Rule) -> u32 {
    let set = matchers(rule);
    [
        Criterion::Exe,
        Criterion::Class,
        Criterion::Path,
        Criterion::Package,
        Criterion::Title,
    ]
    .iter()
    .zip(set.iter())
    .filter(|(_, value)| value.is_some())
    .map(|(criterion, _)| criterion.weight())
    .sum()
}

/// Rule pairs whose outcome depends on nothing but file order.
pub fn find_conflicts(rules: &[Rule]) -> Vec<Conflict> {
    let mut found = Vec::new();
    for first in 0..rules.len() {
        for second in (first + 1)..rules.len() {
            let (a, b) = (&rules[first], &rules[second]);
            if a.priority != b.priority {
                continue;
            }
            let (ma, mb) = (matchers(a), matchers(b));
            if ma == mb {
                found.push(Conflict {
                    first,
                    second,
                    kind: ConflictKind::Duplicate,
                });
                continue;
            }
            if specificity(a) != specificity(b) {
                continue;
            }
            // Could one window match both? Only when no criterion set on both disagrees.
            let contradict = ma
                .iter()
                .zip(mb.iter())
                .any(|(x, y)| matches!((x, y), (Some(x), Some(y)) if x != y && !x.contains('*') && !y.contains('*')));
            if !contradict {
                found.push(Conflict {
                    first,
                    second,
                    kind: ConflictKind::Ambiguous,
                });
            }
        }
    }
    found
}

/// Human text for a [`Conflict`].
pub fn describe_conflict(rules: &[Rule], conflict: &Conflict) -> String {
    let (a, b) = (&rules[conflict.first], &rules[conflict.second]);
    match conflict.kind {
        ConflictKind::Duplicate => format!(
            "\"{}\" and \"{}\" match the same windows at the same priority. The second can never win.",
            a.label(),
            b.label()
        ),
        ConflictKind::Ambiguous => format!(
            "\"{}\" and \"{}\" can match the same window at the same priority and specificity. File order decides. Give one a higher priority.",
            a.label(),
            b.label()
        ),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{parse_config, Backdrop};

    fn config(json: &str) -> Config {
        parse_config(json).expect("test config parses")
    }

    #[test]
    fn glob_basics_are_case_and_slash_insensitive() {
        assert!(glob_match(
            "C:\\Program Files\\Vendor\\*",
            "c:/program files/vendor/app.exe"
        ));
        assert!(glob_match("*\\app.exe", "D:\\x\\y\\App.exe"));
        assert!(glob_match(
            "Microsoft.Windows*_8wekyb3d8bbwe",
            "Microsoft.WindowsTerminal_8wekyb3d8bbwe"
        ));
        assert!(glob_match("a?c", "abc"));
        assert!(!glob_match("a?c", "ac"));
        assert!(!glob_match("*.exe", "app.dll"));
        assert!(glob_match("*", ""));
    }

    #[test]
    fn exe_only_rules_keep_working_like_1x() {
        let c = config(r#"{"rules":[{"process":"notepad.exe","backdrop":"acrylic"}]}"#);
        let hit = resolve(&c, &WindowIdentity::exe("NOTEPAD.EXE"), 0);
        assert_eq!(hit.style.backdrop, Backdrop::Acrylic);
        assert_eq!(hit.winner.as_ref().map(|w| w.index), Some(0));
        let miss = resolve(&c, &WindowIdentity::exe("calc.exe"), 0);
        assert_eq!(miss.style.backdrop, Backdrop::Mica);
        assert!(miss.winner.is_none());
    }

    #[test]
    fn priority_then_specificity_then_order() {
        let c = config(
            r#"{"rules":[
              {"process":"app.exe","backdrop":"mica"},
              {"process":"app.exe","class":"MainWnd","backdrop":"acrylic"},
              {"process":"app.exe","priority":5,"backdrop":"none"}
            ]}"#,
        );
        let id = WindowIdentity {
            exe: "app.exe",
            class: "MainWnd",
            ..Default::default()
        };
        let ranked = rank(&c.rules, &id);
        assert_eq!(
            ranked.iter().map(|r| r.index).collect::<Vec<_>>(),
            vec![2, 1, 0]
        );
        assert_eq!(
            why_it_won(&ranked[0], &ranked[1]),
            "priority 5 beats priority 0"
        );
        assert!(why_it_won(&ranked[1], &ranked[2]).contains("more specifically"));

        // Same priority and specificity: file order.
        let c = config(
            r#"{"rules":[{"process":"a.exe","backdrop":"mica"},{"process":"a.exe","backdrop":"none"}]}"#,
        );
        let ranked = rank(&c.rules, &WindowIdentity::exe("a.exe"));
        assert_eq!(ranked[0].index, 0);
        assert!(why_it_won(&ranked[0], &ranked[1]).contains("earlier rule"));
    }

    #[test]
    fn class_package_and_path_rules_match_without_an_exe() {
        let c = config(
            r#"{"rules":[
              {"class":"CASCADIA_HOSTING_WINDOW_CLASS","backdrop":"acrylic"},
              {"package":"Microsoft.WindowsNotepad_*","backdrop":"none"},
              {"path":"C:\\Tools\\*","backdrop":"micaAlt"}
            ]}"#,
        );
        let terminal = WindowIdentity {
            exe: "WindowsTerminal.exe",
            class: "CASCADIA_HOSTING_WINDOW_CLASS",
            ..Default::default()
        };
        assert_eq!(resolve(&c, &terminal, 0).style.backdrop, Backdrop::Acrylic);
        let notepad = WindowIdentity {
            exe: "Notepad.exe",
            class: "Notepad",
            package: "Microsoft.WindowsNotepad_8wekyb3d8bbwe",
            ..Default::default()
        };
        assert_eq!(resolve(&c, &notepad, 0).style.backdrop, Backdrop::None);
        let tool = WindowIdentity {
            exe: "x.exe",
            path: "c:\\tools\\x.exe",
            ..Default::default()
        };
        assert_eq!(resolve(&c, &tool, 0).style.backdrop, Backdrop::MicaAlt);
    }

    #[test]
    fn a_path_or_package_rule_never_matches_an_unknown_path_or_package() {
        let c = config(
            r#"{"rules":[{"path":"*","backdrop":"none"},{"package":"*","backdrop":"none"}]}"#,
        );
        let unknown = WindowIdentity::exe("a.exe");
        assert!(rank(&c.rules, &unknown).is_empty());
    }

    #[test]
    fn title_rules_only_match_when_a_title_was_supplied() {
        let c = config(r#"{"rules":[{"process":"app.exe","title":"report","backdrop":"none"}]}"#);
        let without = WindowIdentity::exe("app.exe");
        assert!(rank(&c.rules, &without).is_empty());
        let with = WindowIdentity {
            exe: "app.exe",
            title: Some("Quarterly REPORT - app"),
            ..Default::default()
        };
        assert_eq!(rank(&c.rules, &with).len(), 1);
        let other = WindowIdentity {
            exe: "app.exe",
            title: Some("Untitled"),
            ..Default::default()
        };
        assert!(rank(&c.rules, &other).is_empty());
    }

    #[test]
    fn provenance_names_the_source_of_each_field() {
        let c = config(
            r##"{"default":{"backdrop":"mica","captionColor":"#000000"},
                "rules":[{"process":"a.exe","borderColor":"#FF0000"}]}"##,
        );
        let resolved = resolve(&c, &WindowIdentity::exe("a.exe"), 0);
        let source = |name: &str| resolved.sources.iter().find(|(n, _)| *n == name).unwrap().1;
        assert_eq!(source("border color"), Source::Rule(0));
        assert_eq!(source("backdrop"), Source::Default);
        assert_eq!(source("corners"), Source::BuiltIn);
        assert_eq!(source("text color"), Source::Derived);
    }

    #[test]
    fn rules_can_switch_theming_off_and_say_so() {
        let c = config(r#"{"rules":[{"process":"game.exe","enabled":false}]}"#);
        let off = resolve(&c, &WindowIdentity::exe("game.exe"), 0);
        assert!(!off.enabled);
        assert_eq!(off.enabled_source, Source::Rule(0));
        assert!(resolve(&c, &WindowIdentity::exe("other.exe"), 0).enabled);
    }

    #[test]
    fn conflicts_are_found_and_explained() {
        let c = config(
            r#"{"rules":[
              {"process":"a.exe","backdrop":"mica"},
              {"process":"A.EXE","backdrop":"none"},
              {"process":"b.exe","class":"X"},
              {"process":"b.exe","class":"Y"},
              {"class":"X","backdrop":"none"},
              {"process":"c.exe"},
              {"process":"c.exe","priority":1}
            ]}"#,
        );
        let conflicts = find_conflicts(&c.rules);
        assert!(conflicts
            .iter()
            .any(|x| x.first == 0 && x.second == 1 && x.kind == ConflictKind::Duplicate));
        // b.exe+X vs b.exe+Y contradict: no conflict. b.exe+X vs X only: different specificity.
        assert!(!conflicts.iter().any(|x| x.first == 2 && x.second == 3));
        assert!(!conflicts.iter().any(|x| x.first == 2 && x.second == 4));
        // Different priorities never conflict.
        assert!(!conflicts.iter().any(|x| x.first == 5 && x.second == 6));
        assert!(describe_conflict(&c.rules, &conflicts[0]).contains("can never win"));
    }

    #[test]
    fn a_rule_without_a_matcher_is_rejected() {
        let err = parse_config(r#"{"rules":[{"backdrop":"none"}]}"#).unwrap_err();
        assert!(err.contains("rule 1 has no matcher"), "{err}");
    }
}
