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
}
