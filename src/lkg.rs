//! Choosing which config to run: the one on disk, the last known good copy, or the defaults.
//! Pure so the failure paths are tested. A corrupted `config.json` must never leave the user
//! with theming silently off and no explanation, and never overwrite their broken file.

use crate::config::{bundled_default_json, parse_config};
use crate::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// `config.json` parsed.
    Primary,
    /// `config.json` did not parse; `config.lkg.json` did.
    LastKnownGood,
    /// Neither parsed. Defaults, with theming switched off so nothing surprising happens.
    SafeDefaults,
    /// There is no config yet. Defaults as shipped.
    Fresh,
}

#[derive(Debug, Clone)]
pub struct Picked {
    pub config: Config,
    pub source: Source,
    /// Something to tell the user, when the primary file was not used.
    pub warning: Option<String>,
}

/// A config file that predates schema 2 (no `schemaVersion`, or 1). 3.5 can read it, but a
/// 3.0 build cannot read what 3.5 saves, so the original is copied aside once.
pub fn is_pre_schema_2(text: &str) -> bool {
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(value) => value
            .get("schemaVersion")
            .and_then(|v| v.as_u64())
            .map(|v| v < 2)
            .unwrap_or(true),
        Err(_) => false,
    }
}

/// How long a config must have been running before it counts as "known good".
pub const STABLE_MS: u64 = 60_000;
/// The least a run must last before a *clean exit* can promote the config it ran.
pub const MIN_CLEAN_MS: u64 = 10_000;

/// When a config becomes the "last known good" copy.
///
/// Parsing is not enough: a config can parse and still misbehave once applied, and saving it as
/// the recovery copy at that moment would leave nothing to recover to. A config is only
/// promoted after it was applied and then survived (`STABLE_MS` of running, or a clean exit
/// after at least `MIN_CLEAN_MS`), and never when the run was not a real test of it: safe mode
/// (experimental parts were forced off), a running trial, a pause, or styling switched off.
/// "Applied" is not "looks right"; this only says it did not crash or get abandoned.
#[derive(Debug, Default)]
pub struct Promotion {
    candidate: Option<(String, u64)>,
}

impl Promotion {
    /// A config just parsed and is being applied. A newer offer replaces an older one and
    /// restarts the clock, so the config that ran is always the config that is promoted.
    pub fn offer(&mut self, text: &str, now_ms: u64) {
        self.candidate = Some((text.to_string(), now_ms));
    }

    pub fn is_pending(&self) -> bool {
        self.candidate.is_some()
    }

    /// The text to save now, if it has run long enough while `eligible`. Taking it clears the
    /// candidate; not eligible keeps it for a later check.
    pub fn stable(&mut self, now_ms: u64, eligible: bool) -> Option<String> {
        self.take_if(now_ms, STABLE_MS, eligible)
    }

    /// The same at a clean shutdown.
    pub fn clean_exit(&mut self, now_ms: u64, eligible: bool) -> Option<String> {
        self.take_if(now_ms, MIN_CLEAN_MS, eligible)
    }

    /// Something went wrong that makes the running config suspect (a crash is the usual case,
    /// and nothing runs to say so; this is for failures that are seen): never promote it.
    pub fn reject(&mut self) {
        self.candidate = None;
    }

    fn take_if(&mut self, now_ms: u64, after_ms: u64, eligible: bool) -> Option<String> {
        let (_, since) = self.candidate.as_ref()?;
        if !eligible || now_ms.saturating_sub(*since) < after_ms {
            return None;
        }
        self.candidate.take().map(|(text, _)| text)
    }
}

pub fn pick(primary: Option<&str>, last_known_good: Option<&str>) -> Picked {
    let defaults = || parse_config(bundled_default_json()).expect("bundled config");
    let Some(text) = primary else {
        return Picked {
            config: defaults(),
            source: Source::Fresh,
            warning: None,
        };
    };
    match parse_config(text) {
        Ok(config) => Picked {
            config,
            source: Source::Primary,
            warning: None,
        },
        Err(error) => match last_known_good.map(parse_config) {
            Some(Ok(config)) => Picked {
                config,
                source: Source::LastKnownGood,
                warning: Some(format!(
                    "config.json was not applied ({error}). Running the last known good config instead. Your file was left as it is."
                )),
            },
            _ => {
                let mut config = defaults();
                config.enabled = false;
                Picked {
                    config,
                    source: Source::SafeDefaults,
                    warning: Some(format!(
                        "config.json was not applied, so theming is off. {error}"
                    )),
                }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"{"enabled":true,"rules":[{"process":"a.exe","backdrop":"none"}]}"#;
    const OLDER_GOOD: &str = r#"{"enabled":true}"#;

    #[test]
    fn a_good_file_wins_and_a_missing_file_means_fresh() {
        let picked = pick(Some(GOOD), Some(OLDER_GOOD));
        assert_eq!(picked.source, Source::Primary);
        assert_eq!(picked.config.rules.len(), 1);
        assert!(picked.warning.is_none());
        assert_eq!(pick(None, Some(GOOD)).source, Source::Fresh);
    }

    #[test]
    fn a_corrupted_file_falls_back_to_the_last_known_good_one_and_says_so() {
        for broken in [
            "{not json",
            "",
            r#"{"rules":[{"backdrop":"none"}]}"#,
            r#"{"schemaVersion":99}"#,
        ] {
            let picked = pick(Some(broken), Some(GOOD));
            assert_eq!(picked.source, Source::LastKnownGood, "{broken:?}");
            assert_eq!(picked.config.rules.len(), 1);
            assert!(picked.warning.as_deref().unwrap().contains("left as it is"));
        }
    }

    #[test]
    fn with_nothing_usable_theming_is_off_and_the_reason_is_given() {
        let picked = pick(Some("{broken"), Some("{also broken"));
        assert_eq!(picked.source, Source::SafeDefaults);
        assert!(!picked.config.enabled);
        assert!(picked.warning.is_some());
        let picked = pick(Some("{broken"), None);
        assert_eq!(picked.source, Source::SafeDefaults);
    }

    #[test]
    fn a_config_is_not_known_good_just_because_it_parsed() {
        let mut p = Promotion::default();
        p.offer("{\"a\":1}", 1_000);
        assert!(p.is_pending());
        assert_eq!(p.stable(1_000, true), None, "nothing is promoted at once");
        assert_eq!(
            p.stable(1_000 + STABLE_MS - 1, true),
            None,
            "not before it has run"
        );
        assert_eq!(
            p.stable(1_000 + STABLE_MS, true),
            Some("{\"a\":1}".to_string())
        );
        assert!(!p.is_pending(), "promoting clears the candidate");
    }

    #[test]
    fn a_newer_offer_restarts_the_clock_and_replaces_the_text() {
        let mut p = Promotion::default();
        p.offer("old", 0);
        p.offer("new", 50_000);
        assert_eq!(
            p.stable(STABLE_MS, true),
            None,
            "the new text has run for 10 s only"
        );
        assert_eq!(p.stable(50_000 + STABLE_MS, true), Some("new".to_string()));
    }

    #[test]
    fn runs_that_do_not_test_the_config_never_promote_it() {
        // Safe mode, a trial, a pause or styling switched off: the caller says "not eligible".
        let mut p = Promotion::default();
        p.offer("x", 0);
        assert_eq!(p.stable(10 * STABLE_MS, false), None);
        assert!(
            p.is_pending(),
            "it stays a candidate for a later, eligible check"
        );
        assert_eq!(p.stable(10 * STABLE_MS, true), Some("x".to_string()));
    }

    #[test]
    fn a_clean_exit_promotes_only_after_a_real_run() {
        let mut p = Promotion::default();
        p.offer("x", 0);
        assert_eq!(
            p.clean_exit(MIN_CLEAN_MS - 1, true),
            None,
            "start and quit proves nothing"
        );
        assert_eq!(p.clean_exit(MIN_CLEAN_MS, true), Some("x".to_string()));
        assert_eq!(
            p.clean_exit(10 * STABLE_MS, true),
            None,
            "nothing is left to promote"
        );
    }

    #[test]
    fn a_rejected_config_is_never_promoted() {
        let mut p = Promotion::default();
        p.offer("x", 0);
        p.reject();
        assert_eq!(p.stable(10 * STABLE_MS, true), None);
        assert_eq!(p.clean_exit(10 * STABLE_MS, true), None);
    }

    #[test]
    fn no_candidate_means_nothing_to_promote() {
        let mut p = Promotion::default();
        assert!(!p.is_pending());
        assert_eq!(p.stable(u64::MAX, true), None);
        assert_eq!(p.clean_exit(u64::MAX, true), None);
    }
}
