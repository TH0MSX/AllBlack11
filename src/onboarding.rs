//! First-run setup: where the user is, and what counts as done.
//!
//! Pure, so the promises are tested: setup resumes where it stopped, is never marked complete
//! before the last step or an explicit dismissal, never prompts an existing user after an
//! upgrade, and can be replayed. Nothing here enables anything: startup, hooks and automation
//! are separate choices the user makes on their own step.

use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 1;

/// The steps, in order. Titles are what the setup page shows.
pub const STEP_TITLES: [&str; 7] = [
    "Welcome",
    "Choose a starting look",
    "Try it safely",
    "Pick an app",
    "Keep or revert",
    "Preferences",
    "Ready",
];

pub const LAST_STEP: usize = STEP_TITLES.len() - 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    /// A new install that has not started setup.
    #[default]
    NotStarted,
    /// Started and not finished. Resumes at `step`.
    InProgress,
    /// Reached the end and pressed Finish.
    Completed,
    /// The user chose Skip. Never prompts again; replayable from Help.
    Dismissed,
    /// Installed before setup existed. Never prompted.
    ExistingUser,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct State {
    pub version: u32,
    pub status: Status,
    pub step: usize,
    /// The look picked on the starting-look step: a scene id, or "keep" for the current one.
    pub choice: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        State {
            version: VERSION,
            status: Status::NotStarted,
            step: 0,
            choice: None,
        }
    }
}

/// What to start from. `fresh_install`: there was no config file when this version first ran.
/// A readable state file always wins; with no usable file, a fresh install starts setup and an
/// existing one is left alone (an upgrade must not force anyone through first-run setup).
pub fn initial(fresh_install: bool, file: Option<&str>) -> State {
    if let Some(state) = file.and_then(parse) {
        return state;
    }
    State {
        status: if fresh_install {
            Status::NotStarted
        } else {
            Status::ExistingUser
        },
        ..State::default()
    }
}

/// Objects only; anything else is "no usable file" rather than a state.
pub fn parse(text: &str) -> Option<State> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    if !value.is_object() {
        return None;
    }
    let mut state: State = serde_json::from_value(value).ok()?;
    if state.version > VERSION {
        return None;
    }
    state.step = state.step.min(LAST_STEP);
    Some(state)
}

pub fn to_json(state: &State) -> String {
    serde_json::to_string_pretty(state).unwrap_or_else(|_| "{}".into())
}

impl State {
    /// Should AllBlack11 put setup in front of the user without being asked?
    pub fn should_prompt(&self) -> bool {
        self.status == Status::NotStarted
    }

    /// Is there an unfinished setup to offer to continue (Home shows a card)?
    pub fn can_resume(&self) -> bool {
        matches!(self.status, Status::NotStarted | Status::InProgress)
    }

    /// Begin, or begin again from Help. Always from the first step.
    pub fn start(&mut self) {
        self.status = Status::InProgress;
        self.step = 0;
        self.choice = None;
    }

    /// Continue where it stopped (a saved InProgress state), or begin if there is none.
    pub fn resume(&mut self) {
        if self.status != Status::InProgress {
            self.start();
        }
    }

    /// Next step. Does not complete setup, even on the last step: only `finish` does.
    pub fn next(&mut self) -> bool {
        if self.status != Status::InProgress || self.step >= LAST_STEP {
            return false;
        }
        self.step += 1;
        true
    }

    pub fn back(&mut self) -> bool {
        if self.status != Status::InProgress || self.step == 0 {
            return false;
        }
        self.step -= 1;
        true
    }

    /// Finish: valid only on the last step of a setup in progress.
    pub fn finish(&mut self) -> bool {
        if self.status == Status::InProgress && self.step == LAST_STEP {
            self.status = Status::Completed;
            true
        } else {
            false
        }
    }

    /// The user explicitly skipped setup.
    pub fn dismiss(&mut self) {
        if matches!(self.status, Status::NotStarted | Status::InProgress) {
            self.status = Status::Dismissed;
        }
    }

    pub fn is_last(&self) -> bool {
        self.step == LAST_STEP
    }

    pub fn title(&self) -> &'static str {
        STEP_TITLES[self.step.min(LAST_STEP)]
    }

    /// "Step 3 of 7".
    pub fn progress(&self) -> String {
        format!("Step {} of {}", self.step + 1, STEP_TITLES.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_install_prompts_and_an_upgrade_never_does() {
        assert!(initial(true, None).should_prompt());
        let existing = initial(false, None);
        assert_eq!(existing.status, Status::ExistingUser);
        assert!(!existing.should_prompt());
        assert!(!existing.can_resume());
    }

    #[test]
    fn a_saved_state_wins_over_the_install_check() {
        let saved = State {
            status: Status::Completed,
            ..State::default()
        };
        let state = initial(true, Some(&to_json(&saved)));
        assert_eq!(
            state.status,
            Status::Completed,
            "a finished setup is not shown again"
        );
    }

    #[test]
    fn setup_resumes_at_the_step_it_stopped_on() {
        let mut state = initial(true, None);
        state.start();
        assert!(state.next() && state.next() && state.next());
        state.choice = Some("graphite".into());
        // The app is closed and reopened: the file is read back.
        let back = initial(true, Some(&to_json(&state)));
        assert_eq!(back.status, Status::InProgress);
        assert_eq!(back.step, 3);
        assert_eq!(back.choice.as_deref(), Some("graphite"));
        assert!(
            !back.should_prompt(),
            "it does not re-prompt, Home offers to continue"
        );
        assert!(back.can_resume());
    }

    #[test]
    fn it_is_never_complete_before_the_last_step_and_an_explicit_finish() {
        let mut state = State::default();
        assert!(!state.finish(), "cannot finish what was not started");
        state.start();
        for _ in 0..LAST_STEP {
            assert!(!state.finish(), "not on the last step yet");
            assert!(state.next());
        }
        assert!(state.is_last());
        assert!(!state.next(), "next never completes setup");
        assert_eq!(state.status, Status::InProgress);
        assert!(state.finish());
        assert_eq!(state.status, Status::Completed);
        assert!(!state.finish(), "already done");
    }

    #[test]
    fn back_stops_at_the_first_step_and_skipping_is_an_explicit_dismissal() {
        let mut state = State::default();
        state.start();
        assert!(!state.back());
        state.next();
        assert!(state.back());
        state.dismiss();
        assert_eq!(state.status, Status::Dismissed);
        assert!(!state.should_prompt() && !state.can_resume());
        assert!(!state.next(), "no stepping once dismissed");
    }

    #[test]
    fn existing_users_and_finished_setups_ignore_dismiss_but_can_replay() {
        let mut existing = initial(false, None);
        existing.dismiss();
        assert_eq!(existing.status, Status::ExistingUser);
        existing.start();
        assert_eq!((existing.status, existing.step), (Status::InProgress, 0));
        let mut done = State {
            status: Status::Completed,
            step: 6,
            choice: Some("x".into()),
            ..State::default()
        };
        done.start();
        assert_eq!(
            (done.status, done.step, done.choice.clone()),
            (Status::InProgress, 0, None)
        );
    }

    #[test]
    fn resume_continues_but_never_loses_progress() {
        let mut state = State::default();
        state.start();
        state.next();
        state.next();
        state.resume();
        assert_eq!(state.step, 2);
        let mut fresh = State::default();
        fresh.resume();
        assert_eq!((fresh.status, fresh.step), (Status::InProgress, 0));
    }

    #[test]
    fn a_damaged_or_future_file_is_not_trusted_and_steps_are_clamped() {
        for bad in [
            "",
            "not json",
            "[]",
            "7",
            r#"{"status":"almost"}"#,
            r#"{"version":99}"#,
        ] {
            assert!(parse(bad).is_none(), "{bad:?}");
        }
        // With no usable file, a fresh install still prompts and an existing one stays quiet.
        assert!(initial(true, Some("garbage")).should_prompt());
        assert!(!initial(false, Some("garbage")).should_prompt());
        let wild = parse(r#"{"status":"inProgress","step":900}"#).unwrap();
        assert_eq!(wild.step, LAST_STEP);
        // Unknown extra fields from a newer build are ignored.
        assert!(parse(r#"{"status":"completed","extra":1}"#).is_some());
    }

    #[test]
    fn progress_text_and_titles_line_up() {
        let mut state = State::default();
        state.start();
        assert_eq!(state.progress(), "Step 1 of 7");
        assert_eq!(state.title(), "Welcome");
        for _ in 0..LAST_STEP {
            state.next();
        }
        assert_eq!(state.progress(), "Step 7 of 7");
        assert_eq!(state.title(), "Ready");
    }
}
