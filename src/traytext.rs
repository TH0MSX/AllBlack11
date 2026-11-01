//! What the tray icon's tooltip says, and what quitting would undo.
//!
//! Pure, so the wording is tested: it must say whether styling is running, paused, off or held
//! back by high contrast, and mention the states a person would want to know about (a look being
//! tried, safe mode, the experimental classic-dialog layer, reduced effects) without exceeding the
//! 127 characters a tray tooltip can hold.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TrayState {
    pub enabled: bool,
    pub paused: bool,
    pub high_contrast: bool,
    pub windows: usize,
    pub reduced_effects: bool,
    pub safe_mode: bool,
    pub trying_a_look: bool,
    /// The experimental classic-dialog layer is on and running.
    pub classic_layer: bool,
}

pub const TIP_LIMIT: usize = 127;

pub fn tip(state: &TrayState) -> String {
    let head = if state.paused {
        "AllBlack11: paused".to_string()
    } else if state.high_contrast {
        "AllBlack11: paused, high contrast".to_string()
    } else if !state.enabled {
        "AllBlack11: off".to_string()
    } else if state.windows == 1 {
        "AllBlack11: 1 window".to_string()
    } else {
        format!("AllBlack11: {} windows", state.windows)
    };
    let mut notes: Vec<&str> = Vec::new();
    if state.trying_a_look {
        notes.push("trying a look");
    }
    if state.safe_mode {
        notes.push("safe mode");
    }
    if state.reduced_effects && state.enabled && !state.paused && !state.high_contrast {
        notes.push("reduced effects");
    }
    if state.classic_layer {
        notes.push("classic dialogs on");
    }
    let mut text = if notes.is_empty() {
        head
    } else {
        format!("{head} ({})", notes.join(", "))
    };
    if text.chars().count() > TIP_LIMIT {
        text = text.chars().take(TIP_LIMIT - 1).collect::<String>() + "…";
    }
    text
}

/// What quitting would undo or stop right now. Empty means there is nothing to decide, and Exit
/// can simply exit (every styled window is put back either way).
pub fn exit_consequences(
    trying_a_look: bool,
    classic_layer: bool,
    helper: bool,
) -> Vec<&'static str> {
    let mut notes = Vec::new();
    if trying_a_look {
        notes.push(
            "The look you are trying will be reverted. Nothing is saved unless you pressed Keep.",
        );
    }
    if classic_layer {
        notes.push(
            "The dark classic-dialog layer will be removed from the apps it was loaded into.",
        );
    }
    if helper {
        notes.push("The x64 helper will stop.");
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on() -> TrayState {
        TrayState {
            enabled: true,
            windows: 12,
            ..TrayState::default()
        }
    }

    #[test]
    fn the_plain_states_read_clearly() {
        assert_eq!(tip(&on()), "AllBlack11: 12 windows");
        assert_eq!(
            tip(&TrayState { windows: 1, ..on() }),
            "AllBlack11: 1 window"
        );
        assert_eq!(
            tip(&TrayState {
                paused: true,
                ..on()
            }),
            "AllBlack11: paused"
        );
        assert_eq!(
            tip(&TrayState {
                high_contrast: true,
                ..on()
            }),
            "AllBlack11: paused, high contrast"
        );
        assert_eq!(
            tip(&TrayState {
                enabled: false,
                ..on()
            }),
            "AllBlack11: off"
        );
    }

    #[test]
    fn paused_wins_over_everything_else_in_the_headline() {
        let s = TrayState {
            paused: true,
            high_contrast: true,
            enabled: false,
            ..on()
        };
        assert_eq!(tip(&s), "AllBlack11: paused");
    }

    #[test]
    fn notable_states_are_named() {
        let s = TrayState {
            safe_mode: true,
            reduced_effects: true,
            classic_layer: true,
            trying_a_look: true,
            ..on()
        };
        assert_eq!(
            tip(&s),
            "AllBlack11: 12 windows (trying a look, safe mode, reduced effects, classic dialogs on)"
        );
    }

    #[test]
    fn reduced_effects_is_not_claimed_while_nothing_is_styled() {
        let s = TrayState {
            paused: true,
            reduced_effects: true,
            ..on()
        };
        assert!(!tip(&s).contains("reduced"));
    }

    #[test]
    fn it_always_fits_a_tray_tooltip() {
        for bits in 0u32..256 {
            let s = TrayState {
                enabled: bits & 1 != 0,
                paused: bits & 2 != 0,
                high_contrast: bits & 4 != 0,
                windows: if bits & 8 != 0 { 99_999 } else { 0 },
                reduced_effects: bits & 16 != 0,
                safe_mode: bits & 32 != 0,
                trying_a_look: bits & 64 != 0,
                classic_layer: bits & 128 != 0,
            };
            assert!(tip(&s).chars().count() <= TIP_LIMIT, "{s:?}");
        }
    }

    #[test]
    fn quitting_only_asks_when_it_would_undo_or_stop_something() {
        assert!(exit_consequences(false, false, false).is_empty());
        let all = exit_consequences(true, true, true);
        assert_eq!(all.len(), 3);
        assert!(all[0].contains("reverted"));
        assert!(all.iter().all(|n| n.ends_with('.')));
    }
}
