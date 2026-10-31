//! The trial ("try it, then Keep or Revert") as a pure state machine.
//!
//! This is the *only* place that decides what a trial may do. The Windows host owns one of
//! these and calls into it; nothing about timing, restoring or persistence is decided
//! anywhere else, so it can be tested without Windows.
//!
//! Rules, each covered by a test:
//!  * Starting a trial never writes anything. The running config changes in memory only.
//!  * A trial can only change *window looks*. The shell, startup, logging and the legacy
//!    layer keep their current values (`constrain`).
//!  * Time is event-driven: the host asks "has it expired?" on its own timer tick.
//!  * Revert, expiry and any outside change to the config all return the config to restore.
//!  * Keep returns nothing to restore; the caller then saves exactly once.
//!  * Starting again while a trial runs keeps the *original* config as the one to restore.

use crate::Config;

pub const TRIAL_MS: u64 = 15_000;

/// What a trial config is allowed to differ in. Everything with a side effect outside window
/// frames (registry, hooks, logging) is taken from the running config.
pub fn constrain(next: Config, running: &Config) -> Config {
    let mut out = next;
    out.shell = running.shell.clone();
    out.experimental_dark_legacy = running.experimental_dark_legacy;
    out.legacy = running.legacy.clone();
    out.logging = running.logging;
    out
}

#[derive(Debug, Clone)]
struct Active {
    previous: Config,
    deadline: u64,
}

#[derive(Debug, Default)]
pub struct TrialMachine {
    active: Option<Active>,
}

impl TrialMachine {
    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }

    pub fn deadline(&self) -> Option<u64> {
        self.active.as_ref().map(|trial| trial.deadline)
    }

    /// Begin (or extend) a trial. `running` is the config in effect right now.
    pub fn start(&mut self, now: u64, duration_ms: u64, running: &Config) {
        let deadline = now.saturating_add(duration_ms);
        match self.active.as_mut() {
            Some(trial) => trial.deadline = deadline,
            None => {
                self.active = Some(Active {
                    previous: running.clone(),
                    deadline,
                })
            }
        }
    }

    pub fn expired(&self, now: u64) -> bool {
        self.active
            .as_ref()
            .is_some_and(|trial| now >= trial.deadline)
    }

    /// End the trial. `keep == true` returns `None` (nothing to restore; the caller saves).
    /// Otherwise returns the config to put back. No trial running returns `None`.
    pub fn finish(&mut self, keep: bool) -> Option<Config> {
        let trial = self.active.take()?;
        if keep {
            None
        } else {
            Some(trial.previous)
        }
    }

    /// Something else is about to change the permanent config (a tray preset, a reload from
    /// disk, a scene switch). A trial cannot survive that: put the old config back first so
    /// the new change starts from a clean, known state.
    pub fn cancel_for_external_change(&mut self) -> Option<Config> {
        self.finish(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_config;

    fn config(json: &str) -> Config {
        parse_config(json).unwrap()
    }

    /// A stand-in for the pair "config in memory" + "config.json on disk", so persistence
    /// can be asserted at every step.
    struct World {
        running: Config,
        disk: Config,
        disk_writes: u32,
        machine: TrialMachine,
    }

    impl World {
        fn new(initial: &Config) -> Self {
            Self {
                running: initial.clone(),
                disk: initial.clone(),
                disk_writes: 0,
                machine: TrialMachine::default(),
            }
        }

        fn try_look(&mut self, now: u64, next: Config) {
            self.machine.start(now, TRIAL_MS, &self.running);
            self.running = constrain(next, &self.running);
        }

        fn tick(&mut self, now: u64) {
            if self.machine.expired(now) {
                if let Some(previous) = self.machine.finish(false) {
                    self.running = previous;
                }
            }
        }

        fn keep(&mut self) {
            assert!(self.machine.finish(true).is_none());
            self.disk = self.running.clone();
            self.disk_writes += 1;
        }

        fn revert(&mut self) {
            if let Some(previous) = self.machine.finish(false) {
                self.running = previous;
            }
        }
    }

    fn base() -> Config {
        config(
            r#"{"default":{"backdrop":"mica"},"shell":{"oledTaskbar":false,"blackShell":false}}"#,
        )
    }

    fn other() -> Config {
        config(
            r#"{"default":{"backdrop":"none"},"shell":{"oledTaskbar":true,"blackShell":true},"legacy":{"palette":"black"},"logging":false,"experimentalDarkLegacy":true}"#,
        )
    }

    #[test]
    fn starting_a_trial_changes_the_look_but_writes_nothing() {
        let mut world = World::new(&base());
        world.try_look(1_000, other());
        assert!(world.machine.is_active());
        assert_eq!(world.disk_writes, 0, "a trial must never write the config");
        assert_eq!(world.disk.defaults.backdrop, base().defaults.backdrop);
        assert_eq!(world.running.defaults.backdrop, other().defaults.backdrop);
    }

    #[test]
    fn a_trial_cannot_change_the_shell_the_legacy_layer_or_logging() {
        let mut world = World::new(&base());
        world.try_look(0, other());
        assert!(!world.running.shell.oled_taskbar && !world.running.shell.black_shell);
        assert!(!world.running.experimental_dark_legacy);
        assert_eq!(world.running.legacy.palette, base().legacy.palette);
        assert_eq!(world.running.logging, base().logging);
    }

    #[test]
    fn it_reverts_by_itself_exactly_at_the_deadline() {
        let mut world = World::new(&base());
        world.try_look(1_000, other());
        world.tick(1_000 + TRIAL_MS - 1);
        assert!(world.machine.is_active(), "still running one tick early");
        world.tick(1_000 + TRIAL_MS);
        assert!(!world.machine.is_active());
        assert_eq!(world.running.defaults.backdrop, base().defaults.backdrop);
        assert_eq!(world.disk_writes, 0);
    }

    #[test]
    fn revert_restores_and_never_persists() {
        let mut world = World::new(&base());
        world.try_look(0, other());
        world.revert();
        assert_eq!(world.running.defaults.backdrop, base().defaults.backdrop);
        assert_eq!(world.disk_writes, 0);
        // A second revert with nothing running is harmless.
        world.revert();
        assert!(!world.machine.is_active());
    }

    #[test]
    fn keep_saves_exactly_once_and_what_is_saved_is_what_was_tried() {
        let mut world = World::new(&base());
        world.try_look(0, other());
        world.keep();
        assert_eq!(world.disk_writes, 1);
        assert_eq!(world.disk.defaults.backdrop, other().defaults.backdrop);
        // The shell part was not part of the trial, so it is not smuggled onto disk by Keep.
        assert!(!world.disk.shell.black_shell);
        // After Keep the expiry timer must not undo it.
        world.tick(1_000_000);
        assert_eq!(world.running.defaults.backdrop, other().defaults.backdrop);
    }

    #[test]
    fn trying_twice_still_restores_the_original_not_the_first_trial() {
        let mut world = World::new(&base());
        world.try_look(0, config(r#"{"default":{"backdrop":"acrylic"}}"#));
        world.try_look(5_000, other());
        world.revert();
        assert_eq!(world.running.defaults.backdrop, base().defaults.backdrop);
    }

    #[test]
    fn trying_again_extends_the_deadline() {
        let mut machine = TrialMachine::default();
        machine.start(0, TRIAL_MS, &base());
        machine.start(10_000, TRIAL_MS, &base());
        assert_eq!(machine.deadline(), Some(10_000 + TRIAL_MS));
        assert!(!machine.expired(TRIAL_MS + 1));
    }

    #[test]
    fn an_outside_change_cancels_the_trial_and_hands_back_the_old_config() {
        let mut machine = TrialMachine::default();
        machine.start(0, TRIAL_MS, &base());
        let restore = machine
            .cancel_for_external_change()
            .expect("a config to restore");
        assert_eq!(restore.defaults.backdrop, base().defaults.backdrop);
        assert!(!machine.is_active());
        assert!(machine.cancel_for_external_change().is_none());
    }

    #[test]
    fn a_clock_that_overflows_does_not_panic_or_expire_early() {
        let mut machine = TrialMachine::default();
        machine.start(u64::MAX - 5, TRIAL_MS, &base());
        assert_eq!(machine.deadline(), Some(u64::MAX));
        assert!(!machine.expired(1));
    }
}
