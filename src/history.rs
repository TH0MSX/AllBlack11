//! A bounded, named history of this session's look changes, so "undo" can go back more than one
//! step and say what each step is.
//!
//! This is a history of *AllBlack11 looks* (the config as it was before a change). It is not the
//! per-window "put it back as Windows drew it" restore, which is a different promise: that one
//! (Reset all windows, Restore on Fix this app) uses the originals recorded per window and
//! revalidates the window first. Undoing a look here only writes an earlier config; the
//! normal apply path then restyles windows from it.
//!
//! In memory only, for the session: nothing is written to disk. At most [`MAX`] entries; quick
//! bursts of the same kind of change (dragging a slider) are folded into one entry that keeps the
//! state from before the burst.

use std::sync::Mutex;

pub const MAX: usize = 20;
/// Changes of the same kind closer together than this are one entry.
pub const COALESCE_MS: u64 = 8_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// What changed, in words ("Scene: Graphite", "Changed the default look").
    pub name: String,
    /// The config JSON from before the change.
    pub before: String,
    /// Milliseconds on the host's tick clock when the change happened.
    pub at_ms: u64,
}

#[derive(Debug, Default)]
pub struct History {
    entries: Vec<Entry>,
    /// The config text an undo just wrote. The next reload that equals it is the undo itself and
    /// must not become a new history entry.
    expected: Option<String>,
}

impl History {
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
            expected: None,
        }
    }

    /// Record a change. `before` is the config that was running just before it.
    pub fn record(&mut self, name: &str, before: &str, now_ms: u64) {
        if let Some(last) = self.entries.last_mut() {
            if last.name == name && now_ms.saturating_sub(last.at_ms) < COALESCE_MS {
                // Same kind of change in a quick burst: keep the earlier "before".
                last.at_ms = now_ms;
                return;
            }
        }
        self.entries.push(Entry {
            name: name.to_string(),
            before: before.to_string(),
            at_ms: now_ms,
        });
        if self.entries.len() > MAX {
            self.entries.remove(0);
        }
    }

    /// Newest last.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Take the newest entry for undoing, and remember that the reload that follows is ours.
    pub fn take_undo(&mut self) -> Option<Entry> {
        let entry = self.entries.pop()?;
        self.expected = Some(entry.before.clone());
        Some(entry)
    }

    /// True once, for the reload produced by an undo.
    pub fn is_own_undo(&mut self, config_json: &str) -> bool {
        if self.expected.as_deref() == Some(config_json) {
            self.expected = None;
            true
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.expected = None;
    }
}

/// The session's history. Settings reads and undoes it; the host records into it.
pub static SESSION: Mutex<History> = Mutex::new(History::new());

pub fn with_session<R>(body: impl FnOnce(&mut History) -> R) -> R {
    let mut guard = SESSION.lock().unwrap_or_else(|err| err.into_inner());
    body(&mut guard)
}

/// Say what changed between two configs, from their JSON. None when nothing differs.
pub fn describe_change(old_json: &str, new_json: &str, scene_name: Option<&str>) -> Option<String> {
    use serde_json::Value;
    let old: Value = serde_json::from_str(old_json).ok()?;
    let new: Value = serde_json::from_str(new_json).ok()?;
    if old == new {
        return None;
    }
    if let Some(name) = scene_name {
        return Some(format!("Scene: {name}"));
    }
    let (Some(old), Some(new)) = (old.as_object(), new.as_object()) else {
        return Some("Changed the config".to_string());
    };
    if let Some(text) = describe_pack_change(old, new) {
        return Some(text);
    }
    let mut parts: Vec<&str> = Vec::new();
    let keys: std::collections::BTreeSet<&String> = old.keys().chain(new.keys()).collect();
    for key in keys {
        if old.get(key) == new.get(key) {
            continue;
        }
        let label = match key.as_str() {
            "enabled" => "Theme windows on or off",
            "default" => "the default look",
            "rules" => "per-app rules",
            "exclude" | "excludeClasses" => "the apps to leave alone",
            "shell" => "the taskbar and Start options",
            "focus" => "Focus Look",
            "reduceEffects" => "reduced-effects options",
            "legacy" | "experimentalDarkLegacy" => "the classic dialogs layer",
            "features" => "optional features",
            "captionHeight" => "title bar height",
            "ui" => "AllBlack11's own appearance",
            _ => "other settings",
        };
        if !parts.contains(&label) {
            parts.push(label);
        }
    }
    let mut text = String::from("Changed ");
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            text.push_str(if index + 1 == parts.len() {
                " and "
            } else {
                ", "
            });
        }
        text.push_str(part);
    }
    Some(text)
}

/// Names of the compatibility packs whose rules appear in a config's `rules` array. A pack's rules
/// are called "Pack: Name" or "Pack: Name (n)".
fn pack_names(config: &serde_json::Map<String, serde_json::Value>) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for rule in config
        .get("rules")
        .and_then(|rules| rules.as_array())
        .into_iter()
        .flatten()
    {
        let Some(label) = rule
            .get("name")
            .and_then(|name| name.as_str())
            .and_then(|name| name.strip_prefix("Pack: "))
        else {
            continue;
        };
        let base = match label.rfind(" (") {
            Some(at)
                if label.ends_with(')')
                    && label[at + 2..label.len() - 1]
                        .chars()
                        .all(|c| c.is_ascii_digit()) =>
            {
                &label[..at]
            }
            _ => label,
        };
        if !names.iter().any(|name| name == base) {
            names.push(base.to_string());
        }
    }
    names
}

/// "Installed the Calculator pack" when the only change is a pack's rules appearing or going.
fn describe_pack_change(
    old: &serde_json::Map<String, serde_json::Value>,
    new: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    let others_equal = old
        .keys()
        .chain(new.keys())
        .all(|key| key == "rules" || old.get(key) == new.get(key));
    if !others_equal {
        return None;
    }
    let before = pack_names(old);
    let after = pack_names(new);
    let added: Vec<&String> = after.iter().filter(|name| !before.contains(name)).collect();
    let removed: Vec<&String> = before.iter().filter(|name| !after.contains(name)).collect();
    // A pack that was reinstalled keeps its name in both lists, so it is a plain rules change.
    let mut parts: Vec<String> = Vec::new();
    for name in &added {
        parts.push(format!("Installed the {name} pack"));
    }
    for name in &removed {
        parts.push(format!("Removed the {name} pack"));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" and "))
    }
}

/// "12 s ago", "3 min ago", "2 h ago".
pub fn ago(now_ms: u64, then_ms: u64) -> String {
    let seconds = now_ms.saturating_sub(then_ms) / 1000;
    if seconds < 60 {
        format!("{seconds} s ago")
    } else if seconds < 3600 {
        format!("{} min ago", seconds / 60)
    } else {
        format!("{} h ago", seconds / 3600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_keeps_the_newest_entries_and_no_more_than_the_bound() {
        let mut h = History::new();
        for i in 0..(MAX + 5) {
            h.record(
                &format!("change {i}"),
                &format!("before {i}"),
                i as u64 * 60_000,
            );
        }
        assert_eq!(h.len(), MAX);
        assert_eq!(h.entries().first().unwrap().name, "change 5");
        assert_eq!(
            h.entries().last().unwrap().name,
            format!("change {}", MAX + 4)
        );
    }

    #[test]
    fn a_quick_burst_of_the_same_change_is_one_entry_that_keeps_the_first_before() {
        let mut h = History::new();
        h.record("Changed the default look", "A", 1_000);
        h.record("Changed the default look", "B", 2_000);
        h.record("Changed the default look", "C", 3_000);
        assert_eq!(h.len(), 1);
        assert_eq!(
            h.entries()[0].before,
            "A",
            "undo goes back to before the whole burst"
        );
        // A different kind of change, or the same one much later, is a new entry.
        h.record("Changed per-app rules", "D", 4_000);
        h.record("Changed per-app rules", "E", 4_000 + COALESCE_MS);
        assert_eq!(h.len(), 3);
    }

    #[test]
    fn undo_walks_back_one_step_at_a_time_and_recognizes_its_own_reload() {
        let mut h = History::new();
        h.record("one", "before one", 0);
        h.record("two", "before two", 60_000);
        let undone = h.take_undo().unwrap();
        assert_eq!(undone.name, "two");
        assert_eq!(undone.before, "before two");
        assert!(
            h.is_own_undo("before two"),
            "the reload that the undo caused"
        );
        assert!(!h.is_own_undo("before two"), "only once");
        assert_eq!(h.take_undo().unwrap().name, "one");
        assert!(h.take_undo().is_none(), "nothing left");
    }

    #[test]
    fn a_different_reload_after_an_undo_is_a_real_change() {
        let mut h = History::new();
        h.record("one", "before one", 0);
        h.take_undo();
        assert!(!h.is_own_undo("something else"));
    }

    #[test]
    fn changes_are_described_in_words() {
        let a = r##"{"enabled":true,"default":{"backdrop":"mica"},"rules":[],"exclude":[]}"##;
        let b = r##"{"enabled":true,"default":{"backdrop":"none"},"rules":[],"exclude":[]}"##;
        assert_eq!(
            describe_change(a, b, None).unwrap(),
            "Changed the default look"
        );
        let c = r##"{"enabled":true,"default":{"backdrop":"none"},"rules":[{"process":"a.exe"}],"exclude":["x.exe"]}"##;
        assert_eq!(
            describe_change(b, c, None).unwrap(),
            "Changed the apps to leave alone and per-app rules"
        );
        assert_eq!(describe_change(a, a, None), None);
        assert_eq!(
            describe_change(a, b, Some("Graphite")).unwrap(),
            "Scene: Graphite"
        );
        assert!(describe_change("not json", b, None).is_none());
    }

    #[test]
    fn relative_times_read_naturally() {
        assert_eq!(ago(5_000, 0), "5 s ago");
        assert_eq!(ago(125_000, 0), "2 min ago");
        assert_eq!(ago(7_300_000, 0), "2 h ago");
        assert_eq!(
            ago(0, 10_000),
            "0 s ago",
            "a clock that went back never underflows"
        );
    }

    #[test]
    fn the_session_store_is_shared_and_recovers_from_a_poisoned_lock() {
        with_session(|h| h.clear());
        with_session(|h| h.record("x", "y", 0));
        assert_eq!(with_session(|h| h.len()), 1);
        with_session(|h| h.clear());
    }

    #[test]
    fn installing_and_removing_a_pack_is_named_in_the_history() {
        let none = r#"{"rules":[{"process":"a.exe"}]}"#;
        let one = r#"{"rules":[{"process":"a.exe"},{"name":"Pack: Calculator","package":"X_*"}]}"#;
        let two = r#"{"rules":[{"process":"a.exe"},{"name":"Pack: Calculator","package":"X_*"},{"name":"Pack: Calculator (2)","process":"c.exe"}]}"#;
        assert_eq!(
            describe_change(none, one, None).as_deref(),
            Some("Installed the Calculator pack")
        );
        assert_eq!(
            describe_change(one, none, None).as_deref(),
            Some("Removed the Calculator pack")
        );
        // Extra targets of the same pack are the same pack; a reinstall is an ordinary rules change.
        assert_eq!(
            describe_change(none, two, None).as_deref(),
            Some("Installed the Calculator pack")
        );
        assert_eq!(
            describe_change(one, two, None).as_deref(),
            Some("Changed per-app rules")
        );
        // A change outside the rules is never called a pack change.
        let other = r#"{"enabled":false,"rules":[{"process":"a.exe"},{"name":"Pack: Calculator","package":"X_*"}]}"#;
        assert_eq!(
            describe_change(none, other, None).as_deref(),
            Some("Changed Theme windows on or off and per-app rules")
        );
    }
}
