//! Command palette: which commands exist right now and how a typed query finds them.
//!
//! Pure. The palette window only draws the list this produces and sends the chosen
//! [`Action`] to the host; every action maps to something the tray or Settings can already do,
//! so the palette adds a faster way in, not new behavior.

use crate::destinations::{Dest, ALL as DESTINATIONS};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Open Settings at a place, and, when given, bring one control into view and focus it.
    /// The anchor names a control (`toggle:Autostart`) or a section heading (`header:Updates`);
    /// see [`SETTINGS`]. Opening a place never changes a setting.
    Go(Dest, Option<&'static str>),
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
    /// Where this lives, for a result row: "Appearance › Wallpapers". Empty for plain actions.
    pub path: String,
    /// Segoe Fluent Icons code point.
    pub glyph: char,
    /// Extra words that find it ("blur" finds the backdrop control). Lower case.
    pub keywords: String,
}

#[derive(Debug, Clone, Default)]
pub struct Context {
    pub paused: bool,
    pub can_undo_scene: bool,
    pub scenes: Vec<String>,
    pub looks: Vec<String>,
}

/// One searchable control or section: its words, and exactly where it is.
pub struct Setting {
    pub title: &'static str,
    pub hint: &'static str,
    pub keywords: &'static str,
    pub dest: Dest,
    /// `toggle:Name`, `choice:Name`, `slider:Name`, `button:Name`, `color:caption|border|text|tint`,
    /// `header:Section title` or `presets`. Names are the variant names in `settings.rs`;
    /// `scripts/check-controls.py` fails the build if one does not exist.
    pub anchor: &'static str,
}

const fn setting(
    title: &'static str,
    hint: &'static str,
    keywords: &'static str,
    dest: Dest,
    anchor: &'static str,
) -> Setting {
    Setting {
        title,
        hint,
        keywords,
        dest,
        anchor,
    }
}

/// Every control and section worth finding by name. Titles match the labels on the page.
pub const SETTINGS: &[Setting] = &[
    setting(
        "Theme windows",
        "Master switch for all styling",
        "on off enable disable master switch",
        Dest::Home,
        "toggle:Theme",
    ),
    setting(
        "Dark title bar",
        "Dark caption buttons and system menu",
        "dark black night mode",
        Dest::AppearanceEffects,
        "toggle:Dark",
    ),
    setting(
        "Backdrop material",
        "Mica, Acrylic or none behind a window",
        "blur frosted glass acrylic mica translucent transparent material backdrop",
        Dest::AppearanceEffects,
        "choice:Backdrop",
    ),
    setting(
        "Blur while dragging",
        "A cheaper blur during a drag or resize",
        "blur acrylic smooth drag resize fallback",
        Dest::AppearanceEffects,
        "toggle:Blur",
    ),
    setting(
        "Material tint opacity",
        "How strongly the tint covers the blur",
        "blur acrylic opacity transparency tint",
        Dest::AppearanceEffects,
        "slider:Opacity",
    ),
    setting(
        "Corners",
        "Rounded, small or square corners",
        "round square radius shape",
        Dest::AppearanceEffects,
        "choice:Corners",
    ),
    setting(
        "Title bar color",
        "The color of the window frame's top bar",
        "color colour caption black dark",
        Dest::AppearanceEffects,
        "color:caption",
    ),
    setting(
        "Border color",
        "The thin line around a window",
        "color colour outline accent",
        Dest::AppearanceEffects,
        "color:border",
    ),
    setting(
        "Title text color",
        "The color of the title bar text",
        "color colour text font readable",
        Dest::AppearanceEffects,
        "color:text",
    ),
    setting(
        "Acrylic tint color",
        "Tint used by the tinted acrylic backdrop",
        "color colour blur glass",
        Dest::AppearanceEffects,
        "color:tint",
    ),
    setting(
        "Title bar height",
        "Make title bars taller or shorter",
        "caption size thickness",
        Dest::AppearanceEffects,
        "slider:Caption",
    ),
    setting(
        "Quick looks",
        "Try a ready-made look for 15 seconds",
        "preset black dark glass accent theme",
        Dest::AppearanceEffects,
        "presets",
    ),
    setting(
        "Focus Look",
        "The window you are in gets an accent border",
        "focus active highlight border",
        Dest::AppearanceEffects,
        "toggle:FocusLook",
    ),
    setting(
        "Reapply when a window is focused",
        "Re-checks a window each time you switch to it",
        "repaint refresh focus",
        Dest::AppearanceEffects,
        "toggle:Reapply",
    ),
    setting(
        "Reduce effects automatically",
        "Drop materials on battery saver, remote sessions or when transparency is off",
        "battery saver remote desktop transparency performance",
        Dest::AppearanceEffects,
        "toggle:ReduceBattery",
    ),
    setting(
        "OLED taskbar",
        "Windows' own flag for a pure black taskbar",
        "taskbar black oled shell",
        Dest::AppearanceEffects,
        "toggle:Oled",
    ),
    setting(
        "Black Start, taskbar and notification center",
        "Black system surfaces",
        "start menu taskbar dark black shell notification",
        Dest::AppearanceEffects,
        "toggle:Black",
    ),
    setting(
        "Contrast check",
        "Measures the colors a look uses",
        "contrast readable accessibility wcag",
        Dest::AppearanceEffects,
        "header:Contrast check",
    ),
    setting(
        "Compare before and after",
        "Two sample windows, one styled and one not",
        "preview compare side by side",
        Dest::AppearanceScenes,
        "button:DemoCompare",
    ),
    setting(
        "Day and night scenes",
        "Switch scene at set local times",
        "schedule timer sunrise sunset automatic night day",
        Dest::AppearanceSchedule,
        "toggle:DayNight",
    ),
    setting(
        "Identity accents",
        "A border color of its own for each app",
        "accent border color per app",
        Dest::AppearanceSchedule,
        "toggle:IdentityAccents",
    ),
    setting(
        "Build a look from a picture",
        "Reads a few points from a picture on this PC",
        "palette image photo colors wallpaper",
        Dest::AppearanceTools,
        "button:PaletteFromImage",
    ),
    setting(
        "Import a recipe",
        "Load a look from a file after checking it",
        "recipe file import load",
        Dest::AppearanceTools,
        "button:ImportRecipe",
    ),
    setting(
        "Export the current look",
        "Save the default look as a recipe file",
        "recipe file export share save",
        Dest::AppearanceTools,
        "button:ExportRecipe",
    ),
    setting(
        "Remix two recipes",
        "Take one group of fields from another recipe",
        "recipe mix combine remix",
        Dest::AppearanceTools,
        "header:Remix two recipes",
    ),
    setting(
        "Pick a window",
        "Drag onto a window to inspect it",
        "picker crosshair inspect select app customize",
        Dest::AppsFix,
        "button:PickWindow",
    ),
    setting(
        "Style receipt",
        "What was tried on the picked window and what Windows said",
        "report receipt diagnostics result",
        Dest::AppsFix,
        "header:Style receipt",
    ),
    setting(
        "Add a rule",
        "Give one app its own look",
        "rule per app exception program",
        Dest::AppsRules,
        "button:AddRule",
    ),
    setting(
        "Test a rule",
        "See which rule would win, without applying it",
        "playground rule priority test",
        Dest::AppsRules,
        "button:TestRule",
    ),
    setting(
        "Exclude an app",
        "Games and launchers AllBlack11 should never touch",
        "never theme exclude ignore skip game launcher steam",
        Dest::AppsExclusions,
        "button:AddExclude",
    ),
    setting(
        "Exclusion manager",
        "Every reason a window is left alone",
        "exclusions protected precedence classes",
        Dest::AppsExclusions,
        "header:Exclusion manager",
    ),
    setting(
        "Microsoft Store",
        "Compatibility status of the Store pack",
        "store microsoft windows store uwp pack template",
        Dest::AppsPacks,
        "choice:Pack",
    ),
    setting(
        "Calculator",
        "Compatibility status of the Calculator pack",
        "calculator calc uwp pack template",
        Dest::AppsPacks,
        "choice:Pack",
    ),
    setting(
        "Import a pack",
        "Load a compatibility pack from a file",
        "pack template import",
        Dest::AppsPacks,
        "button:ImportPack",
    ),
    setting(
        "Compatibility notebook",
        "Your notes on what worked for an app",
        "notes notebook works partly fails",
        Dest::AppsPacks,
        "header:Compatibility notebook",
    ),
    setting(
        "Undo the last change",
        "Take back the most recent look change",
        "undo history revert back",
        Dest::Safety,
        "button:UndoLook",
    ),
    setting(
        "Restore last known good config",
        "Replace the config with the last one that ran cleanly",
        "restore reset recovery backup repair broken",
        Dest::Safety,
        "button:RestoreLkg",
    ),
    setting(
        "Reset all windows",
        "Put every styled window back as Windows draws it",
        "reset restore original undo everything",
        Dest::Safety,
        "button:ResetAllWindows",
    ),
    setting(
        "Safe mode",
        "Start without applying anything",
        "safe mode shift start recovery",
        Dest::Safety,
        "header:Safe mode",
    ),
    setting(
        "Stop experimental features",
        "Turn off the classic dialogs layer and the helper",
        "experimental off legacy helper stop",
        Dest::Safety,
        "button:StopExperimental",
    ),
    setting(
        "Dark legacy windows",
        "Recolor classic dialogs such as Run",
        "classic dialogs legacy experimental hook run",
        Dest::SafetyExperimental,
        "toggle:Experimental",
    ),
    setting(
        "Dialog palette",
        "True black or graphite classic dialogs",
        "classic dialogs legacy black graphite",
        Dest::SafetyExperimental,
        "choice:Palette",
    ),
    setting(
        "Cover emulated x64 apps",
        "ARM64 only: a helper for x64 programs under emulation",
        "arm64 x64 emulation helper experimental",
        Dest::SafetyExperimental,
        "toggle:EmulatedX64",
    ),
    setting(
        "Control harness",
        "A test window with every control type",
        "test harness classic dialogs",
        Dest::SafetyExperimental,
        "button:OpenHarness",
    ),
    setting(
        "Start with Windows",
        "Launch AllBlack11 when you sign in",
        "startup autostart login boot run at sign in",
        Dest::Settings,
        "toggle:Autostart",
    ),
    setting(
        "AllBlack11 theme",
        "Light, dark or the Windows setting for AllBlack11's own windows",
        "light dark theme appearance app",
        Dest::Settings,
        "choice:AppAppearance",
    ),
    setting(
        "Command palette hotkey",
        "Ctrl+Alt+Space opens the command palette",
        "shortcut keyboard hotkey ctrl alt space palette",
        Dest::Settings,
        "toggle:PaletteHotkey",
    ),
    setting(
        "Check for updates",
        "Ask GitHub whether a newer version exists",
        "updates update version upgrade new release download",
        Dest::Settings,
        "button:CheckUpdates",
    ),
    setting(
        "Update channel",
        "Stable only, or include previews",
        "updates stable preview channel version",
        Dest::Settings,
        "choice:UpdateChannel",
    ),
    setting(
        "Check for updates once a day",
        "Optional, off by default",
        "updates automatic daily schedule",
        Dest::Settings,
        "toggle:AutoUpdateCheck",
    ),
    setting(
        "Walkthroughs",
        "Short guided tours of the main tasks",
        "tutorial guide tour learn how",
        Dest::HelpTopics,
        "header:Walkthroughs",
    ),
    setting(
        "Run setup again",
        "The first-run setup, from the start",
        "setup first run onboarding welcome",
        Dest::HelpTopics,
        "button:SetupReplay",
    ),
    setting(
        "Write a log",
        "Save a log file in the data folder",
        "log debug troubleshoot",
        Dest::HelpDiagnostics,
        "toggle:Logging",
    ),
    setting(
        "Open the log",
        "Show allblack11.log",
        "log debug troubleshoot",
        Dest::HelpDiagnostics,
        "button:OpenLog",
    ),
    setting(
        "Version and CPU type",
        "Which build this is and whether it has been verified",
        "version about build arm64 x64 verified",
        Dest::HelpAbout,
        "header:About",
    ),
];

/// Extra words that find a whole page.
fn dest_keywords(dest: Dest) -> &'static str {
    match dest {
        Dest::Home => "status overview start",
        Dest::Appearance => "look theme style colors",
        Dest::AppearanceScenes => "scene look black dark graphite oled saved",
        Dest::AppearanceEffects => "blur material mica acrylic color corner dark black",
        Dest::AppearanceWallpapers => "wallpaper background desktop picture image oled",
        Dest::AppearanceSchedule => "schedule day night timer accent",
        Dest::AppearanceTools => "palette recipe remix image import export",
        Dest::Apps => "app program window",
        Dest::AppsFix => "fix picker inspect window customize one app",
        Dest::AppsRules => "rule priority exception",
        Dest::AppsPacks => "pack template store calculator compatibility",
        Dest::AppsExclusions => "exclude never ignore protected game",
        Dest::Safety => "restore reset recovery undo pause trial keep revert history",
        Dest::SafetyExperimental => "experimental legacy classic helper x64",
        Dest::Settings => "preferences startup shortcut theme updates version",
        Dest::Help => "help support about",
        Dest::HelpTopics => "help tutorial guide search learn setup",
        Dest::HelpDiagnostics => "diagnostics report log support export troubleshoot",
        Dest::HelpAbout => "about version credits license architecture",
        Dest::Search => "",
    }
}

fn plain(
    title: &str,
    hint: &str,
    action: Action,
    path: String,
    glyph: char,
    keywords: &str,
) -> Command {
    Command {
        title: title.into(),
        hint: hint.into(),
        action,
        path,
        glyph,
        keywords: keywords.to_lowercase(),
    }
}

/// The commands available now. Order is the order shown for an empty query.
pub fn commands(ctx: &Context) -> Vec<Command> {
    let mut list = Vec::new();
    list.push(plain(
        "Customize an app",
        "Pick a window and see what can change",
        Action::Go(Dest::AppsFix, None),
        Dest::AppsFix.path(),
        '\u{E90F}',
        "fix picker inspect style window",
    ));
    list.push(if ctx.paused {
        plain(
            "Resume styling",
            "Apply your saved settings again",
            Action::TogglePause,
            String::new(),
            '\u{E768}',
            "continue start unpause",
        )
    } else {
        plain(
            "Pause all styling",
            "Emergency stop: windows go back as Windows draws them",
            Action::TogglePause,
            String::new(),
            '\u{E769}',
            "stop off disable restore emergency",
        )
    });
    for (index, name) in ctx.scenes.iter().enumerate() {
        list.push(plain(
            &format!("Scene: {name}"),
            "Switch the default look",
            Action::Scene(index),
            Dest::AppearanceScenes.path(),
            '\u{E91B}',
            "scene look theme black dark",
        ));
    }
    for (offset, name) in ctx.looks.iter().enumerate() {
        list.push(plain(
            &format!("Look: {name}"),
            "From your Look Book",
            Action::Scene(ctx.scenes.len() + offset),
            Dest::AppearanceScenes.path(),
            '\u{E91B}',
            "saved look scene",
        ));
    }
    if ctx.can_undo_scene {
        list.push(plain(
            "Undo last scene change",
            "Put the previous config back",
            Action::UndoScene,
            String::new(),
            '\u{E7A7}',
            "revert back history",
        ));
    }
    for info in DESTINATIONS.iter().filter(|i| i.dest.searchable()) {
        list.push(plain(
            info.title,
            info.description,
            Action::Go(info.dest, None),
            info.dest.path(),
            info.glyph,
            dest_keywords(info.dest),
        ));
    }
    for entry in SETTINGS {
        list.push(plain(
            entry.title,
            entry.hint,
            Action::Go(entry.dest, Some(entry.anchor)),
            entry.dest.path(),
            entry.dest.glyph(),
            entry.keywords,
        ));
    }
    for wallpaper in crate::wallpaper::catalog() {
        list.push(plain(
            &format!("Wallpaper: {}", wallpaper.name),
            &wallpaper.description,
            Action::Go(Dest::AppearanceWallpapers, None),
            Dest::AppearanceWallpapers.path(),
            '\u{E8B9}',
            "wallpaper background desktop picture oled",
        ));
    }
    for pack in crate::packs::catalog() {
        list.push(plain(
            &format!("Pack: {}", pack.name),
            pack.verdict.label(),
            Action::Go(Dest::AppsPacks, Some("choice:Pack")),
            Dest::AppsPacks.path(),
            '\u{E7B8}',
            "compatibility template pack",
        ));
    }
    for topic in crate::help::TOPICS.iter() {
        list.push(plain(
            &format!("Help: {}", topic.title),
            topic.body.first().copied().unwrap_or(""),
            Action::Go(Dest::HelpTopics, None),
            Dest::HelpTopics.path(),
            '\u{E897}',
            &topic.keywords.join(" "),
        ));
    }
    list.extend([
        plain(
            "Reset all windows",
            "Put every styled window back until the next reload",
            Action::ResetWindows,
            String::new(),
            '\u{E777}',
            "restore reset original undo everything",
        ),
        plain(
            "Restore last known good config",
            "Replace config.json with the last config that loaded cleanly",
            Action::RestoreLastKnownGood,
            String::new(),
            '\u{E777}',
            "restore reset recovery backup repair broken",
        ),
        plain(
            "Write diagnostics report",
            "Version, Windows build and settings in one text file",
            Action::Diagnostics,
            String::new(),
            '\u{E9D9}',
            "support troubleshoot export",
        ),
        plain(
            "Open data folder",
            "Config, backups and log",
            Action::OpenFolder,
            String::new(),
            '\u{E8B7}',
            "files config log",
        ),
        plain(
            "Restart Explorer",
            "Asks first. File Explorer windows close",
            Action::RestartExplorer,
            String::new(),
            '\u{E72C}',
            "taskbar shell refresh",
        ),
    ]);
    list
}

fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut chars = haystack.chars();
    needle.chars().all(|n| chars.any(|h| h == n))
}

/// Lower is better. `None` means the token does not match at all.
fn token_score(token: &str, command: &Command, title: &str, hint: &str) -> Option<u32> {
    if title.starts_with(token) {
        return Some(0);
    }
    let word_start = |text: &str| {
        text.split(|c: char| !c.is_alphanumeric())
            .any(|word| word.starts_with(token))
    };
    if word_start(title) {
        return Some(1);
    }
    if title.contains(token) {
        return Some(2);
    }
    // A synonym ("blur" for the backdrop control) ranks just below the title's own words.
    if word_start(&command.keywords) {
        return Some(3);
    }
    if hint.contains(token) {
        return Some(4);
    }
    if command.keywords.contains(token) || command.path.to_lowercase().contains(token) {
        return Some(5);
    }
    if is_subsequence(token, title) {
        return Some(6);
    }
    None
}

/// Indexes of matching commands, best first, at most `limit`. Every word of the query must
/// match. Ties keep the original order, so results never shuffle between keystrokes. Cost is one
/// pass over a list of about a hundred short entries; nothing is cached or rebuilt per keystroke
/// except lowercasing the query.
pub fn search(commands: &[Command], query: &str, limit: usize) -> Vec<usize> {
    let query = query.to_lowercase();
    let tokens: Vec<&str> = query.split_whitespace().take(8).collect();
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
            match token_score(token, command, &title, &hint) {
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

    fn first_destination(list: &[Command], query: &str) -> Option<Dest> {
        let found = search(list, query, 1);
        found.first().and_then(|i| match list[*i].action {
            Action::Go(dest, _) => Some(dest),
            _ => None,
        })
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
        assert_eq!(list[0].title, "Customize an app");
        assert_eq!(list[1].title, "Pause all styling");
    }

    #[test]
    fn customize_an_app_opens_the_customize_page_not_a_stale_index() {
        // The regression this module had: the palette's "Fix an app" opened Appearance.
        let list = commands(&ctx());
        assert_eq!(list[0].action, Action::Go(Dest::AppsFix, None));
        assert_eq!(list[0].path, "Apps › Customize an app");
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
        assert!(
            titles(&list, &search(&list, "room", 3)).contains(&"Scene: Living Room".to_string())
        );
        assert!(titles(&list, &search(&list, "emergency", 6))
            .contains(&"Pause all styling".to_string()));
        assert!(titles(&list, &search(&list, "gphte", 6)).contains(&"Scene: Graphite".to_string()));
        assert!(search(&list, "graphite zebra", 5).is_empty());
    }

    #[test]
    fn better_matches_come_first_and_ties_keep_their_order() {
        let list = commands(&ctx());
        let focus = titles(&list, &search(&list, "focus", 5));
        assert_eq!(
            focus[0], "Focus Look",
            "a title that starts with the word comes first"
        );
        assert!(focus.contains(&"Scene: Focus".to_string()), "{focus:?}");
        let emergency = titles(&list, &search(&list, "emergency", 3));
        assert!(
            emergency.iter().any(|t| t == "Pause all styling"),
            "a hint match is still found"
        );
        let a = search(&list, "color", 20);
        assert_eq!(a, search(&list, "color", 20), "stable between calls");
    }

    #[test]
    fn matching_is_case_insensitive_and_safe_on_odd_input() {
        let list = commands(&ctx());
        assert_eq!(search(&list, "GRAPHITE", 3), search(&list, "graphite", 3));
        for odd in [
            "\u{0}",
            "ß",
            "😀",
            "   \t ",
            &"x".repeat(5000),
            &"a ".repeat(500),
        ] {
            let _ = search(&list, odd, 5); // must not panic
        }
    }

    // ---- the destination registry and the index agree --------------------------------------

    #[test]
    fn every_destination_is_reachable_from_search_by_its_own_title() {
        let list = commands(&ctx());
        for info in DESTINATIONS.iter().filter(|i| i.dest.searchable()) {
            let found = search(&list, info.title, 40);
            assert!(
                found
                    .iter()
                    .any(|i| list[*i].action == Action::Go(info.dest, None)),
                "{} is not found by its own title",
                info.title
            );
        }
        assert!(
            !list
                .iter()
                .any(|c| c.action == Action::Go(Dest::Search, None)),
            "the results page is never a result"
        );
    }

    #[test]
    fn every_setting_entry_names_a_real_place_and_a_well_formed_anchor() {
        for entry in SETTINGS {
            assert!(
                entry.dest.searchable() && entry.dest != Dest::Search,
                "{}",
                entry.title
            );
            assert!(
                !entry.title.is_empty() && !entry.hint.is_empty() && !entry.keywords.is_empty()
            );
            let (kind, name) = entry.anchor.split_once(':').unwrap_or((entry.anchor, ""));
            match kind {
                "toggle" | "choice" | "slider" | "button" => {
                    assert!(
                        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric()),
                        "{}",
                        entry.anchor
                    )
                }
                "color" => assert!(
                    ["caption", "border", "text", "tint"].contains(&name),
                    "{}",
                    entry.anchor
                ),
                "header" => assert!(!name.is_empty()),
                "presets" => assert!(name.is_empty()),
                other => panic!("unknown anchor kind {other} in {}", entry.title),
            }
        }
        let mut seen: Vec<(&str, &str)> = SETTINGS.iter().map(|s| (s.title, s.anchor)).collect();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), SETTINGS.len(), "a setting is listed twice");
    }

    #[test]
    fn the_synonyms_the_owner_asked_for_lead_to_the_right_place() {
        let list = commands(&ctx());
        let first = |q: &str| first_destination(&list, q);
        // blur -> material controls
        let blur = titles(&list, &search(&list, "blur", 6));
        assert!(blur.contains(&"Backdrop material".to_string()), "{blur:?}");
        // wallpaper / background -> the gallery
        assert_eq!(first("wallpaper"), Some(Dest::AppearanceWallpapers));
        assert!(search(&list, "background", 8)
            .iter()
            .any(|i| list[*i].action == Action::Go(Dest::AppearanceWallpapers, None)));
        // restore / reset -> recovery options
        let restore = titles(&list, &search(&list, "restore", 8));
        assert!(
            restore.contains(&"Restore last known good config".to_string())
                && restore.contains(&"Reset all windows".to_string()),
            "{restore:?}"
        );
        let reset = search(&list, "reset", 8);
        assert!(reset
            .iter()
            .any(|i| list[*i].action == Action::ResetWindows));
        // updates / version -> update settings
        let updates = titles(&list, &search(&list, "updates", 6));
        assert!(
            updates.contains(&"Check for updates".to_string()),
            "{updates:?}"
        );
        let version = search(&list, "version", 10);
        assert!(version.iter().any(|i| matches!(
            list[*i].action,
            Action::Go(Dest::Settings, Some("button:CheckUpdates"))
        )));
        // Store / Calculator -> their compatibility status
        for q in ["store", "calculator"] {
            let found = search(&list, q, 6);
            assert!(
                found
                    .iter()
                    .any(|i| list[*i].action == Action::Go(Dest::AppsPacks, Some("choice:Pack"))),
                "{q} should lead to the packs page"
            );
        }
        // black / dark -> scenes and color controls
        let dark = titles(&list, &search(&list, "black", 12));
        assert!(
            dark.iter().any(|t| t.starts_with("Scene: OLED Black"))
                && dark.contains(&"Dark title bar".to_string()),
            "{dark:?}"
        );
    }

    #[test]
    fn scenes_looks_packs_wallpapers_and_help_are_all_in_the_index() {
        let list = commands(&ctx());
        let has = |prefix: &str| list.iter().any(|c| c.title.starts_with(prefix));
        assert!(
            has("Scene: ") && has("Look: ") && has("Pack: ") && has("Wallpaper: ") && has("Help: ")
        );
        assert_eq!(
            list.iter()
                .filter(|c| c.title.starts_with("Wallpaper: "))
                .count(),
            crate::wallpaper::catalog().len()
        );
        assert_eq!(
            list.iter()
                .filter(|c| c.title.starts_with("Help: "))
                .count(),
            crate::help::TOPICS.len()
        );
    }

    #[test]
    fn selecting_a_result_only_navigates_it_never_changes_a_setting() {
        // The only actions an index entry can carry that change anything are the ones that already
        // existed as explicit commands; every settings entry is a navigation.
        for entry in SETTINGS {
            let list = commands(&ctx());
            let command = list.iter().find(|c| c.title == entry.title).unwrap();
            assert!(
                matches!(command.action, Action::Go(_, Some(_))),
                "{}",
                entry.title
            );
        }
    }

    #[test]
    fn every_result_has_an_icon_a_title_and_a_hint() {
        for c in commands(&ctx()) {
            assert!(!c.title.is_empty() && !c.hint.is_empty(), "{}", c.title);
            assert!(
                c.glyph as u32 >= 0xE000 && (c.glyph as u32) < 0xF900,
                "{}",
                c.title
            );
            if let Action::Go(dest, _) = c.action {
                assert_eq!(c.path, dest.path(), "{}", c.title);
            }
        }
    }

    #[test]
    fn indexing_is_bounded_and_fast() {
        let list = commands(&ctx());
        assert!(list.len() < 200, "{} entries", list.len());
        let started = std::time::Instant::now();
        for _ in 0..200 {
            let _ = search(&list, "restore black blur", 12);
        }
        assert!(
            started.elapsed().as_millis() < 1000,
            "200 searches took {:?}",
            started.elapsed()
        );
    }
}
