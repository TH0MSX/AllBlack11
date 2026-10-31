//! App identity accents: a stable border color per app, so windows are easy to tell apart.
//!
//! Assignments are deterministic (a hash of the executable name picks a color from a curated
//! palette), stored locally so they never change when the palette does, and can be overridden
//! per app. Only the *border color* is ever set, and only on windows whose border is not set by
//! a per-app rule. Nothing else about the window is touched.
//!
//! The file holds executable names and colors, never window titles or paths.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::config::Color;

pub const MAX_ENTRIES: usize = 200;
const MAX_KEY: usize = 64;
const MAX_FILE: usize = 64 * 1024;
pub const FILE_VERSION: u32 = 1;

/// Curated hues. Each is far enough from black to read as a thin border on a black frame
/// (a test enforces 4.5:1), and each is a distinct hue.
pub const PALETTE: [(u8, u8, u8); 12] = [
    (0x4C, 0x9A, 0xFF), // blue
    (0x36, 0xD1, 0xB2), // teal
    (0x7B, 0xD6, 0x4A), // green
    (0xE6, 0xD2, 0x4A), // yellow
    (0xFF, 0xA8, 0x4C), // orange
    (0xFF, 0x6B, 0x6B), // red
    (0xFF, 0x77, 0xB7), // pink
    (0xB5, 0x8C, 0xFF), // violet
    (0x5E, 0xD0, 0xF2), // cyan
    (0xC9, 0xA6, 0x6B), // sand
    (0xA0, 0xE0, 0xA0), // mint
    (0xFF, 0x9F, 0x80), // coral
];

pub fn hex(rgb: (u8, u8, u8)) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb.0, rgb.1, rgb.2)
}

fn parse_hex(text: &str) -> Option<(u8, u8, u8)> {
    let body = text.strip_prefix('#')?;
    if body.len() != 6 || !body.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(body, 16).ok()?;
    Some(((value >> 16) as u8, (value >> 8) as u8, value as u8))
}

/// The same app always maps to the same key: its file name, lower-cased.
pub fn key_for(exe: &str) -> Option<String> {
    let name = exe.rsplit(['\\', '/']).next()?.trim().to_ascii_lowercase();
    if name.is_empty() || name.len() > MAX_KEY || name.chars().any(char::is_control) {
        return None;
    }
    Some(name)
}

fn fnv1a(text: &str) -> u32 {
    let mut hash: u32 = 0x811C_9DC5;
    for byte in text.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Accents {
    pub version: u32,
    /// App key -> `#RRGGBB`, chosen automatically the first time the app was seen.
    #[serde(default)]
    pub assigned: BTreeMap<String, String>,
    /// App key -> `#RRGGBB`, chosen by the user. Wins over `assigned`.
    #[serde(default)]
    pub overrides: BTreeMap<String, String>,
}

impl Default for Accents {
    fn default() -> Self {
        Self {
            version: FILE_VERSION,
            assigned: BTreeMap::new(),
            overrides: BTreeMap::new(),
        }
    }
}

impl Accents {
    pub fn parse(text: &str) -> Result<Accents, String> {
        if text.trim().is_empty() {
            return Ok(Accents::default());
        }
        if text.len() > MAX_FILE {
            return Err("the accents file is larger than 64 KB".into());
        }
        let accents: Accents = serde_json::from_str(text).map_err(|err| err.to_string())?;
        if accents.version != FILE_VERSION {
            return Err(format!(
                "accents file version {} is not supported",
                accents.version
            ));
        }
        if accents.assigned.len() > MAX_ENTRIES || accents.overrides.len() > MAX_ENTRIES {
            return Err(format!("at most {MAX_ENTRIES} entries"));
        }
        for (key, value) in accents.assigned.iter().chain(accents.overrides.iter()) {
            if key_for(key).as_deref() != Some(key.as_str()) {
                return Err(format!("“{key}” is not a valid app key"));
            }
            if parse_hex(value).is_none() {
                return Err(format!("“{value}” is not a #RRGGBB color"));
            }
        }
        Ok(accents)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }

    /// The border color for an app and whether a new assignment was recorded (so the caller
    /// knows to save). A user override wins; an earlier assignment is reused; otherwise a new
    /// one is made from the hash, skipping colors already given to other apps while any remain.
    pub fn color_for(&mut self, exe: &str) -> Option<(Color, bool)> {
        let key = key_for(exe)?;
        if let Some(color) = self.overrides.get(&key).and_then(|text| parse_hex(text)) {
            return Some((rgb(color), false));
        }
        if let Some(color) = self.assigned.get(&key).and_then(|text| parse_hex(text)) {
            return Some((rgb(color), false));
        }
        if self.assigned.len() >= MAX_ENTRIES {
            // Full: still give a stable color, just do not record more.
            let index = fnv1a(&key) as usize % PALETTE.len();
            return Some((rgb(PALETTE[index]), false));
        }
        let start = fnv1a(&key) as usize % PALETTE.len();
        let used: Vec<String> = self.assigned.values().cloned().collect();
        let index = (0..PALETTE.len())
            .map(|step| (start + step) % PALETTE.len())
            .find(|candidate| !used.contains(&hex(PALETTE[*candidate])))
            .unwrap_or(start);
        let chosen = PALETTE[index];
        self.assigned.insert(key, hex(chosen));
        Some((rgb(chosen), true))
    }

    /// Set the color for one app. Only `#RRGGBB` is accepted.
    pub fn set_override(&mut self, exe: &str, color: &str) -> Result<(), String> {
        let key = key_for(exe).ok_or("That app has no usable name.")?;
        let parsed = parse_hex(color).ok_or("Use a color like #4C9AFF.")?;
        if self.overrides.len() >= MAX_ENTRIES && !self.overrides.contains_key(&key) {
            return Err(format!(
                "At most {MAX_ENTRIES} apps can have their own accent."
            ));
        }
        self.overrides.insert(key, hex(parsed));
        Ok(())
    }

    /// Go back to the automatic color for this app.
    pub fn clear_override(&mut self, exe: &str) -> bool {
        key_for(exe).is_some_and(|key| self.overrides.remove(&key).is_some())
    }

    /// Advance an app's override to the next palette color (a one-button way to choose).
    pub fn next_override(&mut self, exe: &str) -> Result<String, String> {
        let key = key_for(exe).ok_or("That app has no usable name.")?;
        let current = self
            .overrides
            .get(&key)
            .or_else(|| self.assigned.get(&key))
            .and_then(|text| parse_hex(text));
        let next = match current.and_then(|c| PALETTE.iter().position(|p| *p == c)) {
            Some(index) => PALETTE[(index + 1) % PALETTE.len()],
            None => PALETTE[0],
        };
        let text = hex(next);
        self.set_override(exe, &text)?;
        Ok(text)
    }

    pub fn describe(&self, exe: &str) -> Option<String> {
        let key = key_for(exe)?;
        if let Some(color) = self.overrides.get(&key) {
            Some(format!("{color} (your choice)"))
        } else {
            self.assigned
                .get(&key)
                .map(|color| format!("{color} (automatic)"))
        }
    }
}

fn rgb((r, g, b): (u8, u8, u8)) -> Color {
    Color::Rgb { r, g, b }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::legacy_policy::contrast_ratio;

    fn colorref((r, g, b): (u8, u8, u8)) -> u32 {
        u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16)
    }

    #[test]
    fn every_palette_color_is_visible_on_black_and_all_are_distinct() {
        for color in PALETTE {
            assert!(contrast_ratio(colorref(color), 0) >= 4.5, "{}", hex(color));
        }
        let mut all: Vec<_> = PALETTE.to_vec();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), PALETTE.len());
    }

    #[test]
    fn the_same_app_always_gets_the_same_color_and_it_is_remembered() {
        let mut a = Accents::default();
        let (first, changed) = a.color_for("Code.exe").unwrap();
        assert!(changed);
        let (again, changed_again) = a.color_for("code.EXE").unwrap();
        assert_eq!(first, again);
        assert!(!changed_again, "an existing assignment is not re-recorded");
        // A fresh store (e.g. after deleting the file) reaches the same color by the same hash.
        let mut b = Accents::default();
        assert_eq!(b.color_for("code.exe").unwrap().0, first);
        // Path forms collapse to the file name.
        assert_eq!(a.color_for(r"C:\Apps\Code.exe").unwrap().0, first);
    }

    #[test]
    fn assignments_survive_a_palette_change_because_colors_are_stored() {
        let mut a = Accents::default();
        a.color_for("notepad.exe");
        a.assigned.insert("notepad.exe".into(), "#123456".into());
        // Whatever the palette says now, the stored color is used.
        assert_eq!(
            a.color_for("notepad.exe").unwrap().0,
            Color::Rgb {
                r: 0x12,
                g: 0x34,
                b: 0x56
            }
        );
    }

    #[test]
    fn new_apps_get_different_colors_until_the_palette_runs_out() {
        let mut a = Accents::default();
        let mut seen = Vec::new();
        for n in 0..PALETTE.len() {
            let (color, _) = a.color_for(&format!("app{n}.exe")).unwrap();
            assert!(!seen.contains(&color), "app{n} repeated a color too early");
            seen.push(color);
        }
        // The 13th app must still get a color rather than none.
        assert!(a.color_for("one-more.exe").is_some());
    }

    #[test]
    fn a_user_override_wins_and_can_be_cleared() {
        let mut a = Accents::default();
        let (auto, _) = a.color_for("steam.exe").unwrap();
        a.set_override("steam.exe", "#112233").unwrap();
        assert_eq!(
            a.color_for("steam.exe").unwrap().0,
            Color::Rgb {
                r: 0x11,
                g: 0x22,
                b: 0x33
            }
        );
        assert_eq!(a.describe("steam.exe").unwrap(), "#112233 (your choice)");
        assert!(a.clear_override("steam.exe"));
        assert!(!a.clear_override("steam.exe"));
        assert_eq!(a.color_for("steam.exe").unwrap().0, auto);
    }

    #[test]
    fn invalid_colors_and_names_are_refused() {
        let mut a = Accents::default();
        for bad in ["", "red", "#12345", "#GGGGGG", "112233", "#1122334"] {
            assert!(a.set_override("x.exe", bad).is_err(), "{bad:?}");
        }
        assert!(a.set_override("", "#112233").is_err());
        assert!(a.color_for("").is_none());
        assert!(a.color_for(&"x".repeat(200)).is_none());
        assert!(a.color_for("bad\u{7}.exe").is_none());
    }

    #[test]
    fn the_next_button_walks_the_palette_and_wraps() {
        let mut a = Accents::default();
        let mut seen = Vec::new();
        for _ in 0..=PALETTE.len() {
            seen.push(a.next_override("calc.exe").unwrap());
        }
        assert_eq!(seen[0], hex(PALETTE[0]));
        assert_eq!(seen[PALETTE.len()], hex(PALETTE[0]), "wraps around");
    }

    #[test]
    fn the_file_round_trips_and_hostile_files_are_refused() {
        let mut a = Accents::default();
        a.color_for("code.exe");
        a.set_override("steam.exe", "#ABCDEF").unwrap();
        assert_eq!(Accents::parse(&a.to_json()).unwrap(), a);
        assert!(Accents::parse("").unwrap().assigned.is_empty());
        for bad in [
            "not json",
            r##"{"version":2}"##,
            r##"{"version":1,"run":"x"}"##,
            r##"{"version":1,"assigned":{"Code.exe":"#112233"}}"##,
            r##"{"version":1,"assigned":{"code.exe":"blue"}}"##,
            r##"{"version":1,"assigned":{"c:\\x\\code.exe":"#112233"}}"##,
        ] {
            assert!(Accents::parse(bad).is_err(), "{bad}");
        }
        assert!(Accents::parse(&"{".repeat(MAX_FILE + 1)).is_err());
    }

    #[test]
    fn the_store_is_bounded() {
        let mut a = Accents::default();
        for n in 0..MAX_ENTRIES + 20 {
            a.color_for(&format!("app{n}.exe"));
        }
        assert_eq!(a.assigned.len(), MAX_ENTRIES);
        // Beyond the cap an app still gets a stable color, just not a recorded one.
        let (color, changed) = a.color_for("overflow.exe").unwrap();
        assert!(!changed);
        assert_eq!(a.color_for("overflow.exe").unwrap().0, color);
    }
}
