//! Command palette: which commands exist right now and how a typed query finds them.
//!
//! Pure. The palette window only draws the list this produces and sends the chosen
//! [`Action`] to the host; every action maps to something the tray or Settings can already do,
//! so the palette adds a faster way in, not new behavior.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Open Settings on page `n` (Overview, Effects, Apps, Scenes, Legacy, Safety).
    Page(usize),
    /// Switch to scene `n` in tray order: the five scenes, then saved looks.
    Scene(usize),
    /// Pause or resume (one toggle in the host).
    TogglePause,
    UndoScene,
    ResetWindows,
    Diagnostics,
    OpenFolder,
    RestoreLastKnownGood,
    RestartExplorer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub title: String,
    pub hint: String,
    pub action: Action,
}

#[derive(Debug, Clone, Default)]
pub struct Context {
    pub paused: bool,
    pub can_undo_scene: bool,
    pub scenes: Vec<String>,
    pub looks: Vec<String>,
}

const PAGES: [(&str, &str); 6] = [
    ("Settings: Overview", "Quick looks, shell, what is running"),
    ("Settings: Effects", "Colors, material, corners, Focus Look"),
    ("Settings: Apps", "Pick a window, rules, exclusions"),
    (
        "Settings: Scenes",
        "Scenes, Look Book, palette from a picture",
    ),
    ("Settings: Legacy", "Dark legacy windows (experimental)"),
    ("Settings: Safety", "Pause, restore, recovery, helper"),
];

/// The commands available now. Order is the order shown for an empty query.
pub fn commands(ctx: &Context) -> Vec<Command> {
    let mut list = Vec::new();
    list.push(Command {
        title: "Fix an app: pick a window".into(),
        hint: "Inspect any window and see what can change".into(),
        action: Action::Page(2),
    });
    list.push(if ctx.paused {
        Command {
            title: "Resume styling".into(),
            hint: "Apply your saved settings again".into(),
            action: Action::TogglePause,
        }
    } else {
        Command {
            title: "Pause all styling".into(),
            hint: "Emergency stop: windows go back as Windows draws them".into(),
            action: Action::TogglePause,
        }
    });
    for (index, name) in ctx.scenes.iter().enumerate() {
        list.push(Command {
            title: format!("Scene: {name}"),
            hint: "Switch the default look".into(),
            action: Action::Scene(index),
        });
    }
    for (offset, name) in ctx.looks.iter().enumerate() {
        list.push(Command {
            title: format!("Look: {name}"),
            hint: "From your Look Book".into(),
            action: Action::Scene(ctx.scenes.len() + offset),
        });
    }
    if ctx.can_undo_scene {
        list.push(Command {
            title: "Undo last scene change".into(),
            hint: "Put the previous config back".into(),
            action: Action::UndoScene,
        });
    }
    for (index, (title, hint)) in PAGES.iter().enumerate() {
        list.push(Command {
            title: (*title).into(),
            hint: (*hint).into(),
            action: Action::Page(index),
        });
    }
    list.extend([
        Command {
            title: "Reset all windows".into(),
            hint: "Put every styled window back until the next reload".into(),
            action: Action::ResetWindows,
        },
        Command {
            title: "Restore last known good config".into(),
            hint: "Replace config.json with the last config that loaded cleanly".into(),
            action: Action::RestoreLastKnownGood,
        },
        Command {
            title: "Write diagnostics report".into(),
            hint: "Version, Windows build and settings in one text file".into(),
            action: Action::Diagnostics,
        },
        Command {
            title: "Open data folder".into(),
            hint: "Config, backups and log".into(),
            action: Action::OpenFolder,
        },
        Command {
            title: "Restart Explorer".into(),
            hint: "Asks first. File Explorer windows close".into(),
            action: Action::RestartExplorer,
        },
    ]);
    list
}

fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut chars = haystack.chars();
    needle.chars().all(|n| chars.any(|h| h == n))
}

/// Lower is better. `None` means the token does not match at all.
fn token_score(token: &str, title: &str, hint: &str) -> Option<u32> {
    if title.starts_with(token) {
        return Some(0);
    }
    if title
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word.starts_with(token))
    {
        return Some(1);
    }
    if title.contains(token) {
        return Some(2);
    }
    if hint.contains(token) {
        return Some(4);
    }
    if is_subsequence(token, title) {
        return Some(6);
    }
    None
}

/// Indexes of matching commands, best first, at most `limit`. Every word of the query must
/// match. Ties keep the original order, so results never shuffle between keystrokes.
pub fn search(commands: &[Command], query: &str, limit: usize) -> Vec<usize> {
    let query = query.to_lowercase();
    let tokens: Vec<&str> = query.split_whitespace().collect();
    let mut scored: Vec<(u32, usize)> = Vec::new();
    for (index, command) in commands.iter().enumerate() {
        if tokens.is_empty() {
            scored.push((0, index));
            continue;
        }
        let title = command.title.to_lowercase();
        let hint = command.hint.to_lowercase();
        let mut total = 0;
        let mut all = true;
        for token in &tokens {
            match token_score(token, &title, &hint) {
                Some(score) => total += score,
                None => {
                    all = false;
                    break;
                }
            }
        }
        if all {
            scored.push((total, index));
        }
    }
    scored.sort();
    scored
        .into_iter()
        .take(limit)
        .map(|(_, index)| index)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Context {
        Context {
            paused: false,
            can_undo_scene: false,
            scenes: vec![
                "OLED Black".into(),
                "Graphite".into(),
                "Midnight Accent".into(),
                "Focus".into(),
                "Living Room".into(),
            ],
            looks: vec!["Desk setup".into()],
        }
    }

    fn titles(list: &[Command], found: &[usize]) -> Vec<String> {
        found.iter().map(|i| list[*i].title.clone()).collect()
    }

    #[test]
    fn an_empty_query_lists_everything_in_order_up_to_the_limit() {
        let list = commands(&ctx());
        let found = search(&list, "  ", 8);
        assert_eq!(found, (0..8).collect::<Vec<_>>());
        assert_eq!(search(&list, "", usize::MAX).len(), list.len());
        assert!(search(&list, "", 0).is_empty());
    }

    #[test]
    fn the_first_commands_are_the_signature_ones() {
        let list = commands(&ctx());
        assert_eq!(list[0].title, "Fix an app: pick a window");
        assert_eq!(list[1].title, "Pause all styling");
    }

    #[test]
    fn pause_becomes_resume_while_paused_and_never_both() {
        let mut c = ctx();
        let running = commands(&c);
        assert!(running.iter().any(|x| x.title == "Pause all styling"));
        assert!(!running.iter().any(|x| x.title == "Resume styling"));
        c.paused = true;
        let paused = commands(&c);
        assert!(paused.iter().any(|x| x.title == "Resume styling"));
        assert!(!paused.iter().any(|x| x.title == "Pause all styling"));
    }

    #[test]
    fn scenes_and_looks_map_to_the_tray_order() {
        let list = commands(&ctx());
        let graphite = list.iter().find(|c| c.title == "Scene: Graphite").unwrap();
        assert_eq!(graphite.action, Action::Scene(1));
        let desk = list.iter().find(|c| c.title == "Look: Desk setup").unwrap();
        assert_eq!(
            desk.action,
            Action::Scene(5),
            "looks follow the five scenes"
        );
    }

    #[test]
    fn undo_only_appears_when_there_is_something_to_undo() {
        let mut c = ctx();
        assert!(!commands(&c).iter().any(|x| x.action == Action::UndoScene));
        c.can_undo_scene = true;
        assert!(commands(&c).iter().any(|x| x.action == Action::UndoScene));
    }

    #[test]
    fn typing_finds_by_prefix_word_substring_hint_and_fuzzy() {
        let list = commands(&ctx());
        assert_eq!(
            titles(&list, &search(&list, "pau", 3))[0],
            "Pause all styling"
        );
        assert_eq!(
            titles(&list, &search(&list, "grap", 3))[0],
            "Scene: Graphite"
        );
        // A word in the middle of the title.
        assert!(
            titles(&list, &search(&list, "room", 3)).contains(&"Scene: Living Room".to_string())
        );
        // Found through the hint text.
        assert!(titles(&list, &search(&list, "emergency", 3))
            .contains(&"Pause all styling".to_string()));
        // Letters in order, not together.
        assert!(titles(&list, &search(&list, "gphte", 3)).contains(&"Scene: Graphite".to_string()));
        // Every word has to match.
        assert!(search(&list, "graphite zebra", 5).is_empty());
        assert_eq!(titles(&list, &search(&list, "scene dark", 5)).len(), 0);
    }

    #[test]
    fn better_matches_come_first_and_ties_keep_their_order() {
        let list = commands(&ctx());
        let found = titles(&list, &search(&list, "settings", 10));
        // Six pages by title, then the diagnostics report, which only mentions settings in
        // its hint.
        assert_eq!(found.len(), 7);
        assert_eq!(
            found[0], "Settings: Overview",
            "original order among equals"
        );
        assert_eq!(
            found[6], "Write diagnostics report",
            "a hint match ranks below a title match"
        );
        let focus = titles(&list, &search(&list, "focus", 5));
        assert_eq!(
            focus[0], "Scene: Focus",
            "a title word beats a hint mention"
        );
    }

    #[test]
    fn matching_is_case_insensitive_and_safe_on_odd_input() {
        let list = commands(&ctx());
        assert_eq!(search(&list, "GRAPHITE", 3), search(&list, "graphite", 3));
        for odd in ["\u{0}", "ß", "😀", "   \t ", &"x".repeat(5000)] {
            let _ = search(&list, odd, 5); // must not panic
        }
    }

    #[test]
    fn every_page_has_a_command() {
        let list = commands(&ctx());
        for page in 0..6 {
            assert!(
                list.iter().any(|c| c.action == Action::Page(page)),
                "page {page}"
            );
        }
    }
}
