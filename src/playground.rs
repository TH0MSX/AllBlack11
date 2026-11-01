//! Rule playground: what would my rules do to this app? Answers in words and applies nothing.
//!
//! A subject is either typed ("notepad.exe", optionally with `class=`, `package=`, `path=`) or
//! taken from a window picked on Fix this app. The answer uses the same engine the theming pass
//! uses (`policy::exclusion_for`, `rules::rank`, `rules::resolve`), so the playground cannot
//! disagree with what really happens. It never reads a window title, and nothing it does is
//! written anywhere.

use crate::policy::exclusion_for;
use crate::report::source_text;
use crate::rules::{rank, resolve, why_it_won, WindowIdentity};
use crate::Config;

/// What is being tested. Empty strings mean "not given".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Subject {
    pub exe: String,
    pub class: String,
    pub path: String,
    pub package: String,
}

const LIMIT: usize = 260;

/// Read `notepad.exe`, or `notepad.exe class=Notepad package=Microsoft.WindowsNotepad_*`.
/// The first word is the executable name (a file name, not a path); `class=`, `package=` and
/// `path=` are optional and may appear in any order. Anything else is refused with a reason.
pub fn parse_subject(text: &str) -> Result<Subject, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Type an app name first, for example notepad.exe.".into());
    }
    if text.chars().any(|c| c.is_control()) {
        return Err("That text has characters that cannot be part of an app name.".into());
    }
    let mut parts = text.split_whitespace();
    let exe = parts.next().unwrap_or_default();
    if exe.contains('=') {
        return Err("Start with the app's file name, for example notepad.exe.".into());
    }
    if exe.contains(['\\', '/', ':']) {
        return Err(
            "Use the file name only (notepad.exe). Add path=C:\\... after it to test a path rule."
                .into(),
        );
    }
    let mut subject = Subject {
        exe: exe.to_string(),
        ..Subject::default()
    };
    for part in parts {
        let Some((key, value)) = part.split_once('=') else {
            return Err(format!(
                "I did not understand \"{part}\". Use class=..., package=... or path=..."
            ));
        };
        if value.is_empty() {
            return Err(format!("{key}= needs a value."));
        }
        let slot = match key.to_ascii_lowercase().as_str() {
            "class" => &mut subject.class,
            "package" => &mut subject.package,
            "path" => &mut subject.path,
            _ => {
                return Err(format!(
                    "\"{key}=\" is not something I can test. Use class, package or path."
                ))
            }
        };
        if !slot.is_empty() {
            return Err(format!("{key}= was given twice."));
        }
        *slot = value.to_string();
    }
    if [
        &subject.exe,
        &subject.class,
        &subject.path,
        &subject.package,
    ]
    .iter()
    .any(|v| v.len() > LIMIT)
    {
        return Err("One of those values is too long to be a real name.".into());
    }
    Ok(subject)
}

/// The answer, one line per fact, ready to show. Applies nothing.
pub fn evaluate(config: &Config, subject: &Subject, accent_bgr: u32) -> Vec<String> {
    let labels: Vec<String> = config.rules.iter().map(|rule| rule.label()).collect();
    let identity = WindowIdentity {
        exe: &subject.exe,
        class: &subject.class,
        path: &subject.path,
        package: &subject.package,
        title: None,
    };
    let mut lines = vec![format!(
        "Testing {}. Nothing is applied.",
        describe(subject)
    )];
    if !config.enabled {
        lines.push("Theme windows is off, so no window is styled. The answer below is what would happen if it were on.".into());
    }
    if let Some(reason) = exclusion_for(&subject.exe, &subject.class, config) {
        lines.push(format!("Skipped. {}", reason.explain()));
        lines.push(if reason.can_be_lifted() {
            "No rule is consulted while it is on that list.".into()
        } else {
            "It stays protected whatever your rules say, so no rule is consulted.".into()
        });
        return lines;
    }
    let ranked = rank(&config.rules, &identity);
    match ranked.split_first() {
        None => lines.push(if config.rules.is_empty() {
            "You have no rules, so the default look applies.".into()
        } else {
            "No rule matches, so the default look applies.".into()
        }),
        Some((winner, rest)) => {
            let criteria = winner
                .criteria
                .iter()
                .map(|c| c.name())
                .collect::<Vec<_>>()
                .join(" + ");
            lines.push(format!(
                "{} wins. It matched {criteria} (priority {}).",
                source_text(crate::Source::Rule(winner.index), &labels),
                winner.priority
            ));
            for loser in rest {
                lines.push(format!(
                    "  {} also matches and loses: {}.",
                    source_text(crate::Source::Rule(loser.index), &labels),
                    why_it_won(winner, loser)
                ));
            }
        }
    }
    let resolved = resolve(config, &identity, accent_bgr);
    if !resolved.enabled {
        lines.push(format!(
            "Result: not styled. Theming is turned off for it by {}.",
            source_text(resolved.enabled_source, &labels)
        ));
        return lines;
    }
    lines.push("Result: styled. Where each setting comes from:".into());
    for (field, source) in &resolved.sources {
        lines.push(format!("  {field}: {}", source_text(*source, &labels)));
    }
    lines.push(
        "This tests rules only. Whether the app shows the result depends on how it draws its window."
            .into(),
    );
    lines
}

fn describe(subject: &Subject) -> String {
    let mut text = subject.exe.clone();
    for (key, value) in [
        ("class", &subject.class),
        ("package", &subject.package),
        ("path", &subject.path),
    ] {
        if !value.is_empty() {
            text.push_str(&format!(" ({key} {value})"));
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse_config;

    fn config(json: &str) -> Config {
        parse_config(json).expect("test config")
    }

    #[test]
    fn typed_subjects_are_parsed_strictly() {
        let s = parse_subject("  notepad.exe class=Notepad package=Microsoft.WindowsNotepad_* ")
            .unwrap();
        assert_eq!(s.exe, "notepad.exe");
        assert_eq!(s.class, "Notepad");
        assert_eq!(s.package, "Microsoft.WindowsNotepad_*");
        assert_eq!(
            parse_subject("app.exe path=C:\\Tools\\app.exe")
                .unwrap()
                .path,
            "C:\\Tools\\app.exe"
        );
        for bad in [
            "",
            "   ",
            "class=Notepad",
            "C:\\Windows\\notepad.exe",
            "notepad.exe junk",
            "notepad.exe class=",
            "notepad.exe color=red",
            "notepad.exe class=A class=B",
            "note\u{0}pad.exe",
        ] {
            assert!(parse_subject(bad).is_err(), "{bad:?} should be refused");
        }
        assert!(parse_subject(&format!("{}.exe", "a".repeat(300))).is_err());
    }

    #[test]
    fn with_no_rules_the_default_look_applies_and_every_source_is_named() {
        let lines = evaluate(
            &config("{}"),
            &parse_subject("notepad.exe").unwrap(),
            0x00FF_FFFF,
        );
        let text = lines.join("\n");
        assert!(text.contains("You have no rules"), "{text}");
        assert!(text.contains("Result: styled"), "{text}");
        assert!(text.contains("title bar color:"), "{text}");
        assert!(text.contains("Nothing is applied"), "{text}");
    }

    #[test]
    fn the_winner_the_loser_and_the_reason_are_stated() {
        let cfg = config(
            r##"{"rules":[
                {"name":"all notepad","process":"notepad.exe","backdrop":"mica"},
                {"name":"notepad dialogs","process":"notepad.exe","class":"#32770","priority":5,"backdrop":"none"}
            ]}"##,
        );
        let s = parse_subject("notepad.exe class=#32770").unwrap();
        let text = evaluate(&cfg, &s, 0).join("\n");
        assert!(text.contains("rule 2 (notepad dialogs) wins"), "{text}");
        assert!(
            text.contains("rule 1 (all notepad) also matches and loses"),
            "{text}"
        );
        assert!(text.contains("priority 5 beats priority 0"), "{text}");
        assert!(text.contains("backdrop: rule 2"), "{text}");
    }

    #[test]
    fn protected_and_listed_apps_are_skipped_before_any_rule_is_consulted() {
        let cfg =
            config(r#"{"exclude":["mygame.exe"],"rules":[{"process":"*","backdrop":"mica"}]}"#);
        let protected = evaluate(&cfg, &parse_subject("dwm.exe").unwrap(), 0).join("\n");
        assert!(protected.contains("Skipped."), "{protected}");
        assert!(protected.contains("protected"), "{protected}");
        assert!(protected.contains("no rule is consulted"), "{protected}");
        let listed = evaluate(&cfg, &parse_subject("mygame.exe").unwrap(), 0).join("\n");
        assert!(listed.contains("Never theme these apps"), "{listed}");
        assert!(!listed.contains("Result: styled"), "{listed}");
    }

    #[test]
    fn a_rule_that_turns_theming_off_is_reported_as_not_styled() {
        let cfg = config(r#"{"rules":[{"name":"leave it","process":"calc.exe","enabled":false}]}"#);
        let text = evaluate(&cfg, &parse_subject("calc.exe").unwrap(), 0).join("\n");
        assert!(text.contains("Result: not styled"), "{text}");
        assert!(text.contains("rule 1 (leave it)"), "{text}");
    }

    #[test]
    fn it_says_so_when_theming_is_off_globally_and_never_reads_titles() {
        let cfg = config(r#"{"enabled":false}"#);
        let text = evaluate(&cfg, &parse_subject("notepad.exe").unwrap(), 0).join("\n");
        assert!(text.contains("Theme windows is off"), "{text}");
        // A title rule cannot match: no title is ever supplied.
        let cfg = config(r#"{"rules":[{"title":"secret","backdrop":"none"}]}"#);
        let text = evaluate(&cfg, &parse_subject("notepad.exe").unwrap(), 0).join("\n");
        assert!(text.contains("No rule matches"), "{text}");
    }
}
