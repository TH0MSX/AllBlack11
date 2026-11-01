//! The compatibility notebook: what the user (or a smoke test) found for an app on a given
//! Windows build and CPU. Local only. No telemetry, no account, no network.
//!
//! Statuses are about *appearance*, not API success: a window can accept every attribute and
//! still look unchanged. Only a person looking at the app can mark it "verified".

use serde::{Deserialize, Serialize};

pub const NOTEBOOK_VERSION: u32 = 1;
const MAX_ENTRIES: usize = 500;
const MAX_NOTE: usize = 240;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Verified,
    Partial,
    Experimental,
    Unsupported,
    Untested,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Verified => "works",
            Status::Partial => "partly works",
            Status::Experimental => "experimental",
            Status::Unsupported => "does not work",
            Status::Untested => "not tested",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Entry {
    /// Lowercase executable name, or a package family name.
    pub key: String,
    /// Windows build the observation was made on. Results differ between builds.
    pub build: u32,
    /// "arm64", "x64" or "x64-emulated": the process, not the PC.
    pub arch: String,
    pub status: Status,
    #[serde(default)]
    pub note: String,
    /// Unix seconds.
    #[serde(default)]
    pub updated: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Notebook {
    #[serde(default = "version_one")]
    pub version: u32,
    #[serde(default)]
    pub entries: Vec<Entry>,
}

fn version_one() -> u32 {
    NOTEBOOK_VERSION
}

impl Default for Notebook {
    fn default() -> Self {
        Self {
            version: NOTEBOOK_VERSION,
            entries: Vec::new(),
        }
    }
}

impl Notebook {
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.trim().is_empty() {
            return Ok(Self::default());
        }
        let book: Notebook = serde_json::from_str(text).map_err(|err| err.to_string())?;
        if book.version > NOTEBOOK_VERSION {
            return Err(format!(
                "compat.json is version {}, newer than this build understands",
                book.version
            ));
        }
        Ok(book)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }

    pub fn get(&self, key: &str, build: u32, arch: &str) -> Option<&Entry> {
        let key = key.to_ascii_lowercase();
        self.entries
            .iter()
            .find(|entry| entry.key == key && entry.build == build && entry.arch == arch)
    }

    /// Never returns anything stronger than what was recorded; no record means untested.
    pub fn status_for(&self, key: &str, build: u32, arch: &str) -> Status {
        self.get(key, build, arch)
            .map(|entry| entry.status)
            .unwrap_or(Status::Untested)
    }

    pub fn set(&mut self, key: &str, build: u32, arch: &str, status: Status, note: &str, now: u64) {
        let key = key.to_ascii_lowercase();
        let note: String = note
            .chars()
            .filter(|c| !c.is_control())
            .take(MAX_NOTE)
            .collect();
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.key == key && entry.build == build && entry.arch == arch)
        {
            entry.status = status;
            entry.note = note;
            entry.updated = now;
            return;
        }
        self.entries.push(Entry {
            key,
            build,
            arch: arch.to_string(),
            status,
            note,
            updated: now,
        });
        if self.entries.len() > MAX_ENTRIES {
            // Drop the stalest record.
            if let Some(oldest) = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, entry)| entry.updated)
                .map(|(index, _)| index)
            {
                self.entries.remove(oldest);
            }
        }
    }

    /// Back to "not tested". Returns whether anything was removed.
    pub fn reset(&mut self, key: &str, build: u32, arch: &str) -> bool {
        let key = key.to_ascii_lowercase();
        let before = self.entries.len();
        self.entries
            .retain(|entry| !(entry.key == key && entry.build == build && entry.arch == arch));
        self.entries.len() != before
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_record_means_untested_and_results_are_per_build_and_arch() {
        let mut book = Notebook::default();
        assert_eq!(book.status_for("app.exe", 26100, "arm64"), Status::Untested);
        book.set(
            "App.exe",
            26100,
            "arm64",
            Status::Verified,
            "frame and backdrop fine",
            10,
        );
        assert_eq!(book.status_for("app.exe", 26100, "arm64"), Status::Verified);
        // Another build or another process architecture is a different question.
        assert_eq!(book.status_for("app.exe", 22631, "arm64"), Status::Untested);
        assert_eq!(
            book.status_for("app.exe", 26100, "x64-emulated"),
            Status::Untested
        );
    }

    #[test]
    fn set_updates_in_place_and_reset_returns_to_untested() {
        let mut book = Notebook::default();
        book.set("a.exe", 1, "x64", Status::Partial, "caption only", 1);
        book.set(
            "a.exe",
            1,
            "x64",
            Status::Unsupported,
            "custom title bar",
            2,
        );
        assert_eq!(book.entries.len(), 1);
        assert_eq!(book.get("a.exe", 1, "x64").unwrap().updated, 2);
        assert!(book.reset("A.EXE", 1, "x64"));
        assert!(!book.reset("a.exe", 1, "x64"));
        assert_eq!(book.status_for("a.exe", 1, "x64"), Status::Untested);
    }

    #[test]
    fn round_trip_notes_are_bounded_and_future_versions_are_refused() {
        let mut book = Notebook::default();
        book.set(
            "a.exe",
            1,
            "x64",
            Status::Experimental,
            &"x".repeat(1000),
            3,
        );
        assert_eq!(book.entries[0].note.chars().count(), MAX_NOTE);
        let parsed = Notebook::parse(&book.to_json()).unwrap();
        assert_eq!(parsed, book);
        assert!(Notebook::parse(r#"{"version":99,"entries":[]}"#).is_err());
        assert!(Notebook::parse("").unwrap().entries.is_empty());
        assert!(Notebook::parse("{not json").is_err());
    }

    #[test]
    fn the_notebook_is_capped_by_dropping_the_stalest_entry() {
        let mut book = Notebook::default();
        for index in 0..(MAX_ENTRIES + 5) {
            book.set(
                &format!("app{index}.exe"),
                1,
                "x64",
                Status::Partial,
                "",
                index as u64 + 1,
            );
        }
        assert_eq!(book.entries.len(), MAX_ENTRIES);
        assert!(book.get("app0.exe", 1, "x64").is_none());
        assert!(book
            .get(&format!("app{}.exe", MAX_ENTRIES + 4), 1, "x64")
            .is_some());
    }
}
