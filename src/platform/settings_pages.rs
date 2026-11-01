//! Settings pages beyond the original single scroll: the page model, the window picker and
//! inspector, Effects Studio actions, recipes and the "try it" trial, the compatibility
//! notebook, and keyboard navigation of the custom-drawn rows.
//!
//! This is a child module of `settings`, so it can use that module's private types directly.

use std::sync::atomic::Ordering;

use super::*;
use crate::{
    attribute_support, capability, describe_conflict, find_conflicts, Level, Notebook, Recipe,
    Support, Trust,
};

use super::super::app::TRIAL_DEADLINE;
use super::super::cmdq::{self, AppCommand, WM_APP_COMMAND};
use super::super::inspect::{self, WindowEntry};
use super::super::updater;

#[path = "setup_ui.rs"]
mod setup;
#[path = "uia_ui.rs"]
mod uia_bridge;
pub use setup::{paint_tip, paint_tutorial_highlight, SetupUi, TutorialRun};

pub const CMD_RESET_ALL: usize = 4;

#[derive(Default)]
pub struct Ext {
    pub windows: Vec<WindowEntry>,
    pub selected: Option<WindowEntry>,
    pub report: Option<crate::Report>,
    pub report_lines: Vec<String>,
    /// The last Rule playground answer. Never saved.
    pub playground: Vec<String>,
    pub filter: String,
    pub recipes: Vec<(Recipe, Trust)>,
    pub recipe_sel: Option<usize>,
    /// Compatibility packs: the built-in catalog first, then valid imported files (true).
    pub packs: Vec<(crate::Pack, bool)>,
    pub pack_sel: usize,
    pub class_sel: usize,
    /// Global search: what is typed, the results shown (at most 12), the index they come from
    /// (rebuilt when the box gains focus, never per keystroke), and where to go back to.
    pub query: String,
    pub results: Vec<crate::PaletteCommand>,
    pub index: Vec<crate::PaletteCommand>,
    pub search_from: Option<Page>,
    /// The search box is being changed by the program, not typed in: ignore its change event.
    pub search_silent: bool,
    /// Sections the person opened (keyed by title).
    pub open_sections: Vec<&'static str>,
    /// A row to mark for a moment after a search result took you to it: (row index, until tick).
    pub highlight: Option<(usize, u64)>,
    pub remix_with: usize,
    pub remix_part: crate::RemixPart,
    /// The wallpaper gallery: the catalog, which one is selected, the preview bitmaps, what is
    /// remembered, and the display Apply targets (0 = all displays).
    pub walls: Vec<crate::wallpaper::Entry>,
    pub wall_sel: usize,
    pub wall_previews: Vec<(String, windows_sys::Win32::Graphics::Gdi::HBITMAP)>,
    pub wall_state: crate::wallpaper::State,
    pub wall_display: usize,
    pub wall_seen: u64,
    pub wall_loaded: bool,
    /// What the last pack install or removal did, in plain lines. Never saved.
    pub pack_report: Vec<String>,
    /// The last "Check now": the status line and what it found. Never saved.
    pub pack_check: Vec<String>,
    /// A matching window was found by the last check (so "I see ..." has something to be about).
    pub pack_found: bool,
    pub pack_marks: crate::packs::Marks,
    pub recipe_diff: Vec<String>,
    /// 0 executable, 1 executable + class, 2 package, 3 class only.
    pub rule_scope: u8,
    pub show_titles: bool,
    pub picking: bool,
    pub focus: Option<usize>,
    pub focus_sub: usize,
    pub trial_name: String,
    /// Keyboard focus on the trial bar while a trial runs: 0 Keep, 1 Revert.
    pub trial_focus: Option<u8>,
    /// A screen reader was told that focus is on the trial bar.
    pub trial_announced: bool,
    /// Whether the last layout had the trial bar, so it can be redone when that changes.
    pub trial_shown: bool,
    /// The updater phase the page was last laid out for.
    pub update_seen: u64,
    pub loaded: bool,
    /// The navigation pane holds keyboard focus (arrow keys move between destinations).
    pub nav_focus: bool,
    /// The destination under the pointer, for the hover state.
    pub nav_hover: Option<usize>,
    /// The guided "Fix this app" plan for the selected window, and its style receipt.
    pub fix: Option<crate::FixPlan>,
    pub receipt: Vec<String>,
    /// The host handles window commands a moment after Settings queues them; re-read the
    /// report once it has had time to.
    pub refresh_pending: bool,
    pub scenes: Vec<crate::Scene>,
    pub book: crate::LookBook,
    /// Selected item in the combined list (scenes first, then saved looks).
    pub look_sel: Option<usize>,
    /// Item marked with "Compare…" to diff the next selection against.
    pub look_mark: Option<usize>,
    pub look_diff: Vec<String>,
    /// The scene built from a picture, kept until replaced. Not saved anywhere.
    pub image_scene: Option<crate::Scene>,
    /// What the selected app's identity accent is, read once per selection (no disk reads
    /// while laying out rows).
    pub accent_line: String,
    pub setup: SetupUi,
    pub tutorial: Option<TutorialRun>,
    pub help_query: String,
}

impl Ext {
    /// The first-run setup flow is showing instead of a page. (Filled in with the setup flow.)
    pub fn setup_active(&self) -> bool {
        self.setup.active
    }
}

/// The title block every page starts with.
fn page_title(page: Page) -> Row {
    Row::PageTitle(page.title(), page.description().to_string())
}

/// What every page starts with: the way back (on a subpage), then the title and its line.
fn push_head(b: &mut Builder, page: Page) {
    if page.parent().is_some() {
        b.push(Row::Crumb);
    }
    b.push(page_title(page));
}

pub struct Builder {
    y: i32,
    out: Vec<Laid>,
}

impl Builder {
    fn new() -> Self {
        Self {
            y: 8,
            out: Vec::new(),
        }
    }

    fn push(&mut self, row: Row) {
        let h = row_height(&row);
        self.push_h(row, h);
    }

    fn push_h(&mut self, row: Row, h: i32) {
        self.out.push(Laid { y: self.y, h, row });
        self.y += h;
    }
}

const TILE_GAP: i32 = 12;
const TILE_LABEL_H: i32 = 40;

/// Columns, tile width, preview height and tile height of the wallpaper grid.
fn wall_geom(lw: i32) -> (i32, i32, i32, i32) {
    let inner = lw - 2 * MARGIN - 2 * PAD;
    let cols = ((inner + TILE_GAP) / (180 + TILE_GAP)).clamp(2, 4);
    let tile_w = ((inner - TILE_GAP * (cols - 1)) / cols).max(60);
    let thumb_h = tile_w * 9 / 16;
    (cols, tile_w, thumb_h, thumb_h + TILE_LABEL_H)
}

/// A tile's rectangle relative to the top of the grid row.
fn wall_rect(lw: i32, index: usize) -> (i32, i32, i32, i32) {
    let (cols, tile_w, _, tile_h) = wall_geom(lw);
    let col = index as i32 % cols;
    let row = index as i32 / cols;
    let left = MARGIN + PAD + col * (tile_w + TILE_GAP);
    let top = PAD + row * (tile_h + TILE_GAP);
    (left, top, left + tile_w, top + tile_h)
}

pub fn wall_rect_pub(lw: i32, index: usize) -> (i32, i32, i32, i32) {
    wall_rect(lw, index)
}

fn wall_height(lw: i32, count: usize) -> i32 {
    let (cols, _, _, tile_h) = wall_geom(lw);
    let rows = (count as i32 + cols - 1) / cols;
    2 * PAD + rows * tile_h + (rows - 1).max(0) * TILE_GAP + GAP
}

fn list_height(len: usize) -> i32 {
    (len.max(1) as i32) * LIST_LINE + 16 + GAP
}

// ---------------------------------------------------------------------------
// Rows per page
// ---------------------------------------------------------------------------

impl Ui {
    pub fn rows_for_page(&self) -> Vec<Laid> {
        let _measure = MeasureScope::new(self, None, self.lw(self.hwnd));
        if self.ext.setup.active {
            return self.rows_setup();
        }
        let rows = self.rows_for_current_page();
        self.with_tip(rows)
    }

    fn rows_for_current_page(&self) -> Vec<Laid> {
        match self.page {
            Page::Home => self.rows_home(),
            Page::Appearance | Page::Apps | Page::Help => self.rows_hub(self.page),
            Page::AppearanceScenes => self.rows_scenes(),
            Page::AppearanceEffects => self.rows_appearance(),
            Page::AppearanceWallpapers => self.rows_gallery(),
            Page::AppearanceSchedule => self.rows_schedule(),
            Page::AppearanceTools => self.rows_tools(),
            Page::AppsFix => self.rows_fix(),
            Page::AppsRules => self.rows_rules(),
            Page::AppsPacks => self.rows_packs(),
            Page::AppsExclusions => self.rows_exclusions(),
            Page::Safety => self.rows_safety(),
            Page::SafetyExperimental => self.rows_experimental(),
            Page::Settings => self.rows_settings(),
            Page::HelpTopics => self.rows_help(),
            Page::HelpDiagnostics => self.rows_diagnostics(),
            Page::HelpAbout => self.rows_about(),
            Page::Search => self.rows_search(),
        }
    }

    /// A category's landing page: a line of status, then a card for each subpage.
    fn rows_hub(&self, hub: Page) -> Vec<Laid> {
        let mut b = Builder::new();
        push_head(&mut b, hub);
        let status = match hub {
            Page::Appearance => Some(self.current_look_line()),
            Page::Apps => Some(format!(
                "{} per-app rule(s), {} app(s) left alone, {} compatibility pack(s) installed.",
                self.config
                    .rules
                    .iter()
                    .filter(|r| !r
                        .name
                        .as_deref()
                        .is_some_and(|n| n.starts_with(crate::packs::RULE_PREFIX)))
                    .count(),
                self.config.exclude.len() + self.config.exclude_classes.len(),
                crate::installed_packs(&self.config).len()
            )),
            _ => None,
        };
        if let Some(line) = status {
            b.push(Row::About(vec![line]));
        }
        for child in hub.children() {
            b.push(Row::Link(child));
        }
        b.out
    }

    /// Search results: a card per result, each opening its place. Nothing here changes a setting.
    fn rows_search(&self) -> Vec<Laid> {
        let mut b = Builder::new();
        // On the narrow rail the search box has no room in the pane, so it sits here.
        if self.nav_w <= crate::ui_metrics::NAV_COMPACT {
            b.push(Row::GlobalSearch);
        }
        push_head(&mut b, Page::Search);
        if self.ext.query.trim().is_empty() {
            b.push(Row::Note("Type a setting, an action, a scene or a help topic. Results open the place; nothing changes until you do it there."));
        } else if self.ext.results.is_empty() {
            b.push(Row::About(vec![
                format!("Nothing matches \"{}\".", self.ext.query.trim()),
                "Try one word, such as blur, restore, updates or wallpaper.".to_string(),
            ]));
        } else {
            for index in 0..self.ext.results.len() {
                b.push(Row::Result(index));
            }
        }
        b.out
    }

    fn rows_home(&self) -> Vec<Laid> {
        let mut b = Builder::new();
        b.push(Row::Brand);
        let paused = super::super::app::is_paused();
        let safe = super::super::app::safe_mode_text();
        let headline = if paused {
            "Customization is paused. Every window is as Windows draws it."
        } else if !self.config.enabled {
            "Customization is off."
        } else if safe.is_some() {
            "Customization is on, in safe mode."
        } else {
            "Customization is on."
        };
        let mut status = vec![headline.to_string()];
        if let Some(text) = &safe {
            status.push(text.clone());
        }
        status.push(self.experimental_summary());
        if let Some(reason) = super::super::app::reduce_reason_now() {
            status.push(format!(
                "Effects are reduced because of {reason}. Colors still apply; backdrop materials do not."
            ));
        }
        b.push(Row::About(status));
        match updater::phase() {
            crate::update::Phase::Available(c) => {
                b.push(Row::About(vec![
                    format!(
                        "Update available: {} ({})",
                        c.tag,
                        if c.preview { "preview" } else { "stable" }
                    ),
                    "Nothing is downloaded or installed until you choose.".to_string(),
                ]));
                b.push(Row::Buttons(vec![Button::GoUpdates, Button::SkipUpdate]));
            }
            crate::update::Phase::Ready { candidate, .. } => {
                b.push(Row::About(vec![
                    format!("Update ready: {} is downloaded", candidate.tag),
                    "Nothing has been installed yet.".to_string(),
                ]));
                b.push(Row::Buttons(vec![Button::GoUpdates]));
            }
            _ => {}
        }
        self.push_setup_card(&mut b);
        b.push(Row::Header("Customize"));
        b.push(Row::Buttons(vec![
            Button::GoFix,
            if paused {
                Button::ResumeAll
            } else {
                Button::PauseAll
            },
        ]));
        b.push(Row::Note(
            "Customize an app: pick a window, preview a look, keep it or undo it. Pause puts every window back at once; nothing is lost. Recovery options are under Safety.",
        ));
        b.push(Row::Header("Looks"));
        let mut looks = vec![self.current_look_line()];
        let saved: Vec<&str> = self
            .ext
            .book
            .looks
            .iter()
            .take(3)
            .map(|look| look.name.as_str())
            .collect();
        if saved.is_empty() {
            looks.push(
                "No saved looks yet. Save one under Appearance > Scenes and looks.".to_string(),
            );
        } else {
            looks.push(format!("Your saved looks: {}.", saved.join(", ")));
        }
        b.push(Row::Lines(looks));
        b.push(Row::Buttons(vec![Button::GoScenes, Button::GoHelp]));
        b.push(Row::Header("Customization"));
        b.push(Row::Toggle(Toggle::Theme));
        b.push(Row::Credit);
        b.out
    }

    /// One line for Home: which scene or look is active.
    fn current_look_line(&self) -> String {
        if let Some(id) = crate::active_scene(&self.config) {
            if let Some(scene) = crate::scene_catalog().iter().find(|s| s.id == id) {
                return format!("Scene: {}.", scene.name);
            }
        }
        if self.config.focus.enabled {
            "Look: custom, with Focus Look on.".to_string()
        } else {
            "Look: custom (not one of the scenes).".to_string()
        }
    }

    /// One line for Home: every experimental or optional feature that is on, by name.
    fn experimental_summary(&self) -> String {
        let mut on: Vec<&str> = Vec::new();
        if self.config.experimental_dark_legacy {
            on.push("dark classic dialogs (experimental)");
        }
        if self.config.legacy.emulated_x64 {
            on.push("emulated x64 coverage (experimental)");
        }
        if self.config.features.identity_accents {
            on.push("identity accents");
        }
        if self.config.features.day_night.enabled {
            on.push("Day / Night scenes");
        }
        if self.config.features.palette_hotkey {
            on.push("palette hotkey");
        }
        if on.is_empty() {
            "Experimental and optional features: none are on.".to_string()
        } else {
            format!("Experimental and optional features on: {}.", on.join(", "))
        }
    }

    fn rows_help(&self) -> Vec<Laid> {
        let mut b = Builder::new();
        push_head(&mut b, Page::HelpTopics);
        self.push_help(&mut b);
        b.out
    }

    /// General preferences: how AllBlack11 itself looks and starts, shortcuts, updates.
    fn rows_settings(&self) -> Vec<Laid> {
        let mut b = Builder::new();
        push_head(&mut b, Page::Settings);
        b.push(Row::Header("AllBlack11 itself"));
        b.push(Row::Choice(Choice::AppAppearance));
        b.push(Row::Toggle(Toggle::Autostart));
        b.push(Row::Header("Shortcuts"));
        b.push(Row::Toggle(Toggle::PaletteHotkey));
        b.push(Row::Action(Button::OpenPalette));
        self.push_updates(&mut b);
        b.out
    }

    /// Version, CPU type, verification status, credits and licenses.
    fn rows_about(&self) -> Vec<Laid> {
        let mut b = Builder::new();
        push_head(&mut b, Page::HelpAbout);
        b.push(Row::Header("About"));
        let mut about = vec![
            format!("AllBlack11 {}", env!("CARGO_PKG_VERSION")),
            self.arch_line.clone(),
            format!(
                "Windows build {} · {}",
                self.build,
                if self.build >= 22621 {
                    "Mica and backdrops supported"
                } else {
                    "Mica needs build 22621 or newer"
                }
            ),
        ];
        if let Some(advice) = self.arch.advice() {
            about.push(advice.to_string());
        }
        about.push(crate::VERIFICATION_STATUS.to_string());
        about.push(crate::credit_text());
        b.push(Row::About(about));
        b.push(Row::Credit);
        b.push(Row::Header("Licenses"));
        b.push(Row::Lines(vec![
            "AllBlack11 is free software under the MIT license (the LICENSE file in the source).".to_string(),
            "It uses the Rust standard library, serde and serde_json, and Microsoft's windows-sys bindings, all MIT or Apache-2.0.".to_string(),
            "Windows, Segoe Fluent Icons and the Fluent design guidance belong to Microsoft; no Microsoft code or font is bundled.".to_string(),
            "The wallpapers and the icon are original works made for this project.".to_string(),
        ]));
        b.out
    }

    fn rows_appearance(&self) -> Vec<Laid> {
        let mut b = Builder::new();
        push_head(&mut b, Page::AppearanceEffects);
        b.push(Row::Header("Quick looks"));
        b.push(Row::Presets);
        b.push(Row::Note(
            "A quick look runs for 15 seconds first. Keep it, or it goes back by itself.",
        ));
        let target = match self.rule.and_then(|index| self.config.rules.get(index)) {
            Some(rule) => format!("Editing the look for: {}", rule.label()),
            None => "Editing the default look for all apps without a rule.".to_string(),
        };
        b.push(Row::About(vec![
            target,
            "Pick a different target under Apps > Customize an app or Per-app rules.".to_string(),
        ]));
        b.push(Row::Header(if self.rule.is_some() {
            "Look for one app"
        } else {
            "Window look"
        }));
        if self.rule.is_some() {
            b.push(Row::Toggle(Toggle::RuleOn));
            b.push(Row::Action(Button::UseDefault));
        } else {
            b.push(Row::Toggle(Toggle::DefaultOn));
        }
        b.push(Row::Toggle(Toggle::Dark));
        b.push(Row::Choice(Choice::Backdrop));
        b.push(Row::Choice(Choice::Corners));
        for id in [ID_CAPTION, ID_BORDER, ID_TEXT, ID_TINT] {
            b.push(Row::Color(id));
        }
        b.push(Row::Note(
            "Colors take #RRGGBB, accent, none or default. They apply as soon as the value is valid.",
        ));
        b.push(Row::Slider(Slider::Opacity));
        b.push(Row::Note(
            "Material tint opacity is how strongly the tint covers the blur behind a window. It is not how see-through the whole window is: AllBlack11 does not change whole-window opacity.",
        ));
        b.push(Row::Toggle(Toggle::Blur));
        b.push(Row::Action(Button::PreferAppDefaults));
        b.push(Row::Expander("Focus Look and automatic reductions"));
        if self.section_open("Focus Look and automatic reductions") {
            b.push(Row::Header("Focus Look"));
            b.push(Row::Toggle(Toggle::FocusLook));
            b.push(Row::Toggle(Toggle::Reapply));
            b.push(Row::Header("Reduce effects automatically"));
            b.push(Row::Toggle(Toggle::ReduceBattery));
            b.push(Row::Toggle(Toggle::ReduceRemote));
            b.push(Row::Toggle(Toggle::ReduceTransparency));
        }
        b.push(Row::Expander("Title bar height and test windows"));
        if self.section_open("Title bar height and test windows") {
            b.push(Row::Header("Title bar height"));
            b.push(Row::Slider(Slider::Caption));
            b.push(Row::Action(Button::DemoMaterials));
            b.push(Row::Action(Button::DemoCompare));
        }
        b.push(Row::Expander("Taskbar and Start (shell)"));
        if self.section_open("Taskbar and Start (shell)") {
            b.push(Row::Header("Taskbar and Start (shell)"));
            b.push(Row::Toggle(Toggle::Oled));
            b.push(Row::Toggle(Toggle::Black));
            b.push(Row::Action(Button::RestartExplorer));
        }
        b.push(Row::Expander("Contrast check"));
        if self.section_open("Contrast check") {
            self.push_contrast_lab(&mut b);
        }
        b.out
    }

    /// Contrast Lab: the colors this look uses, measured, with the nearest fix for any that fail.
    fn push_contrast_lab(&self, b: &mut Builder) {
        let accent = registry::accent_bgr() & 0x00FF_FFFF;
        let mut findings =
            crate::contrast_check(&crate::contrast_style_pairs(&self.current_style(), accent));
        findings.extend(crate::contrast_check(&crate::contrast_palette_pairs(
            &crate::legacy_policy::palette(&self.config.legacy.palette),
        )));
        let failing = findings.iter().filter(|f| !f.pass).count();
        let mut lines = vec![if failing == 0 {
            format!("All {} measured pairs pass.", findings.len())
        } else {
            format!(
                "{failing} of {} measured pairs fail. Each fix keeps the color's hue and moves it the least.",
                findings.len()
            )
        }];
        lines.extend(crate::contrast_lines(&findings));
        b.push(Row::Header("Contrast check"));
        b.push(Row::Note(
            "Measures the colors this look uses, and the classic dialog palette. Hover, focus and selection colors of classic controls are drawn by Windows, so they are not measured here.",
        ));
        b.push(Row::Lines(lines));
    }

    /// The picker: button, hint, filter and window list.
    pub fn push_pick(&self, b: &mut Builder) {
        b.push(Row::Header("Pick a window"));
        b.push(Row::Buttons(vec![
            Button::PickWindow,
            Button::RefreshWindows,
        ]));
        b.push(Row::Note(
            "Press Pick, drag the crosshair onto any window, and let go. Or choose from the list.",
        ));
        b.push(Row::Filter);
        let shown = self.visible_windows().len();
        b.push_h(Row::WindowList, list_height(shown.min(40)));
        b.push(Row::Toggle(Toggle::ShowTitles));
    }

    /// What can change for the picked window, the safe actions and the style receipt.
    pub fn push_fix_card(&self, b: &mut Builder) {
        if let (Some(plan), Some(_)) = (&self.ext.fix, &self.ext.selected) {
            // The guided path: what this is, what can change, then the safe actions.
            b.push(Row::Header("This app"));
            b.push(Row::Lines(plan.identity.clone()));
            b.push(Row::Header("What can change here"));
            b.push(Row::Lines(
                plan.surfaces
                    .iter()
                    .map(|row| {
                        format!(
                            "{}: {}. {}",
                            row.surface.label(),
                            row.level.label(),
                            row.why
                        )
                    })
                    .collect(),
            ));
            b.push(Row::Note(
                "Each line is about one surface. Choosing an app never means every control inside it can be themed.",
            ));
            if !plan.actions.is_empty() {
                b.push(Row::Header("Safe actions"));
                b.push(Row::Note(
                    "Preview shows the look from Appearance > Effects and colors on this one window for 20 seconds, then puts it back. Nothing is saved until you press Save as rule.",
                ));
                b.push(Row::Choice(Choice::RuleScope));
                let buttons = plan
                    .actions
                    .iter()
                    .map(|action| match action {
                        crate::FixAction::Preview => Button::TryLook,
                        crate::FixAction::SaveRule => Button::CreateRule,
                        crate::FixAction::Exclude => Button::ExcludeApp,
                        crate::FixAction::Restore => Button::RestoreWindow,
                    })
                    .collect();
                b.push(Row::Buttons(buttons));
            }
            if !plan.limits.is_empty() {
                b.push(Row::Header("Limits"));
                b.push(Row::Lines(plan.limits.clone()));
            }
            if self.config.features.identity_accents {
                b.push(Row::Header("Identity accent"));
                b.push(Row::Lines(vec![self.ext.accent_line.clone()]));
                b.push(Row::Buttons(vec![Button::NextAccent, Button::ClearAccent]));
            }
            b.push(Row::Header("Style receipt"));
            b.push(Row::Lines(self.ext.receipt.clone()));
            b.push(Row::Buttons(vec![Button::CopyReport]));
        }
    }

    fn rows_fix(&self) -> Vec<Laid> {
        let mut b = Builder::new();
        push_head(&mut b, Page::AppsFix);
        self.push_pick(&mut b);
        self.push_fix_card(&mut b);
        b.push(Row::Expander("Full inspector report"));
        if self.section_open("Full inspector report") {
            let lines = if self.ext.report_lines.is_empty() {
                vec!["Pick a window to see what AllBlack11 does with it, and why.".to_string()]
            } else {
                self.ext.report_lines.clone()
            };
            b.push(Row::Lines(lines));
        }
        if self.ext.selected.is_some() {
            b.push(Row::Header("Your note for this app on this Windows build"));
            b.push(Row::Buttons(vec![
                Button::MarkWorks,
                Button::MarkPartial,
                Button::MarkFails,
                Button::MarkReset,
            ]));
        }
        b.out
    }

    fn rows_rules(&self) -> Vec<Laid> {
        let mut b = Builder::new();
        push_head(&mut b, Page::AppsRules);
        b.push(Row::Header("Per-app rules"));
        b.push(Row::Note(
            "Most specific match wins, then the higher priority, then the earlier rule. Pick a rule to edit its look under Appearance > Effects and colors.",
        ));
        b.push_h(Row::RuleList, list_height(self.config.rules.len()));
        if let Some(rule) = self.rule.and_then(|index| self.config.rules.get(index)) {
            let mut detail = vec![
                format!("Selected: {}", rule.label()),
                format!("Priority {}", rule.priority),
            ];
            if !rule.process.is_empty() {
                detail.push(format!("Executable: {}", rule.process));
            }
            if let Some(class) = &rule.class {
                detail.push(format!("Window class: {class}"));
            }
            if let Some(package) = &rule.package {
                detail.push(format!("Package: {package}"));
            }
            if let Some(path) = &rule.path {
                detail.push(format!("Path: {path}"));
            }
            if let Some(title) = &rule.title {
                detail.push(format!(
                    "Title contains \"{title}\" (titles change with the document and the display language)"
                ));
            }
            b.push(Row::Lines(detail));
            b.push(Row::Buttons(vec![
                Button::RaisePriority,
                Button::LowerPriority,
                Button::RemoveRule,
            ]));
        }
        b.push(Row::AddField(Button::AddRule));
        let conflicts = find_conflicts(&self.config.rules);
        let conflict_lines: Vec<String> = if conflicts.is_empty() {
            vec!["No rule conflicts.".to_string()]
        } else {
            conflicts
                .iter()
                .map(|conflict| describe_conflict(&self.config.rules, conflict))
                .collect()
        };
        b.push(Row::Lines(conflict_lines));
        b.push(Row::Header("Test without applying"));
        b.push(Row::Note(
            "Type an app name in the box above (notepad.exe, or notepad.exe class=Notepad), then press Test. It shows which rule would win and why. Nothing is applied or saved.",
        ));
        b.push(Row::Buttons(vec![Button::TestRule, Button::TestPicked]));
        if self.ext.playground.is_empty() {
            b.push(Row::Note("No test yet. The answer appears here."));
        } else {
            b.push(Row::Lines(self.ext.playground.clone()));
        }
        b.out
    }

    fn rows_exclusions(&self) -> Vec<Laid> {
        let mut b = Builder::new();
        push_head(&mut b, Page::AppsExclusions);
        b.push(Row::Header("Never theme these apps"));
        b.push(Row::Note(
            "Games and launchers belong here. Use the exe name, such as steam.exe. To leave a window class alone in every app instead, type class=Name.",
        ));
        b.push_h(Row::ExcludeList, list_height(self.config.exclude.len()));
        b.push(Row::AddField(Button::AddExclude));
        b.push(Row::Action(Button::RemoveExclude));
        self.push_exclusions(&mut b);
        b.out
    }

    fn rows_packs(&self) -> Vec<Laid> {
        let mut b = Builder::new();
        push_head(&mut b, Page::AppsPacks);
        self.push_packs(&mut b);
        self.push_notebook(&mut b);
        b.out
    }

    fn rows_experimental(&self) -> Vec<Laid> {
        let mut b = Builder::new();
        push_head(&mut b, Page::SafetyExperimental);
        self.push_legacy(&mut b);
        b.out
    }

    fn rows_gallery(&self) -> Vec<Laid> {
        use super::super::gallery;
        let mut b = Builder::new();
        push_head(&mut b, Page::AppearanceWallpapers);
        b.push(Row::Note(
            "Original wallpapers drawn by a script in this project: a true-black foundation, no large bright areas, and only the silver and blue of the AllBlack11 brand. Nothing is downloaded until you press Download, and nothing is sent anywhere.",
        ));
        if self.ext.walls.is_empty() {
            b.push(Row::Note("No wallpapers are available in this build."));
            return b.out;
        }
        let lw = self.lw(self.hwnd);
        b.push_h(Row::WallGrid, wall_height(lw, self.ext.walls.len()));
        let Some(entry) = self.ext.walls.get(self.ext.wall_sel) else {
            return b.out;
        };
        let have = gallery::downloaded(entry);
        let job = gallery::job();
        let mut lines = vec![
            format!("{}: {}", entry.name, entry.description),
            if have {
                format!(
                    "On this PC, {}, {}x{}. Checked against the checksum built into AllBlack11.",
                    crate::wallpaper::size_text(entry.bytes),
                    entry.width,
                    entry.height
                )
            } else {
                format!(
                    "Not on this PC yet. Download is {} ({}x{}), from the {} release on GitHub, and is checked against a checksum built into AllBlack11.",
                    crate::wallpaper::size_text(entry.bytes),
                    entry.width,
                    entry.height,
                    crate::wallpaper::release_tag()
                )
            },
        ];
        match &job {
            gallery::Job::Downloading { id, done, total } if *id == entry.id => {
                lines.push(format!(
                    "Downloading: {} of {}.",
                    crate::update::format_bytes(*done),
                    crate::update::format_bytes(*total)
                ))
            }
            gallery::Job::Applying { .. } => lines.push("Applying through Windows…".to_string()),
            gallery::Job::PuttingBack => {
                lines.push("Putting the earlier wallpaper back…".to_string())
            }
            _ => {}
        }
        if let Some(applied) = &self.ext.wall_state.applied {
            if applied.id == entry.id {
                lines.push("This is the wallpaper you applied last.".to_string());
            }
        }
        b.push(Row::Lines(lines));
        b.push(Row::Choice(Choice::WallDisplay));
        b.push(Row::Choice(Choice::WallFit));
        let mut buttons = match &job {
            gallery::Job::Downloading { id, .. } if *id == entry.id => vec![Button::WallCancel],
            _ if have => vec![
                Button::WallApply,
                Button::WallPreview,
                Button::WallOpenFolder,
            ],
            _ => vec![
                Button::WallDownload,
                Button::WallPreview,
                Button::WallOpenFolder,
            ],
        };
        if self.ext.wall_state.can_put_back() {
            buttons.push(Button::WallPutBack);
        }
        b.push(Row::Buttons(buttons));
        b.push(Row::Note(
            "Preview shows the picture on this display the way Windows will place it; Enter applies it and Esc closes. Apply keeps what was there before, so Put back previous can undo it. Wallpapers are not part of Look history.",
        ));
        b.out
    }

    fn rows_scenes(&self) -> Vec<Laid> {
        let mut b = Builder::new();
        push_head(&mut b, Page::AppearanceScenes);
        b.push(Row::Note(
            "A scene sets the default look and Focus Look. It never changes your per-app rules, exclusions, the shell or the legacy layer, so an app with its own rule keeps its own colors. Scenes change window frames only.",
        ));
        let current = match crate::active_scene(&self.config) {
            Some(id) => self
                .ext
                .scenes
                .iter()
                .find(|scene| scene.id == id)
                .map(|scene| format!("Right now: the {} scene.", scene.name))
                .unwrap_or_else(|| "Right now: a built-in scene.".to_string()),
            None => match self
                .ext
                .book
                .looks
                .iter()
                .find(|look| crate::preview_scene(&self.config, *look).is_empty())
            {
                Some(look) => format!("Right now: your look “{}”.", look.name),
                None => "Right now: a custom look (not one of the scenes below).".to_string(),
            },
        };
        b.push(Row::About(vec![current]));
        b.push_h(Row::LookList, list_height(self.look_count().min(40)));
        b.push(Row::Lines(self.look_details()));
        b.push(Row::Buttons(vec![
            Button::TryScene,
            Button::MarkCompare,
            Button::RenameLook,
            Button::DeleteLook,
        ]));
        b.push(Row::Note(
            "Try runs the look for 15 seconds. Nothing is saved until you press Keep. Type a name to save, rename or label a look.",
        ));
        b.push(Row::AddField(Button::SaveLook));
        b.push(Row::Action(Button::DemoCompare));
        b.out
    }

    /// Day and night scenes, and a border color for each app.
    fn rows_schedule(&self) -> Vec<Laid> {
        let mut b = Builder::new();
        push_head(&mut b, Page::AppearanceSchedule);
        b.push(Row::Header("Day / Night (optional)"));
        b.push(Row::Toggle(Toggle::DayNight));
        if self.config.features.day_night.enabled {
            b.push(Row::Choice(Choice::DayScene));
            b.push(Row::Choice(Choice::DayStarts));
            b.push(Row::Choice(Choice::NightScene));
            b.push(Row::Choice(Choice::NightStarts));
            b.push(Row::Note(
                "Picking a scene by hand stays in place until the next boundary, then the schedule takes over again.",
            ));
        }
        b.push(Row::Header("Identity accents (optional)"));
        b.push(Row::Toggle(Toggle::IdentityAccents));
        b.push(Row::Note(
            "Pick an app under Apps > Customize an app to change its accent. Colors are chosen once per app, saved on this PC (app names only, never window titles), and stay the same.",
        ));
        b.out
    }

    /// Palette from a picture, recipes and the remixer.
    fn rows_tools(&self) -> Vec<Laid> {
        let mut b = Builder::new();
        push_head(&mut b, Page::AppearanceTools);
        b.push(Row::Header("Palette from a picture"));
        b.push(Row::Note(
            "Pick a picture on this PC. AllBlack11 reads up to 64 x 64 sample points with Windows' own picture support, builds a frame look that keeps text readable, and adds it to the list above as a scene. The picture is not stored, copied or uploaded.",
        ));
        b.push(Row::Action(Button::PaletteFromImage));
        b.push(Row::Header("Recipes"));
        b.push(Row::Note(
            "A recipe is a small data file describing a look. It cannot run anything or change the shell. Built-in recipes are trusted; imported ones are checked and shown to you before they are tried.",
        ));
        b.push_h(Row::RecipeList, list_height(self.ext.recipes.len()));
        if let Some(index) = self.ext.recipe_sel {
            if let Some((recipe, trust)) = self.ext.recipes.get(index) {
                let mut lines = vec![
                    format!(
                        "{} ({})",
                        recipe.name,
                        if *trust == Trust::Bundled {
                            "built in"
                        } else {
                            "imported, review before use"
                        }
                    ),
                    recipe.description.clone(),
                    format!("Applies to: {}", recipe.target_label()),
                ];
                lines.retain(|line| !line.is_empty());
                if self.ext.recipe_diff.is_empty() {
                    lines.push("It would change nothing: your look already matches.".to_string());
                } else {
                    lines.push("It would change:".to_string());
                    lines.extend(self.ext.recipe_diff.iter().map(|line| format!("  {line}")));
                }
                for note in &recipe.notes {
                    lines.push(format!("Note: {note}"));
                }
                if let Some(min) = recipe.min_build {
                    if self.build < min {
                        lines.push(format!(
                            "This look is meant for Windows build {min} or newer; you have {}.",
                            self.build
                        ));
                    }
                }
                b.push(Row::Lines(lines));
            }
        }
        b.push(Row::Buttons(vec![
            Button::TryRecipe,
            Button::ImportRecipe,
            Button::ExportRecipe,
        ]));
        b.push(Row::Header("Remix two recipes"));
        b.push(Row::Note(
            "Take one group of fields (frame colors, material, or dark title bar and corners) from another recipe and keep the rest of the recipe picked above. The mix is checked like an imported file, and refused if the title text would be hard to read. Try runs it for 15 seconds; nothing is saved until you press Keep.",
        ));
        if self.remix_donor().is_some() && self.ext.recipe_sel.is_some() {
            b.push(Row::Choice(Choice::RemixWith));
            b.push(Row::Choice(Choice::RemixPart));
            let lines = match self.current_remix() {
                Some(Ok(recipe)) => {
                    let diff = crate::preview_recipe(&self.config, &recipe);
                    let mut lines = vec![format!("Result: {}", recipe.name)];
                    if diff.is_empty() {
                        lines.push(
                            "It would change nothing: your look already matches.".to_string(),
                        );
                    } else {
                        lines.push("It would change:".to_string());
                        lines.extend(
                            diff.iter()
                                .map(|c| format!("  {}: {} → {}", c.field, c.from, c.to)),
                        );
                    }
                    lines
                }
                Some(Err(reason)) => vec![reason],
                None => vec!["Pick a recipe above first.".to_string()],
            };
            b.push(Row::Lines(lines));
            b.push(Row::Buttons(vec![Button::TryRemix, Button::SaveRemix]));
        } else {
            b.push(Row::Note(
                "Pick a recipe in the list above to remix it with another.",
            ));
        }
        b.out
    }

    fn rows_diagnostics(&self) -> Vec<Laid> {
        let mut b = Builder::new();
        push_head(&mut b, Page::HelpDiagnostics);
        b.push(Row::Header("This PC"));
        let mut pc = vec![
            self.arch_line.clone(),
            format!("Windows build {}", self.build),
            if super::super::dwm::high_contrast_enabled() {
                "High contrast is on: AllBlack11 stays out of the way.".to_string()
            } else {
                "High contrast is off.".to_string()
            },
        ];
        match super::super::app::reduce_reason_now() {
            Some(reason) => pc.push(format!("Effects are reduced: {reason}.")),
            None => pc.push("Effects are at full strength.".to_string()),
        }
        b.push(Row::About(pc));
        b.push(Row::Expander("What this Windows build supports"));
        if self.section_open("What this Windows build supports") {
            let support: Vec<String> = attribute_support(self.build)
                .iter()
                .map(|item| format!("{} — {}", item.name, item.support.label()))
                .collect();
            b.push(Row::Lines(support));
            b.push(Row::Note(
                "Supported means Windows accepts the call on this build. It does not mean every app shows the result: an app's own opaque content can hide a backdrop, and apps with their own title bar ignore caption colors.",
            ));
        }
        b.push(Row::Header("Tools"));
        b.push(Row::Toggle(Toggle::Logging));
        b.push(Row::Action(Button::Diagnostics));
        b.push(Row::Action(Button::OpenFolder));
        b.push(Row::Action(Button::OpenLog));
        b.out
    }
}

impl Ui {
    /// Compatibility notebook summary (sits with the rules, next to the
    /// per-app notes it summarizes).
    fn push_notebook(&self, b: &mut Builder) {
        b.push(Row::Header("Compatibility notebook"));
        let book = super::super::cfgio::load_notebook();
        let mut counts = [0usize; 5];
        for entry in &book.entries {
            counts[match entry.status {
                crate::CompatStatus::Verified => 0,
                crate::CompatStatus::Partial => 1,
                crate::CompatStatus::Experimental => 2,
                crate::CompatStatus::Unsupported => 3,
                crate::CompatStatus::Untested => 4,
            }] += 1;
        }
        b.push(Row::Lines(vec![
            format!(
                "{} notes on this PC: {} work, {} partly, {} experimental, {} do not work.",
                book.entries.len(),
                counts[0],
                counts[1],
                counts[2],
                counts[3]
            ),
            "Notes stay on this PC (compat.json). Nothing is sent anywhere. Add one from Customize an app.".to_string(),
        ]));
        b.push(Row::Buttons(vec![Button::ClearNotebook]));
    }

    fn push_legacy(&self, b: &mut Builder) {
        if super::super::app::safe_mode_text().is_some() {
            b.push(Row::About(vec![
                "Safe mode is on for this run, so the switches below are not in effect. Leave safe mode on the Safety page.".to_string(),
            ]));
        }
        b.push(Row::Header("Classic dialogs (experimental)"));
        b.push(Row::Toggle(Toggle::Experimental));
        b.push(Row::Choice(Choice::Palette));
        b.push(Row::Toggle(Toggle::EmulatedX64));
        let mut state = vec![if self.config.experimental_dark_legacy {
            "On. Classic dialogs in eligible, non-elevated apps are recolored from inside the app."
                .to_string()
        } else {
            "Off. Nothing is loaded into any other process.".to_string()
        }];
        if self.arch == ArchStatus::NativeArm64 {
            state.push(if self.config.legacy.emulated_x64 {
                if super::super::legacy::helper_installed() {
                    "Emulated x64 apps: on (experimental). The x64 helper is installed.".to_string()
                } else {
                    "Emulated x64 apps: on, but the x64 helper is not installed in this build, so they are skipped.".to_string()
                }
            } else {
                "Emulated x64 apps are skipped. Native ARM64 apps are covered.".to_string()
            });
        }
        b.push(Row::About(state));
        b.push(Row::Header("What each control gets"));
        b.push(Row::Note(
            "Only classic dialogs are ever touched, never Explorer, Control Panel, or windows that embed a web or Explorer view. Where a control cannot be darkened safely it keeps its normal look.",
        ));
        b.push(Row::Lines(capability_lines()));
        b.push(Row::Header("Test it without a real app"));
        b.push(Row::Action(Button::OpenHarness));
    }

    fn rows_safety(&self) -> Vec<Laid> {
        let mut b = Builder::new();
        push_head(&mut b, Page::Safety);
        if let Some(text) = super::super::app::safe_mode_text() {
            b.push(Row::Header("Safe mode"));
            b.push(Row::About(vec![
                text,
                "Experimental features are off for this run. Your saved settings were not changed."
                    .to_string(),
            ]));
            b.push(Row::Action(Button::LeaveSafe));
        }
        b.push(Row::Header("Emergency"));
        if super::super::app::is_paused() {
            b.push(Row::About(vec![
                "AllBlack11 is paused. Every window is back as Windows draws it.".to_string(),
            ]));
            b.push(Row::Action(Button::ResumeAll));
        } else {
            b.push(Row::Action(Button::PauseAll));
        }
        b.push(Row::Action(Button::ResetAllWindows));
        b.push(Row::Action(Button::UndoShell));
        b.push(Row::Action(Button::StopExperimental));
        b.push(Row::Header("Look history"));
        b.push(Row::Note(
            "Changes to your looks this session, newest first. Undo goes back one step at a time. This is separate from Reset all windows, which puts windows back as Windows draws them.",
        ));
        let now = unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount64() };
        let history: Vec<String> = crate::with_history(|history| {
            history
                .entries()
                .iter()
                .rev()
                .take(10)
                .map(|entry| format!("{} · {}", crate::history_ago(now, entry.at_ms), entry.name))
                .collect()
        });
        if history.is_empty() {
            b.push(Row::Note(
                "Nothing yet. Changes you make in this session are listed here.",
            ));
        } else {
            b.push(Row::Lines(history));
        }
        b.push(Row::Action(Button::UndoLook));
        b.push(Row::Header("Config recovery"));
        b.push(Row::Lines(vec![
            "AllBlack11 keeps the last config that loaded and applied without errors (config.lkg.json). If config.json is ever broken, that copy runs instead and your file is left alone.".to_string(),
            if super::super::cfgio::load_lkg().is_some() {
                "A last known good config exists.".to_string()
            } else {
                "No last known good copy yet. It is saved after the first successful load.".to_string()
            },
        ]));
        b.push(Row::Buttons(vec![Button::RestoreLkg]));
        b.push(Row::Header("Experimental features"));
        b.push(Row::Note(
            "Each one is a separate switch and is off until you turn it on. Turning one off stops its work straight away.",
        ));
        b.push(Row::About(self.experimental_lines()));
        b.push(Row::Header("x64 helper (ARM64 only)"));
        b.push(Row::About(vec![super::super::legacy::helper_status()]));
        b.push(Row::Note(
            "Turn off emulated x64 coverage under Rules and compatibility, or press Turn off experimental features above. Either one asks the helper to stop and ends it if it does not.",
        ));
        b.push(Row::Link(Page::SafetyExperimental));
        b.out
    }

    fn experimental_lines(&self) -> Vec<String> {
        let on = |flag: bool| if flag { "on" } else { "off" };
        vec![
            format!(
                "Dark classic dialogs: {} (Rules and compatibility)",
                on(self.config.experimental_dark_legacy)
            ),
            format!(
                "Emulated x64 coverage: {} (Rules and compatibility)",
                on(self.config.legacy.emulated_x64)
            ),
            format!("Focus Look: {} (Appearance)", on(self.config.focus.enabled)),
        ]
    }
}

fn capability_lines() -> Vec<String> {
    let families: [(&str, u32, &str); 12] = [
        ("Static", 0, "Labels"),
        ("Edit", 0, "Edit boxes"),
        ("Button", 0, "Push buttons"),
        ("Button", 3, "Check boxes, radio buttons, group boxes"),
        ("Button", 0xB, "Owner-drawn buttons"),
        ("ComboBox", 0, "Combo boxes"),
        ("ListBox", 0, "List boxes"),
        ("SysListView32", 0, "List views"),
        ("SysTreeView32", 0, "Tree views"),
        ("SysTabControl32", 0, "Tab strips"),
        ("RichEdit20W", 0, "Rich edit"),
        ("DirectUIHWND", 0, "Explorer and web views"),
    ];
    families
        .iter()
        .map(|(class, style, label)| {
            let cap = capability(class, *style);
            let how = match cap.level {
                Level::Supported => "supported",
                Level::BestEffort => "best effort",
                Level::Partial => "partly (stays light)",
                Level::Unsupported => "left alone",
                Level::Untouched => "already readable",
            };
            format!("{label}: {how}. {}", cap.note)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Pinned chrome: page tabs and the trial bar
// ---------------------------------------------------------------------------

pub fn trial_pills(lw: i32, status_top: i32) -> ((i32, i32, i32, i32), (i32, i32, i32, i32)) {
    let top = status_top + 3;
    let bottom = status_top + STATUS_H - 3;
    (
        (lw - MARGIN - 190, top, lw - MARGIN - 104, bottom),
        (lw - MARGIN - 96, top, lw - MARGIN, bottom),
    )
}

/// `color:<name>` anchors, for the four color boxes.
fn color_anchor(id: u16) -> &'static str {
    match id {
        ID_CAPTION => "caption",
        ID_BORDER => "border",
        ID_TEXT => "text",
        _ => "tint",
    }
}

pub const NAV_ITEM_H: i32 = 40;
/// The search box sits under the brand line; the categories start below it.
const NAV_SEARCH_TOP: i32 = 52;
const NAV_SEARCH_H: i32 = 32;
const NAV_TOP: i32 = 100;

/// The global search box (expanded pane) or its magnifier button (narrow rail), in client logical
/// px.
pub fn nav_search_rect(ui: &Ui) -> (i32, i32, i32, i32) {
    if ui.nav_w > crate::ui_metrics::NAV_COMPACT {
        (
            12,
            NAV_SEARCH_TOP,
            ui.nav_w - 12,
            NAV_SEARCH_TOP + NAV_SEARCH_H,
        )
    } else {
        (
            4,
            NAV_SEARCH_TOP,
            ui.nav_w - 4,
            NAV_SEARCH_TOP + NAV_ITEM_H - 4,
        )
    }
}

/// The small clear (x) button at the right of a non-empty search box.
pub fn nav_search_clear_rect(ui: &Ui) -> (i32, i32, i32, i32) {
    let (_, t, r, b) = nav_search_rect(ui);
    (r - 28, t + 2, r - 2, b - 2)
}

/// Rectangle (l, t, r, b) of navigation item `index` (a category), in client logical px.
pub fn nav_item_rect(ui: &Ui, index: usize) -> (i32, i32, i32, i32) {
    let top = NAV_TOP + index as i32 * NAV_ITEM_H;
    (4, top, ui.nav_w - 4, top + NAV_ITEM_H - 4)
}

/// Which category is under a point (client logical px), if any.
pub fn nav_hit(ui: &Ui, lx: i32, ly: i32) -> Option<crate::Category> {
    if ui.nav_w == 0 || lx < 0 || lx >= ui.nav_w {
        return None;
    }
    crate::Category::ALL
        .iter()
        .enumerate()
        .find_map(|(index, category)| {
            let (l, t, r, b) = nav_item_rect(ui, index);
            (lx >= l && lx < r && ly >= t && ly < b).then_some(*category)
        })
}

/// The frame around the native search box (or the magnifier button on the narrow rail), with a
/// magnifier and, once something is typed, a clear button.
fn paint_nav_search(c: &Canvas, ui: &Ui, expanded: bool) {
    let theme = th();
    let (l, t, r, b) = nav_search_rect(ui);
    let active = !ui.ext.query.is_empty() || ui.page == Page::Search;
    c.round(
        l,
        t,
        r,
        b,
        crate::ui_metrics::CONTROL_RADIUS,
        Some(theme.edit_bg),
        Some(if active {
            ui.accent
        } else {
            theme.control_edge
        }),
    );
    if ui.fonts.icon.is_null() {
        return;
    }
    let glyph = '\u{E721}'.to_string();
    if expanded {
        c.text(
            l + 6,
            t,
            l + 30,
            b,
            &glyph,
            theme.muted,
            ui.fonts.icon,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        if !ui.ext.query.is_empty() {
            let (cl, ct, cr, cb) = nav_search_clear_rect(ui);
            c.text(
                cl,
                ct,
                cr,
                cb,
                &'\u{E711}'.to_string(),
                theme.text,
                ui.fonts.icon,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
        }
    } else {
        c.text(
            l,
            t,
            r,
            b,
            &glyph,
            theme.text,
            ui.fonts.icon,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
    }
}

/// The navigation pane: the search box, then the six categories, the selected one marked by a
/// filled row *and* an accent bar *and* a bold label (never color alone), and a visible focus
/// ring when it holds keyboard focus.
pub fn paint_nav(c: &Canvas, ui: &Ui) {
    let fonts = &ui.fonts;
    let theme = th();
    let height = ui.client_lh;
    c.fill(0, 0, ui.nav_w, height, theme.nav);
    c.fill(ui.nav_w - 1, 0, ui.nav_w, height, theme.edge);
    let expanded = ui.nav_w > crate::ui_metrics::NAV_COMPACT;
    // The brand mark leads the pane: beside the name when it is expanded, alone on the rail.
    // Under a contrast theme there is no mark and the name stands alone.
    let mark = uistyle::brand_mark(c.px(24));
    if let Some(icon) = mark {
        c.icon(icon, if expanded { 16 } else { 12 }, 16, 24);
    }
    if expanded {
        c.text(
            if mark.is_some() { 48 } else { 16 },
            16,
            ui.nav_w - 8,
            44,
            "AllBlack11",
            theme.text,
            fonts.head,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
    }
    paint_nav_search(c, ui, expanded);
    for (index, category) in crate::Category::ALL.iter().enumerate() {
        let (l, t, r, b) = nav_item_rect(ui, index);
        let selected = *category == ui.page.category();
        if selected {
            c.round(
                l,
                t,
                r,
                b,
                crate::ui_metrics::CONTROL_RADIUS,
                Some(theme.card_hi),
                None,
            );
            c.round(
                l,
                (t + b) / 2 - 8,
                l + 3,
                (t + b) / 2 + 8,
                2,
                Some(ui.accent),
                None,
            );
        } else if ui.ext.nav_hover == Some(index) {
            c.round(
                l,
                t,
                r,
                b,
                crate::ui_metrics::CONTROL_RADIUS,
                Some(crate::blend_colors(theme.nav, theme.card_hi, 0.6)),
                None,
            );
        }
        let icon_box = (l + 8, t, l + 40, b);
        if fonts.icon.is_null() {
            // No icon font installed: text only, never a box of missing-glyph squares. The
            // compact rail then shows an initial so each item stays distinguishable.
            if !expanded {
                let initial: String = category.title().chars().take(1).collect();
                c.text(
                    l,
                    t,
                    r,
                    b,
                    &initial,
                    theme.text,
                    fonts.head,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
            }
        } else {
            let glyph = category.glyph().to_string();
            let (gl, gr) = if expanded {
                (icon_box.0, icon_box.2)
            } else {
                (l, r)
            };
            c.text(
                gl,
                t,
                gr,
                b,
                &glyph,
                theme.text,
                fonts.icon,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
        }
        if expanded {
            c.text(
                l + if fonts.icon.is_null() { 12 } else { 44 },
                t,
                r - 4,
                b,
                category.title(),
                theme.text,
                if selected { fonts.head } else { fonts.body },
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
            );
        }
        if ui.ext.nav_focus && selected {
            c.round(
                l - 1,
                t - 1,
                r + 1,
                b + 1,
                crate::ui_metrics::CONTROL_RADIUS,
                None,
                Some(theme.focus),
            );
            c.round(
                l,
                t,
                r,
                b,
                crate::ui_metrics::CONTROL_RADIUS,
                None,
                Some(theme.focus),
            );
        }
    }
}

pub fn paint_trial_bar(c: &Canvas, ui: &Ui, lw: i32) {
    let top = ui.status_top();
    c.text(
        MARGIN,
        top,
        lw - MARGIN - 200,
        top + STATUS_H,
        &ui.trial_message(),
        th().text,
        ui.fonts.small,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
    );
    let (keep, revert) = trial_pills(lw, top);
    let focus = ui.trial_focus();
    controls::draw_button(
        c,
        ui,
        keep,
        "Keep",
        controls::Emphasis::Primary,
        controls::State {
            focused: focus == Some(0),
            ..controls::State::default()
        },
    );
    controls::draw_button(
        c,
        ui,
        revert,
        "Revert",
        controls::Emphasis::Standard,
        controls::State {
            focused: focus == Some(1),
            ..controls::State::default()
        },
    );
}

impl Ui {
    pub fn trial_active(&self) -> bool {
        TRIAL_DEADLINE.load(Ordering::Relaxed) != 0
    }

    /// What the trial bar says, for painting and for screen readers.
    pub fn trial_message(&self) -> String {
        let now = unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount64() };
        let deadline = TRIAL_DEADLINE.load(Ordering::Relaxed);
        let seconds = deadline.saturating_sub(now).div_ceil(1000);
        format!(
            "Trying “{}”. It reverts in {seconds} s unless you keep it.",
            self.ext.trial_name
        )
    }

    /// The trial bar button with keyboard focus (0 Keep, 1 Revert), only while a trial runs.
    pub fn trial_focus(&self) -> Option<u8> {
        self.trial_active()
            .then_some(self.ext.trial_focus)
            .flatten()
    }

    /// Clicks on the page tabs and the Keep/Revert buttons. True when the click was ours.
    pub fn chrome_click(&mut self, hwnd: HWND, lparam: LPARAM) -> bool {
        let client_x = (lparam_x(lparam) as f32 / self.k) as i32;
        let ly = (lparam_y(lparam) as f32 / self.k) as i32;
        // Navigation pane.
        if self.nav_w > 0 {
            let inside = |r: (i32, i32, i32, i32)| {
                client_x >= r.0 && client_x < r.2 && ly >= r.1 && ly < r.3
            };
            if !self.ext.query.is_empty()
                && self.nav_w > crate::ui_metrics::NAV_COMPACT
                && inside(nav_search_clear_rect(self))
            {
                self.clear_search(hwnd);
                return true;
            }
            if inside(nav_search_rect(self)) {
                self.focus_search(hwnd);
                return true;
            }
        }
        if let Some(category) = nav_hit(self, client_x, ly) {
            self.ext.nav_focus = true;
            self.set_page(hwnd, category.landing());
            return true;
        }
        if client_x < self.nav_w {
            return true; // empty pane area: swallow the click
        }
        let lx = client_x - self.content_x;
        let lw = self.lw(hwnd);
        let status_top = self.status_top();
        if self.trial_active() && ly >= status_top && ly < status_top + STATUS_H {
            let (keep, revert) = trial_pills(lw, status_top);
            let inside = |r: (i32, i32, i32, i32)| lx >= r.0 && lx < r.2 && ly >= r.1 && ly < r.3;
            if inside(keep) {
                self.trial_keep();
            } else if inside(revert) {
                self.trial_revert();
            }
            return true;
        }
        false
    }

    pub fn set_page(&mut self, hwnd: HWND, page: Page) {
        if self.page == page {
            return;
        }
        self.page = page;
        self.scroll = 0;
        self.ext.focus = None;
        self.ext.highlight = None;
        if page != Page::Search && !self.ext.query.is_empty() {
            self.set_search_text("");
        }
        match page {
            Page::AppsFix => self.refresh_windows(),
            Page::AppearanceWallpapers => self.open_gallery(),
            Page::AppearanceScenes => {
                self.refresh_recipes();
                self.refresh_looks();
            }
            _ => {}
        }
        self.relayout(hwnd);
        // A screen reader is told the content changed, and where focus now is.
        unsafe { uia::raise_layout_changed(hwnd) };
        self.uia_announce_focus(hwnd);
    }

    fn post_command(&self) {
        unsafe { PostMessageW(self.owner, WM_APP_COMMAND, 0, 0) };
    }

    pub fn begin_trial(&mut self, name: &str, next: Config) {
        cmdq::push(AppCommand::TrialStart(Box::new(next.clone())));
        self.post_command();
        self.config = next;
        self.ext.trial_name = name.to_string();
        // Someone using the keyboard lands on Keep, so the choice is one key press away.
        if self.ext.focus.is_some() || self.ext.nav_focus {
            self.ext.trial_focus = Some(0);
            self.ext.focus = None;
            self.ext.nav_focus = false;
        }
        self.ext.trial_announced = false;
        self.status = "Trying a new look. Nothing is saved until you press Keep.".into();
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::SetTimer(self.hwnd, TIMER_UI, 250, None)
        };
    }

    /// Keep is the only moment a trial reaches the disk. The file is written first; only if
    /// that succeeds is the host told the trial is kept. If the write fails the trial is
    /// reverted, so what is running never differs from what is saved.
    pub fn trial_keep(&mut self) {
        match cfgio::store(&self.config) {
            Ok(()) => {
                cmdq::push(AppCommand::TrialKeep);
                self.post_command();
                // The host ends the trial when it handles the command above. Mark it ended
                // here too, so later clicks are not blocked. Posting "applied" makes the host
                // read the file back, which also applies any shell part the trial skipped.
                TRIAL_DEADLINE.store(0, Ordering::Relaxed);
                self.ext.trial_focus = None;
                self.status = "Kept.".into();
                unsafe { PostMessageW(self.owner, WM_SETTINGS_APPLIED, 0, 0) };
            }
            Err(err) => {
                cmdq::push(AppCommand::TrialRevert);
                self.post_command();
                self.status = format!("Could not save, so the trial was reverted: {err}");
            }
        }
    }

    pub fn trial_revert(&mut self) {
        cmdq::push(AppCommand::TrialRevert);
        self.post_command();
        self.ext.trial_focus = None;
        self.status = "Reverted.".into();
    }

    fn accent_bgr_for_demo(&self) -> u32 {
        registry::accent_bgr()
    }

    pub fn on_timer(&mut self, hwnd: HWND) {
        // The trial bar appears and disappears when the host starts and ends a trial, which
        // moves the page; the native edit boxes have to follow it.
        if self.trial_active() != self.ext.trial_shown {
            self.ext.trial_shown = self.trial_active();
            self.relayout(hwnd);
        }
        if self.ext.refresh_pending {
            self.ext.refresh_pending = false;
            self.refresh_report();
        }
        if updater::stamp() != self.ext.update_seen {
            self.ext.update_seen = updater::stamp();
            if let Some(line) = crate::update::short_status(&updater::phase()) {
                self.status = line;
            }
            self.relayout(hwnd);
        }
        if let Some((_, until)) = self.ext.highlight {
            let now = unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount64() };
            if now >= until {
                self.ext.highlight = None;
            }
        }
        if super::super::gallery::stamp() != self.ext.wall_seen {
            self.ext.wall_seen = super::super::gallery::stamp();
            if let Some(message) = super::super::gallery::take_message() {
                self.status = message;
            }
            self.ext.wall_state = super::super::gallery::load_state();
            let count = super::super::gallery::displays().len() + 1;
            if self.ext.wall_display >= count {
                self.ext.wall_display = 0;
            }
            self.relayout(hwnd);
        }
        if !self.trial_active()
            && !updater::busy()
            && !super::super::gallery::busy()
            && self.ext.highlight.is_none()
        {
            unsafe { windows_sys::Win32::UI::WindowsAndMessaging::KillTimer(hwnd, TIMER_UI) };
        } else if self.trial_focus().is_some() && !self.ext.trial_announced {
            // The trial has started and focus is on Keep: tell a screen reader once.
            self.ext.trial_announced = true;
            self.uia_announce_focus(hwnd);
        }
        unsafe { InvalidateRect(hwnd, std::ptr::null(), 0) };
    }
}

// ---------------------------------------------------------------------------
// Picker and inspector
// ---------------------------------------------------------------------------

impl Ui {
    pub fn visible_windows(&self) -> Vec<&WindowEntry> {
        let needle = self.ext.filter.to_ascii_lowercase();
        self.ext
            .windows
            .iter()
            .filter(|entry| {
                needle.is_empty()
                    || entry.exe.to_ascii_lowercase().contains(&needle)
                    || entry.class.to_ascii_lowercase().contains(&needle)
                    || (self.ext.show_titles && entry.title.to_ascii_lowercase().contains(&needle))
            })
            .collect()
    }

    pub fn refresh_windows(&mut self) {
        self.ext.windows = inspect::list_windows();
        self.ext.loaded = true;
    }

    pub fn window_line(&self, entry: &WindowEntry) -> String {
        if self.ext.show_titles && !entry.title.is_empty() {
            format!("{}  ·  {}  ·  {}", entry.exe, entry.class, entry.title)
        } else {
            format!("{}  ·  {}", entry.exe, entry.class)
        }
    }

    pub fn click_window_list(&mut self, index: i32) {
        if index < 0 {
            return;
        }
        let Some(entry) = self
            .visible_windows()
            .get(index as usize)
            .map(|e| (*e).clone())
        else {
            return;
        };
        self.select_window(entry);
    }

    pub fn select_window(&mut self, entry: WindowEntry) {
        self.ext.selected = Some(entry);
        self.refresh_report();
    }

    pub fn refresh_report(&mut self) {
        let Some(selected) = self.ext.selected.clone() else {
            return;
        };
        match super::super::app::report_for(selected.key) {
            Some(report) => {
                let labels: Vec<String> = self.config.rules.iter().map(|r| r.label()).collect();
                let (user, profile) = user_strings();
                self.ext.report_lines =
                    crate::render_report(&report, &labels, self.ext.show_titles, &user, &profile);
                let context = crate::FixContext {
                    reduced: super::super::app::reduce_reason_now(),
                    legacy_enabled: self.config.experimental_dark_legacy,
                    helper_enabled: self.config.legacy.emulated_x64,
                    helper_installed: super::super::legacy::helper_installed(),
                    touched: super::super::app::window_touched(selected.key),
                };
                self.ext.accent_line = {
                    let accents = super::super::cfgio::load_accents();
                    match accents.describe(&report.exe) {
                        Some(text) => format!("{}: {text}", report.exe),
                        None => format!(
                            "{}: not assigned yet. A color is chosen the first time this app is styled.",
                            report.exe
                        ),
                    }
                };
                self.ext.fix = Some(crate::fix_plan(&report, &context));
                self.ext.receipt = crate::receipt_lines(&report);
                self.ext.report = Some(report);
            }
            None => {
                self.ext.fix = None;
                self.ext.receipt.clear();
                self.ext.report = None;
                self.ext.report_lines = vec!["That window has closed. Pick another.".to_string()];
                self.ext.selected = None;
            }
        }
    }

    pub fn begin_pick(&mut self, hwnd: HWND) {
        self.ext.picking = true;
        self.status = "Drag onto a window and let go.".into();
        unsafe {
            SetCapture(hwnd);
            windows_sys::Win32::UI::WindowsAndMessaging::SetCursor(LoadCursorW(
                std::ptr::null_mut(),
                windows_sys::Win32::UI::WindowsAndMessaging::IDC_CROSS,
            ));
        }
    }

    pub fn end_pick(&mut self, _hwnd: HWND) {
        self.ext.picking = false;
        unsafe { ReleaseCapture() };
        match inspect::window_under_cursor() {
            Some(entry) => {
                self.status = format!("Picked {}.", entry.exe);
                self.select_window(entry);
            }
            None => {
                self.status =
                    "Drag the crosshair onto a window other than this one, then let go.".into();
            }
        }
    }

    pub fn pick_hit(&self, lw: i32, lx: i32, cy: i32) -> bool {
        if self.page != Page::AppsFix {
            return false;
        }
        for row in self.rows() {
            if let Row::Buttons(list) = &row.row {
                if list.first() == Some(&Button::PickWindow) && cy >= row.y && cy < row.y + row.h {
                    let (l, t, r, b) = buttons_rect(lw, list.len(), 0, row.h);
                    return lx >= l && lx < r && cy - row.y >= t && cy - row.y < b;
                }
            }
        }
        false
    }
}

fn user_strings() -> (String, String) {
    (
        std::env::var("USERNAME").unwrap_or_default(),
        std::env::var("USERPROFILE").unwrap_or_default(),
    )
}

// ---------------------------------------------------------------------------
// Buttons rows: N pills across one card
// ---------------------------------------------------------------------------

pub fn buttons_rect(lw: i32, count: usize, index: usize, row_h: i32) -> (i32, i32, i32, i32) {
    let gap = 8;
    let count = count.max(1) as i32;
    let inner = lw - 2 * MARGIN - 2 * PAD;
    let width = (inner - (count - 1) * gap) / count;
    let left = MARGIN + PAD + index as i32 * (width + gap);
    let height = 32;
    let top = ((row_h - GAP) - height) / 2;
    (left, top, left + width, top + height)
}

impl Ui {
    pub fn click_buttons(&mut self, lw: i32, lx: i32, local: i32, list: Vec<Button>, row_h: i32) {
        for (index, id) in list.iter().enumerate() {
            let (l, t, r, b) = buttons_rect(lw, list.len(), index, row_h);
            if lx >= l && lx < r && local >= t && local < b {
                self.button(*id);
                return;
            }
        }
    }

    /// Row kinds that can take keyboard focus, in page order.
    pub fn interactive_rows(&self) -> Vec<(usize, usize)> {
        // (row index, sub-item count)
        self.rows()
            .iter()
            .enumerate()
            .filter_map(|(index, laid)| match &laid.row {
                Row::Toggle(_)
                | Row::Choice(_)
                | Row::Action(_)
                | Row::AddField(_)
                | Row::Credit
                | Row::Crumb
                | Row::Link(_)
                | Row::Result(_)
                | Row::Expander(_) => Some((index, 1)),
                Row::Buttons(list) => Some((index, list.len())),
                Row::Presets => Some((index, PRESETS.len())),
                Row::LookList if self.look_count() > 0 => Some((index, self.look_count())),
                Row::WallGrid if !self.ext.walls.is_empty() => Some((index, self.ext.walls.len())),
                // Sliders are reachable by keyboard: Left and Right change the value.
                Row::Slider(_) => Some((index, 1)),
                _ => None,
            })
            .collect()
    }

    /// Tab and Shift+Tab. The navigation pane is one stop (arrow keys move inside it), the
    /// content's controls follow it in reading order, and Tab from the last control comes back
    /// to the pane, so focus is never trapped (Microsoft: keyboard accessibility).
    pub fn move_focus(&mut self, hwnd: HWND, backwards: bool) {
        self.move_focus_inner(hwnd, backwards);
        self.uia_announce_focus(hwnd);
    }

    fn move_focus_inner(&mut self, hwnd: HWND, backwards: bool) {
        #[derive(Clone, Copy, PartialEq)]
        enum Stop {
            Nav,
            Content(usize),
            Trial(u8),
        }
        let list = self.interactive_rows();
        // One ring: the pane, the content's controls in reading order, then Keep and Revert
        // while a trial runs, then back to the pane. Focus is never trapped and the trial's
        // answer is always reachable (Microsoft: keyboard accessibility).
        let mut ring: Vec<Stop> = Vec::new();
        if self.nav_w > 0 {
            ring.push(Stop::Nav);
        }
        ring.extend((0..list.len()).map(Stop::Content));
        if self.trial_active() {
            ring.push(Stop::Trial(0));
            ring.push(Stop::Trial(1));
        }
        if ring.is_empty() {
            return;
        }
        let here = if let Some(button) = self.trial_focus() {
            Some(Stop::Trial(button))
        } else if self.ext.nav_focus {
            Some(Stop::Nav)
        } else {
            self.ext.focus.map(Stop::Content)
        };
        let at = here.and_then(|stop| ring.iter().position(|s| *s == stop));
        let next = match (at, backwards) {
            (None, false) => 0,
            (None, true) => ring.len() - 1,
            (Some(i), false) => (i + 1) % ring.len(),
            (Some(i), true) => (i + ring.len() - 1) % ring.len(),
        };
        self.ext.nav_focus = false;
        self.ext.focus = None;
        self.ext.trial_focus = None;
        match ring[next] {
            Stop::Nav => self.ext.nav_focus = true,
            Stop::Content(index) => {
                self.ext.focus = Some(index);
                self.ext.focus_sub = 0;
                self.scroll_focus_into_view(hwnd);
            }
            Stop::Trial(button) => self.ext.trial_focus = Some(button),
        }
    }

    /// Left/Right: change a focused slider by 5 percent, otherwise move within the focused
    /// composite control (a row of buttons, a list).
    pub fn arrow_key(&mut self, hwnd: HWND, delta: i32) {
        let list = self.interactive_rows();
        let rows = self.rows();
        let slider = self
            .ext
            .focus
            .and_then(|ordinal| list.get(ordinal))
            .and_then(|(row_index, _)| rows.get(*row_index))
            .and_then(|laid| match laid.row {
                Row::Slider(id) => Some(id),
                _ => None,
            });
        if let Some(id) = slider {
            if self.trial_active() {
                self.status = "Keep or revert the trial look first.".into();
                return;
            }
            let (fraction, _) = slider_state(self, id);
            let (x0, x1) = slider_track(self.lw(hwnd));
            let next = (fraction + 0.05 * delta as f32).clamp(0.0, 1.0);
            let lx = x0 + ((x1 - x0) as f32 * next).round() as i32;
            self.set_slider(hwnd, id, lx);
            self.commit();
        } else {
            self.move_sub(delta);
        }
        self.uia_announce_focus(hwnd);
    }

    pub fn move_sub(&mut self, delta: i32) {
        let list = self.interactive_rows();
        let Some((_, count)) = self.ext.focus.and_then(|index| list.get(index).copied()) else {
            return;
        };
        if count > 1 {
            self.ext.focus_sub =
                (self.ext.focus_sub as i32 + delta).rem_euclid(count as i32) as usize;
        }
    }

    pub fn scroll_focus_into_view_pub(&mut self, hwnd: HWND) {
        self.scroll_focus_into_view(hwnd);
    }

    fn scroll_focus_into_view(&mut self, hwnd: HWND) {
        let list = self.interactive_rows();
        let rows = self.rows();
        let Some((row_index, _)) = self.ext.focus.and_then(|index| list.get(index).copied()) else {
            return;
        };
        let Some(row) = rows.get(row_index) else {
            return;
        };
        let view = self.view_height(hwnd);
        if row.y < self.scroll {
            self.scroll = (row.y - 8).max(0);
        } else if row.y + row.h > self.scroll + view {
            self.scroll = row.y + row.h - view + 8;
        }
    }

    /// Space or Enter on the focused row.
    pub fn activate_focus(&mut self, hwnd: HWND) {
        if let Some(button) = self.trial_focus() {
            if button == 0 {
                self.trial_keep();
            } else {
                self.trial_revert();
            }
            self.relayout(hwnd);
            return;
        }
        if self.trial_active() {
            // Enter or Space during a trial: say what is possible and put focus on Keep, so the
            // next key press answers it.
            self.status = "Keep or revert the trial look first.".into();
            self.ext.nav_focus = false;
            self.ext.trial_focus = Some(0);
            self.uia_announce_focus(hwnd);
            self.relayout(hwnd);
            return;
        }
        let list = self.interactive_rows();
        let rows = self.rows();
        let Some((row_index, _)) = self.ext.focus.and_then(|index| list.get(index).copied()) else {
            return;
        };
        let Some(laid) = rows.get(row_index) else {
            return;
        };
        let sub = self.ext.focus_sub;
        match &laid.row {
            Row::Toggle(id) => self.toggle(*id),
            Row::Choice(id) => self.cycle(*id),
            Row::Action(id) | Row::AddField(id) => self.button(*id),
            Row::Buttons(buttons) => {
                if let Some(id) = buttons.get(sub) {
                    if *id == Button::PickWindow {
                        self.status =
                            "Use the mouse for the crosshair, or pick from the list.".into();
                    } else {
                        self.button(*id);
                    }
                }
            }
            Row::Presets => self.apply_preset(sub.min(PRESETS.len() - 1)),
            Row::Credit => self.open_credit(),
            Row::LookList => self.click_look_list(sub as i32),
            Row::WallGrid => self.select_wall(sub),
            Row::Crumb => {
                if let Some(parent) = self.page.parent() {
                    self.set_page(hwnd, parent);
                }
            }
            Row::Link(dest) => self.set_page(hwnd, *dest),
            Row::Result(index) => self.open_result(hwnd, *index),
            Row::Expander(title) => self.toggle_section(title),
            Row::GlobalSearch => self.focus_search(hwnd),
            _ => {}
        }
        self.relayout(hwnd);
    }
}

// ---------------------------------------------------------------------------
// Scenes and the Look Book
// ---------------------------------------------------------------------------

impl Ui {
    pub fn refresh_looks(&mut self) {
        self.ext.scenes = crate::scene_catalog();
        if let Some(scene) = &self.ext.image_scene {
            self.ext.scenes.push(scene.clone());
        }
        self.ext.book = super::super::cfgio::load_lookbook();
        if self.ext.look_sel.is_some_and(|i| i >= self.look_count()) {
            self.ext.look_sel = None;
        }
        if self.ext.look_mark.is_some_and(|i| i >= self.look_count()) {
            self.ext.look_mark = None;
        }
        self.update_look_diff();
    }

    pub fn look_count(&self) -> usize {
        self.ext.scenes.len() + self.ext.book.looks.len()
    }

    pub fn look_at(&self, index: usize) -> Option<&dyn crate::SceneLike> {
        let scenes = self.ext.scenes.len();
        if index < scenes {
            self.ext
                .scenes
                .get(index)
                .map(|s| s as &dyn crate::SceneLike)
        } else {
            self.ext
                .book
                .looks
                .get(index - scenes)
                .map(|l| l as &dyn crate::SceneLike)
        }
    }

    fn look_name(&self, index: usize) -> String {
        let scenes = self.ext.scenes.len();
        if index < scenes {
            self.ext.scenes[index].name.clone()
        } else {
            self.ext
                .book
                .looks
                .get(index - scenes)
                .map(|l| l.name.clone())
                .unwrap_or_default()
        }
    }

    fn look_is_mine(&self, index: usize) -> bool {
        index >= self.ext.scenes.len()
    }

    pub fn click_look_list(&mut self, index: i32) {
        if index >= 0 && (index as usize) < self.look_count() {
            self.ext.look_sel = Some(index as usize);
            self.update_look_diff();
            if self.ext.setup.active {
                self.setup_note_choice(index as usize);
            }
        }
    }

    fn update_look_diff(&mut self) {
        let Some(selected) = self.ext.look_sel else {
            self.ext.look_diff.clear();
            return;
        };
        let Some(item) = self.look_at(selected) else {
            self.ext.look_diff.clear();
            return;
        };
        let changes = match self.ext.look_mark.filter(|mark| *mark != selected) {
            Some(mark) => match self.look_at(mark) {
                Some(base) => crate::compare_looks(base, item),
                None => crate::preview_scene(&self.config, item),
            },
            None => crate::preview_scene(&self.config, item),
        };
        self.ext.look_diff = changes
            .iter()
            .map(|c| format!("{}: {} → {}", c.field, c.from, c.to))
            .collect();
    }

    pub fn look_details(&self) -> Vec<String> {
        let Some(selected) = self.ext.look_sel else {
            return vec![
                "Pick a scene or one of your looks to see what it would change.".to_string(),
            ];
        };
        let scenes = self.ext.scenes.len();
        let mut lines = Vec::new();
        if selected < scenes {
            let scene = &self.ext.scenes[selected];
            lines.push(format!("{}: {}", scene.name, scene.blurb));
            lines.push(format!("Not included: {}", scene.limit));
        } else if let Some(look) = self.ext.book.looks.get(selected - scenes) {
            lines.push(format!("{} (your look)", look.name));
            lines.push(format!("Saved {}", crate::format_unix_utc(look.created)));
        }
        let base = self
            .ext
            .look_mark
            .filter(|mark| *mark != selected)
            .map(|mark| self.look_name(mark));
        match (&base, self.ext.look_diff.is_empty()) {
            (Some(base), true) => lines.push(format!("Same as {base}.")),
            (Some(base), false) => lines.push(format!("Compared with {base}:")),
            (None, true) => {
                lines.push("It would change nothing: your look already matches.".to_string())
            }
            (None, false) => lines.push("Applying it would change:".to_string()),
        }
        lines.extend(self.ext.look_diff.iter().map(|line| format!("  {line}")));
        lines
    }

    fn look_arg_name(&self) -> String {
        get_text(self.edits.look).trim().to_string()
    }

    fn try_scene(&mut self) {
        let Some(index) = self.ext.look_sel else {
            self.status = "Pick a scene or a look first.".into();
            return;
        };
        let name = self.look_name(index);
        let mut next = self.config.clone();
        match self.look_at(index) {
            Some(item) => {
                crate::apply_scene(&mut next, item);
            }
            None => return,
        }
        self.begin_trial(&name, next);
    }

    fn change_accent(&mut self, id: Button) {
        let Some(exe) = self.ext.selected.as_ref().map(|s| s.exe.clone()) else {
            self.status = "Pick an app first.".into();
            return;
        };
        let mut accents = super::super::cfgio::load_accents();
        let outcome = if id == Button::NextAccent {
            accents
                .next_override(&exe)
                .map(|color| format!("{exe} now uses {color}."))
        } else if accents.clear_override(&exe) {
            Ok(format!("{exe} goes back to its automatic accent."))
        } else {
            Ok(format!("{exe} already uses its automatic accent."))
        };
        match outcome {
            Ok(message) => match super::super::cfgio::save_accents(&accents) {
                Ok(()) => {
                    self.status = message;
                    cmdq::push(AppCommand::ReloadAccents);
                    self.post_command();
                    self.schedule_report_refresh();
                }
                Err(err) => self.status = format!("Could not save the accents: {err}"),
            },
            Err(err) => self.status = err,
        }
    }

    fn palette_from_image(&mut self) {
        let Some(path) = file_dialog_filtered(
            self.hwnd,
            false,
            "Choose a picture",
            "",
            "Pictures (*.png;*.jpg;*.jpeg;*.bmp;*.gif)\0*.png;*.jpg;*.jpeg;*.bmp;*.gif\0All files\0*.*\0\0",
        ) else {
            return;
        };
        let stem = std::path::Path::new(&path)
            .file_stem()
            .map(|s| s.to_string_lossy().chars().take(24).collect::<String>())
            .unwrap_or_default();
        let result = super::super::image::sample(&path)
            .and_then(|samples| crate::derive_palette(&samples))
            .and_then(|palette| {
                let name = if stem.is_empty() {
                    "From a picture".to_string()
                } else {
                    format!("From {stem}")
                };
                crate::palette_scene(&palette, &name).map(|scene| (scene, palette.note))
            });
        match result {
            Ok((scene, note)) => {
                self.status = match note {
                    Some(note) => format!("Built “{}”. {note}", scene.name),
                    None => format!("Built “{}”. Try it to see it on your windows.", scene.name),
                };
                self.ext.image_scene = Some(scene);
                self.refresh_looks();
                self.ext.look_sel = Some(self.ext.scenes.len() - 1);
                self.update_look_diff();
            }
            Err(err) => self.status = err,
        }
    }

    fn save_look(&mut self) {
        let name = self.look_arg_name();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut book = super::super::cfgio::load_lookbook();
        match book.add(&name, &self.config, now) {
            Ok(_) => match super::super::cfgio::save_lookbook(&book) {
                Ok(()) => {
                    set_text(self.edits.look, "");
                    self.status = format!("Saved “{name}” to your Look Book.");
                    self.refresh_looks();
                    self.ext.look_sel = Some(self.look_count() - 1);
                    self.update_look_diff();
                }
                Err(err) => self.status = format!("Could not save the Look Book: {err}"),
            },
            Err(err) => self.status = err,
        }
    }

    fn rename_look(&mut self) {
        let Some(index) = self.ext.look_sel.filter(|i| self.look_is_mine(*i)) else {
            self.status =
                "Pick one of your own looks to rename. Built-in scenes keep their names.".into();
            return;
        };
        let id = self.ext.book.looks[index - self.ext.scenes.len()]
            .id
            .clone();
        let name = self.look_arg_name();
        let mut book = super::super::cfgio::load_lookbook();
        match book.rename(&id, &name) {
            Ok(()) => match super::super::cfgio::save_lookbook(&book) {
                Ok(()) => {
                    set_text(self.edits.look, "");
                    self.status = format!("Renamed to “{name}”.");
                    self.refresh_looks();
                }
                Err(err) => self.status = format!("Could not save the Look Book: {err}"),
            },
            Err(err) => self.status = err,
        }
    }

    fn delete_look(&mut self) {
        let Some(index) = self.ext.look_sel.filter(|i| self.look_is_mine(*i)) else {
            self.status =
                "Pick one of your own looks to delete. Built-in scenes cannot be deleted.".into();
            return;
        };
        let look = &self.ext.book.looks[index - self.ext.scenes.len()];
        let (id, name) = (look.id.clone(), look.name.clone());
        let mut book = super::super::cfgio::load_lookbook();
        book.remove(&id);
        match super::super::cfgio::save_lookbook(&book) {
            Ok(()) => {
                self.status = format!("Deleted “{name}”.");
                self.ext.look_sel = None;
                self.ext.look_mark = None;
                self.refresh_looks();
            }
            Err(err) => self.status = format!("Could not save the Look Book: {err}"),
        }
    }

    fn mark_compare(&mut self) {
        match self.ext.look_sel {
            Some(index) => {
                self.ext.look_mark = Some(index);
                self.status = format!(
                    "Now pick another look to compare it with {}.",
                    self.look_name(index)
                );
                self.update_look_diff();
            }
            None => self.status = "Pick a scene or a look first.".into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Painting the new row kinds
// ---------------------------------------------------------------------------

pub fn paint_extra(c: &Canvas, ui: &Ui, row: &Laid, top: i32, lw: i32) {
    let fonts = &ui.fonts;
    let left = MARGIN + PAD;
    let right = lw - MARGIN - PAD;
    let card_h = row.h - GAP;
    let text_left = left + if fonts.icon.is_null() { 0 } else { 44 };
    // One icon from one family (Segoe Fluent Icons) in a fixed box, or nothing when the font is
    // missing, so a gap never shows a "missing glyph" square.
    let glyph_at = |ch: char, l: i32, t: i32, r: i32, b: i32, color: u32| {
        if !fonts.icon.is_null() {
            c.text(
                l,
                t,
                r,
                b,
                &ch.to_string(),
                color,
                fonts.icon,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
        }
    };
    match &row.row {
        Row::Crumb => {
            let parent = ui.page.parent().map(|p| p.title()).unwrap_or("");
            glyph_at(
                '\u{E72B}',
                MARGIN,
                top + 6,
                MARGIN + 28,
                top + 30,
                th().brand,
            );
            c.text(
                MARGIN + 32,
                top + 6,
                right,
                top + 30,
                parent,
                th().brand,
                fonts.body,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
            );
        }
        Row::Link(dest) => {
            card(c, top, row.h, lw);
            glyph_at(dest.glyph(), left, top, left + 32, top + card_h, th().text);
            c.text(
                text_left,
                top + 10,
                right - 24,
                top + 32,
                dest.title(),
                th().text,
                fonts.body,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
            );
            c.text(
                text_left,
                top + 32,
                right - 24,
                top + card_h - 6,
                dest.description(),
                th().muted,
                fonts.small,
                DT_LEFT | DT_WORDBREAK,
            );
            glyph_at('\u{E76C}', right - 20, top, right, top + card_h, th().muted);
        }
        Row::Result(index) => {
            card(c, top, row.h, lw);
            if let Some(result) = ui.ext.results.get(*index) {
                glyph_at(result.glyph, left, top, left + 32, top + card_h, th().text);
                c.text(
                    text_left,
                    top + 8,
                    right - 24,
                    top + 30,
                    &result.title,
                    th().text,
                    fonts.body,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
                let place = if result.path.is_empty() {
                    "Action".to_string()
                } else {
                    result.path.clone()
                };
                c.text(
                    text_left,
                    top + 30,
                    right - 24,
                    top + 48,
                    &place,
                    th().brand,
                    fonts.small,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
                c.text(
                    text_left,
                    top + 48,
                    right - 24,
                    top + card_h - 4,
                    &result.hint,
                    th().muted,
                    fonts.small,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
            }
            glyph_at('\u{E76C}', right - 20, top, right, top + card_h, th().muted);
        }
        Row::Expander(title) => {
            card(c, top, row.h, lw);
            let open = ui.section_open(title);
            c.text(
                left,
                top,
                right - 90,
                top + card_h,
                title,
                th().text,
                fonts.body,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
            );
            c.text(
                right - 84,
                top,
                right - 24,
                top + card_h,
                if open { "Hide" } else { "Show" },
                th().muted,
                fonts.small,
                DT_RIGHT | DT_VCENTER | DT_SINGLELINE,
            );
            glyph_at(
                if open { '\u{E70E}' } else { '\u{E70D}' },
                right - 20,
                top,
                right,
                top + card_h,
                th().muted,
            );
        }
        Row::GlobalSearch => {
            card(c, top, row.h, lw);
            glyph_at('\u{E721}', left, top, left + 24, top + card_h, th().muted);
        }
        Row::Lines(lines) => {
            card(c, top, row.h, lw);
            let mut y = top + 10;
            for line in lines {
                let height = line_height(line, lw);
                let (font, color) = if line.is_empty() {
                    (fonts.small, th().muted)
                } else {
                    (fonts.small, th().text)
                };
                c.text(
                    left,
                    y,
                    right,
                    y + height,
                    line,
                    color,
                    font,
                    DT_LEFT | DT_WORDBREAK,
                );
                y += height;
            }
        }
        Row::Buttons(list) => {
            card(c, top, row.h, lw);
            for (index, id) in list.iter().enumerate() {
                let (l, t, r, b) = buttons_rect(lw, list.len(), index, row.h);
                let (_, _, label) = button_meta(*id);
                controls::draw_button(
                    c,
                    ui,
                    (l, top + t, r, top + b),
                    label,
                    controls::emphasis_of(*id),
                    controls::State {
                        active: *id == Button::PickWindow && ui.ext.picking,
                        ..controls::State::default()
                    },
                );
            }
        }
        Row::WindowList => {
            card(c, top, row.h, lw);
            let visible = ui.visible_windows();
            if visible.is_empty() {
                let text = if ui.ext.loaded {
                    "No matching windows. Clear the filter or press Refresh list."
                } else {
                    "Open this page to list windows."
                };
                c.text(
                    left,
                    top + 8,
                    right,
                    top + 8 + LIST_LINE,
                    text,
                    th().muted,
                    fonts.small,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE,
                );
            }
            for (index, entry) in visible.iter().take(40).enumerate() {
                let line_top = top + 8 + index as i32 * LIST_LINE;
                let selected = ui
                    .ext
                    .selected
                    .as_ref()
                    .is_some_and(|s| s.key == entry.key && s.pid == entry.pid);
                if selected {
                    c.round(
                        MARGIN + 6,
                        line_top,
                        lw - MARGIN - 6,
                        line_top + LIST_LINE,
                        4,
                        Some(th().card_hi),
                        Some(ui.accent),
                    );
                }
                c.text(
                    left,
                    line_top,
                    right,
                    line_top + LIST_LINE,
                    &ui.window_line(entry),
                    th().text,
                    fonts.small,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
            }
        }
        Row::WallGrid => {
            card(c, top, row.h, lw);
            let (_, _, thumb_h, _) = wall_geom(lw);
            for (index, entry) in ui.ext.walls.iter().enumerate() {
                let (l, t, r, b) = wall_rect(lw, index);
                let selected = ui.ext.wall_sel == index;
                c.round(
                    l - 3,
                    top + t - 3,
                    r + 3,
                    top + b + 1,
                    6,
                    Some(if selected { th().card_hi } else { th().card }),
                    Some(if selected { ui.accent } else { th().edge }),
                );
                c.fill(l, top + t, r, top + t + thumb_h, 0x0000_0000);
                if let Some((_, bitmap)) =
                    ui.ext.wall_previews.iter().find(|(id, _)| *id == entry.id)
                {
                    c.bitmap(*bitmap, l, top + t, r, top + t + thumb_h);
                }
                let status = crate::wallpaper::status_text(
                    entry,
                    super::super::gallery::downloaded(entry),
                    &ui.ext.wall_state,
                );
                c.text(
                    l + 2,
                    top + t + thumb_h + 2,
                    r - 2,
                    top + t + thumb_h + 20,
                    &entry.name,
                    th().text,
                    fonts.body,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
                c.text(
                    l + 2,
                    top + t + thumb_h + 20,
                    r - 2,
                    top + t + thumb_h + 38,
                    &status,
                    th().muted,
                    fonts.small,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
            }
        }
        Row::RecipeList => {
            card(c, top, row.h, lw);
            if ui.ext.recipes.is_empty() {
                c.text(
                    left,
                    top + 8,
                    right,
                    top + 8 + LIST_LINE,
                    "No recipes found.",
                    th().muted,
                    fonts.small,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE,
                );
            }
            for (index, (recipe, trust)) in ui.ext.recipes.iter().enumerate() {
                let line_top = top + 8 + index as i32 * LIST_LINE;
                if ui.ext.recipe_sel == Some(index) {
                    c.round(
                        MARGIN + 6,
                        line_top,
                        lw - MARGIN - 6,
                        line_top + LIST_LINE,
                        4,
                        Some(th().card_hi),
                        Some(ui.accent),
                    );
                }
                let tag = if *trust == Trust::Bundled {
                    "built in"
                } else {
                    "imported"
                };
                c.text(
                    left,
                    line_top,
                    right,
                    line_top + LIST_LINE,
                    &format!("{}  ·  {tag}", recipe.name),
                    th().text,
                    fonts.small,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
            }
        }
        Row::LookList => {
            card(c, top, row.h, lw);
            let scenes = ui.ext.scenes.len();
            for index in 0..ui.look_count().min(40) {
                let Some(item) = ui.look_at(index) else {
                    continue;
                };
                let line_top = top + 8 + index as i32 * LIST_LINE;
                if ui.ext.look_sel == Some(index) {
                    c.round(
                        MARGIN + 6,
                        line_top,
                        lw - MARGIN - 6,
                        line_top + LIST_LINE,
                        4,
                        Some(th().card_hi),
                        Some(ui.accent),
                    );
                }
                let tag = if index < scenes { "scene" } else { "yours" };
                let mark = if ui.ext.look_mark == Some(index) {
                    "  ·  compare base"
                } else {
                    ""
                };
                let chips = crate::scene_swatch(item, ui.accent_bgr_for_demo());
                let chips_left = right - 3 * 20;
                c.text(
                    left,
                    line_top,
                    chips_left - 6,
                    line_top + LIST_LINE,
                    &format!("{}  ·  {tag}{mark}", ui.look_name(index)),
                    th().text,
                    fonts.small,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
                for (slot, color) in [chips.caption, chips.border, chips.text].iter().enumerate() {
                    let x = chips_left + slot as i32 * 20;
                    let (l, t, r, b) = (x, line_top + 5, x + 14, line_top + 19);
                    match color {
                        Some(value) => c.round(l, t, r, b, 3, Some(*value), Some(th().muted)),
                        // Nothing is drawn there by this look: show an empty outline.
                        None => c.round(l, t, r, b, 3, None, Some(th().muted)),
                    }
                }
            }
        }
        Row::SearchField => {
            card(c, top, row.h, lw);
            c.text(
                left,
                top,
                left + 96,
                top + row.h - GAP,
                "Search help",
                th().text,
                fonts.body,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
        }
        Row::Filter => {
            card(c, top, row.h, lw);
            c.text(
                left,
                top,
                left + 90,
                top + row.h - GAP,
                "Filter",
                th().text,
                fonts.body,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
        }
        _ => {}
    }
}

/// Mark the row a search result took you to: a thick double outline *and* a bar down its left
/// edge *and* the word "Found" (never color alone). Expires by itself after a moment.
pub fn paint_found(c: &Canvas, ui: &Ui, row_index: usize, row: &Laid, top: i32, lw: i32) {
    let Some((marked, _)) = ui.ext.highlight else {
        return;
    };
    if marked != row_index {
        return;
    }
    let (l, t, r, b) = (MARGIN, top, lw - MARGIN, top + row.h - GAP);
    for grow in [3, 5] {
        c.round(
            l - grow,
            t - grow,
            r + grow,
            b + grow,
            6 + grow / 2,
            None,
            Some(ui.accent),
        );
    }
    c.round(l - 7, t, l - 3, b, 2, Some(ui.accent), None);
    c.text(
        r - 70,
        t - 18,
        r,
        t - 2,
        "Found",
        ui.accent,
        ui.fonts.small,
        DT_RIGHT | DT_VCENTER | DT_SINGLELINE,
    );
}

/// Focus ring around the keyboard-focused row.
pub fn paint_focus_ring(c: &Canvas, ui: &Ui, row: &Laid, top: i32, lw: i32) {
    // Composite rows (a row of buttons, the preset list, the scene list) ring the one item that
    // has focus; every other row rings its card. Same two-pixel ring everywhere.
    match &row.row {
        Row::Buttons(list) => {
            let (l, t, r, b) =
                buttons_rect(lw, list.len(), ui.ext.focus_sub.min(list.len() - 1), row.h);
            controls::focus_ring(c, (l, top + t, r, top + b));
        }
        Row::LookList => {
            let line_top = top
                + 8
                + ui.ext.focus_sub.min(ui.look_count().saturating_sub(1)) as i32 * LIST_LINE;
            controls::focus_ring(
                c,
                (MARGIN + 4, line_top, lw - MARGIN - 4, line_top + LIST_LINE),
            );
        }
        Row::WallGrid => {
            let (l, t, r, b) = wall_rect(
                lw,
                ui.ext.focus_sub.min(ui.ext.walls.len().saturating_sub(1)),
            );
            controls::focus_ring(c, (l - 3, top + t - 3, r + 3, top + b + 3));
        }
        Row::Presets => {
            let (l, t, r, b) = preset_rect(lw, ui.ext.focus_sub.min(PRESETS.len() - 1));
            controls::focus_ring(c, (l, top + t, r, top + b));
        }
        Row::Credit => {
            let (l, t, r, b) = credit_layout(lw).link;
            controls::focus_ring(c, (l - 2, top + t, r + 2, top + b));
        }
        _ => controls::focus_ring(c, (MARGIN, top, lw - MARGIN, top + row.h - GAP)),
    }
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

impl Ui {
    /// Open the creator's GitHub profile in the default browser. Called only from a click or an
    /// activation key; nothing opens it automatically.
    pub fn open_credit(&mut self) {
        self.status = match registry::open_url(crate::CREDIT_URL) {
            Ok(()) => "Opened TH0MSX's GitHub profile in your browser.".to_string(),
            Err(err) => format!(
                "Could not open the link: {err}. The address is {}.",
                crate::CREDIT_URL
            ),
        };
    }

    /// Keep the page repainting while the updater works.
    fn watch_updates(&mut self) {
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::SetTimer(self.hwnd, TIMER_UI, 250, None)
        };
    }

    fn scroll_to_header(&mut self, text: &str) {
        let rows = self.rows();
        if let Some(row) = rows
            .iter()
            .find(|row| matches!(&row.row, Row::Header(title) if *title == text))
        {
            self.scroll = (row.y - 8).max(0);
        }
    }

    /// Undo the most recent look change from the session history. The earlier config is parsed
    /// and checked before anything is written; if it is not usable nothing changes.
    fn undo_look(&mut self) {
        if self.trial_active() {
            self.status = "Keep or revert the trial look first.".into();
            return;
        }
        let Some(entry) = crate::with_history(|history| history.take_undo()) else {
            self.status =
                "There is nothing to undo yet. Changes made in this session are listed above."
                    .into();
            return;
        };
        match crate::parse_config(&entry.before) {
            Ok(config) => {
                self.config = config;
                self.rule = None;
                self.exclude_sel = None;
                self.push_colors_to_edits();
                self.commit();
                self.status = format!("Undid: {}.", entry.name);
            }
            Err(err) => {
                self.status = format!(
                    "Could not undo \"{}\": the earlier look is not usable ({err}). Nothing was changed.",
                    entry.name
                );
            }
        }
    }

    /// The exclusion manager: every reason a window is left alone, in order, and what is yours.
    fn push_exclusions(&self, b: &mut Builder) {
        b.push(Row::Header("Exclusion manager"));
        let mut lines = crate::exclusion_summary(&self.config);
        lines.extend(crate::exclusion_precedence());
        b.push(Row::Lines(lines));
        if !self.config.exclude_classes.is_empty() {
            b.push(Row::Choice(Choice::ExcludedClass));
            b.push(Row::Buttons(vec![Button::RemoveExcludedClass]));
        }
        b.push(Row::Note(
            "To see why one app is or is not styled, use Test without applying above: it names the layer that wins.",
        ));
    }

    // ---- global search and navigation ----------------------------------------------------

    pub fn section_open(&self, title: &str) -> bool {
        self.ext.open_sections.iter().any(|open| *open == title)
    }

    pub fn toggle_section(&mut self, title: &'static str) {
        match self
            .ext
            .open_sections
            .iter()
            .position(|open| *open == title)
        {
            Some(index) => {
                self.ext.open_sections.remove(index);
            }
            None => self.ext.open_sections.push(title),
        }
    }

    /// Change the search box's text without treating it as typing.
    fn set_search_text(&mut self, text: &str) {
        self.ext.search_silent = true;
        set_text(self.edits.search, text);
        self.ext.search_silent = false;
        self.ext.query = text.to_string();
        if text.is_empty() {
            self.ext.results.clear();
        }
    }

    /// The index search runs on: built when the box gains focus, so typing only filters it.
    fn rebuild_search_index(&mut self) {
        let context = crate::PaletteContext {
            paused: super::super::app::is_paused(),
            can_undo_scene: false,
            scenes: self
                .ext
                .scenes
                .iter()
                .map(|scene| scene.name.clone())
                .collect(),
            looks: self
                .ext
                .book
                .looks
                .iter()
                .map(|look| look.name.clone())
                .collect(),
        };
        self.ext.index = crate::palette_commands(&context);
    }

    /// Ctrl+K and the search box's own click: put the caret in the search box (or, on the narrow
    /// rail, open the results page where the box is).
    pub fn focus_search(&mut self, hwnd: HWND) {
        self.rebuild_search_index();
        if self.nav_w <= crate::ui_metrics::NAV_COMPACT && self.page != Page::Search {
            self.ext.search_from = Some(self.page);
            self.set_page(hwnd, Page::Search);
        }
        self.ext.nav_focus = false;
        self.ext.focus = None;
        unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(self.edits.search) };
        self.relayout(hwnd);
    }

    /// The box changed because someone typed: filter the cached index, show the results page.
    pub fn on_search_changed(&mut self, hwnd: HWND) {
        if self.ext.search_silent {
            return;
        }
        let text = get_text(self.edits.search);
        self.ext.query = text.clone();
        if text.trim().is_empty() {
            self.ext.results.clear();
            if self.page == Page::Search {
                let back = self.ext.search_from.take().unwrap_or(Page::Home);
                self.set_page(hwnd, back);
            }
            self.relayout(hwnd);
            return;
        }
        if self.ext.index.is_empty() {
            self.rebuild_search_index();
        }
        let found = crate::palette_search(&self.ext.index, &text, 12);
        self.ext.results = found
            .into_iter()
            .filter_map(|index| self.ext.index.get(index).cloned())
            .collect();
        if self.page != Page::Search {
            self.ext.search_from = Some(self.page);
            self.set_page(hwnd, Page::Search);
        } else {
            self.ext.focus = None;
            self.scroll = 0;
            self.relayout(hwnd);
        }
        // Keep the caret in the box while the results page replaces the content.
        unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(self.edits.search) };
    }

    /// Clear the box and leave the results page for where the person was.
    pub fn clear_search(&mut self, hwnd: HWND) {
        self.set_search_text("");
        if self.page == Page::Search {
            let back = self.ext.search_from.take().unwrap_or(Page::Home);
            self.set_page(hwnd, back);
        }
        self.relayout(hwnd);
    }

    /// Keys the search box passes up: Enter opens the first result, Down moves into the list,
    /// Escape clears, Tab leaves the box.
    pub fn search_key(&mut self, hwnd: HWND, key: u32, shift: bool) {
        match key {
            0x0D => {
                if !self.ext.results.is_empty() {
                    self.open_result(hwnd, 0);
                }
            }
            0x1B => {
                if self.ext.query.is_empty() {
                    self.ext.nav_focus = false;
                    unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(hwnd) };
                } else {
                    self.clear_search(hwnd);
                }
            }
            _ => {
                // Down arrow or Tab: into the results (or the content, when there are none).
                unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(hwnd) };
                if shift {
                    self.ext.nav_focus = true;
                    self.ext.focus = None;
                } else {
                    self.ext.nav_focus = false;
                    self.ext.focus = None;
                    self.move_focus(hwnd, false);
                }
                self.relayout(hwnd);
            }
        }
    }

    /// Open a result. The query is cleared first; the place is opened and the control, if the
    /// result names one, is scrolled to, focused and marked. Nothing is switched or changed.
    pub fn open_result(&mut self, hwnd: HWND, index: usize) {
        let Some(command) = self.ext.results.get(index).cloned() else {
            return;
        };
        let (dest, anchor) = crate::palette_destination(&command.action);
        self.ext.search_from = None;
        self.set_search_text("");
        self.go(hwnd, dest, anchor);
    }

    /// Go to a place and, when an anchor is given, to one control in it.
    pub fn go(&mut self, hwnd: HWND, dest: Page, anchor: Option<&'static str>) {
        if self.ext.setup_active() {
            // First-run setup owns the window; a request waits for it to finish.
            return;
        }
        if self.page == dest {
            self.ext.highlight = None;
        }
        self.set_page(hwnd, dest);
        if let Some(anchor) = anchor {
            self.reveal(hwnd, anchor);
        }
    }

    /// The row an anchor names, and which item of it (for a row of buttons).
    fn find_anchor(&self, anchor: &str) -> Option<(usize, usize)> {
        let (kind, name) = anchor.split_once(':').unwrap_or((anchor, ""));
        for (index, laid) in self.rows().iter().enumerate() {
            let hit = match (&laid.row, kind) {
                (Row::Toggle(id), "toggle") if format!("{id:?}") == name => Some(0),
                (Row::Choice(id), "choice") if format!("{id:?}") == name => Some(0),
                (Row::Slider(id), "slider") if format!("{id:?}") == name => Some(0),
                (Row::Action(id) | Row::AddField(id), "button") if format!("{id:?}") == name => {
                    Some(0)
                }
                (Row::Buttons(list), "button") => {
                    list.iter().position(|id| format!("{id:?}") == name)
                }
                (Row::Color(id), "color") if color_anchor(*id) == name => Some(0),
                (Row::Header(text), "header") if *text == name => Some(0),
                (Row::Presets, "presets") => Some(0),
                _ => None,
            };
            if let Some(sub) = hit {
                return Some((index, sub));
            }
        }
        None
    }

    /// Scroll to, focus and mark the control an anchor names. If it is inside a closed section,
    /// the sections of this page are opened first. If it is not on the page right now (a control
    /// that only exists in some states), the page opens at the top and the status line says so.
    fn reveal(&mut self, hwnd: HWND, anchor: &'static str) {
        let mut found = self.find_anchor(anchor);
        if found.is_none() {
            let closed: Vec<&'static str> = self
                .rows()
                .iter()
                .filter_map(|laid| match laid.row {
                    Row::Expander(title) if !self.section_open(title) => Some(title),
                    _ => None,
                })
                .collect();
            if !closed.is_empty() {
                self.ext.open_sections.extend(closed);
                found = self.find_anchor(anchor);
            }
        }
        let Some((row_index, sub)) = found else {
            self.status = format!(
                "Opened {}. That control is not shown right now.",
                self.page.title()
            );
            self.relayout(hwnd);
            return;
        };
        let rows = self.rows();
        let interactive = self.interactive_rows();
        // A heading is not a control: aim at the first control after it.
        let target_row = if matches!(rows.get(row_index).map(|r| &r.row), Some(Row::Header(_))) {
            interactive
                .iter()
                .map(|(index, _)| *index)
                .find(|index| *index > row_index)
                .unwrap_or(row_index)
        } else {
            row_index
        };
        if let Some(row) = rows.get(row_index) {
            self.scroll = (row.y - 24).max(0);
        }
        self.ext.nav_focus = false;
        let ordinal = interactive
            .iter()
            .position(|(index, _)| *index == target_row);
        match ordinal {
            Some(ordinal) => {
                self.ext.focus = Some(ordinal);
                self.ext.focus_sub = sub;
            }
            None => {
                self.ext.focus = None;
                if let Some(Row::Color(id)) = rows.get(target_row).map(|r| &r.row) {
                    if let Some(edit) = self.color_edit(*id) {
                        self.relayout(hwnd);
                        unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(edit) };
                    }
                }
            }
        }
        let until =
            unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount64() } + 2500;
        self.ext.highlight = Some((target_row, until));
        self.watch_updates();
        self.relayout(hwnd);
        self.scroll_focus_into_view_pub(hwnd);
        self.relayout(hwnd);
        self.uia_announce_focus(hwnd);
    }

    // ---- recipe remix --------------------------------------------------------------------

    /// Index of the recipe the remix takes from: never the one picked in the list.
    pub fn remix_donor(&self) -> Option<usize> {
        let count = self.ext.recipes.len();
        let base = self.ext.recipe_sel?;
        if count < 2 {
            return None;
        }
        let candidate = self.ext.remix_with % count;
        Some(if candidate == base {
            (candidate + 1) % count
        } else {
            candidate
        })
    }

    fn current_remix(&self) -> Option<Result<Recipe, String>> {
        let base = &self.ext.recipes.get(self.ext.recipe_sel?)?.0;
        let donor = &self.ext.recipes.get(self.remix_donor()?)?.0;
        Some(crate::remix_recipes(base, donor, self.ext.remix_part))
    }

    fn try_remix(&mut self) {
        match self.current_remix() {
            Some(Ok(recipe)) => {
                let mut next = self.config.clone();
                crate::apply_recipe(&mut next, &recipe);
                self.begin_trial(&recipe.name, next);
            }
            Some(Err(reason)) => self.status = reason,
            None => self.status = "Pick a recipe first.".into(),
        }
    }

    fn save_remix(&mut self) {
        let recipe = match self.current_remix() {
            Some(Ok(recipe)) => recipe,
            Some(Err(reason)) => {
                self.status = reason;
                return;
            }
            None => {
                self.status = "Pick a recipe first.".into();
                return;
            }
        };
        let dir = recipes_dir();
        let target = dir.join(format!("{}.json", recipe.id));
        let result = std::fs::create_dir_all(&dir)
            .map_err(|e| e.to_string())
            .and_then(|_| write_atomic(&target, &format!("{}\n", recipe.to_json())));
        match result {
            Ok(()) => {
                self.status = format!("Saved “{}” with your recipes.", recipe.name);
                self.refresh_recipes();
                self.ext.recipe_sel = self.ext.recipes.iter().position(|(r, _)| r.id == recipe.id);
                self.update_recipe_diff();
            }
            Err(err) => self.status = format!("Could not save the remix: {err}"),
        }
    }

    // ---- wallpaper gallery ---------------------------------------------------------------

    /// First open of the Wallpapers page: the catalog, what is remembered, the preview pictures,
    /// and (on a worker) the attached displays.
    fn open_gallery(&mut self) {
        if !self.ext.wall_loaded {
            self.ext.wall_loaded = true;
            self.ext.walls = crate::wallpaper::catalog();
            self.ext.wall_previews = super::super::gallery::load_previews(&self.ext.walls);
        }
        self.ext.wall_state = super::super::gallery::load_state();
        if let Some(index) = self
            .ext
            .wall_state
            .applied
            .as_ref()
            .and_then(|applied| self.ext.walls.iter().position(|w| w.id == applied.id))
        {
            self.ext.wall_sel = index;
        }
        super::super::gallery::refresh_displays();
        self.watch_updates();
    }

    pub fn select_wall(&mut self, index: usize) {
        if index < self.ext.walls.len() {
            self.ext.wall_sel = index;
            if let Some(entry) = self.ext.walls.get(index) {
                self.status = format!("{}. {}", entry.name, entry.description);
            }
        }
    }

    pub fn click_wall_grid(&mut self, lw: i32, lx: i32, local: i32) {
        for index in 0..self.ext.walls.len() {
            let (l, t, r, b) = wall_rect(lw, index);
            if lx >= l && lx < r && local >= t && local < b {
                self.select_wall(index);
                self.ext.focus_sub = index;
                return;
            }
        }
    }

    fn selected_wall(&self) -> Option<crate::wallpaper::Entry> {
        self.ext.walls.get(self.ext.wall_sel).cloned()
    }

    fn wall_target(&self) -> Option<String> {
        match self.ext.wall_display {
            0 => None,
            n => super::super::gallery::displays()
                .get(n - 1)
                .map(|d| d.id.clone()),
        }
    }

    fn wall_action(&mut self, id: Button) {
        use super::super::gallery;
        let Some(entry) = self.selected_wall() else {
            return;
        };
        match id {
            Button::WallDownload => {
                gallery::download(&entry);
                self.status = format!(
                    "Downloading {} ({}). Nothing is applied until you press Apply.",
                    entry.name,
                    crate::wallpaper::size_text(entry.bytes)
                );
            }
            Button::WallCancel => {
                gallery::cancel();
                self.status = "Cancelling the download.".into();
            }
            Button::WallApply => {
                let fit = self.ext.wall_state.fit_for(&entry);
                gallery::apply(&entry, self.wall_target(), fit);
                self.status = format!("Applying {}…", entry.name);
            }
            Button::WallPutBack => {
                gallery::put_back();
                self.status = "Putting the earlier wallpaper back…".into();
            }
            Button::WallOpenFolder => {
                self.status = match gallery::open_location(&entry) {
                    Ok(()) => "Opened the folder.".into(),
                    Err(message) => message,
                };
            }
            Button::WallPreview => {
                let fit = self.ext.wall_state.fit_for(&entry);
                super::super::wallpreview::open(self.hwnd, &entry, fit);
            }
            _ => {}
        }
        self.watch_updates();
    }

    /// Enter in the full-screen preview.
    pub fn wall_apply_selected(&mut self) {
        self.wall_action(Button::WallApply);
    }

    /// Compatibility packs: pick one, read what it does and what it cannot, install or remove it.
    fn push_packs(&self, b: &mut Builder) {
        b.push(Row::Header("Packs"));
        b.push(Row::Note(
            "A pack is a small data file that tells AllBlack11 how to treat one app, and says plainly what it cannot do. It adds ordinary rules; it runs nothing. Your own rules always win over a pack.",
        ));
        b.push(Row::About(vec![
            "Recommended templates: none yet.".to_string(),
            "The two built-in packs are experiments. They only ask Windows to color the window frame, and no visible effect has been shown. Use Check now to see where the request stops.".to_string(),
        ]));
        b.push(Row::Choice(Choice::Pack));
        if let Some((pack, imported)) = self.ext.packs.get(self.ext.pack_sel) {
            let installed = crate::pack_is_installed(&self.config, pack);
            let mut lines = vec![
                format!(
                    "{} {} · {} · {}",
                    pack.name,
                    pack.version,
                    pack.verdict.label(),
                    if installed {
                        "installed"
                    } else {
                        "not installed"
                    }
                ),
                format!(
                    "Source: {}",
                    if *imported {
                        "imported from a file on this PC"
                    } else {
                        "built into AllBlack11"
                    }
                ),
                format!("Applies to: {}", crate::pack_scope_text(pack)),
            ];
            if !pack.description.is_empty() {
                lines.push(pack.description.clone());
            }
            if !pack.reason.is_empty() {
                lines.push(pack.reason.clone());
            }
            let actions = crate::pack_look_actions(&pack.look);
            if actions.is_empty() {
                lines.push("Changes: nothing.".to_string());
            } else {
                lines.push(format!("Changes: {}.", actions.join(", ")));
            }
            for limit in &pack.limits {
                lines.push(format!("Limit: {limit}"));
            }
            for note in &pack.notes {
                lines.push(format!("Note: {note}"));
            }
            for tested in &pack.tested {
                lines.push(format!("Tested: {tested}"));
            }
            if let Some(reason) = crate::pack_unavailable(pack, self.build) {
                lines.push(format!("Unavailable: {reason}"));
            }
            let changes = crate::packs::delta(&self.config, pack);
            lines.push(if changes.is_empty() {
                "Compared with your default look: it asks for exactly what is already in force, so no visible change is possible from this pack.".to_string()
            } else {
                format!(
                    "Compared with your default look, it would change: {}.",
                    changes.join("; ")
                )
            });
            b.push(Row::Lines(lines));
            if !self.ext.pack_check.is_empty() {
                b.push(Row::Lines(self.ext.pack_check.clone()));
            }
        } else {
            b.push(Row::Note("No packs are available."));
        }
        if !self.ext.pack_report.is_empty() {
            b.push(Row::Lines(self.ext.pack_report.clone()));
        }
        b.push(Row::Buttons(vec![
            Button::PackInstall,
            Button::PackRemove,
            Button::ImportPack,
        ]));
        let mut check = vec![Button::PackCheck];
        if self.ext.pack_found {
            check.push(Button::PackSeesChange);
            check.push(Button::PackSeesNothing);
        }
        b.push(Row::Buttons(check));
        b.push(Row::Action(Button::PackOpenColors));
        b.push(Row::Note(
            "Install and Remove are saved like any other change. They appear under Look history and Undo takes them back.",
        ));
    }

    /// Built-in packs first, then valid files from the packs folder. A file that fails the same
    /// validation the built-ins pass is left out, never half-used.
    pub fn refresh_packs(&mut self) {
        let mut list: Vec<(crate::Pack, bool)> = crate::pack_catalog()
            .into_iter()
            .map(|pack| (pack, false))
            .collect();
        if let Ok(entries) = std::fs::read_dir(packs_dir()) {
            for entry in entries.flatten().take(40) {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                if let Ok(pack) = crate::parse_pack(&text) {
                    if !list.iter().any(|(existing, _)| existing.id == pack.id) {
                        list.push((pack, true));
                    }
                }
            }
        }
        self.ext.packs = list;
        if self.ext.pack_sel >= self.ext.packs.len() {
            self.ext.pack_sel = 0;
        }
    }

    fn marks_path() -> std::path::PathBuf {
        config_dir().join("pack-marks.json")
    }

    /// Record what the person saw on the window the last check found.
    fn mark_pack(&mut self, mark: Option<crate::packs::Mark>) {
        let Some((pack, _)) = self.ext.packs.get(self.ext.pack_sel).cloned() else {
            return;
        };
        self.ext.pack_marks.set(&pack.id, mark);
        let _ = write_atomic(&Self::marks_path(), &self.ext.pack_marks.to_json());
        self.check_selected_pack();
    }

    /// Walk the chain for the selected pack against the windows open right now: found, matched,
    /// winning, applied, accepted. Reads only; nothing is changed or applied.
    fn check_selected_pack(&mut self) {
        let Some((pack, _)) = self.ext.packs.get(self.ext.pack_sel).cloned() else {
            return;
        };
        if self.ext.pack_marks.marks.is_empty() {
            if let Ok(text) = std::fs::read_to_string(Self::marks_path()) {
                self.ext.pack_marks = crate::packs::Marks::parse(&text);
            }
        }
        let observed = self.observe_pack(&pack);
        let mark = self.ext.pack_marks.get(&pack.id);
        let (status, sentence) = crate::packs::assess(observed.as_ref(), mark);
        self.ext.pack_found = observed.is_some();
        let mut lines = vec![format!("Check: {}", status.label()), sentence];
        if !crate::pack_is_installed(&self.config, &pack) {
            lines.push("The pack is not installed, so no rule of it exists yet.".to_string());
        }
        self.ext.pack_check = lines;
        self.status = format!("{}: {}.", pack.name, status.label());
    }

    fn observe_pack(&self, pack: &crate::Pack) -> Option<crate::packs::Observed> {
        for entry in inspect::list_windows().into_iter().take(300) {
            let class_ok = pack.targets.iter().any(|t| {
                t.class
                    .as_deref()
                    .map_or(true, |c| crate::glob_match(c, &entry.class))
                    && t.process
                        .as_deref()
                        .map_or(true, |p| crate::glob_match(p, &entry.exe))
            });
            if !class_ok {
                continue;
            }
            let hwnd = entry.key as isize as HWND;
            let (path, package) = super::super::procinfo::path_and_package_of_window(
                hwnd, entry.pid, &entry.exe, true,
            );
            let target_ok = pack.targets.iter().any(|t| {
                t.package.as_deref().map_or(true, |p| {
                    !package.is_empty() && crate::glob_match(p, &package)
                })
            });
            if !target_ok {
                continue;
            }
            let identity = crate::WindowIdentity {
                exe: &entry.exe,
                class: &entry.class,
                path: &path,
                package: &package,
                title: None,
            };
            let ranked = crate::rank(&self.config.rules, &identity);
            let pack_rule = |index: usize| {
                self.config
                    .rules
                    .get(index)
                    .is_some_and(|rule| crate::packs::owns(rule, &pack.name))
            };
            let rule_matched = ranked.iter().any(|m| pack_rule(m.index));
            let winner = ranked.first();
            let report = super::super::app::report_for(entry.key);
            let rejected: Vec<String> = report
                .as_ref()
                .map(|r| {
                    r.attempts
                        .iter()
                        .filter_map(|a| match a.confirm {
                            crate::Confirm::Rejected(hr) => {
                                Some(format!("{} (0x{:08X})", a.name, hr as u32))
                            }
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default();
            return Some(crate::packs::Observed {
                exe: entry.exe.clone(),
                class: entry.class.clone(),
                package,
                excluded: crate::exclusion_why(&entry.exe, &entry.class, &self.config),
                rule_matched,
                pack_wins: winner.is_some_and(|m| pack_rule(m.index)),
                winner: winner
                    .and_then(|m| self.config.rules.get(m.index))
                    .map(|rule| rule.label())
                    .unwrap_or_default(),
                applied: report.as_ref().is_some_and(|r| r.explanation.applied),
                rejected,
            });
        }
        None
    }

    fn install_selected_pack(&mut self) {
        if self.trial_active() {
            self.status = "Keep or revert the trial look first.".into();
            return;
        }
        let Some((pack, _)) = self.ext.packs.get(self.ext.pack_sel).cloned() else {
            self.status = "No pack is selected.".into();
            return;
        };
        match crate::install_pack(&mut self.config, &pack, self.build) {
            Ok(lines) => {
                self.ext.pack_report = lines;
                self.rule = None;
                self.status = if pack.verdict == crate::PackVerdict::Experimental {
                    format!(
                        "Installed the {} pack as an experiment. It may show no visible change: press Check now. Undo takes it back.",
                        pack.name
                    )
                } else {
                    format!("Installed the {} pack. Undo takes it back.", pack.name)
                };
                self.commit();
            }
            Err(reason) => {
                self.ext.pack_report = vec![reason.clone()];
                self.status = format!("{} was not installed. {reason}", pack.name);
            }
        }
    }

    /// Remove a pack's rules; for an imported pack that is not installed, forget the file instead.
    fn remove_selected_pack(&mut self) {
        if self.trial_active() {
            self.status = "Keep or revert the trial look first.".into();
            return;
        }
        let Some((pack, imported)) = self.ext.packs.get(self.ext.pack_sel).cloned() else {
            self.status = "No pack is selected.".into();
            return;
        };
        let removed = crate::remove_pack(&mut self.config, &pack.name);
        if removed > 0 {
            self.ext.pack_report = vec![format!(
                "Removed the {removed} rule(s) added by the {} pack. Your own rules were not touched.",
                pack.name
            )];
            self.rule = None;
            self.status = format!("Removed the {} pack. Undo takes it back.", pack.name);
            self.commit();
        } else if imported {
            let file = packs_dir().join(format!("{}.json", pack.id));
            self.status = match std::fs::remove_file(&file) {
                Ok(()) => format!("Deleted the imported {} pack file.", pack.name),
                Err(err) => format!("Could not delete the pack file: {err}"),
            };
            self.ext.pack_report.clear();
            self.refresh_packs();
        } else {
            self.status = format!("The {} pack is not installed.", pack.name);
        }
    }

    fn import_pack(&mut self) {
        let Some(path) = file_dialog(self.hwnd, false, "Import a compatibility pack", "pack.json")
        else {
            return;
        };
        let text = match std::fs::metadata(&path) {
            Ok(meta) if meta.len() as usize > crate::PACK_MAX_BYTES => {
                self.status = format!(
                    "That file is larger than {} KB, which is more than any pack needs.",
                    crate::PACK_MAX_BYTES / 1024
                );
                return;
            }
            Ok(_) => match std::fs::read_to_string(&path) {
                Ok(text) => text,
                Err(err) => {
                    self.status = format!("Could not read the file: {err}");
                    return;
                }
            },
            Err(err) => {
                self.status = format!("Could not read the file: {err}");
                return;
            }
        };
        match crate::parse_pack(&text) {
            Ok(pack) => {
                let dir = packs_dir();
                let target = dir.join(format!("{}.json", pack.id));
                let body = serde_json::to_string_pretty(&pack).unwrap_or_default();
                let result = std::fs::create_dir_all(&dir)
                    .map_err(|e| e.to_string())
                    .and_then(|_| write_atomic(&target, &format!("{body}\n")));
                match result {
                    Ok(()) => {
                        self.refresh_packs();
                        if let Some(index) =
                            self.ext.packs.iter().position(|(p, _)| p.id == pack.id)
                        {
                            self.ext.pack_sel = index;
                        }
                        let preview = crate::preview_pack(&self.config, &pack, self.build);
                        self.ext.pack_report = std::iter::once(
                            "Imported. Nothing is installed yet. Installing would:".to_string(),
                        )
                        .chain(preview)
                        .collect();
                        self.status =
                            format!("Imported the {} pack. Review it, then Install.", pack.name);
                    }
                    Err(err) => self.status = format!("Could not save the pack: {err}"),
                }
            }
            Err(err) => self.status = format!("Not a valid pack: {err}"),
        }
    }

    /// The Updates card: what is running, what is known, and the few things the user can do.
    fn push_updates(&self, b: &mut Builder) {
        let phase = updater::phase();
        b.push(Row::Header("Updates"));
        b.push(Row::About(crate::update::status_lines(
            &phase,
            updater::current_version(),
        )));
        let notes = match &phase {
            crate::update::Phase::Available(c) | crate::update::Phase::Skipped(c) => {
                c.notes.clone()
            }
            crate::update::Phase::Ready { candidate, .. } => candidate.notes.clone(),
            _ => String::new(),
        };
        if !notes.is_empty() {
            let mut lines = vec!["What is in this release:".to_string()];
            lines.extend(notes.lines().map(str::to_string));
            b.push(Row::Lines(lines));
        }
        b.push(Row::Choice(Choice::UpdateChannel));
        b.push(Row::Toggle(Toggle::AutoUpdateCheck));
        let buttons = match phase {
            crate::update::Phase::Available(_) => {
                vec![
                    Button::DownloadUpdate,
                    Button::SkipUpdate,
                    Button::OpenReleasePage,
                ]
            }
            crate::update::Phase::Skipped(_) => {
                vec![
                    Button::UnskipUpdate,
                    Button::CheckUpdates,
                    Button::OpenReleasePage,
                ]
            }
            crate::update::Phase::Downloading { .. } => vec![Button::CancelUpdate],
            crate::update::Phase::Ready { .. } => {
                vec![
                    Button::InstallUpdate,
                    Button::DiscardUpdate,
                    Button::OpenReleasePage,
                ]
            }
            crate::update::Phase::Checking => vec![Button::OpenReleasePage],
            _ => vec![Button::CheckUpdates, Button::OpenReleasePage],
        };
        b.push(Row::Buttons(buttons));
        b.push(Row::Note(
            "Install opens the downloaded Setup and closes AllBlack11 so it can be replaced; your settings are kept. To go back, install an older release from the release page.",
        ));
    }

    /// Rule playground, typed: the text in the rule box is read, never applied.
    fn test_typed_rule(&mut self) {
        match crate::playground_parse(&get_text(self.edits.rule)) {
            Ok(subject) => {
                self.ext.playground = crate::playground_evaluate(
                    &self.config,
                    &subject,
                    registry::accent_bgr() & 0x00FF_FFFF,
                );
                self.status = "Tested. Nothing was applied.".into();
            }
            Err(message) => self.status = message,
        }
    }

    /// Rule playground, with the window picked on Fix this app (its program and class only).
    fn test_picked_window(&mut self) {
        let Some(entry) = self.ext.selected.clone() else {
            self.status =
                "Nothing is picked. Pick a window under Customize an app, then test it here."
                    .into();
            return;
        };
        let subject = crate::PlaygroundSubject {
            exe: entry.exe,
            class: entry.class,
            ..crate::PlaygroundSubject::default()
        };
        self.ext.playground = crate::playground_evaluate(
            &self.config,
            &subject,
            registry::accent_bgr() & 0x00FF_FFFF,
        );
        self.status = "Tested the picked window. Nothing was applied.".into();
    }

    pub fn toggle_ext(&mut self, id: Toggle) {
        match id {
            Toggle::FocusLook => self.config.focus.enabled = !self.config.focus.enabled,
            Toggle::IdentityAccents => {
                self.config.features.identity_accents = !self.config.features.identity_accents;
                self.status = if self.config.features.identity_accents {
                    "Identity accents are on. Each app gets its own border color unless a rule sets one. Focus Look, if on, decides the border instead.".into()
                } else {
                    "Identity accents are off.".into()
                };
            }
            Toggle::AutoUpdateCheck => {
                self.config.updates.auto_check = !self.config.updates.auto_check;
                self.status = if self.config.updates.auto_check {
                    "AllBlack11 will ask GitHub once a day whether a newer release exists. Nothing is downloaded without you.".into()
                } else {
                    "Automatic update checks are off.".into()
                };
            }
            Toggle::PaletteHotkey => {
                self.config.features.palette_hotkey = !self.config.features.palette_hotkey;
                self.status = if self.config.features.palette_hotkey {
                    "Ctrl+Alt+Space now opens the command palette.".into()
                } else {
                    "The palette hotkey is off.".into()
                };
            }
            Toggle::DayNight => {
                self.config.features.day_night.enabled = !self.config.features.day_night.enabled;
                self.status = if self.config.features.day_night.enabled {
                    "Day / Night is on. The scene for the current time is applied now and saved."
                        .into()
                } else {
                    "Day / Night is off. The current look stays as it is.".into()
                };
            }
            Toggle::ReduceBattery => {
                self.config.reduce_effects.on_battery_saver =
                    !self.config.reduce_effects.on_battery_saver
            }
            Toggle::ReduceRemote => {
                self.config.reduce_effects.on_remote_session =
                    !self.config.reduce_effects.on_remote_session
            }
            Toggle::ReduceTransparency => {
                self.config.reduce_effects.on_transparency_off =
                    !self.config.reduce_effects.on_transparency_off
            }
            Toggle::EmulatedX64 => {
                self.config.legacy.emulated_x64 = !self.config.legacy.emulated_x64;
                self.status = if self.config.legacy.emulated_x64 {
                    "Emulated x64 coverage is experimental and needs the x64 helper that ships in the ARM64 package.".into()
                } else {
                    "Emulated x64 apps will be skipped.".into()
                };
            }
            Toggle::ShowTitles => {
                // A display option only: nothing is saved. Titles are never written to disk.
                self.ext.show_titles = !self.ext.show_titles;
                self.refresh_report();
                return;
            }
            _ => return,
        }
        self.commit();
    }

    pub fn cycle_ext(&mut self, id: Choice) {
        match id {
            Choice::Palette => {
                let next = if self.config.legacy.palette == "black" {
                    "graphite"
                } else {
                    "black"
                };
                self.config.legacy.palette = next.to_string();
                self.commit();
            }
            Choice::RuleScope => {
                self.ext.rule_scope = (self.ext.rule_scope + 1) % 4;
            }
            Choice::ExcludedClass => {
                let count = self.config.exclude_classes.len();
                if count > 0 {
                    self.ext.class_sel = (self.ext.class_sel + 1) % count;
                }
            }
            Choice::RemixWith => {
                let count = self.ext.recipes.len().max(1);
                let current = self.remix_donor().unwrap_or(0);
                self.ext.remix_with = (current + 1) % count;
                if Some(self.ext.remix_with) == self.ext.recipe_sel {
                    self.ext.remix_with = (self.ext.remix_with + 1) % count;
                }
            }
            Choice::RemixPart => self.ext.remix_part = self.ext.remix_part.next(),
            Choice::WallDisplay => {
                let count = super::super::gallery::displays().len() + 1;
                self.ext.wall_display = (self.ext.wall_display + 1) % count;
            }
            Choice::WallFit => {
                if let Some(entry) = self.selected_wall() {
                    let next = self.ext.wall_state.fit_for(&entry).next();
                    self.ext.wall_state.set_fit(&entry.id, next);
                    super::super::gallery::save_state(&self.ext.wall_state);
                    self.status = format!("{}: {}.", entry.name, next.label());
                }
            }
            Choice::Pack => {
                if !self.ext.packs.is_empty() {
                    self.ext.pack_sel = (self.ext.pack_sel + 1) % self.ext.packs.len();
                }
                self.ext.pack_report.clear();
                self.ext.pack_check.clear();
                self.ext.pack_found = false;
            }
            Choice::UpdateChannel => {
                self.config.updates.channel = self.config.updates.channel.next();
                self.status = format!("Update channel: {}.", self.config.updates.channel.label());
                self.commit();
            }
            Choice::AppAppearance => {
                self.config.ui.appearance = self.config.ui.appearance.next();
                self.status = format!(
                    "AllBlack11 now uses: {}. This changes only its own windows.",
                    self.config.ui.appearance.label()
                );
                self.commit();
                let hwnd = self.hwnd;
                self.refresh_theme(hwnd);
            }
            Choice::DayScene | Choice::NightScene => {
                let ids = self.scene_ids();
                let current = if id == Choice::DayScene {
                    self.config.features.day_night.day_scene.clone()
                } else {
                    self.config.features.day_night.night_scene.clone()
                };
                let next = match ids.iter().position(|candidate| *candidate == current) {
                    Some(index) => ids[(index + 1) % ids.len()].clone(),
                    None => ids.first().cloned().unwrap_or(current),
                };
                if id == Choice::DayScene {
                    self.config.features.day_night.day_scene = next;
                } else {
                    self.config.features.day_night.night_scene = next;
                }
                self.commit();
            }
            Choice::DayStarts | Choice::NightStarts => {
                let setting = &mut self.config.features.day_night;
                let field = if id == Choice::DayStarts {
                    &mut setting.day_starts
                } else {
                    &mut setting.night_starts
                };
                let minutes = crate::parse_schedule_times(field, "00:00")
                    .map(|t| t.day_starts)
                    .unwrap_or(0);
                let next = (minutes + 30) % 1440;
                *field = format!("{:02}:{:02}", next / 60, next % 60);
                // Equal start times are not allowed; nudge the one just changed.
                if setting.day_starts == setting.night_starts {
                    let bump = (next + 30) % 1440;
                    let text = format!("{:02}:{:02}", bump / 60, bump % 60);
                    if id == Choice::DayStarts {
                        setting.day_starts = text;
                    } else {
                        setting.night_starts = text;
                    }
                }
                self.commit();
            }
            _ => {}
        }
    }

    /// Ids a Day/Night choice can cycle through: the five scenes, then saved looks.
    fn scene_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = crate::scene_catalog().into_iter().map(|s| s.id).collect();
        ids.extend(
            super::super::cfgio::load_lookbook()
                .looks
                .into_iter()
                .map(|l| l.id),
        );
        ids
    }

    /// Display name for a Day/Night scene id.
    pub fn scene_name_for(&self, id: &str) -> String {
        if let Some(scene) = crate::scene_catalog().into_iter().find(|s| s.id == id) {
            return scene.name;
        }
        if let Some(look) = self.ext.book.get(id) {
            return look.name.clone();
        }
        format!("{id} (missing)")
    }

    /// Returns true when the action was handled here.
    pub fn run_button_ext(&mut self, id: Button) -> bool {
        if self.setup_button(id) {
            return true;
        }
        match id {
            Button::PickWindow => {
                self.status = "Press the button, drag onto a window, and let go.".into();
            }
            Button::RefreshWindows => {
                self.refresh_windows();
                self.refresh_report();
                self.status = format!("{} windows.", self.ext.windows.len());
            }
            Button::CreateRule => self.create_rule_from_window(),
            Button::TryLook => self.try_look_on_window(),
            Button::ExcludeApp => self.exclude_selected_app(),
            Button::RestoreWindow => {
                if let Some(selected) = &self.ext.selected {
                    cmdq::push(AppCommand::RestoreWindow {
                        key: selected.key,
                        fingerprint: selected.fingerprint.clone(),
                    });
                    self.post_command();
                    self.schedule_report_refresh();
                    self.status =
                        "Restoring that window. It stays as Windows draws it until you reload."
                            .into();
                }
            }
            Button::CopyReport => self.copy_report(),
            Button::OpenPalette => palette_ui::open(self.owner),
            Button::GoFix => self.set_page(self.hwnd, Page::AppsFix),
            Button::GoScenes => self.set_page(self.hwnd, Page::AppearanceScenes),
            Button::GoHelp => self.set_page(self.hwnd, Page::HelpTopics),
            Button::NextAccent | Button::ClearAccent => self.change_accent(id),
            Button::PaletteFromImage => self.palette_from_image(),
            Button::TryScene => self.try_scene(),
            Button::SaveLook => self.save_look(),
            Button::RenameLook => self.rename_look(),
            Button::DeleteLook => self.delete_look(),
            Button::MarkCompare => self.mark_compare(),
            Button::PauseAll => {
                cmdq::push(AppCommand::Pause);
                self.post_command();
                self.status = "Paused. Every window is going back to how Windows draws it.".into();
            }
            Button::ResumeAll => {
                cmdq::push(AppCommand::Resume);
                self.post_command();
                self.status = "Resuming.".into();
            }
            Button::LeaveSafe => {
                cmdq::push(AppCommand::LeaveSafeMode);
                self.post_command();
                self.status = "Leaving safe mode. Your saved settings are applied.".into();
            }
            Button::StopExperimental => {
                self.config.experimental_dark_legacy = false;
                self.config.legacy.emulated_x64 = false;
                self.status =
                    "Experimental features are off. The legacy layer and helper are stopping."
                        .into();
                self.commit();
            }
            Button::MarkWorks => self.mark_compat(crate::CompatStatus::Verified),
            Button::MarkPartial => self.mark_compat(crate::CompatStatus::Partial),
            Button::MarkFails => self.mark_compat(crate::CompatStatus::Unsupported),
            Button::MarkReset => self.mark_reset(),
            Button::RaisePriority => self.bump_priority(1),
            Button::LowerPriority => self.bump_priority(-1),
            Button::PreferAppDefaults => self.prefer_app_defaults(),
            Button::TryRecipe => self.try_recipe(),
            Button::ImportRecipe => self.import_recipe(),
            Button::RemoveExcludedClass => {
                match crate::remove_excluded_class(&mut self.config, self.ext.class_sel) {
                    Some(class) => {
                        self.status = format!("Window class {class} is styled again.");
                        self.ext.class_sel = 0;
                        self.commit();
                    }
                    None => self.status = "Pick a window class first.".into(),
                }
            }
            Button::WallDownload
            | Button::WallCancel
            | Button::WallApply
            | Button::WallPreview
            | Button::WallOpenFolder
            | Button::WallPutBack => self.wall_action(id),
            Button::TryRemix => self.try_remix(),
            Button::SaveRemix => self.save_remix(),
            Button::PackCheck => self.check_selected_pack(),
            Button::PackSeesChange => self.mark_pack(Some(crate::packs::Mark::SawChange)),
            Button::PackSeesNothing => self.mark_pack(Some(crate::packs::Mark::SawNothing)),
            Button::PackOpenColors => {
                self.status = match registry::open_windows_colors() {
                    Ok(()) => "Opened Windows color settings.".into(),
                    Err(message) => message,
                };
            }
            Button::PackInstall => self.install_selected_pack(),
            Button::PackRemove => self.remove_selected_pack(),
            Button::ImportPack => self.import_pack(),
            Button::ExportRecipe => self.export_recipe(),
            Button::UndoLook => self.undo_look(),
            Button::CheckUpdates => {
                updater::check(true, self.config.updates.channel);
                self.watch_updates();
                self.status = "Checking GitHub…".into();
            }
            Button::DownloadUpdate => {
                updater::download();
                self.watch_updates();
                self.status = "Downloading. Nothing is installed until you choose.".into();
            }
            Button::CancelUpdate => {
                updater::cancel();
                self.status = "Cancelling the download.".into();
            }
            Button::InstallUpdate => match updater::install() {
                Ok(()) => {
                    self.status =
                        "Starting Setup. AllBlack11 is closing so it can be replaced.".into();
                    unsafe { PostMessageW(self.owner, WM_CLOSE, 0, 0) };
                }
                Err(message) => self.status = message,
            },
            Button::DiscardUpdate => {
                updater::discard();
                self.status = "The downloaded installer was deleted.".into();
            }
            Button::SkipUpdate => {
                updater::skip();
                self.status =
                    "That version is skipped. You can stop skipping it under Settings > Updates."
                        .into();
            }
            Button::UnskipUpdate => updater::unskip(),
            Button::OpenReleasePage => {
                let url = match updater::phase() {
                    crate::update::Phase::Available(c) | crate::update::Phase::Skipped(c) => {
                        c.page_url
                    }
                    crate::update::Phase::Ready { candidate, .. } => candidate.page_url,
                    _ => crate::update::releases_page_url(),
                };
                self.status = match registry::open_release_url(&url) {
                    Ok(()) => "Opened the release page in your browser.".into(),
                    Err(err) => format!("Could not open the link: {err}. The address is {url}."),
                };
            }
            Button::GoUpdates => {
                let hwnd = self.hwnd;
                self.set_page(hwnd, Page::Help);
                self.scroll_to_header("Updates");
            }
            Button::TestRule => self.test_typed_rule(),
            Button::TestPicked => self.test_picked_window(),
            Button::RestoreLkg => {
                cmdq::push(AppCommand::RestoreLastKnownGood);
                self.post_command();
                self.status = "Restoring the last known good config.".into();
            }
            Button::ClearNotebook => {
                let mut book = super::super::cfgio::load_notebook();
                book.clear();
                self.status = match super::super::cfgio::save_notebook(&book) {
                    Ok(()) => "Notebook cleared.".into(),
                    Err(err) => err,
                };
            }
            Button::ResetAllWindows => {
                unsafe { PostMessageW(self.owner, WM_RUN_COMMAND, CMD_RESET_ALL, 0) };
                self.status = "Every themed window was put back until you reload.".into();
            }
            Button::UndoShell => {
                self.config.shell.oled_taskbar = false;
                self.config.shell.black_shell = false;
                self.status = "Shell changes are being undone from the saved originals.".into();
                self.commit();
            }
            Button::DemoCompare => {
                // The selected scene or look if there is one; otherwise the look being edited.
                let (style, name) = match self
                    .ext
                    .look_sel
                    .and_then(|i| self.look_at(i).map(|item| (i, item)))
                {
                    Some((index, item)) => (
                        item.recipe().look.merge_onto(crate::base_style()),
                        self.look_name(index),
                    ),
                    None => (self.current_style(), "your current look".to_string()),
                };
                super::super::demo::open_pair(style, self.accent_bgr_for_demo(), self.build, &name);
                self.status = "Opened two windows to compare. The left one is untouched.".into();
            }
            Button::DemoMaterials => {
                super::super::demo::open(
                    self.current_style(),
                    self.accent_bgr_for_demo(),
                    self.build,
                );
                self.status = "Opened the demonstration window.".into();
            }
            Button::OpenHarness => {
                self.status = match super::super::legacy::hook_dll_path("AllBlack11-Harness.exe") {
                    Some(path) if path.exists() => match registry::open_file(&path) {
                        Ok(()) => "Opened the control harness.".into(),
                        Err(err) => err,
                    },
                    _ => "AllBlack11-Harness.exe is not in the install folder. It ships in the portable zip.".into(),
                };
            }
            Button::RemoveRule => {
                // Share the original behavior; handled by the caller's match.
                return false;
            }
            _ => return false,
        }
        true
    }

    fn create_rule_from_window(&mut self) {
        let Some(selected) = self.ext.selected.clone() else {
            return;
        };
        let package = self
            .ext
            .report
            .as_ref()
            .map(|r| r.package.clone())
            .unwrap_or_default();
        let mut rule = Rule::for_process("");
        match self.ext.rule_scope {
            0 => rule.process = selected.exe.clone(),
            1 => {
                rule.process = selected.exe.clone();
                rule.class = Some(selected.class.clone());
            }
            2 if !package.is_empty() => rule.package = Some(package),
            2 => rule.process = selected.exe.clone(),
            _ => rule.class = Some(selected.class.clone()),
        }
        let same = |other: &Rule| {
            other.process.eq_ignore_ascii_case(&rule.process)
                && other.class == rule.class
                && other.package == rule.package
                && other.path.is_none()
                && other.title.is_none()
        };
        if let Some(index) = self.config.rules.iter().position(same) {
            self.rule = Some(index);
            self.status = "A rule for that already exists. It is selected.".into();
        } else {
            rule.style.enabled = Some(true);
            rule.name = Some(selected.exe.clone());
            self.config.rules.push(rule);
            self.rule = Some(self.config.rules.len() - 1);
            self.status =
                "Rule created. Edit its look under Appearance > Effects and colors.".into();
        }
        self.push_colors_to_edits();
        self.commit();
        self.refresh_report();
    }

    fn try_look_on_window(&mut self) {
        let Some(selected) = &self.ext.selected else {
            return;
        };
        cmdq::push(AppCommand::TryOnWindow {
            key: selected.key,
            fingerprint: selected.fingerprint.clone(),
            style: self.current_style(),
        });
        self.post_command();
        self.schedule_report_refresh();
        self.status = "Trying the look from Effects and colors on that window for 20 seconds. Nothing is saved.".into();
    }

    /// Re-read the selected window's report shortly, once the host has acted.
    fn schedule_report_refresh(&mut self) {
        self.ext.refresh_pending = true;
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::SetTimer(self.hwnd, TIMER_UI, 400, None)
        };
    }

    fn exclude_selected_app(&mut self) {
        let Some(selected) = &self.ext.selected else {
            return;
        };
        let exe = selected.exe.clone();
        if !self
            .config
            .exclude
            .iter()
            .any(|item| item.eq_ignore_ascii_case(&exe))
        {
            self.config.exclude.push(exe.clone());
        }
        self.status = format!("{exe} will no longer be themed.");
        self.commit();
        self.refresh_report();
    }

    fn copy_report(&mut self) {
        let Some(report) = self.ext.report.clone() else {
            return;
        };
        let labels: Vec<String> = self.config.rules.iter().map(|r| r.label()).collect();
        let (user, profile) = user_strings();
        let mut lines =
            crate::render_report(&report, &labels, self.ext.show_titles, &user, &profile);
        lines.push(String::new());
        lines.push("Style receipt".to_string());
        lines.extend(crate::receipt_lines(&report));
        let mut text = lines.join("\r\n");
        if !self.ext.show_titles {
            // One more pass over everything, receipt included: no user name, no profile path.
            text = crate::redact(&text, &user, &profile);
        }
        self.status = if set_clipboard_text(self.hwnd, &text) {
            if self.ext.show_titles {
                "Copied, including the window title.".into()
            } else {
                "Copied. The title and your user name are left out.".into()
            }
        } else {
            "Could not open the clipboard.".into()
        };
    }

    fn compat_key(&self) -> Option<(String, u32, String)> {
        let report = self.ext.report.as_ref()?;
        let key = if report.package.is_empty() {
            report.exe.to_ascii_lowercase()
        } else {
            report.package.clone()
        };
        Some((key, report.build, report.arch.clone()))
    }

    fn mark_compat(&mut self, status: crate::CompatStatus) {
        let Some((key, build, arch)) = self.compat_key() else {
            return;
        };
        let mut book = super::super::cfgio::load_notebook();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        book.set(&key, build, &arch, status, "", now);
        self.status = match super::super::cfgio::save_notebook(&book) {
            Ok(()) => format!(
                "Noted: {} on build {build} ({arch}): {}.",
                key,
                status.label()
            ),
            Err(err) => err,
        };
        self.refresh_report();
    }

    fn mark_reset(&mut self) {
        let Some((key, build, arch)) = self.compat_key() else {
            return;
        };
        let mut book = super::super::cfgio::load_notebook();
        book.reset(&key, build, &arch);
        let _ = super::super::cfgio::save_notebook(&book);
        self.status = "Back to not tested.".into();
        self.refresh_report();
    }

    fn bump_priority(&mut self, delta: i32) {
        if let Some(rule) = self.rule.and_then(|index| self.config.rules.get_mut(index)) {
            rule.priority = (rule.priority + delta).clamp(-1000, 1000);
            self.status = format!("Priority {}.", rule.priority);
            self.commit();
        }
    }

    /// "Leave this exactly as the app draws it": the safest possible look for the target.
    fn prefer_app_defaults(&mut self) {
        match self.rule.and_then(|index| self.config.rules.get_mut(index)) {
            Some(rule) => {
                rule.style.enabled = Some(false);
                self.status = "This app keeps the look it draws itself.".into();
            }
            None => {
                self.config.defaults.enabled = Some(false);
                self.status = "Apps without a rule keep the look they draw themselves.".into();
            }
        }
        self.commit();
    }

    // ---- recipes -------------------------------------------------------------------------

    pub fn refresh_recipes(&mut self) {
        let mut list: Vec<(Recipe, Trust)> = crate::bundled_recipes()
            .into_iter()
            .map(|recipe| (recipe, Trust::Bundled))
            .collect();
        if let Ok(entries) = std::fs::read_dir(recipes_dir()) {
            for entry in entries.flatten().take(60) {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                if let Ok(text) = std::fs::read_to_string(&path) {
                    if let Ok(recipe) = crate::parse_recipe(&text) {
                        if !list.iter().any(|(existing, _)| existing.id == recipe.id) {
                            list.push((recipe, Trust::Imported));
                        }
                    }
                }
            }
        }
        self.ext.recipes = list;
        if self
            .ext
            .recipe_sel
            .is_some_and(|i| i >= self.ext.recipes.len())
        {
            self.ext.recipe_sel = None;
        }
        self.update_recipe_diff();
    }

    pub fn click_recipe_list(&mut self, index: i32) {
        if index >= 0 && (index as usize) < self.ext.recipes.len() {
            self.ext.recipe_sel = Some(index as usize);
            self.update_recipe_diff();
        }
    }

    fn update_recipe_diff(&mut self) {
        self.ext.recipe_diff = match self.ext.recipe_sel.and_then(|i| self.ext.recipes.get(i)) {
            Some((recipe, _)) => crate::preview_recipe(&self.config, recipe)
                .iter()
                .map(|c| format!("{}: {} → {}", c.field, c.from, c.to))
                .collect(),
            None => Vec::new(),
        };
    }

    fn try_recipe(&mut self) {
        let Some((recipe, _)) = self
            .ext
            .recipe_sel
            .and_then(|i| self.ext.recipes.get(i))
            .cloned()
        else {
            self.status = "Pick a recipe first.".into();
            return;
        };
        let mut next = self.config.clone();
        crate::apply_recipe(&mut next, &recipe);
        self.begin_trial(&recipe.name, next);
    }

    fn import_recipe(&mut self) {
        let Some(path) = file_dialog(self.hwnd, false, "Import a recipe", "recipe.json") else {
            return;
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) => {
                self.status = format!("Could not read the file: {err}");
                return;
            }
        };
        match crate::parse_recipe(&text) {
            Ok(recipe) => {
                let dir = recipes_dir();
                let target = dir.join(format!("{}.json", recipe.id));
                let result = std::fs::create_dir_all(&dir)
                    .map_err(|e| e.to_string())
                    .and_then(|_| write_atomic(&target, &format!("{}\n", recipe.to_json())));
                match result {
                    Ok(()) => {
                        self.status =
                            format!("Imported “{}”. Review it, then try it.", recipe.name);
                        self.refresh_recipes();
                        self.ext.recipe_sel =
                            self.ext.recipes.iter().position(|(r, _)| r.id == recipe.id);
                        self.update_recipe_diff();
                    }
                    Err(err) => self.status = format!("Could not save the recipe: {err}"),
                }
            }
            Err(err) => self.status = format!("Not a valid recipe: {err}"),
        }
    }

    fn export_recipe(&mut self) {
        let Some(path) = file_dialog(self.hwnd, true, "Export the current look", "my-look.json")
        else {
            return;
        };
        let recipe = crate::export_recipe(&self.config, "my-look", "My look");
        self.status = match write_atomic(
            std::path::Path::new(&path),
            &format!("{}\n", recipe.to_json()),
        ) {
            Ok(()) => "Exported.".into(),
            Err(err) => format!("Could not write the file: {err}"),
        };
    }
}

// ---------------------------------------------------------------------------
// Clipboard and file dialogs
// ---------------------------------------------------------------------------

fn set_clipboard_text(hwnd: HWND, text: &str) -> bool {
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE,
    };
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        if OpenClipboard(hwnd) == 0 {
            return false;
        }
        EmptyClipboard();
        let bytes = wide.len() * 2;
        let handle = GlobalAlloc(GMEM_MOVEABLE, bytes);
        let mut ok = false;
        if !handle.is_null() {
            let target = GlobalLock(handle) as *mut u16;
            if !target.is_null() {
                std::ptr::copy_nonoverlapping(wide.as_ptr(), target, wide.len());
                GlobalUnlock(handle);
                // 13 = CF_UNICODETEXT. After a successful SetClipboardData the system owns the memory.
                ok = !SetClipboardData(13, handle).is_null();
            }
        }
        CloseClipboard();
        ok
    }
}

/// A standard Open or Save dialog. Returns the chosen path.
fn file_dialog(owner: HWND, save: bool, title: &str, default_name: &str) -> Option<String> {
    file_dialog_filtered(
        owner,
        save,
        title,
        default_name,
        "AllBlack11 recipe (*.json)\0*.json\0All files\0*.*\0\0",
    )
}

fn file_dialog_filtered(
    owner: HWND,
    save: bool,
    title: &str,
    default_name: &str,
    filter: &str,
) -> Option<String> {
    use windows_sys::Win32::UI::Controls::Dialogs::{
        GetOpenFileNameW, GetSaveFileNameW, OFN_FILEMUSTEXIST, OFN_OVERWRITEPROMPT,
        OFN_PATHMUSTEXIST, OPENFILENAMEW,
    };
    let mut file = [0u16; 520];
    for (index, unit) in default_name.encode_utf16().take(500).enumerate() {
        file[index] = unit;
    }
    let filter: Vec<u16> = filter.encode_utf16().collect();
    let title = to_wide(title);
    let mut dialog: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    dialog.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    dialog.hwndOwner = owner;
    dialog.lpstrFilter = filter.as_ptr();
    dialog.lpstrFile = file.as_mut_ptr();
    dialog.nMaxFile = file.len() as u32;
    dialog.lpstrTitle = title.as_ptr();
    dialog.Flags = if save {
        OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST
    } else {
        OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST
    };
    let ok = unsafe {
        if save {
            GetSaveFileNameW(&mut dialog)
        } else {
            GetOpenFileNameW(&mut dialog)
        }
    };
    if ok == 0 {
        return None;
    }
    let len = file
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(file.len());
    Some(String::from_utf16_lossy(&file[..len]))
}

#[allow(dead_code)]
fn _support_labels() -> [Support; 1] {
    [Support::Supported]
}

#[allow(dead_code)]
fn _notebook() -> Notebook {
    Notebook::default()
}
