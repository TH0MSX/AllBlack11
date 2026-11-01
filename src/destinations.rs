//! The one registry of places in Settings: every page, its category, its parent for "back", its
//! icon and its words.
//!
//! Navigation, global search, the command palette, Help walkthroughs, the first-run setup and the
//! `--open-settings` switch all name a place by [`Dest`], never by a number. Numbers broke once
//! (the palette's "Fix an app" opened Appearance after the pages were reordered); a typed id cannot
//! point at the wrong page, and the tests below fail if a place is added without being described.

/// The six top-level categories, in navigation order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Home,
    Appearance,
    Apps,
    Safety,
    Settings,
    Help,
}

impl Category {
    pub const ALL: [Category; 6] = [
        Category::Home,
        Category::Appearance,
        Category::Apps,
        Category::Safety,
        Category::Settings,
        Category::Help,
    ];

    /// The label in the navigation pane.
    pub fn title(self) -> &'static str {
        match self {
            Category::Home => "Home",
            Category::Appearance => "Appearance",
            Category::Apps => "Apps",
            Category::Safety => "Safety",
            Category::Settings => "Settings",
            Category::Help => "Help & About",
        }
    }

    /// The page the navigation item opens.
    pub fn landing(self) -> Dest {
        match self {
            Category::Home => Dest::Home,
            Category::Appearance => Dest::Appearance,
            Category::Apps => Dest::Apps,
            Category::Safety => Dest::Safety,
            Category::Settings => Dest::Settings,
            Category::Help => Dest::Help,
        }
    }

    /// Segoe Fluent Icons code point.
    pub fn glyph(self) -> char {
        self.landing().glyph()
    }
}

/// Every place in Settings. A hub is a category's landing page: cards that open its subpages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dest {
    Home,
    Appearance,
    AppearanceScenes,
    AppearanceEffects,
    AppearanceWallpapers,
    AppearanceSchedule,
    AppearanceTools,
    Apps,
    AppsFix,
    AppsRules,
    AppsPacks,
    AppsExclusions,
    Safety,
    SafetyExperimental,
    Settings,
    Help,
    HelpTopics,
    HelpDiagnostics,
    HelpAbout,
    /// Search results. Reached only by typing in the search box; never listed or searched.
    Search,
}

pub struct Info {
    pub dest: Dest,
    /// Stable text id, used by `--open-settings=` and kept for ever: "appearance/wallpapers".
    pub id: &'static str,
    /// Page title.
    pub title: &'static str,
    /// One short line under the title, and on the card that opens it.
    pub description: &'static str,
    /// Where "back" goes. `None` for a category landing page.
    pub parent: Option<Dest>,
    pub category: Category,
    /// Segoe Fluent Icons code point.
    pub glyph: char,
}

const fn info(
    dest: Dest,
    id: &'static str,
    title: &'static str,
    description: &'static str,
    parent: Option<Dest>,
    category: Category,
    glyph: char,
) -> Info {
    Info {
        dest,
        id,
        title,
        description,
        parent,
        category,
        glyph,
    }
}

pub const ALL: [Info; 20] = [
    info(
        Dest::Home,
        "home",
        "Home",
        "What AllBlack11 is doing now, and what to do next.",
        None,
        Category::Home,
        '\u{E80F}',
    ),
    info(
        Dest::Appearance,
        "appearance",
        "Appearance",
        "Scenes, colors and effects, wallpapers and schedules.",
        None,
        Category::Appearance,
        '\u{E790}',
    ),
    info(
        Dest::AppearanceScenes,
        "appearance/scenes",
        "Scenes and looks",
        "Switch the whole default look in one step, or save your own.",
        Some(Dest::Appearance),
        Category::Appearance,
        '\u{E91B}',
    ),
    info(
        Dest::AppearanceEffects,
        "appearance/effects",
        "Effects and colors",
        "Colors, materials and corners for window frames.",
        Some(Dest::Appearance),
        Category::Appearance,
        '\u{E790}',
    ),
    info(
        Dest::AppearanceWallpapers,
        "appearance/wallpapers",
        "Wallpapers",
        "Eight original black wallpapers for OLED screens.",
        Some(Dest::Appearance),
        Category::Appearance,
        '\u{E8B9}',
    ),
    info(
        Dest::AppearanceSchedule,
        "appearance/schedule",
        "Schedule and accents",
        "Day and night scenes, and a border color for each app.",
        Some(Dest::Appearance),
        Category::Appearance,
        '\u{E121}',
    ),
    info(
        Dest::AppearanceTools,
        "appearance/tools",
        "Palette and recipes",
        "Build a look from a picture, or import, export and remix recipes.",
        Some(Dest::Appearance),
        Category::Appearance,
        '\u{E70F}',
    ),
    info(
        Dest::Apps,
        "apps",
        "Apps",
        "Customize one app, set rules, and choose what to leave alone.",
        None,
        Category::Apps,
        '\u{E71D}',
    ),
    info(
        Dest::AppsFix,
        "apps/fix",
        "Customize an app",
        "Pick any window to see what can change, preview a look, then keep it or undo it.",
        Some(Dest::Apps),
        Category::Apps,
        '\u{E90F}',
    ),
    info(
        Dest::AppsRules,
        "apps/rules",
        "Per-app rules",
        "Give one app its own look, and test which rule wins.",
        Some(Dest::Apps),
        Category::Apps,
        '\u{E8EC}',
    ),
    info(
        Dest::AppsPacks,
        "apps/packs",
        "Compatibility packs",
        "Templates for apps that need special handling, and what each can and cannot do.",
        Some(Dest::Apps),
        Category::Apps,
        '\u{E7B8}',
    ),
    info(
        Dest::AppsExclusions,
        "apps/exclusions",
        "Exclusions",
        "Apps and window classes AllBlack11 never styles.",
        Some(Dest::Apps),
        Category::Apps,
        '\u{E711}',
    ),
    info(
        Dest::Safety,
        "safety",
        "Safety",
        "Try, keep or revert; undo; pause; restore. Everything here is reversible.",
        None,
        Category::Safety,
        '\u{EA18}',
    ),
    info(
        Dest::SafetyExperimental,
        "safety/experimental",
        "Experimental features",
        "Classic dialogs and the x64 helper. Off by default.",
        Some(Dest::Safety),
        Category::Safety,
        '\u{E7BA}',
    ),
    info(
        Dest::Settings,
        "settings",
        "Settings",
        "How AllBlack11 looks and starts, shortcuts and update preferences.",
        None,
        Category::Settings,
        '\u{E713}',
    ),
    info(
        Dest::Help,
        "help",
        "Help & About",
        "Guides, diagnostics and information about this build.",
        None,
        Category::Help,
        '\u{E897}',
    ),
    info(
        Dest::HelpTopics,
        "help/topics",
        "Help and tutorials",
        "Search the guides, take a walkthrough, or run setup again.",
        Some(Dest::Help),
        Category::Help,
        '\u{E897}',
    ),
    info(
        Dest::HelpDiagnostics,
        "help/diagnostics",
        "Diagnostics and support",
        "Details for troubleshooting. Nothing is sent anywhere.",
        Some(Dest::Help),
        Category::Help,
        '\u{E9D9}',
    ),
    info(
        Dest::HelpAbout,
        "help/about",
        "About",
        "Version, CPU type, verification status, credits and licenses.",
        Some(Dest::Help),
        Category::Help,
        '\u{E946}',
    ),
    info(
        Dest::Search,
        "search",
        "Search results",
        "Results for what you typed.",
        None,
        Category::Home,
        '\u{E721}',
    ),
];

impl Dest {
    pub fn info(self) -> &'static Info {
        // The table is checked by a test to hold every destination exactly once.
        ALL.iter().find(|i| i.dest == self).unwrap_or(&ALL[0])
    }

    pub fn id(self) -> &'static str {
        self.info().id
    }

    pub fn title(self) -> &'static str {
        self.info().title
    }

    pub fn description(self) -> &'static str {
        self.info().description
    }

    pub fn parent(self) -> Option<Dest> {
        self.info().parent
    }

    pub fn category(self) -> Category {
        self.info().category
    }

    pub fn glyph(self) -> char {
        self.info().glyph
    }

    pub fn is_hub(self) -> bool {
        matches!(self, Dest::Appearance | Dest::Apps | Dest::Help)
    }

    /// The subpages a hub opens, in order.
    pub fn children(self) -> Vec<Dest> {
        ALL.iter()
            .filter(|i| i.parent == Some(self))
            .map(|i| i.dest)
            .collect()
    }

    /// "Appearance › Wallpapers": where this place is, for search results and the page header.
    pub fn path(self) -> String {
        match self.parent() {
            Some(parent) => format!("{} › {}", parent.title(), self.title()),
            None => self.title().to_string(),
        }
    }

    /// Places search and the palette may offer.
    pub fn searchable(self) -> bool {
        self != Dest::Search
    }

    pub fn from_id(id: &str) -> Option<Dest> {
        ALL.iter().find(|i| i.id == id).map(|i| i.dest)
    }

    /// The numbers `--open-settings=N` took before 3.8 (the old page order), so a shortcut someone
    /// made keeps working. New callers use ids.
    pub fn from_legacy_index(index: usize) -> Option<Dest> {
        Some(match index {
            0 => Dest::Home,
            1 => Dest::AppsFix,
            2 => Dest::AppearanceEffects,
            3 => Dest::AppearanceScenes,
            4 => Dest::AppsRules,
            5 => Dest::Safety,
            6 => Dest::HelpDiagnostics,
            7 => Dest::AppearanceWallpapers,
            8 => Dest::Settings,
            _ => return None,
        })
    }

    /// What `--open-settings=<text>` accepts: an id, or a legacy number.
    pub fn parse_switch(text: &str) -> Option<Dest> {
        let text = text.trim();
        Dest::from_id(text).or_else(|| text.parse::<usize>().ok().and_then(Dest::from_legacy_index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVERY: [Dest; 20] = [
        Dest::Home,
        Dest::Appearance,
        Dest::AppearanceScenes,
        Dest::AppearanceEffects,
        Dest::AppearanceWallpapers,
        Dest::AppearanceSchedule,
        Dest::AppearanceTools,
        Dest::Apps,
        Dest::AppsFix,
        Dest::AppsRules,
        Dest::AppsPacks,
        Dest::AppsExclusions,
        Dest::Safety,
        Dest::SafetyExperimental,
        Dest::Settings,
        Dest::Help,
        Dest::HelpTopics,
        Dest::HelpDiagnostics,
        Dest::HelpAbout,
        Dest::Search,
    ];

    #[test]
    fn every_destination_is_described_exactly_once() {
        assert_eq!(ALL.len(), EVERY.len());
        for dest in EVERY {
            let count = ALL.iter().filter(|i| i.dest == dest).count();
            assert_eq!(count, 1, "{dest:?} appears {count} times in the table");
            let info = dest.info();
            assert!(!info.title.is_empty() && !info.description.is_empty() && !info.id.is_empty());
            assert!(info.description.ends_with('.'), "{}", info.description);
            assert!(
                info.glyph as u32 >= 0xE000 && (info.glyph as u32) < 0xF900,
                "{dest:?} icon is not from the Segoe Fluent Icons range"
            );
        }
    }

    #[test]
    fn ids_are_unique_stable_and_round_trip() {
        for dest in EVERY {
            assert_eq!(Dest::from_id(dest.id()), Some(dest), "{dest:?}");
            assert!(
                dest.id()
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '/'),
                "{}",
                dest.id()
            );
        }
        let mut ids: Vec<&str> = ALL.iter().map(|i| i.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), ALL.len());
        assert_eq!(Dest::from_id("nope"), None);
    }

    #[test]
    fn the_tree_is_one_level_and_every_parent_is_a_landing_page() {
        for dest in EVERY {
            if let Some(parent) = dest.parent() {
                assert_eq!(
                    parent.parent(),
                    None,
                    "{dest:?} is nested more than one level"
                );
                assert_eq!(
                    parent.category(),
                    dest.category(),
                    "{dest:?} sits under another category"
                );
                assert!(parent.is_hub() || parent == Dest::Safety, "{parent:?}");
            }
        }
        for category in Category::ALL {
            let landing = category.landing();
            assert_eq!(landing.category(), category);
            assert_eq!(landing.parent(), None);
        }
        // Hubs have children, and children say so.
        for hub in [Dest::Appearance, Dest::Apps, Dest::Help] {
            assert!(hub.children().len() >= 3, "{hub:?}");
        }
        assert_eq!(Dest::Safety.children(), vec![Dest::SafetyExperimental]);
        assert!(Dest::Settings.children().is_empty() && Dest::Home.children().is_empty());
    }

    #[test]
    fn six_categories_in_the_order_the_owner_asked_for() {
        let titles: Vec<&str> = Category::ALL.iter().map(|c| c.title()).collect();
        assert_eq!(
            titles,
            [
                "Home",
                "Appearance",
                "Apps",
                "Safety",
                "Settings",
                "Help & About"
            ]
        );
    }

    #[test]
    fn paths_read_naturally() {
        assert_eq!(Dest::AppearanceWallpapers.path(), "Appearance › Wallpapers");
        assert_eq!(Dest::AppsPacks.path(), "Apps › Compatibility packs");
        assert_eq!(Dest::Safety.path(), "Safety");
        assert_eq!(
            Dest::SafetyExperimental.path(),
            "Safety › Experimental features"
        );
    }

    #[test]
    fn the_old_numeric_switch_still_opens_the_page_it_always_did() {
        // 3.7 and 3.8 order: Home, Fix this app, Appearance, Scenes, Rules, Safety, Diagnostics,
        // Wallpapers, Settings and help.
        assert_eq!(Dest::parse_switch("0"), Some(Dest::Home));
        assert_eq!(Dest::parse_switch("1"), Some(Dest::AppsFix), "Fix this app");
        assert_eq!(
            Dest::parse_switch("2"),
            Some(Dest::AppearanceEffects),
            "Appearance"
        );
        assert_eq!(Dest::parse_switch("3"), Some(Dest::AppearanceScenes));
        assert_eq!(Dest::parse_switch("4"), Some(Dest::AppsRules));
        assert_eq!(Dest::parse_switch("5"), Some(Dest::Safety));
        assert_eq!(Dest::parse_switch("6"), Some(Dest::HelpDiagnostics));
        assert_eq!(Dest::parse_switch("7"), Some(Dest::AppearanceWallpapers));
        assert_eq!(Dest::parse_switch("8"), Some(Dest::Settings));
        assert_eq!(Dest::parse_switch("9"), None);
        assert_eq!(Dest::parse_switch(" apps/packs "), Some(Dest::AppsPacks));
        assert_eq!(Dest::parse_switch("garbage"), None);
    }

    #[test]
    fn the_category_of_a_page_is_its_navigation_item() {
        assert_eq!(Dest::AppsFix.category(), Category::Apps);
        assert_eq!(Dest::HelpDiagnostics.category(), Category::Help);
        assert_eq!(Dest::AppearanceWallpapers.category(), Category::Appearance);
        assert_eq!(Dest::SafetyExperimental.category(), Category::Safety);
    }
}
