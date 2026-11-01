//! Safe startup: should this run begin with the experimental parts switched off?
//!
//! A small marker file records whether the previous run ended cleanly. If AllBlack11 was
//! killed or crashed twice in a row, or the user asks for it (Shift held while starting, or
//! `--safe`), this run starts in *safe mode*: dark legacy windows and the x64 helper are not
//! started, and automatic extras are off. Nothing is written to `config.json`; safe mode
//! lives in memory for one run, and a clean exit resets the counter.
//!
//! Pure on purpose: the decision and the marker format are tested; reading the file and the
//! keyboard is the host's job.

use serde::{Deserialize, Serialize};

/// Unclean ends in a row before safe mode starts by itself.
pub const CRASH_LIMIT: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Marker {
    /// The last run reached its normal exit path.
    pub clean_exit: bool,
    /// Unclean ends in a row, including the previous run.
    pub crashes: u32,
    /// Unix seconds when the run began (information only).
    pub started: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// The user asked: Shift held at launch, or `--safe`.
    Requested,
    /// AllBlack11 ended uncleanly this many times in a row.
    RepeatedCrash(u32),
}

impl Reason {
    pub fn text(self) -> String {
        match self {
            Reason::Requested => "You asked for a safe start.".to_string(),
            Reason::RepeatedCrash(count) => format!(
                "AllBlack11 did not exit cleanly the last {count} times, so experimental features were left off this time."
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Safe(Reason),
}

impl Mode {
    pub fn is_safe(self) -> bool {
        matches!(self, Mode::Safe(_))
    }
}

/// Decide the mode for this run and the marker to write now (unclean until proven otherwise).
pub fn decide(previous: Option<&Marker>, requested: bool, now: u64) -> (Mode, Marker) {
    let crashes = match previous {
        Some(marker) if !marker.clean_exit => marker.crashes.saturating_add(1),
        _ => 0,
    };
    let mode = if requested {
        Mode::Safe(Reason::Requested)
    } else if crashes >= CRASH_LIMIT {
        Mode::Safe(Reason::RepeatedCrash(crashes))
    } else {
        Mode::Normal
    };
    (
        mode,
        Marker {
            clean_exit: false,
            crashes,
            started: now,
        },
    )
}

/// The marker to write on a normal exit. Resets the counter: a run that ended cleanly, safe
/// or not, is the way out of safe mode.
pub fn clean(marker: &Marker) -> Marker {
    Marker {
        clean_exit: true,
        crashes: 0,
        started: marker.started,
    }
}

/// Unreadable or missing is treated as "no history", never as a crash.
pub fn parse(text: &str) -> Option<Marker> {
    // Objects only: serde would also accept a bare `[]` as an all-defaults marker, which would
    // read as an unclean end.
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    if !value.is_object() {
        return None;
    }
    serde_json::from_value(value).ok()
}

pub fn format(marker: &Marker) -> String {
    serde_json::to_string(marker).unwrap_or_else(|_| "{}".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_first_run_and_a_clean_history_start_normally() {
        let (mode, marker) = decide(None, false, 10);
        assert_eq!(mode, Mode::Normal);
        assert!(!marker.clean_exit, "unclean until it exits properly");
        let previous = clean(&marker);
        assert_eq!(decide(Some(&previous), false, 20).0, Mode::Normal);
    }

    #[test]
    fn one_unclean_end_is_forgiven_two_in_a_row_start_safe_mode() {
        let (_, first) = decide(None, false, 1);
        // The process was killed: the marker still says unclean.
        let (mode, second) = decide(Some(&first), false, 2);
        assert_eq!(mode, Mode::Normal);
        assert_eq!(second.crashes, 1);
        let (mode, third) = decide(Some(&second), false, 3);
        assert_eq!(mode, Mode::Safe(Reason::RepeatedCrash(2)));
        assert_eq!(third.crashes, 2);
    }

    #[test]
    fn a_clean_exit_is_the_way_out_of_safe_mode() {
        let (_, one) = decide(None, false, 1);
        let (_, two) = decide(Some(&one), false, 2);
        let (mode, three) = decide(Some(&two), false, 3);
        assert!(mode.is_safe());
        let after = clean(&three);
        assert_eq!(after.crashes, 0);
        assert_eq!(decide(Some(&after), false, 4).0, Mode::Normal);
    }

    #[test]
    fn the_user_can_always_ask_for_safe_mode() {
        assert_eq!(decide(None, true, 1).0, Mode::Safe(Reason::Requested));
        let (_, marker) = decide(None, false, 1);
        assert_eq!(
            decide(Some(&clean(&marker)), true, 2).0,
            Mode::Safe(Reason::Requested)
        );
    }

    #[test]
    fn a_damaged_marker_means_no_history_not_a_crash() {
        for text in ["", "not json", "[]", "{\"crashes\":\"many\"}"] {
            assert!(parse(text).is_none(), "{text:?}");
        }
        // Missing fields fall back to defaults; unknown fields from a newer build are ignored.
        let marker = parse("{\"cleanExit\":true,\"future\":1}").unwrap();
        assert!(marker.clean_exit);
        assert_eq!(marker.crashes, 0);
        let round = parse(&format(&Marker {
            clean_exit: false,
            crashes: 3,
            started: 9,
        }))
        .unwrap();
        assert_eq!(round.crashes, 3);
    }

    #[test]
    fn the_counter_cannot_overflow() {
        let huge = Marker {
            clean_exit: false,
            crashes: u32::MAX,
            started: 0,
        };
        let (mode, marker) = decide(Some(&huge), false, 1);
        assert_eq!(marker.crashes, u32::MAX);
        assert!(mode.is_safe());
    }

    #[test]
    fn reasons_are_written_for_people() {
        assert!(Reason::RepeatedCrash(2).text().contains("2 times"));
        assert!(Reason::Requested.text().contains("safe start"));
    }
}
