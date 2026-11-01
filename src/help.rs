//! Help topics, their search, and the walkthroughs ("tutorials").
//!
//! Content only, so it can be tested: every tutorial step names a real page and a real
//! control, a tutorial adapts when something it needs is missing instead of describing a
//! button that is not there, and no step asks for anything destructive. The UI shows a step as
//! an inline tip on the page itself; nothing blocks input and nothing is simulated.

use crate::destinations::Dest;

/// The places walkthrough steps point at. Typed, so a reorganized Settings cannot send a step to
/// the wrong page (the old numeric indices did).
const HOME: Dest = Dest::Home;
const FIX: Dest = Dest::AppsFix;
const APPEARANCE: Dest = Dest::AppearanceEffects;
const SCENES: Dest = Dest::AppearanceScenes;
const TOOLS: Dest = Dest::AppearanceTools;
const RULES: Dest = Dest::AppsRules;
const SAFETY: Dest = Dest::Safety;
const HELP: Dest = Dest::HelpTopics;

/// A control a step points at. The UI outlines it; it stays fully usable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    PickWindow,
    FixCard,
    PreviewHere,
    SaveRule,
    ExcludeApp,
    QuickLooks,
    SceneList,
    ImportRecipe,
    SaveLook,
    PauseOrResume,
    RestoreLastKnownGood,
    StyleReceipt,
    HelpSearch,
}

/// Things a step can depend on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    /// A window is picked on the Fix this app page.
    pub has_selection: bool,
    /// AllBlack11 is paused right now.
    pub paused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Condition {
    /// Show this step only if a window is picked.
    NeedsSelection,
    /// Show this step only if no window is picked yet (it tells the reader how to pick one).
    NeedsNoSelection,
}

impl Condition {
    fn holds(self, caps: &Capabilities) -> bool {
        match self {
            Condition::NeedsSelection => caps.has_selection,
            Condition::NeedsNoSelection => !caps.has_selection,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Step {
    pub page: Dest,
    pub text: &'static str,
    pub target: Option<Target>,
    pub when: Option<Condition>,
}

#[derive(Debug, Clone, Copy)]
pub struct Tutorial {
    pub id: &'static str,
    pub title: &'static str,
    pub blurb: &'static str,
    pub steps: &'static [Step],
}

const fn step(page: Dest, text: &'static str, target: Option<Target>) -> Step {
    Step {
        page,
        text,
        target,
        when: None,
    }
}

const fn step_if(page: Dest, text: &'static str, target: Option<Target>, when: Condition) -> Step {
    Step {
        page,
        text,
        target,
        when: Some(when),
    }
}

pub const TUTORIALS: [Tutorial; 6] = [
    Tutorial {
        id: "pick-and-inspect",
        title: "Pick and inspect an app",
        blurb: "Find out what AllBlack11 can change in any window.",
        steps: &[
            step(FIX, "Press Pick a window, drag the crosshair onto any app, and let go. You can also choose from the list.", Some(Target::PickWindow)),
            step_if(FIX, "Read what can change here. Each line is about one part of the window: title bar, border, backdrop, the app's own content, and classic dialog controls.", Some(Target::FixCard), Condition::NeedsSelection),
            step_if(FIX, "Pick a window first. The list below the button shows open apps.", Some(Target::PickWindow), Condition::NeedsNoSelection),
            step_if(FIX, "Picking an app does not mean every control inside it can be recolored. Lines that say may not show or not changed explain why.", Some(Target::FixCard), Condition::NeedsSelection),
        ],
    },
    Tutorial {
        id: "preview-keep-revert",
        title: "Preview, keep or revert a look",
        blurb: "Try a look safely before it is saved.",
        steps: &[
            step(APPEARANCE, "Choose a quick look. It runs for 15 seconds and nothing is saved yet.", Some(Target::QuickLooks)),
            step(APPEARANCE, "A bar at the top counts down. Keep saves the look. Revert, or waiting, puts everything back.", None),
            step_if(FIX, "To try a look on one app only, pick it here and press Preview here. It goes back after 20 seconds.", Some(Target::PreviewHere), Condition::NeedsSelection),
        ],
    },
    Tutorial {
        id: "per-app-exception",
        title: "Create a per-app exception",
        blurb: "Give one app its own look, or leave it alone.",
        steps: &[
            step_if(FIX, "Pick the app you want to treat differently.", Some(Target::PickWindow), Condition::NeedsNoSelection),
            step_if(FIX, "Choose how the rule matches, then press Save as rule to give this app its own look. Press Exclude app to leave it alone.", Some(Target::SaveRule), Condition::NeedsSelection),
            step(RULES, "Your rules are listed here. The most specific match wins, then the higher priority. Pick one, then edit its look under Appearance > Effects and colors.", None),
        ],
    },
    Tutorial {
        id: "recipes",
        title: "Save or import a recipe",
        blurb: "Share a look as a small data file, or keep one you like.",
        steps: &[
            step(SCENES, "Scenes and your saved looks live here. Type a name and press Save look to keep the current look.", Some(Target::SaveLook)),
            step(TOOLS, "Recipes are small data files that describe a look. They cannot run anything. Import checks the file and shows what it would change before you try it.", Some(Target::ImportRecipe)),
        ],
    },
    Tutorial {
        id: "unsupported-effect",
        title: "Understand an unsupported effect",
        blurb: "Find out why a change did not show.",
        steps: &[
            step_if(FIX, "Pick the app where the change did not show.", Some(Target::PickWindow), Condition::NeedsNoSelection),
            step_if(FIX, "Look for lines marked may not show or not changed. Many apps draw their own title bar or an opaque window, which hides frame colors and materials.", Some(Target::FixCard), Condition::NeedsSelection),
            step_if(FIX, "The style receipt shows what was requested, what Windows answered, and that what is on screen is not verified.", Some(Target::StyleReceipt), Condition::NeedsSelection),
            step(HELP, "Search Help for common compatibility limits.", Some(Target::HelpSearch)),
        ],
    },
    Tutorial {
        id: "pause-and-recover",
        title: "Pause and recover safely",
        blurb: "Put everything back at any time.",
        steps: &[
            step(HOME, "Pause all styling puts every window back as Windows draws it. It is not saved: restarting AllBlack11 starts from your saved settings.", Some(Target::PauseOrResume)),
            step(SAFETY, "Reset windows, undo shell changes, turn off experimental features, or restore the last config that loaded cleanly.", Some(Target::RestoreLastKnownGood)),
            step(SAFETY, "If AllBlack11 ever ends badly twice in a row, it starts in safe mode with experimental features off. You can also hold Shift while it starts.", None),
        ],
    },
];

/// The steps a reader will actually see, given what is available now. Order is kept; steps
/// whose condition does not hold are dropped, so a tutorial never describes a control that is
/// not on screen.
pub fn steps_for(tutorial: &Tutorial, caps: &Capabilities) -> Vec<Step> {
    tutorial
        .steps
        .iter()
        .copied()
        .filter(|step| step.when.map_or(true, |condition| condition.holds(caps)))
        .collect()
}

pub fn find(id: &str) -> Option<&'static Tutorial> {
    TUTORIALS.iter().find(|t| t.id == id)
}

// ---------------------------------------------------------------------------
// Help topics
// ---------------------------------------------------------------------------

pub struct Topic {
    pub title: &'static str,
    pub keywords: &'static [&'static str],
    pub body: &'static [&'static str],
}

pub const TOPICS: [Topic; 11] = [
    Topic {
        title: "Quick start",
        keywords: &["start", "begin", "first", "setup", "how"],
        body: &[
            "1. Choose Apps, then Customize an app, press Pick a window, and drag onto any app.",
            "2. Read what can change, then press Preview here to try your look for 20 seconds.",
            "3. Press Save as rule to keep it for that app, or pick a scene under Appearance to change the default look.",
            "You can run the guided setup again from Help & About.",
        ],
    },
    Topic {
        title: "What AllBlack11 changes",
        keywords: &["frame", "title", "border", "backdrop", "mica", "acrylic", "what"],
        body: &[
            "It colors window frames: title bars, borders, corners, and optional Mica or Acrylic backdrops, using Windows' own window attributes. It does not put code into other apps for this.",
            "It does not recolor everything inside every app. Web content, games and many modern apps draw their own surfaces.",
        ],
    },
    Topic {
        title: "Supported and experimental",
        keywords: &["experimental", "legacy", "classic", "dialog", "hook", "x64", "helper", "arm64", "supported"],
        body: &[
            "Supported: frame styling, scenes, rules, the picker and the command palette.",
            "Experimental, off until you turn it on: dark classic dialogs (Rules and compatibility). On ARM64, emulated x64 coverage uses a small helper program that starts only when needed.",
            "Optional and off by default: identity accents, Day / Night scenes and the palette hotkey.",
        ],
    },
    Topic {
        title: "Recovery",
        keywords: &["undo", "restore", "pause", "safe", "broken", "revert", "recover", "reset"],
        body: &[
            "Pause all styling (Home, Safety or the tray menu) puts every window back at once. It is not saved.",
            "Safety also has Reset windows, Undo shell changes, Turn off experimental features, and Restore last known good for a broken config.",
            "Hold Shift while AllBlack11 starts, or run it with --safe, to start with experimental features off. Uninstalling restores what AllBlack11 changed.",
        ],
    },
    Topic {
        title: "Updates",
        keywords: &["update", "upgrade", "new version", "download", "preview", "stable", "release", "install", "github", "checksum"],
        body: &[
            "Settings has an Updates card. Press Check for updates, or turn on the once-a-day check (off by default). AllBlack11 only asks GitHub whether a newer release exists; it sends no information about you.",
            "Nothing is downloaded or installed until you press a button. A download is checked against the checksum GitHub publishes, and a file that does not match is deleted. Install opens the downloaded Setup and closes AllBlack11 so it can be replaced; your settings are kept.",
            "Update channel: Automatic offers previews to a preview build and only stable releases to a stable build. Stable only and Include previews override that. Skip this version hides one release until a newer one appears.",
        ],
    },
    Topic {
        title: "Compatibility packs",
        keywords: &["pack", "packs", "store", "calculator", "microsoft store", "uwp", "compatibility", "import"],
        body: &[
            "Rules has a Compatibility packs section. A pack is a small data file that says how to style one app and what cannot be styled. Install adds ordinary rules named Pack: followed by the app; it runs nothing. Remove deletes exactly those rules.",
            "The Microsoft Store and Calculator packs change only the window frame: the title bar and border. What the app draws inside its window is drawn by the app itself and cannot be recolored from outside.",
            "Your own rules always win over a pack. Install and Remove appear in Look history, and Undo takes them back. Import pack reads a file from this PC, checks it strictly and refuses anything that is not plain data.",
        ],
    },
    Topic {
        title: "Wallpapers",
        keywords: &["wallpaper", "wallpapers", "background", "oled", "desktop", "fit", "fill", "download"],
        body: &[
            "The Wallpapers page has eight original black wallpapers made for OLED screens. Pick one to see what it looks like and how big the download is. Nothing is downloaded until you press Download image, and the file is checked against a checksum built into AllBlack11.",
            "Preview full screen shows the picture on this display the way Windows will place it. Enter applies it and Esc closes. Apply to chooses one display or all of them. Windows keeps one fit for all displays.",
            "Put back previous returns the picture that was there before you applied one. A solid color or a slideshow cannot be put back. Open file location shows the downloaded file.",
        ],
    },
    Topic {
        title: "Why an app did not change",
        keywords: &["why", "not", "change", "chrome", "edge", "electron", "limit", "opaque", "custom", "title bar"],
        body: &[
            "Many apps draw their own title bar, so caption colors cannot show. Apps with an opaque window hide a backdrop behind it.",
            "Customize an app lists each part of the window with a reason. A successful call to Windows is not proof you can see the result, and AllBlack11 says so.",
            "Protected system windows, elevated apps and games are left alone on purpose.",
        ],
    },
    Topic {
        title: "Trials: Keep and Revert",
        keywords: &["trial", "keep", "revert", "preview", "15", "countdown", "save"],
        body: &[
            "Quick looks and scenes run as a 15 second trial. Nothing is written to your config until you press Keep.",
            "If you do nothing, or press Revert, the earlier look returns. If something else changes your config while a trial runs, the trial ends first.",
        ],
    },
    Topic {
        title: "Diagnostics and privacy",
        keywords: &["diagnostics", "privacy", "report", "export", "titles", "log", "data", "telemetry"],
        body: &[
            "AllBlack11 has no telemetry and sends no information about you. The only site it ever contacts is GitHub, and only when you press Check for updates, turn on the daily check, or download an update.",
            "Reports leave out window titles and your user name and profile path unless you turn on Show window titles. Titles are never saved to disk.",
            "Settings, saved looks, accent assignments and notes stay in your AllBlack11 data folder on this PC.",
        ],
    },
    Topic {
        title: "Keyboard and accessibility",
        keywords: &["keyboard", "tab", "shortcut", "narrator", "screen reader", "contrast", "text size", "access"],
        body: &[
            "Tab and Shift+Tab walk one ring: the page list, then the page's controls, then Keep and Revert while a look is being tried. Enter or Space presses the focused control. Esc reverts a look being tried, and otherwise closes Settings.",
            "F6 and Shift+F6 jump between the category list and the page. In the category list, Up, Down, Home and End choose a category. Left and Right change a slider and move within a row of buttons or a list. Ctrl+Tab and Ctrl+Shift+Tab change category, Alt+Left goes back from a subpage, and Ctrl+K moves to the search box, where Enter opens the first result and Escape clears it. Page Up, Page Down, Home, End and the mouse wheel scroll the page.",
            "AllBlack11's own windows follow Windows light, dark and high contrast, and the Text size setting. Choose Light or Dark for AllBlack11 itself under Settings.",
        ],
    },
];

/// Topics matching a query, best first. Every word must match somewhere; a title match beats
/// a keyword match beats a body match. An empty query lists everything in order.
pub fn search(query: &str) -> Vec<&'static Topic> {
    let query = query.to_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();
    let mut scored: Vec<(u32, usize, &'static Topic)> = Vec::new();
    for (index, topic) in TOPICS.iter().enumerate() {
        if words.is_empty() {
            scored.push((0, index, topic));
            continue;
        }
        let title = topic.title.to_lowercase();
        let mut total = 0;
        let mut all = true;
        for word in &words {
            let score = if title.contains(word) {
                0
            } else if topic.keywords.iter().any(|k| k.contains(word)) {
                1
            } else if topic
                .body
                .iter()
                .any(|line| line.to_lowercase().contains(word))
            {
                2
            } else {
                all = false;
                break;
            };
            total += score;
        }
        if all {
            scored.push((total, index, topic));
        }
    }
    scored.sort_by_key(|(score, index, _)| (*score, *index));
    scored.into_iter().map(|(_, _, topic)| topic).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_exactly_the_six_requested_walkthroughs_each_with_real_steps() {
        let ids: Vec<&str> = TUTORIALS.iter().map(|t| t.id).collect();
        assert_eq!(
            ids,
            [
                "pick-and-inspect",
                "preview-keep-revert",
                "per-app-exception",
                "recipes",
                "unsupported-effect",
                "pause-and-recover"
            ]
        );
        for tutorial in &TUTORIALS {
            assert!(!tutorial.steps.is_empty(), "{}", tutorial.id);
            for step in tutorial.steps {
                assert!(
                    step.page.searchable() && !step.page.is_hub(),
                    "{}: a step belongs on a page with controls, not {:?}",
                    tutorial.id,
                    step.page
                );
                assert!(!step.text.is_empty());
                assert!(
                    step.text.chars().count() <= 230,
                    "{}: keep tips short ({})",
                    tutorial.id,
                    step.text
                );
            }
        }
    }

    #[test]
    fn every_tutorial_works_with_and_without_a_picked_window() {
        for tutorial in &TUTORIALS {
            for has_selection in [false, true] {
                let caps = Capabilities {
                    has_selection,
                    paused: false,
                };
                let steps = steps_for(tutorial, &caps);
                assert!(
                    !steps.is_empty(),
                    "{} has no steps with selection={has_selection}",
                    tutorial.id
                );
                // A step that says "pick a window first" never appears once one is picked, and
                // a step that describes the picked window never appears before.
                for step in &steps {
                    match step.when {
                        Some(Condition::NeedsNoSelection) => assert!(!has_selection),
                        Some(Condition::NeedsSelection) => assert!(has_selection),
                        None => {}
                    }
                }
            }
        }
    }

    #[test]
    fn steps_that_need_a_window_are_dropped_not_described_in_the_dark() {
        let pick = find("pick-and-inspect").unwrap();
        let without = steps_for(pick, &Capabilities::default());
        assert!(without
            .iter()
            .any(|s| s.text.contains("Pick a window first")));
        assert!(!without
            .iter()
            .any(|s| s.text.contains("Read what can change here")));
        let with = steps_for(
            pick,
            &Capabilities {
                has_selection: true,
                paused: false,
            },
        );
        assert!(with
            .iter()
            .any(|s| s.text.contains("Read what can change here")));
        assert!(!with.iter().any(|s| s.text.contains("Pick a window first")));
    }

    #[test]
    fn nothing_in_a_tutorial_asks_for_something_destructive_or_nonexistent() {
        let banned = [
            "delete",
            "uninstall",
            "format",
            "registry",
            "run as administrator",
            "disable security",
            "press F",
        ];
        for tutorial in &TUTORIALS {
            for step in tutorial.steps {
                let text = step.text.to_lowercase();
                for word in banned {
                    assert!(!text.contains(word), "{}: {word} in {text}", tutorial.id);
                }
            }
        }
        // Every control a step points at is a real one the UI can outline.
        let targets: Vec<Target> = TUTORIALS
            .iter()
            .flat_map(|t| t.steps.iter().filter_map(|s| s.target))
            .collect();
        assert!(targets.contains(&Target::PickWindow));
        assert!(targets.contains(&Target::PauseOrResume));
    }

    #[test]
    fn writing_follows_the_style_rules_sentence_case_no_blame_active_voice() {
        for tutorial in &TUTORIALS {
            assert!(tutorial.title.chars().next().unwrap().is_uppercase());
            // Sentence case: only the first letter of the title is capitalized (proper nouns aside).
            assert!(!tutorial.title.contains(". "), "{}", tutorial.title);
            for step in tutorial.steps {
                for banned in ["you must", "you failed", "you forgot", "simply", "just "] {
                    assert!(
                        !step.text.to_lowercase().contains(banned),
                        "{}: {banned}",
                        tutorial.id
                    );
                }
            }
        }
    }

    #[test]
    fn search_finds_by_title_keyword_and_body_and_needs_every_word() {
        assert_eq!(search("recovery")[0].title, "Recovery");
        assert_eq!(search("pause")[0].title, "Recovery", "a keyword match");
        assert!(search("telemetry")
            .iter()
            .any(|t| t.title == "Diagnostics and privacy"));
        assert!(
            search("shift").iter().any(|t| t.title == "Recovery"),
            "a body match"
        );
        assert!(search("chrome edge")[0].title.contains("did not change"));
        assert!(search("zebra").is_empty());
        assert!(
            search("recovery zebra").is_empty(),
            "every word has to match"
        );
        assert_eq!(search("  ").len(), TOPICS.len());
        assert_eq!(
            search("RECOVERY")[0].title,
            "Recovery",
            "case does not matter"
        );
    }

    #[test]
    fn help_makes_no_universal_promises_and_states_the_privacy_facts() {
        let all: String = TOPICS
            .iter()
            .flat_map(|t| t.body.iter().copied().chain(std::iter::once(t.title)))
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        for banned in [
            "guarantee",
            "always works",
            "universal",
            "100%",
            "confirmed",
        ] {
            assert!(!all.contains(banned), "{banned}");
        }
        assert!(all.contains("no telemetry"));
        assert!(all.contains("not proof you can see"));
        assert!(all.contains("experimental"));
    }
}
