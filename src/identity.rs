//! Is this still the window I looked at?
//!
//! An HWND is a reusable number. Process id plus process creation time rules out a *different
//! process*, but not a different window **in the same process**: a dialog closes, and a moment
//! later the same app opens another window that gets the same HWND. Without more evidence we
//! would then restore the old window's saved appearance onto the new one.
//!
//! Windows offers no unique, never-reused window id, so this module compares a fingerprint of
//! facts that do not change over a window's life, and refuses to act when they disagree. It
//! cannot *prove* identity; it makes a mistake very unlikely and treats doubt as "no".

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fingerprint {
    pub pid: u32,
    pub create: u64,
    /// The UI thread that owns the window. A new window usually has a different thread or,
    /// failing that, a different instance or class.
    pub thread: u32,
    pub class: String,
    pub instance: u64,
    pub owner: i64,
    pub top_level: bool,
    /// Style bits an app can in principle toggle, so a mismatch is doubt, not proof.
    pub tool_window: bool,
    pub no_redirection: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Identity {
    /// Everything that should not change matches. Still "very likely", never "certain".
    Same,
    /// Proof of a different window (or none). Never act.
    Different(&'static str),
    /// Mostly matches but something an app can change differs. Act only with explicit consent.
    Uncertain(&'static str),
}

impl Identity {
    /// Applying a style to a window: only when it is the same window.
    pub fn allows_apply(self) -> bool {
        self == Identity::Same
    }

    /// Restoring or undoing: only when it is the same window. "Uncertain" is refused too,
    /// because writing old values onto a window that is not the original does damage.
    pub fn allows_restore(self) -> bool {
        self == Identity::Same
    }

    pub fn reason(self) -> &'static str {
        match self {
            Identity::Same => "same window",
            Identity::Different(reason) | Identity::Uncertain(reason) => reason,
        }
    }
}

pub fn check(recorded: &Fingerprint, live: Option<&Fingerprint>) -> Identity {
    let Some(live) = live else {
        return Identity::Different("the window is gone");
    };
    if recorded.pid != live.pid {
        return Identity::Different("a different process");
    }
    if recorded.create != live.create {
        return Identity::Different("the process was restarted");
    }
    if recorded.thread != live.thread {
        return Identity::Different("a different window thread");
    }
    if !recorded.class.eq_ignore_ascii_case(&live.class) {
        return Identity::Different("a different window class");
    }
    if recorded.instance != live.instance {
        return Identity::Different("created by a different module");
    }
    if recorded.owner != live.owner {
        return Identity::Different("a different owner window");
    }
    if recorded.top_level != live.top_level {
        return Identity::Different("no longer a top-level window");
    }
    if recorded.tool_window != live.tool_window || recorded.no_redirection != live.no_redirection {
        return Identity::Uncertain("its window style changed");
    }
    Identity::Same
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fp() -> Fingerprint {
        Fingerprint {
            pid: 100,
            create: 777,
            thread: 4,
            class: "Notepad".into(),
            instance: 0x1400_0000,
            owner: 0,
            top_level: true,
            tool_window: false,
            no_redirection: false,
        }
    }

    #[test]
    fn identical_fingerprints_are_the_same_window() {
        assert_eq!(check(&fp(), Some(&fp())), Identity::Same);
        assert!(Identity::Same.allows_apply() && Identity::Same.allows_restore());
    }

    #[test]
    fn a_missing_window_is_never_acted_on() {
        let result = check(&fp(), None);
        assert!(matches!(result, Identity::Different(_)));
        assert!(!result.allows_apply() && !result.allows_restore());
    }

    #[test]
    fn a_reused_hwnd_in_the_same_process_is_caught_by_the_other_facts() {
        // Same pid and creation time (so the old check would pass), but a new window.
        let old = fp();
        for changed in [
            Fingerprint { thread: 9, ..fp() },
            Fingerprint {
                class: "#32770".into(),
                ..fp()
            },
            Fingerprint {
                instance: 0x7ff0_0000,
                ..fp()
            },
            Fingerprint {
                owner: 4242,
                ..fp()
            },
            Fingerprint {
                top_level: false,
                ..fp()
            },
        ] {
            assert!(
                matches!(check(&old, Some(&changed)), Identity::Different(_)),
                "{changed:?}"
            );
        }
    }

    #[test]
    fn a_restarted_or_different_process_is_different() {
        assert!(matches!(
            check(&fp(), Some(&Fingerprint { create: 1, ..fp() })),
            Identity::Different(_)
        ));
        assert!(matches!(
            check(&fp(), Some(&Fingerprint { pid: 5, ..fp() })),
            Identity::Different(_)
        ));
    }

    #[test]
    fn style_changes_an_app_can_make_are_doubt_not_proof_and_are_not_acted_on() {
        let doubtful = check(
            &fp(),
            Some(&Fingerprint {
                tool_window: true,
                ..fp()
            }),
        );
        assert!(matches!(doubtful, Identity::Uncertain(_)));
        assert!(!doubtful.allows_apply());
        assert!(!doubtful.allows_restore());
        assert!(doubtful.reason().contains("style"));
    }

    #[test]
    fn class_comparison_ignores_case_only() {
        let live = Fingerprint {
            class: "NOTEPAD".into(),
            ..fp()
        };
        assert_eq!(check(&fp(), Some(&live)), Identity::Same);
    }

    #[test]
    fn fingerprints_survive_a_round_trip_through_the_snapshot_file_format() {
        let text = serde_json::to_string(&fp()).unwrap();
        assert!(
            !text.contains("title"),
            "no window title is ever part of identity"
        );
        assert_eq!(serde_json::from_str::<Fingerprint>(&text).unwrap(), fp());
    }
}
