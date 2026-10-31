//! Shell-toggle backup and the Start/taskbar/notification palette edit.
//! Registry I/O lives in the Windows platform module. This module is pure so the
//! byte layout can be tested.

use serde::{Deserialize, Serialize};

/// Bytes 16..28 are AccentPalette slots 4, 5, and 6 (three RGBA colors).
pub const SHELL_SLOT_BYTES: std::ops::Range<usize> = 16..28;
const BLACK_SLOT: [u8; 4] = [0x00, 0x00, 0x00, 0xFF];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DwordSlot {
    pub present: bool,
    pub value: u32,
    /// The value already matched what AllBlack11 would write, so it is not a real original.
    #[serde(default)]
    pub already_applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinarySlot {
    pub present: bool,
    pub bytes: Vec<u8>,
    /// The palette already had black shell slots, so it is not a real original.
    #[serde(default)]
    pub already_applied: bool,
}

/// Original HKCU values captured before AllBlack11 first changed them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ShellBackup {
    #[serde(default = "backup_version")]
    pub version: u32,
    #[serde(default)]
    pub oled_taskbar: Option<DwordSlot>,
    #[serde(default)]
    pub accent_palette: Option<BinarySlot>,
    #[serde(default)]
    pub color_prevalence: Option<DwordSlot>,
    #[serde(default)]
    pub enable_transparency: Option<DwordSlot>,
    #[serde(default)]
    pub system_uses_light_theme: Option<DwordSlot>,
    #[serde(default)]
    pub dwm_color_prevalence: Option<DwordSlot>,
}

fn backup_version() -> u32 {
    1
}

impl ShellBackup {
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.trim().is_empty() {
            return Ok(Self::default());
        }
        serde_json::from_str(text).map_err(|err| err.to_string())
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }
}

/// Force slots 4, 5, and 6 to opaque black. Other slots stay as Windows wrote them.
pub fn blacken_shell_slots(palette: &mut [u8]) -> bool {
    if palette.len() < SHELL_SLOT_BYTES.end {
        return false;
    }
    if slots_are_black(palette) {
        return false;
    }
    for slot in 0..3 {
        let start = SHELL_SLOT_BYTES.start + slot * 4;
        palette[start..start + 4].copy_from_slice(&BLACK_SLOT);
    }
    true
}

pub fn slots_are_black(palette: &[u8]) -> bool {
    palette.len() >= SHELL_SLOT_BYTES.end
        && palette[SHELL_SLOT_BYTES]
            .chunks(4)
            .all(|slot| slot == BLACK_SLOT)
}

/// The values AllBlack11 writes. Restoring only happens while the registry still holds them.
pub const OLED_WRITTEN: u32 = 1;
pub const COLOR_PREVALENCE_WRITTEN: u32 = 1;
pub const ENABLE_TRANSPARENCY_WRITTEN: u32 = 1;
pub const SYSTEM_LIGHT_WRITTEN: u32 = 0;
pub const DWM_COLOR_PREVALENCE_WRITTEN: u32 = 0;

/// Compare-before-restore: the value is still the one AllBlack11 wrote, so putting the saved
/// original back cannot overwrite something you or another program changed since.
pub fn still_ours(current: &DwordSlot, written: u32) -> bool {
    current.present && current.value == written
}

/// Same check for the three palette slots AllBlack11 blackens.
pub fn palette_still_ours(current: &BinarySlot) -> bool {
    current.present && slots_are_black(&current.bytes)
}

pub fn dword_matches(slot: &DwordSlot, value: u32) -> bool {
    slot.present && slot.value == value
}

/// `UseOLEDTaskbarTransparency` is already 1, so there is no earlier value to restore.
pub fn oled_already_applied(slot: &DwordSlot) -> bool {
    dword_matches(slot, 1)
}

/// Every black-shell write already matches. Uncheck must not treat those values as originals.
pub fn black_shell_already_applied(
    palette: &BinarySlot,
    color_prevalence: &DwordSlot,
    enable_transparency: &DwordSlot,
    system_uses_light_theme: &DwordSlot,
    dwm_color_prevalence: &DwordSlot,
) -> bool {
    palette.present
        && slots_are_black(&palette.bytes)
        && dword_matches(color_prevalence, 1)
        && dword_matches(enable_transparency, 1)
        && dword_matches(system_uses_light_theme, 0)
        && dword_matches(dwm_color_prevalence, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blacks_only_the_shell_slots() {
        let mut palette = [
            0x77, 0xfe, 0xf7, 0x00, 0x1a, 0xfd, 0xe9, 0x00, 0x01, 0xb0, 0x9c, 0x00, 0x01, 0x85,
            0x74, 0x00, 0x01, 0x74, 0x63, 0x00, 0x01, 0x4e, 0x3a, 0x00, 0x00, 0x2a, 0x15, 0x00,
            0xd1, 0x34, 0x38, 0x00,
        ];
        assert!(blacken_shell_slots(&mut palette));
        assert_eq!(
            palette,
            [
                0x77, 0xfe, 0xf7, 0x00, 0x1a, 0xfd, 0xe9, 0x00, 0x01, 0xb0, 0x9c, 0x00, 0x01, 0x85,
                0x74, 0x00, 0x00, 0x00, 0x00, 0xff, 0x00, 0x00, 0x00, 0xff, 0x00, 0x00, 0x00, 0xff,
                0xd1, 0x34, 0x38, 0x00,
            ]
        );
        assert!(!blacken_shell_slots(&mut palette));
    }

    #[test]
    fn backup_roundtrip_and_absent_defaults() {
        let backup = ShellBackup {
            version: 1,
            oled_taskbar: Some(DwordSlot {
                present: false,
                value: 0,
                already_applied: false,
            }),
            accent_palette: Some(BinarySlot {
                present: true,
                bytes: vec![1, 2, 3, 4],
                already_applied: false,
            }),
            ..ShellBackup::default()
        };
        let parsed = ShellBackup::parse(&backup.to_json()).unwrap();
        assert_eq!(parsed, backup);
        let old = ShellBackup::parse(r#"{"version":1}"#).unwrap();
        assert!(old.oled_taskbar.is_none());
        let legacy = ShellBackup::parse(
            r#"{"version":1,"oledTaskbar":{"present":true,"value":0},"accentPalette":{"present":true,"bytes":[1,2]}}"#,
        )
        .unwrap();
        assert!(!legacy.oled_taskbar.unwrap().already_applied);
        assert!(!legacy.accent_palette.unwrap().already_applied);
    }

    #[test]
    fn restore_only_while_the_value_is_still_the_one_we_wrote() {
        let slot = |present, value| DwordSlot {
            present,
            value,
            already_applied: false,
        };
        assert!(still_ours(&slot(true, OLED_WRITTEN), OLED_WRITTEN));
        // Changed by the user or another program since: leave it alone.
        assert!(!still_ours(&slot(true, 0), OLED_WRITTEN));
        // Deleted since: nothing of ours to restore over.
        assert!(!still_ours(&slot(false, 0), OLED_WRITTEN));
        // A black-shell value that is legitimately zero.
        assert!(still_ours(&slot(true, 0), SYSTEM_LIGHT_WRITTEN));

        let mut bytes = vec![0u8; 32];
        blacken_shell_slots(&mut bytes);
        let ours = BinarySlot {
            present: true,
            bytes: bytes.clone(),
            already_applied: false,
        };
        assert!(palette_still_ours(&ours));
        bytes[17] = 0x40; // Windows or the user changed the accent
        let changed = BinarySlot {
            present: true,
            bytes,
            already_applied: false,
        };
        assert!(!palette_still_ours(&changed));
        assert!(!palette_still_ours(&BinarySlot {
            present: false,
            bytes: vec![],
            already_applied: false
        }));
    }

    #[test]
    fn already_applied_only_when_every_target_matches() {
        let palette = BinarySlot {
            present: true,
            bytes: {
                let mut bytes = vec![0u8; 32];
                blacken_shell_slots(&mut bytes);
                bytes
            },
            already_applied: false,
        };
        let on = DwordSlot {
            present: true,
            value: 1,
            already_applied: false,
        };
        let off = DwordSlot {
            present: true,
            value: 0,
            already_applied: false,
        };
        assert!(oled_already_applied(&on));
        assert!(!oled_already_applied(&off));
        assert!(black_shell_already_applied(&palette, &on, &on, &off, &off));
        assert!(!black_shell_already_applied(
            &palette, &off, &on, &off, &off
        ));
    }
}
