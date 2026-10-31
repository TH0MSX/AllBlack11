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

pub const CMD_RESET_ALL: usize = 4;

#[derive(Default)]
pub struct Ext {
    pub windows: Vec<WindowEntry>,
    pub selected: Option<WindowEntry>,
    pub report: Option<crate::Report>,
    pub report_lines: Vec<String>,
    pub filter: String,
    pub recipes: Vec<(Recipe, Trust)>,
    pub recipe_sel: Option<usize>,
    pub recipe_diff: Vec<String>,
    /// 0 executable, 1 executable + class, 2 package, 3 class only.
    pub rule_scope: u8,
    pub show_titles: bool,
    pub picking: bool,
    pub focus: Option<usize>,
    pub focus_sub: usize,
    pub trial_name: String,
    pub loaded: bool,
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
}

impl Page {
    pub const ALL: [Page; 6] = [
        Page::Overview,
        Page::Effects,
        Page::Apps,
        Page::Scenes,
        Page::Legacy,
        Page::Safety,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Page::Overview => "Overview",
            Page::Effects => "Effects",
            Page::Apps => "Apps",
            Page::Scenes => "Scenes",
            Page::Legacy => "Legacy",
            Page::Safety => "Safety",
        }
    }
}

struct Builder {
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

fn list_height(len: usize) -> i32 {
    (len.max(1) as i32) * LIST_LINE + 16 + GAP
}

// ---------------------------------------------------------------------------
// Rows per page
// ---------------------------------------------------------------------------

impl Ui {
    pub fn rows_for_page(&self) -> Vec<Laid> {
        match self.page {
            Page::Overview => self.rows_overview(),
            Page::Effects => self.rows_effects(),
            Page::Apps => self.rows_apps(),
            Page::Scenes => self.rows_scenes(),
            Page::Legacy => self.rows_legacy(),
            Page::Safety => self.rows_safety(),
        }
    }

    fn rows_overview(&self) -> Vec<Laid> {
        let mut b = Builder::new();
        b.push(Row::Header("Quick looks"));
        b.push(Row::Presets);
        b.push(Row::Note(
            "A quick look runs for 15 seconds first. Keep it, or it goes back by itself.",
        ));
        b.push(Row::Header("Windows"));
        b.push(Row::Toggle(Toggle::Theme));
        b.push(Row::Toggle(Toggle::Autostart));
        b.push(Row::Header("Shell"));
        b.push(Row::Toggle(Toggle::Oled));
        b.push(Row::Toggle(Toggle::Black));
        b.push(Row::Action(Button::RestartExplorer));
        b.push(Row::Header("Right now"));
        let mut now = Vec::new();
        if super::super::app::is_paused() {
            now.push("Paused: every window is as Windows draws it. Resume on the Safety page or the tray menu.".to_string());
        }
        if let Some(text) = super::super::app::safe_mode_text() {
            now.push(format!("Safe mode. {text}"));
        }
        now.push(if self.config.enabled {
            "Window theming is on.".to_string()
        } else {
            "Window theming is off.".to_string()
        });
        match super::super::app::reduce_reason_now() {
            Some(reason) => now.push(format!(
                "Effects are reduced because of {reason}. Colors still apply; backdrop materials do not."
            )),
            None => now.push("Effects are at full strength.".to_string()),
        }
        now.push(if self.config.experimental_dark_legacy {
            "Dark legacy windows (experimental) are on.".to_string()
        } else {
            "Dark legacy windows (experimental) are off.".to_string()
        });
        if self.config.focus.enabled {
            now.push("Focus Look is on.".to_string());
        }
        if let Some(id) = crate::active_scene(&self.config) {
            if let Some(scene) = crate::scene_catalog().iter().find(|s| s.id == id) {
                now.push(format!("Scene: {}.", scene.name));
            }
        }
        b.push(Row::About(now));
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
        b.push(Row::About(about));
        b.out
    }

    fn rows_effects(&self) -> Vec<Laid> {
        let mut b = Builder::new();
        let target = match self.rule.and_then(|index| self.config.rules.get(index)) {
            Some(rule) => format!("Editing the look for: {}", rule.label()),
            None => "Editing the default look for all apps without a rule.".to_string(),
        };
        b.push(Row::About(vec![
            target,
            "Pick a different target on the Apps page.".to_string(),
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
        b.push(Row::Action(Button::DemoMaterials));
        b.push(Row::Header("Focus Look"));
        b.push(Row::Toggle(Toggle::FocusLook));
        b.push(Row::Toggle(Toggle::Reapply));
        b.push(Row::Header("Reduce effects automatically"));
        b.push(Row::Toggle(Toggle::ReduceBattery));
        b.push(Row::Toggle(Toggle::ReduceRemote));
        b.push(Row::Toggle(Toggle::ReduceTransparency));
        b.push(Row::Header("Title bar height"));
        b.push(Row::Slider(Slider::Caption));
        b.out
    }

    fn rows_apps(&self) -> Vec<Laid> {
        let mut b = Builder::new();
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
        if let (Some(plan), Some(_)) = (&self.ext.fix, &self.ext.selected) {
            // The guided path: what this is, what can change, then the safe actions.
            b.push(Row::Header("Fix this app"));
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
                    "Preview shows the look from the Effects page on this one window for 20 seconds, then puts it back. Nothing is saved until you press Save as rule.",
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
            b.push(Row::Header("Style receipt"));
            b.push(Row::Lines(self.ext.receipt.clone()));
            b.push(Row::Buttons(vec![Button::CopyReport]));
        }
        b.push(Row::Header("Full inspector report"));
        let lines = if self.ext.report_lines.is_empty() {
            vec!["Pick a window to see what AllBlack11 does with it, and why.".to_string()]
        } else {
            self.ext.report_lines.clone()
        };
        b.push(Row::Lines(lines));
        if self.ext.selected.is_some() {
            b.push(Row::Header("Your note for this app on this Windows build"));
            b.push(Row::Buttons(vec![
                Button::MarkWorks,
                Button::MarkPartial,
                Button::MarkFails,
                Button::MarkReset,
            ]));
        }
        b.push(Row::Header("Per-app rules"));
        b.push(Row::Note(
            "Most specific match wins, then the higher priority, then the earlier rule. Pick a rule to edit its look on the Effects page.",
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
        b.push(Row::Header("Never theme these apps"));
        b.push(Row::Note(
            "Games and launchers belong here. Use the exe name, such as steam.exe.",
        ));
        b.push_h(Row::ExcludeList, list_height(self.config.exclude.len()));
        b.push(Row::AddField(Button::AddExclude));
        b.push(Row::Action(Button::RemoveExclude));
        self.push_notebook(&mut b);
        b.out
    }

    fn rows_legacy(&self) -> Vec<Laid> {
        let mut b = Builder::new();
        if super::super::app::safe_mode_text().is_some() {
            b.push(Row::About(vec![
                "Safe mode is on for this run, so the switches below are not in effect. Leave safe mode on the Safety page.".to_string(),
            ]));
        }
        b.push(Row::Header("Dark legacy windows (experimental)"));
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
        b.out
    }

    fn rows_scenes(&self) -> Vec<Laid> {
        let mut b = Builder::new();
        b.push(Row::Header("Scenes and your Look Book"));
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
        b.out
    }

    fn push_diagnostics(&self, b: &mut Builder) {
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
        b.push(Row::Header("What this Windows build supports"));
        let support: Vec<String> = attribute_support(self.build)
            .iter()
            .map(|item| format!("{} — {}", item.name, item.support.label()))
            .collect();
        b.push(Row::Lines(support));
        b.push(Row::Note(
            "Supported means Windows accepts the call on this build. It does not mean every app shows the result: an app's own opaque content can hide a backdrop, and apps with their own title bar ignore caption colors.",
        ));
        b.push(Row::Header("Tools"));
        b.push(Row::Toggle(Toggle::Logging));
        b.push(Row::Action(Button::Diagnostics));
        b.push(Row::Action(Button::OpenFolder));
        b.push(Row::Action(Button::OpenLog));
    }
}

impl Ui {
    /// Compatibility notebook summary (moved from the old Profiles page to sit next to the
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
            "Notes stay on this PC (compat.json). Nothing is sent anywhere. Add one from the Apps page.".to_string(),
        ]));
        b.push(Row::Buttons(vec![Button::ClearNotebook]));
    }

    fn rows_safety(&self) -> Vec<Laid> {
        let mut b = Builder::new();
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
            "Turn off emulated x64 coverage on the Legacy page, or press Turn off experimental features above. Either one asks the helper to stop and ends it if it does not.",
        ));
        self.push_diagnostics(&mut b);
        b.out
    }

    fn experimental_lines(&self) -> Vec<String> {
        let on = |flag: bool| if flag { "on" } else { "off" };
        vec![
            format!(
                "Dark legacy windows: {} (Legacy page)",
                on(self.config.experimental_dark_legacy)
            ),
            format!(
                "Emulated x64 coverage: {} (Legacy page)",
                on(self.config.legacy.emulated_x64)
            ),
            format!(
                "Focus Look: {} (Effects page)",
                on(self.config.focus.enabled)
            ),
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

pub fn paint_nav(c: &Canvas, ui: &Ui, lw: i32) {
    let top = ui.preview_h();
    c.fill(0, top, lw, top + NAV_H, BG);
    let count = Page::ALL.len() as i32;
    let width = (lw - 2 * MARGIN) / count;
    for (index, page) in Page::ALL.iter().enumerate() {
        let left = MARGIN + index as i32 * width;
        let selected = *page == ui.page;
        c.text(
            left,
            top,
            left + width,
            top + NAV_H - 4,
            page.title(),
            if selected { TEXT } else { MUTED },
            if selected {
                ui.fonts.head
            } else {
                ui.fonts.body
            },
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        if selected {
            c.fill(
                left + 10,
                top + NAV_H - 5,
                left + width - 10,
                top + NAV_H - 2,
                ui.accent,
            );
        }
    }
}

pub fn paint_trial_bar(c: &Canvas, ui: &Ui, lw: i32) {
    let top = ui.status_top();
    let now = unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount64() };
    let deadline = TRIAL_DEADLINE.load(Ordering::Relaxed);
    let seconds = deadline.saturating_sub(now).div_ceil(1000);
    c.text(
        MARGIN,
        top,
        lw - MARGIN - 200,
        top + STATUS_H,
        &format!(
            "Trying “{}”. It reverts in {seconds} s unless you keep it.",
            ui.ext.trial_name
        ),
        TEXT,
        ui.fonts.small,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
    );
    let (keep, revert) = trial_pills(lw, top);
    c.round(keep.0, keep.1, keep.2, keep.3, 5, Some(ui.accent), None);
    c.text(
        keep.0,
        keep.1,
        keep.2,
        keep.3,
        "Keep",
        0x0000_0000,
        ui.fonts.small,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    c.round(
        revert.0,
        revert.1,
        revert.2,
        revert.3,
        5,
        Some(CARD_HI),
        Some(CARD_EDGE),
    );
    c.text(
        revert.0,
        revert.1,
        revert.2,
        revert.3,
        "Revert",
        TEXT,
        ui.fonts.small,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
}

impl Ui {
    pub fn trial_active(&self) -> bool {
        TRIAL_DEADLINE.load(Ordering::Relaxed) != 0
    }

    /// Clicks on the page tabs and the Keep/Revert buttons. True when the click was ours.
    pub fn chrome_click(&mut self, hwnd: HWND, lparam: LPARAM) -> bool {
        let lx = (lparam_x(lparam) as f32 / self.k) as i32;
        let ly = (lparam_y(lparam) as f32 / self.k) as i32;
        let lw = self.lw(hwnd);
        let nav_top = self.preview_h();
        if ly >= nav_top && ly < nav_top + NAV_H {
            let width = ((lw - 2 * MARGIN) / Page::ALL.len() as i32).max(1);
            let index = ((lx - MARGIN) / width).clamp(0, Page::ALL.len() as i32 - 1) as usize;
            if lx >= MARGIN && lx < lw - MARGIN {
                self.set_page(hwnd, Page::ALL[index]);
            }
            return true;
        }
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
        match page {
            Page::Apps => self.refresh_windows(),
            Page::Scenes => {
                self.refresh_recipes();
                self.refresh_looks();
            }
            _ => {}
        }
        self.relayout(hwnd);
    }

    fn post_command(&self) {
        unsafe { PostMessageW(self.owner, WM_APP_COMMAND, 0, 0) };
    }

    pub fn begin_trial(&mut self, name: &str, next: Config) {
        cmdq::push(AppCommand::TrialStart(Box::new(next.clone())));
        self.post_command();
        self.config = next;
        self.ext.trial_name = name.to_string();
        self.status = "Trying a new look. Nothing is saved until you press Keep.".into();
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::SetTimer(self.hwnd, TIMER_UI, 500, None)
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
        self.status = "Reverted.".into();
    }

    fn accent_bgr_for_demo(&self) -> u32 {
        registry::accent_bgr()
    }

    pub fn on_timer(&mut self, hwnd: HWND) {
        if self.ext.refresh_pending {
            self.ext.refresh_pending = false;
            self.refresh_report();
        }
        if !self.trial_active() {
            unsafe { windows_sys::Win32::UI::WindowsAndMessaging::KillTimer(hwnd, TIMER_UI) };
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
        if self.page != Page::Apps {
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
                Row::Toggle(_) | Row::Choice(_) | Row::Action(_) | Row::AddField(_) => {
                    Some((index, 1))
                }
                Row::Buttons(list) => Some((index, list.len())),
                Row::Presets => Some((index, PRESETS.len())),
                Row::LookList if self.look_count() > 0 => Some((index, self.look_count())),
                _ => None,
            })
            .collect()
    }

    pub fn move_focus(&mut self, hwnd: HWND, backwards: bool) {
        let list = self.interactive_rows();
        if list.is_empty() {
            return;
        }
        let current = self.ext.focus.unwrap_or(usize::MAX);
        let next = if current == usize::MAX {
            if backwards {
                list.len() - 1
            } else {
                0
            }
        } else if backwards {
            (current + list.len() - 1) % list.len()
        } else {
            (current + 1) % list.len()
        };
        self.ext.focus = Some(next);
        self.ext.focus_sub = 0;
        self.scroll_focus_into_view(hwnd);
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
        if self.trial_active() {
            self.status = "Keep or revert the trial look first.".into();
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
            Row::LookList => self.click_look_list(sub as i32),
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
    match &row.row {
        Row::Lines(lines) => {
            card(c, top, row.h, lw);
            let mut y = top + 10;
            for line in lines {
                let height = 16 * lines_needed(line, 64);
                let (font, color) = if line.is_empty() {
                    (fonts.small, MUTED)
                } else {
                    (fonts.small, TEXT)
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
                let hot = *id == Button::PickWindow && ui.ext.picking;
                c.round(
                    l,
                    top + t,
                    r,
                    top + b,
                    5,
                    Some(CARD_HI),
                    Some(if hot { ui.accent } else { CARD_EDGE }),
                );
                c.text(
                    l,
                    top + t,
                    r,
                    top + b,
                    label,
                    TEXT,
                    fonts.small,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
            }
        }
        Row::WindowList => {
            card(c, top, row.h, lw);
            let visible = ui.visible_windows();
            if visible.is_empty() {
                let text = if ui.ext.loaded {
                    "No matching windows."
                } else {
                    "Open this page to list windows."
                };
                c.text(
                    left,
                    top + 8,
                    right,
                    top + 8 + LIST_LINE,
                    text,
                    MUTED,
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
                        Some(CARD_HI),
                        Some(ui.accent),
                    );
                }
                c.text(
                    left,
                    line_top,
                    right,
                    line_top + LIST_LINE,
                    &ui.window_line(entry),
                    TEXT,
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
                    MUTED,
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
                        Some(CARD_HI),
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
                    TEXT,
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
                        Some(CARD_HI),
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
                    TEXT,
                    fonts.small,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
                for (slot, color) in [chips.caption, chips.border, chips.text].iter().enumerate() {
                    let x = chips_left + slot as i32 * 20;
                    let (l, t, r, b) = (x, line_top + 5, x + 14, line_top + 19);
                    match color {
                        Some(value) => c.round(l, t, r, b, 3, Some(*value), Some(MUTED)),
                        // Nothing is drawn there by this look: show an empty outline.
                        None => c.round(l, t, r, b, 3, None, Some(MUTED)),
                    }
                }
            }
        }
        Row::Filter => {
            card(c, top, row.h, lw);
            c.text(
                left,
                top,
                left + 90,
                top + row.h - GAP,
                "Filter",
                TEXT,
                fonts.body,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
        }
        _ => {}
    }
}

pub fn lines_needed(text: &str, per_line: usize) -> i32 {
    super::lines(text, per_line)
}

/// Focus ring around the keyboard-focused row.
pub fn paint_focus_ring(c: &Canvas, ui: &Ui, row: &Laid, top: i32, lw: i32) {
    c.round(
        MARGIN - 2,
        top - 2,
        lw - MARGIN + 2,
        top + row.h - GAP + 2,
        6,
        None,
        Some(ui.accent),
    );
    if let Row::Buttons(list) = &row.row {
        let (l, t, r, b) =
            buttons_rect(lw, list.len(), ui.ext.focus_sub.min(list.len() - 1), row.h);
        c.round(l - 2, top + t - 2, r + 2, top + b + 2, 6, None, Some(TEXT));
    }
    if matches!(row.row, Row::LookList) {
        let line_top =
            top + 8 + ui.ext.focus_sub.min(ui.look_count().saturating_sub(1)) as i32 * LIST_LINE;
        c.round(
            MARGIN + 4,
            line_top - 1,
            lw - MARGIN - 4,
            line_top + LIST_LINE + 1,
            5,
            None,
            Some(TEXT),
        );
    }
    if matches!(row.row, Row::Presets) {
        let (l, t, r, b) = preset_rect(lw, ui.ext.focus_sub.min(PRESETS.len() - 1));
        c.round(l - 2, top + t - 2, r + 2, top + b + 2, 6, None, Some(TEXT));
    }
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

impl Ui {
    pub fn toggle_ext(&mut self, id: Toggle) {
        match id {
            Toggle::FocusLook => self.config.focus.enabled = !self.config.focus.enabled,
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
            _ => {}
        }
    }

    /// Returns true when the action was handled here.
    pub fn run_button_ext(&mut self, id: Button) -> bool {
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
            Button::ExportRecipe => self.export_recipe(),
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
            self.status = "Rule created. Edit its look on the Effects page.".into();
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
        self.status = "Trying the look from the Effects page on that window for 20 seconds. Nothing is saved.".into();
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
    use windows_sys::Win32::UI::Controls::Dialogs::{
        GetOpenFileNameW, GetSaveFileNameW, OFN_FILEMUSTEXIST, OFN_OVERWRITEPROMPT,
        OFN_PATHMUSTEXIST, OPENFILENAMEW,
    };
    let mut file = [0u16; 520];
    for (index, unit) in default_name.encode_utf16().take(500).enumerate() {
        file[index] = unit;
    }
    let filter: Vec<u16> = "AllBlack11 recipe (*.json)\0*.json\0All files\0*.*\0\0"
        .encode_utf16()
        .collect();
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
