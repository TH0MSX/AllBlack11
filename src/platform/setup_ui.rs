//! First-run setup, walkthrough tips and the Help page, as rows on the same page model as
//! everything else. Child of `settings_pages` so it uses the same builder and controls.
//!
//! What this never does: it never turns on startup, hooks, the hotkey or any automation by
//! itself (each is a control the person operates on the Preferences step), it never marks setup
//! complete before the last step, and its walkthroughs only point at real controls, with
//! nothing simulated and nothing blocked.
//!
//! Evidence: unit-tested content and state (`onboarding.rs`, `help.rs`); this UI is
//! cross-compiled and rendered under Wine for layout only. Resuming across a real restart,
//! keyboard use and screen reader output are unverified on Windows.

use super::*;
use crate::{
    find_tutorial, tutorial_steps, OnboardingState, OnboardingStatus, TutorialCaps, TutorialStep,
    TutorialTarget, SETUP_STEP_TITLES, TUTORIALS,
};

/// The setup wizard's live state.
#[derive(Default)]
pub struct SetupUi {
    pub active: bool,
    pub state: OnboardingState,
}

/// A walkthrough in progress.
pub struct TutorialRun {
    pub id: &'static str,
    pub step: usize,
}

impl Ui {
    /// Called once when Settings opens. A fresh install starts setup; everyone else only gets a
    /// Home card (and only if setup was started and left unfinished).
    pub fn init_onboarding(&mut self) {
        let state = cfgio::load_onboarding().unwrap_or_default();
        self.ext.setup.state = state;
        if self.ext.setup.state.should_prompt() {
            self.ext.setup.state.start();
            self.ext.setup.active = true;
            self.persist_setup();
            self.refresh_looks();
        }
    }

    /// The starting-look step: remember which scene or look was picked.
    pub fn setup_note_choice(&mut self, index: usize) {
        let scenes = self.ext.scenes.len();
        let id = if index < scenes {
            self.ext.scenes.get(index).map(|s| s.id.clone())
        } else {
            self.ext
                .book
                .looks
                .get(index - scenes)
                .map(|l| l.id.clone())
        };
        if let Some(id) = id {
            self.ext.setup.state.choice = Some(id);
            self.persist_setup();
        }
    }

    fn persist_setup(&mut self) {
        if let Err(err) = cfgio::save_onboarding(&self.ext.setup.state) {
            self.status = format!("Could not save your setup progress: {err}");
        }
    }

    fn setup_leave(&mut self, status: &str) {
        self.ext.setup.active = false;
        self.persist_setup();
        self.status = status.to_string();
        self.ext.focus = None;
        let hwnd = self.hwnd;
        self.ext.nav_focus = false;
        self.page = Page::Fix; // forces set_page to treat Home as a change
        self.set_page(hwnd, Page::Home);
    }

    /// The scene or look chosen on the starting-look step, if any (not "keep").
    fn setup_choice_look(&self) -> Option<(String, Box<dyn crate::SceneLike>)> {
        let id = self.ext.setup.state.choice.clone()?;
        if id == "keep" {
            return None;
        }
        if let Some(scene) = self.ext.scenes.iter().find(|s| s.id == id) {
            return Some((scene.name.clone(), Box::new(scene.clone())));
        }
        let look = self.ext.book.get(&id)?;
        Some((look.name.clone(), Box::new(look.clone())))
    }

    fn setup_choice_text(&self) -> String {
        match self.ext.setup.state.choice.as_deref() {
            None => "No look chosen yet. Next keeps how things look now.".to_string(),
            Some("keep") => "Chosen: keep my current look.".to_string(),
            Some(_) => match self.setup_choice_look() {
                Some((name, _)) => format!("Chosen: {name}."),
                None => {
                    "Chosen look is no longer available. Pick another or keep your current look."
                        .to_string()
                }
            },
        }
    }

    /// Rows for the setup flow. The navigation pane is hidden while it shows.
    pub fn rows_setup(&self) -> Vec<Laid> {
        let state = &self.ext.setup.state;
        let mut b = Builder::new();
        let title = SETUP_STEP_TITLES[state.step.min(SETUP_STEP_TITLES.len() - 1)];
        let heading = match state.step {
            0 => "Welcome to AllBlack11",
            _ => title,
        };
        b.push(Row::PageTitle(heading, state.progress()));
        match state.step {
            0 => {
                b.push(Row::Prose(vec![
                    "AllBlack11 recolors window frames: title bars, borders, corners and optional materials, using Windows' own window attributes.".to_string(),
                    "It cannot recolor everything. Web content, games and many modern apps draw their own surfaces, so some parts stay as they are. AllBlack11 tells you which.".to_string(),
                    "Everything can be undone. This takes about two minutes and changes nothing until you say so.".to_string(),
                ]));
            }
            1 => {
                b.push(Row::Note(
                    "Pick a look to start from, or keep how things look now. Nothing is applied yet.",
                ));
                b.push_h(Row::LookList, list_height(self.look_count().min(40)));
                b.push(Row::Lines(self.look_details()));
                b.push(Row::Buttons(vec![
                    Button::SetupKeepCurrent,
                    Button::DemoCompare,
                ]));
                b.push(Row::Lines(vec![self.setup_choice_text()]));
            }
            2 => {
                b.push(Row::Prose(vec![
                    "This opens two sample windows that belong to AllBlack11, so it is safe to try anything on them.".to_string(),
                    "The left one is left exactly as Windows draws it. The right one gets the look you chose. Close both windows to undo it.".to_string(),
                    self.setup_choice_text(),
                ]));
                b.push(Row::Buttons(vec![Button::DemoCompare]));
            }
            3 => {
                b.push(Row::Note(
                    "Now try an app of your own. Press Pick a window, drag onto an app, and read what can change. You can skip this and come back later.",
                ));
                self.push_pick(&mut b);
                self.push_fix_card(&mut b);
            }
            4 => {
                let has_look = self.setup_choice_look().is_some();
                b.push(Row::Prose(vec![
                    "This is the safety net you will use every time. A bar at the top counts down 15 seconds. Keep saves the look. Revert, or doing nothing, puts everything back. Nothing is written until you press Keep.".to_string(),
                    "If anything ever looks wrong, Pause all styling on Home puts every window back at once, and the Safety page has more.".to_string(),
                ]));
                if has_look {
                    b.push(Row::Buttons(vec![Button::SetupTry]));
                } else {
                    b.push(Row::Note(
                        "You chose to keep your current look, so there is nothing to try. You can go back to pick a look, or continue.",
                    ));
                }
            }
            5 => {
                b.push(Row::Note(
                    "These are your choices. Nothing here is turned on for you.",
                ));
                b.push(Row::Toggle(Toggle::Autostart));
                b.push(Row::Toggle(Toggle::PaletteHotkey));
                b.push(Row::Header("Experimental features"));
                b.push(Row::Prose(vec![
                    "Dark classic dialogs and, on ARM64, emulated x64 coverage stay off. They recolor some old dialogs from inside the app and have limits. You can read about them, and turn them on, under Rules and compatibility.".to_string(),
                ]));
            }
            _ => {
                let mut summary = vec![
                    "You are ready. Here is what is set up:".to_string(),
                    self.current_look_line(),
                    if self.autostart {
                        "AllBlack11 starts with Windows.".to_string()
                    } else {
                        "AllBlack11 does not start with Windows. Start it from the Start menu."
                            .to_string()
                    },
                    self.experimental_summary(),
                ];
                summary.push(
                    "Next: pick an app on Fix this app, or open Help for walkthroughs.".to_string(),
                );
                b.push(Row::Prose(summary));
                b.push(Row::Buttons(vec![
                    Button::SetupBack,
                    Button::SetupFinish,
                    Button::SetupFinishHelp,
                ]));
                return b.out;
            }
        }
        let mut footer = Vec::new();
        if state.step > 0 {
            footer.push(Button::SetupBack);
        }
        footer.push(Button::SetupNext);
        footer.push(Button::SetupSkip);
        b.push(Row::Buttons(footer));
        b.out
    }

    /// Load what the step about to be shown lists, so it never says "open this page" while it
    /// is the page: the looks for the starting-look step, the open windows for the picker step.
    fn setup_entered_step(&mut self) {
        match self.ext.setup.state.step {
            1 => self.refresh_looks(),
            3 => self.refresh_windows(),
            _ => {}
        }
    }

    /// Handle the setup, tip and walkthrough buttons. True when the button was one of these.
    pub fn setup_button(&mut self, id: Button) -> bool {
        let hwnd = self.hwnd;
        match id {
            Button::SetupNext => {
                if self.ext.setup.state.next() {
                    self.persist_setup();
                    self.scroll = 0;
                    self.ext.focus = None;
                    self.setup_entered_step();
                }
            }
            Button::SetupBack => {
                if self.ext.setup.state.back() {
                    self.persist_setup();
                    self.scroll = 0;
                    self.ext.focus = None;
                    self.setup_entered_step();
                }
            }
            Button::SetupSkip => {
                self.ext.setup.state.dismiss();
                self.setup_leave("Setup skipped. You can run it again from Settings and help.");
            }
            Button::SetupFinish | Button::SetupFinishHelp => {
                if self.ext.setup.state.finish() {
                    self.setup_leave("Setup finished.");
                    if id == Button::SetupFinishHelp {
                        self.set_page(hwnd, Page::Help);
                    }
                }
            }
            Button::SetupKeepCurrent => {
                self.ext.setup.state.choice = Some("keep".to_string());
                self.persist_setup();
            }
            Button::SetupTry => match self.setup_choice_look() {
                Some((name, item)) => {
                    let mut next = self.config.clone();
                    crate::apply_scene(&mut next, item.as_ref());
                    self.begin_trial(&name, next);
                }
                None => self.status = "Pick a look on the earlier step first.".into(),
            },
            Button::SetupContinue => {
                self.ext.setup.state.resume();
                self.ext.setup.active = true;
                self.persist_setup();
                self.refresh_looks();
                self.scroll = 0;
                self.ext.focus = None;
                self.setup_entered_step();
            }
            Button::SetupReplay => {
                self.ext.setup.state.start();
                self.ext.setup.active = true;
                self.persist_setup();
                self.refresh_looks();
                self.scroll = 0;
                self.ext.focus = None;
            }
            Button::TipBack => self.tutorial_move(-1),
            Button::TipNext => self.tutorial_move(1),
            Button::TipDone | Button::TipClose => {
                self.ext.tutorial = None;
                self.status =
                    "Walkthrough closed. You can start it again from Settings and help.".into();
            }
            Button::Tutorial(index) => {
                if let Some(tutorial) = TUTORIALS.get(index as usize) {
                    self.ext.tutorial = Some(TutorialRun {
                        id: tutorial.id,
                        step: 0,
                    });
                    self.goto_tutorial_page();
                }
            }
            _ => return false,
        }
        self.relayout(hwnd);
        true
    }

    // --- walkthroughs ----------------------------------------------------------------------

    fn tutorial_caps(&self) -> TutorialCaps {
        TutorialCaps {
            has_selection: self.ext.selected.is_some(),
            paused: super::super::super::app::is_paused(),
        }
    }

    /// The current walkthrough's steps as they stand now (some depend on what is picked).
    pub fn tutorial_steps_now(&self) -> Vec<TutorialStep> {
        match &self.ext.tutorial {
            Some(run) => find_tutorial(run.id)
                .map(|t| tutorial_steps(t, &self.tutorial_caps()))
                .unwrap_or_default(),
            None => Vec::new(),
        }
    }

    fn tutorial_move(&mut self, delta: i32) {
        let steps = self.tutorial_steps_now();
        let Some(run) = self.ext.tutorial.as_mut() else {
            return;
        };
        let next = run.step as i32 + delta;
        if next < 0 {
            return;
        }
        if next as usize >= steps.len() {
            self.ext.tutorial = None;
            self.status = "Walkthrough finished.".into();
            return;
        }
        run.step = next as usize;
        self.goto_tutorial_page();
    }

    /// Move to the page the current step is about.
    fn goto_tutorial_page(&mut self) {
        let steps = self.tutorial_steps_now();
        let Some(step) = self
            .ext
            .tutorial
            .as_ref()
            .and_then(|run| steps.get(run.step))
        else {
            return;
        };
        if let Some(page) = Page::ALL.get(step.page).copied() {
            let hwnd = self.hwnd;
            self.set_page(hwnd, page);
        }
    }

    /// The current step, if it belongs on the page being shown.
    fn tip_here(&self) -> Option<(usize, usize, TutorialStep)> {
        let run = self.ext.tutorial.as_ref()?;
        let steps = self.tutorial_steps_now();
        let step = *steps.get(run.step)?;
        let here = Page::ALL.get(step.page).copied()?;
        (here == self.page && !self.ext.setup.active).then_some((run.step, steps.len(), step))
    }

    pub fn tutorial_target(&self) -> Option<TutorialTarget> {
        self.tip_here().and_then(|(_, _, step)| step.target)
    }

    /// Insert the current walkthrough tip under the page title.
    pub fn with_tip(&self, rows: Vec<Laid>) -> Vec<Laid> {
        let Some((index, total, step)) = self.tip_here() else {
            return rows;
        };
        let title = self
            .ext
            .tutorial
            .as_ref()
            .and_then(|run| find_tutorial(run.id))
            .map_or("Tip", |t| t.title);
        let mut buttons = Vec::new();
        if index > 0 {
            buttons.push(Button::TipBack);
        }
        buttons.push(if index + 1 < total {
            Button::TipNext
        } else {
            Button::TipDone
        });
        buttons.push(Button::TipClose);
        let tip = Row::Tip(vec![
            format!("{title} · step {} of {total}", index + 1),
            step.text.to_string(),
        ]);
        let mut out: Vec<(Row, i32)> = rows.into_iter().map(|laid| (laid.row, laid.h)).collect();
        let at = usize::from(!out.is_empty());
        let tip_h = row_height(&tip);
        let buttons_row = Row::Buttons(buttons);
        let buttons_h = row_height(&buttons_row);
        out.insert(at, (buttons_row, buttons_h));
        out.insert(at, (tip, tip_h));
        let mut y = 8;
        out.into_iter()
            .map(|(row, h)| {
                let laid = Laid { y, h, row };
                y += h;
                laid
            })
            .collect()
    }

    // --- help page ---------------------------------------------------------------------------

    /// Search box, matching topics, and the walkthrough list.
    pub fn push_help(&self, b: &mut Builder) {
        b.push(Row::Header("Help"));
        b.push(Row::SearchField);
        let found = crate::search_help(&self.ext.help_query);
        if found.is_empty() {
            b.push(Row::Note("Nothing matches. Try fewer or different words."));
        }
        for topic in found {
            b.push(Row::Header(topic.title));
            b.push(Row::Lines(
                topic.body.iter().map(|line| (*line).to_string()).collect(),
            ));
        }
        b.push(Row::Header("Walkthroughs"));
        b.push(Row::Note(
            "Short tips on the page itself, using the real controls. Nothing is simulated and nothing is changed for you.",
        ));
        for index in 0..TUTORIALS.len() {
            b.push(Row::Action(Button::Tutorial(index as u8)));
        }
        b.push(Row::Header("Setup"));
        b.push(Row::Action(Button::SetupReplay));
    }

    /// Home card for setup that was started and left unfinished.
    pub fn push_setup_card(&self, b: &mut Builder) {
        let state = &self.ext.setup.state;
        if state.status == OnboardingStatus::InProgress && !self.ext.setup.active {
            b.push(Row::Header("Finish setting up"));
            b.push(Row::About(vec![
                format!("Setup is not finished. You stopped at: {}.", state.title()),
                "Nothing has been changed that you did not choose.".to_string(),
            ]));
            b.push(Row::Buttons(vec![Button::SetupContinue, Button::SetupSkip]));
        }
    }
}

/// Ring an element a walkthrough step points at. Painting only: the element stays fully usable
/// and reachable by keyboard, and nothing is drawn over it.
pub fn paint_tutorial_highlight(c: &Canvas, ui: &Ui, row: &Laid, top: i32, lw: i32) {
    let Some(target) = ui.tutorial_target() else {
        return;
    };
    let card = (MARGIN, top, lw - MARGIN, top + row.h - GAP);
    let ring = |rect: (i32, i32, i32, i32)| {
        c.round(
            rect.0 - 3,
            rect.1 - 3,
            rect.2 + 3,
            rect.3 + 3,
            6,
            None,
            Some(ui.accent),
        );
        c.round(
            rect.0 - 4,
            rect.1 - 4,
            rect.2 + 4,
            rect.3 + 4,
            7,
            None,
            Some(ui.accent),
        );
    };
    let button_rect = |wanted: &[Button]| -> Option<(i32, i32, i32, i32)> {
        match &row.row {
            Row::Buttons(list) => list.iter().position(|b| wanted.contains(b)).map(|i| {
                let (l, t, r, b) = buttons_rect(lw, list.len(), i, row.h);
                (l, top + t, r, top + b)
            }),
            Row::Action(id) if wanted.contains(id) => {
                let (l, t, r, b) = action_pill(*id, lw, row.h);
                Some((l, top + t, r, top + b))
            }
            _ => None,
        }
    };
    let rect = match target {
        TutorialTarget::PickWindow => button_rect(&[Button::PickWindow]),
        TutorialTarget::PreviewHere => button_rect(&[Button::TryLook]),
        TutorialTarget::SaveRule => button_rect(&[Button::CreateRule]),
        TutorialTarget::ExcludeApp => button_rect(&[Button::ExcludeApp]),
        TutorialTarget::ImportRecipe => button_rect(&[Button::ImportRecipe]),
        TutorialTarget::PauseOrResume => button_rect(&[Button::PauseAll, Button::ResumeAll]),
        TutorialTarget::RestoreLastKnownGood => button_rect(&[Button::RestoreLkg]),
        TutorialTarget::QuickLooks => matches!(row.row, Row::Presets).then_some(card),
        TutorialTarget::SceneList => matches!(row.row, Row::LookList).then_some(card),
        TutorialTarget::SaveLook => {
            matches!(row.row, Row::AddField(Button::SaveLook)).then_some(card)
        }
        TutorialTarget::HelpSearch => matches!(row.row, Row::SearchField).then_some(card),
        // These point at a block of text; the tip itself names it, so nothing is outlined.
        TutorialTarget::FixCard | TutorialTarget::StyleReceipt => None,
    };
    if let Some(rect) = rect {
        ring(rect);
    }
}

/// The teaching tip card: accent edge, overlay radius, a short heading and the text.
pub fn paint_tip(c: &Canvas, ui: &Ui, parts: &[String], top: i32, row_h: i32, lw: i32) {
    let theme = th();
    let (l, r) = (MARGIN, lw - MARGIN);
    let bottom = top + row_h - GAP;
    c.round(
        l,
        top,
        r,
        bottom,
        crate::ui_metrics::OVERLAY_RADIUS,
        Some(theme.card_hi),
        Some(ui.accent),
    );
    c.text(
        l + PAD,
        top + 10,
        r - PAD,
        top + 30,
        parts.first().map_or("", String::as_str),
        theme.text,
        ui.fonts.head,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
    );
    c.text(
        l + PAD,
        top + 32,
        r - PAD,
        bottom - 6,
        parts.get(1).map_or("", String::as_str),
        theme.text,
        ui.fonts.body,
        DT_LEFT | DT_WORDBREAK,
    );
}
