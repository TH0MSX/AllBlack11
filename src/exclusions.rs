//! The exclusion manager (F18): every reason a window is left alone, in the order it is checked,
//! and the two lists the user owns.
//!
//! This module only reads and edits those lists and describes them. The decision itself stays in
//! `policy::decide` / `policy::exclusion_for`; a test here walks a set of examples through
//! `exclusion_for` and checks that the text this module prints for them agrees.

use crate::config::{Config, Rule};
use crate::legacy_policy::ALWAYS_SKIP_PROCESSES;
use crate::policy::{exclusion_for, protected_classes, Exclusion};

pub const MAX_CLASSES: usize = 64;
const MAX_CLASS_CHARS: usize = 96;

/// The order the checks run in, written for the page. Kept in step with `Exclusion` by a test.
pub fn precedence() -> Vec<String> {
    vec![
        "How AllBlack11 decides to leave a window alone, first match wins:".to_string(),
        "1. Protected window classes (shell, menus, input UI). Fixed.".to_string(),
        "2. Window classes you listed.".to_string(),
        "3. Protected programs (shell hosts, services, AllBlack11 itself). Fixed.".to_string(),
        "4. Programs you listed under Never theme these apps.".to_string(),
        "5. Explorer's own shell windows. Its file windows are styled.".to_string(),
        "Only then do your per-app rules and the default look apply. A rule can turn styling off for one app, but it cannot override 1 to 5.".to_string(),
    ]
}

/// One line each for what is fixed, what is yours, and which rules switch styling off.
pub fn summary(config: &Config) -> Vec<String> {
    let mut lines = vec![format!(
        "Protected and fixed: {} window classes and {} programs. No setting changes these.",
        protected_classes().len(),
        ALWAYS_SKIP_PROCESSES.len()
    )];
    lines.push(match config.exclude.len() {
        0 => "Programs you listed: none.".to_string(),
        n => format!(
            "Programs you listed: {n} ({}).",
            join_short(&config.exclude)
        ),
    });
    lines.push(match config.exclude_classes.len() {
        0 => "Window classes you listed: none.".to_string(),
        n => format!(
            "Window classes you listed: {n} ({}).",
            join_short(&config.exclude_classes)
        ),
    });
    let off = rules_switching_styling_off(config);
    lines.push(if off.is_empty() {
        "Rules that switch styling off for an app: none.".to_string()
    } else {
        format!(
            "Rules that switch styling off for an app: {} ({}).",
            off.len(),
            join_short(&off)
        )
    });
    lines
}

fn join_short(items: &[String]) -> String {
    let shown: Vec<&str> = items.iter().take(4).map(String::as_str).collect();
    let mut text = shown.join(", ");
    if items.len() > shown.len() {
        text.push_str(&format!(" and {} more", items.len() - shown.len()));
    }
    text
}

/// Labels of the rules whose own look says `enabled: false`.
pub fn rules_switching_styling_off(config: &Config) -> Vec<String> {
    config
        .rules
        .iter()
        .filter(|rule: &&Rule| rule.style.enabled == Some(false))
        .map(Rule::label)
        .collect()
}

/// Add a window class to the user's list. Returns the sentence to show.
pub fn add_class(config: &mut Config, text: &str) -> Result<String, String> {
    let class = text.trim();
    if class.is_empty() {
        return Err("Type a window class to leave alone, for example class=Notepad.".into());
    }
    if class.chars().count() > MAX_CLASS_CHARS
        || class
            .chars()
            .any(|c| c.is_control() || matches!(c, '\\' | '/' | '"' | '<' | '>' | '|' | '*' | '?'))
    {
        return Err(
            "A window class is a short name without paths, wildcards or quotes.".to_string(),
        );
    }
    if protected_classes()
        .iter()
        .any(|item| item.eq_ignore_ascii_case(class))
    {
        return Err(format!(
            "{class} is already protected, so it is never styled and does not need listing."
        ));
    }
    if config
        .exclude_classes
        .iter()
        .any(|item| item.eq_ignore_ascii_case(class))
    {
        return Err(format!("{class} is already on your list."));
    }
    if config.exclude_classes.len() >= MAX_CLASSES {
        return Err(format!(
            "The list holds at most {MAX_CLASSES} window classes."
        ));
    }
    config.exclude_classes.push(class.to_string());
    Ok(format!(
        "Windows of class {class} are now left alone, in every app."
    ))
}

/// Remove the class at `index`. Returns its name.
pub fn remove_class(config: &mut Config, index: usize) -> Option<String> {
    (index < config.exclude_classes.len()).then(|| config.exclude_classes.remove(index))
}

/// Say, for one window, whether and why it is excluded: the layer names match `precedence`.
pub fn why(exe: &str, class: &str, config: &Config) -> Option<String> {
    exclusion_for(exe, class, config).map(|reason| {
        let layer = match reason {
            Exclusion::ProtectedClass => "1. Protected window class",
            Exclusion::YourClass => "2. Window class you listed",
            Exclusion::ProtectedProcess => "3. Protected program",
            Exclusion::YourApp => "4. Program you listed",
            Exclusion::ExplorerShell => "5. Explorer shell window",
        };
        format!(
            "{layer}. {}{}",
            reason.explain(),
            if reason.can_be_lifted() {
                " You can lift this."
            } else {
                ""
            }
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse_config;

    fn config(text: &str) -> Config {
        parse_config(text).unwrap()
    }

    #[test]
    fn precedence_text_lists_the_same_layers_in_the_same_order_as_the_policy() {
        let text = precedence().join("\n");
        let positions: Vec<usize> = [
            "Protected window classes",
            "Window classes you listed",
            "Protected programs",
            "Programs you listed",
            "Explorer's own shell windows",
        ]
        .iter()
        .map(|part| text.find(part).unwrap_or_else(|| panic!("{part} missing")))
        .collect();
        assert!(positions.windows(2).all(|w| w[0] < w[1]), "{positions:?}");
        // The walk below proves each numbered layer is what `exclusion_for` returns.
        let cfg = config(r#"{"exclude":["mine.exe"],"excludeClasses":["MyClass"]}"#);
        for (exe, class, layer) in [
            ("any.exe", "Shell_TrayWnd", "1."),
            ("any.exe", "MyClass", "2."),
            ("dwm.exe", "Anything", "3."),
            ("mine.exe", "Anything", "4."),
            ("explorer.exe", "Weird", "5."),
        ] {
            let why = why(exe, class, &cfg).unwrap_or_else(|| panic!("{exe} {class}"));
            assert!(why.starts_with(layer), "{exe}/{class}: {why}");
        }
        assert!(why("notepad.exe", "Notepad", &cfg).is_none());
        assert!(
            why("explorer.exe", "CabinetWClass", &cfg).is_none(),
            "file windows are styled"
        );
    }

    #[test]
    fn what_can_be_lifted_is_only_what_the_user_listed() {
        let cfg = config(r#"{"exclude":["mine.exe"],"excludeClasses":["MyClass"]}"#);
        assert!(why("mine.exe", "X", &cfg)
            .unwrap()
            .ends_with("You can lift this."));
        assert!(why("any.exe", "MyClass", &cfg)
            .unwrap()
            .ends_with("You can lift this."));
        assert!(!why("dwm.exe", "X", &cfg).unwrap().contains("lift"));
        assert!(!why("a.exe", "Shell_TrayWnd", &cfg)
            .unwrap()
            .contains("lift"));
    }

    #[test]
    fn classes_are_validated_deduplicated_and_bounded() {
        let mut cfg = config("{}");
        assert!(add_class(&mut cfg, "  Notepad ")
            .unwrap()
            .contains("Notepad"));
        assert_eq!(cfg.exclude_classes, ["Notepad"]);
        assert!(add_class(&mut cfg, "notepad")
            .unwrap_err()
            .contains("already on your list"));
        assert!(add_class(&mut cfg, "Shell_TrayWnd")
            .unwrap_err()
            .contains("already protected"));
        for bad in [
            "",
            "   ",
            "a\\b",
            "a/b",
            "a*",
            "a\"b",
            "x\u{7}",
            &"x".repeat(200),
        ] {
            assert!(add_class(&mut cfg, bad).is_err(), "{bad:?}");
        }
        for index in 0..MAX_CLASSES {
            let _ = add_class(&mut cfg, &format!("Cls{index}"));
        }
        assert_eq!(cfg.exclude_classes.len(), MAX_CLASSES);
        assert!(add_class(&mut cfg, "OneMore")
            .unwrap_err()
            .contains("at most"));
        assert_eq!(remove_class(&mut cfg, 0).as_deref(), Some("Notepad"));
        assert_eq!(remove_class(&mut cfg, 999), None);
        assert_eq!(cfg.exclude_classes.len(), MAX_CLASSES - 1);
    }

    #[test]
    fn the_summary_counts_protected_and_user_lists_and_disabled_rules() {
        let cfg = config(
            r#"{"exclude":["a.exe","b.exe","c.exe","d.exe","e.exe"],"excludeClasses":["K"],
                "rules":[{"name":"No styling for chat","process":"chat.exe","enabled":false},
                         {"process":"ok.exe","backdrop":"none"}]}"#,
        );
        let text = summary(&cfg).join("\n");
        assert!(text.contains(&format!("{} window classes", protected_classes().len())));
        assert!(
            text.contains("Programs you listed: 5 (a.exe, b.exe, c.exe, d.exe and 1 more)."),
            "{text}"
        );
        assert!(text.contains("Window classes you listed: 1 (K)."));
        assert!(
            text.contains("Rules that switch styling off for an app: 1 (No styling for chat)."),
            "{text}"
        );
        let empty = summary(&config("{}")).join("\n");
        assert!(
            empty.contains("Programs you listed: none.")
                && empty.contains("Window classes you listed: none.")
        );
    }
}
