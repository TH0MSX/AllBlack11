//! The window inspector's report: data plus text rendering, with redaction.
//!
//! A report answers: what is this window, what would AllBlack11 do with it and why, what did
//! Windows say when we tried, and what is *not* known. The default text omits window titles
//! and anything under the user's profile. A caller must opt in to see either.

use crate::compat::Status;
use crate::explain::{reason_text, Confirm, Explanation, Support};
use crate::rules::Source;

#[derive(Debug, Clone)]
pub struct Attempt {
    pub name: &'static str,
    pub attribute: u32,
    pub confirm: Confirm,
}

#[derive(Debug, Clone)]
pub struct LegacyView {
    pub verdict: String,
    pub gate: String,
    pub partial: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Report {
    pub exe: String,
    pub class: String,
    pub pid: u32,
    pub path: String,
    pub package: String,
    /// Shown only when the user opted in.
    pub title: String,
    pub arch: String,
    pub build: u32,
    pub explanation: Explanation,
    pub attempts: Vec<Attempt>,
    pub legacy: Option<LegacyView>,
    pub compat: Status,
    pub compat_note: String,
    /// Who is in the way: rule priority explanations, conflicts.
    pub rule_lines: Vec<String>,
    /// Per-attribute build support for the "supported / untested" column.
    pub support: Vec<(&'static str, Support)>,
}

/// `C:\Users\Alice\Documents\x.exe` -> `%USERPROFILE%\Documents\x.exe`, and the user name is
/// replaced wherever else it appears.
pub fn redact(text: &str, user_name: &str, profile_dir: &str) -> String {
    let mut out = text.to_string();
    if !profile_dir.is_empty() {
        out = replace_ci(&out, profile_dir, "%USERPROFILE%");
    }
    if user_name.len() >= 3 {
        out = replace_ci(&out, user_name, "<user>");
    }
    out
}

fn replace_ci(haystack: &str, needle: &str, with: &str) -> String {
    if needle.is_empty() {
        return haystack.to_string();
    }
    let lower_hay = haystack.to_ascii_lowercase();
    let lower_needle = needle.to_ascii_lowercase();
    let mut out = String::with_capacity(haystack.len());
    let mut rest = 0;
    while let Some(found) = lower_hay[rest..].find(&lower_needle) {
        let at = rest + found;
        out.push_str(&haystack[rest..at]);
        out.push_str(with);
        rest = at + needle.len();
    }
    out.push_str(&haystack[rest..]);
    out
}

pub fn source_text(source: Source, rules: &[String]) -> String {
    match source {
        Source::BuiltIn => "built-in".to_string(),
        Source::Default => "default look".to_string(),
        Source::Derived => "derived for contrast".to_string(),
        Source::Rule(index) => format!(
            "rule {}{}",
            index + 1,
            rules.get(index).map(|label| format!(" ({label})")).unwrap_or_default()
        ),
    }
}

/// Plain-text lines for the inspector and for copy/paste. Never includes the title or user
/// paths unless `private` is true.
pub fn render(report: &Report, rule_labels: &[String], private: bool, user: &str, profile: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let path = if private {
        report.path.clone()
    } else {
        redact(&report.path, user, profile)
    };
    lines.push(format!("{} — {}", report.exe, report.class));
    lines.push(format!("Process {} · {} · Windows build {}", report.pid, report.arch, report.build));
    if !path.is_empty() {
        lines.push(format!("Path: {path}"));
    }
    if !report.package.is_empty() {
        lines.push(format!("Package: {}", report.package));
    }
    if private && !report.title.is_empty() {
        lines.push(format!("Title: {}", report.title));
    }
    lines.push(String::new());
    lines.push(report.explanation.headline.clone());
    let framework = &report.explanation.framework;
    lines.push(format!("Looks like: {} (a guess from the window class).", framework.name));
    lines.push(framework.title_bar.text().to_string());
    lines.push(framework.client_note.to_string());
    for note in &report.explanation.notes {
        lines.push(format!("Note: {note}"));
    }
    if let Some(resolved) = &report.explanation.resolved {
        if let Some(win) = &resolved.winner {
            let list = win
                .criteria
                .iter()
                .map(|c| c.name())
                .collect::<Vec<_>>()
                .join(" + ");
            lines.push(format!(
                "Rule {} won (matched {}; priority {}).",
                win.index + 1,
                list,
                win.priority
            ));
        } else {
            lines.push("No rule matches this window, so the default look applies.".to_string());
        }
        for line in &report.rule_lines {
            lines.push(line.clone());
        }
        if report.explanation.applied {
            lines.push(String::new());
            lines.push("Where each setting comes from:".to_string());
            for (field, source) in &resolved.sources {
                lines.push(format!("  {field}: {}", source_text(*source, rule_labels)));
            }
        }
    }
    if !report.attempts.is_empty() {
        lines.push(String::new());
        lines.push("Last attempt (Windows accepted a value; that is not proof it is visible):".to_string());
        for attempt in &report.attempts {
            lines.push(format!("  {} ({}): {}", attempt.name, attempt.attribute, attempt.confirm.text()));
        }
    }
    if let Some(legacy) = &report.legacy {
        lines.push(String::new());
        lines.push(format!("Legacy layer: {}", legacy.verdict));
        lines.push(format!("  gate: {}", legacy.gate));
        if !legacy.partial.is_empty() {
            lines.push(format!("  stays light: {}", legacy.partial.join(", ")));
        }
    }
    lines.push(String::new());
    lines.push(format!(
        "Your note for this app on this build: {}{}",
        report.compat.label(),
        if report.compat_note.is_empty() {
            String::new()
        } else {
            format!(" — {}", report.compat_note)
        }
    ));
    lines
}

/// Reason text for a stable skip code, re-exported so callers need one import.
pub fn why(reason: &str) -> &'static str {
    reason_text(reason)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explain::explain;
    use crate::policy::normal_notepad;
    use crate::Config;

    fn sample(private_path: &str) -> Report {
        let facts = normal_notepad();
        Report {
            exe: "notepad.exe".into(),
            class: "Notepad".into(),
            pid: 4242,
            path: private_path.into(),
            package: "Microsoft.WindowsNotepad_8wekyb3d8bbwe".into(),
            title: "Secret plans.txt - Notepad".into(),
            arch: "ARM64".into(),
            build: 26100,
            explanation: explain(&facts, &Config::default(), 0, 26100),
            attempts: vec![Attempt {
                name: "DWMWA_CAPTION_COLOR",
                attribute: 35,
                confirm: Confirm::AcceptedUnverified,
            }],
            legacy: None,
            compat: Status::Untested,
            compat_note: String::new(),
            rule_lines: vec![],
            support: vec![],
        }
    }

    #[test]
    fn redaction_removes_profile_paths_and_user_names_case_insensitively() {
        let text = r"C:\Users\Alice\AppData\Local\Programs\x.exe and C:\USERS\ALICE\Desktop";
        let out = redact(text, "Alice", r"C:\Users\Alice");
        assert!(!out.to_lowercase().contains("alice"), "{out}");
        assert!(out.contains(r"%USERPROFILE%\AppData"));
        assert!(redact("anything", "", "").contains("anything"));
        // Very short names are not replaced: it would corrupt unrelated text.
        assert_eq!(redact("ab cd", "ab", ""), "ab cd");
    }

    #[test]
    fn the_default_text_has_no_title_and_no_user_path() {
        let report = sample(r"C:\Users\Alice\AppData\Local\Programs\notepad.exe");
        let text = render(&report, &[], false, "Alice", r"C:\Users\Alice").join("\n");
        assert!(!text.contains("Secret plans"), "title leaked: {text}");
        assert!(!text.contains("Alice"), "user name leaked: {text}");
        assert!(text.contains("%USERPROFILE%"));
        let private = render(&report, &[], true, "Alice", r"C:\Users\Alice").join("\n");
        assert!(private.contains("Secret plans"));
        assert!(private.contains("Alice"));
    }

    #[test]
    fn the_report_says_accepted_never_confirmed_and_names_rule_sources() {
        let report = sample("");
        let text = render(&report, &["Notepad rule".to_string()], false, "", "").join("\n");
        assert!(text.contains("not proof it is visible"));
        assert!(text.contains("appearance not verified"));
        assert!(!text.to_lowercase().contains("confirmed"));
        assert!(text.contains("Where each setting comes from"));
    }

    #[test]
    fn rule_sources_read_as_rule_numbers_with_labels() {
        let labels = vec!["notepad.exe".to_string()];
        assert_eq!(source_text(Source::Rule(0), &labels), "rule 1 (notepad.exe)");
        assert_eq!(source_text(Source::Default, &labels), "default look");
        assert_eq!(source_text(Source::Rule(5), &labels), "rule 6");
    }
}
