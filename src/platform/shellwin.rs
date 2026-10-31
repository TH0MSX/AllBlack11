//! HKCU shell tweaks. Every write is per-user. Originals are snapshotted before
//! the first change and put back exactly on uncheck or uninstall.

use std::ptr::{null, null_mut};

use crate::shell::{
    black_shell_already_applied, blacken_shell_slots, oled_already_applied, palette_still_ours,
    slots_are_black, still_ours, BinarySlot, DwordSlot, ShellBackup, COLOR_PREVALENCE_WRITTEN,
    DWM_COLOR_PREVALENCE_WRITTEN, ENABLE_TRANSPARENCY_WRITTEN, OLED_WRITTEN, SYSTEM_LIGHT_WRITTEN,
};
use windows_sys::Win32::Foundation::{ERROR_SUCCESS, HANDLE, LPARAM, WPARAM};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegNotifyChangeKeyValue, RegOpenKeyExW, RegQueryValueExW,
    RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_NOTIFY, KEY_QUERY_VALUE, KEY_SET_VALUE,
    REG_BINARY, REG_DWORD, REG_NOTIFY_CHANGE_LAST_SET,
};
use windows_sys::Win32::System::Threading::CreateEventW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
};

use super::wide::{pcw, shell_backup_path, to_wide};

const ADVANCED: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Advanced";
const ACCENT: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Accent";
const PERSONALIZE: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize";
const DWM: &str = "Software\\Microsoft\\Windows\\DWM";

const OLED_VALUE: &str = "UseOLEDTaskbarTransparency";
const PALETTE_VALUE: &str = "AccentPalette";
const COLOR_PREVALENCE: &str = "ColorPrevalence";
const ENABLE_TRANSPARENCY: &str = "EnableTransparency";
const SYSTEM_LIGHT: &str = "SystemUsesLightTheme";

pub fn load_backup() -> ShellBackup {
    let text = std::fs::read_to_string(shell_backup_path()).unwrap_or_default();
    ShellBackup::parse(&text).unwrap_or_default()
}

pub fn save_backup(backup: &ShellBackup) {
    if let Err(err) =
        super::wide::write_atomic(&shell_backup_path(), &format!("{}\n", backup.to_json()))
    {
        super::wide::log_line("error", &format!("could not save shell-backup.json: {err}"));
    }
}

pub fn broadcast_immersive_color() {
    let message = to_wide("ImmersiveColorSet");
    let mut result = 0usize;
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0 as WPARAM,
            message.as_ptr() as LPARAM,
            SMTO_ABORTIFHUNG,
            250,
            &mut result,
        );
    }
}

pub fn create_notify_event() -> HANDLE {
    unsafe { CreateEventW(null(), 0, 0, null()) }
}

pub fn open_accent_key() -> Option<HKEY> {
    open_key(ACCENT, KEY_QUERY_VALUE | KEY_SET_VALUE | KEY_NOTIFY)
}

pub fn arm_accent_watch(key: HKEY, event: HANDLE) -> bool {
    if key.is_null() || event.is_null() {
        return false;
    }
    unsafe {
        RegNotifyChangeKeyValue(key, 0, REG_NOTIFY_CHANGE_LAST_SET, event, 1) == ERROR_SUCCESS
    }
}

pub fn enable_oled(backup: &mut ShellBackup) -> Result<(), String> {
    if backup.oled_taskbar.is_none() {
        let mut slot = read_dword(ADVANCED, OLED_VALUE);
        slot.already_applied = oled_already_applied(&slot);
        backup.oled_taskbar = Some(slot);
        save_backup(backup);
    }
    write_dword(ADVANCED, OLED_VALUE, 1)
}

pub fn disable_oled(backup: &mut ShellBackup) -> Result<(), String> {
    if let Some(slot) = backup.oled_taskbar.clone() {
        let current = read_dword(ADVANCED, OLED_VALUE);
        if !still_ours(&current, OLED_WRITTEN) {
            // Changed after AllBlack11 set it. Not ours to overwrite.
            super::wide::log_line(
                "info",
                "left UseOLEDTaskbarTransparency as it is: it was changed after AllBlack11 set it",
            );
        } else if slot.already_applied {
            delete_value(ADVANCED, OLED_VALUE)?;
        } else {
            restore_dword(ADVANCED, OLED_VALUE, &slot)?;
        }
        backup.oled_taskbar = None;
        save_backup(backup);
    }
    Ok(())
}

pub fn enable_black_shell(backup: &mut ShellBackup) -> Result<bool, String> {
    snapshot_black_shell(backup);
    let wrote_palette = write_black_palette()?;
    write_dword(PERSONALIZE, COLOR_PREVALENCE, 1)?;
    write_dword(PERSONALIZE, ENABLE_TRANSPARENCY, 1)?;
    write_dword(PERSONALIZE, SYSTEM_LIGHT, 0)?;
    write_dword(DWM, COLOR_PREVALENCE, 0)?;
    Ok(wrote_palette)
}

pub fn disable_black_shell(backup: &mut ShellBackup) -> Result<(), String> {
    let already = backup
        .accent_palette
        .as_ref()
        .is_some_and(|slot| slot.already_applied);
    let palette_now = read_binary(ACCENT, PALETTE_VALUE);
    let palette_ours = palette_still_ours(&palette_now);
    if !palette_ours {
        super::wide::log_line(
            "info",
            "left AccentPalette as it is: it no longer holds the black slots AllBlack11 wrote",
        );
    }
    if already {
        if palette_ours {
            delete_value(ACCENT, PALETTE_VALUE)?;
        }
        if still_ours(
            &read_dword(PERSONALIZE, COLOR_PREVALENCE),
            COLOR_PREVALENCE_WRITTEN,
        ) {
            write_dword(PERSONALIZE, COLOR_PREVALENCE, 0)?;
        }
    } else {
        if palette_ours {
            if let Some(slot) = backup.accent_palette.clone() {
                restore_binary(ACCENT, PALETTE_VALUE, &slot)?;
            }
        }
        restore_named_if_ours(
            &mut backup.color_prevalence,
            PERSONALIZE,
            COLOR_PREVALENCE,
            COLOR_PREVALENCE_WRITTEN,
        )?;
        restore_named_if_ours(
            &mut backup.enable_transparency,
            PERSONALIZE,
            ENABLE_TRANSPARENCY,
            ENABLE_TRANSPARENCY_WRITTEN,
        )?;
        restore_named_if_ours(
            &mut backup.system_uses_light_theme,
            PERSONALIZE,
            SYSTEM_LIGHT,
            SYSTEM_LIGHT_WRITTEN,
        )?;
        restore_named_if_ours(
            &mut backup.dwm_color_prevalence,
            DWM,
            COLOR_PREVALENCE,
            DWM_COLOR_PREVALENCE_WRITTEN,
        )?;
    }
    backup.accent_palette = None;
    backup.color_prevalence = None;
    backup.enable_transparency = None;
    backup.system_uses_light_theme = None;
    backup.dwm_color_prevalence = None;
    save_backup(backup);
    Ok(())
}

/// Re-blacken slots 4–6 after Windows rebuilds the palette. Returns whether a write happened.
pub fn reapply_black_palette() -> Result<bool, String> {
    write_black_palette()
}

fn snapshot_black_shell(backup: &mut ShellBackup) {
    // An existing shell-backup.json is the source of truth. Never replace a slot
    // that was already recorded, even when the live registry now matches the target.
    if backup.accent_palette.is_some()
        && backup.color_prevalence.is_some()
        && backup.enable_transparency.is_some()
        && backup.system_uses_light_theme.is_some()
        && backup.dwm_color_prevalence.is_some()
    {
        return;
    }
    let palette = read_binary(ACCENT, PALETTE_VALUE);
    let color_prevalence = read_dword(PERSONALIZE, COLOR_PREVALENCE);
    let enable_transparency = read_dword(PERSONALIZE, ENABLE_TRANSPARENCY);
    let system_uses_light_theme = read_dword(PERSONALIZE, SYSTEM_LIGHT);
    let dwm_color_prevalence = read_dword(DWM, COLOR_PREVALENCE);
    let already = black_shell_already_applied(
        &palette,
        &color_prevalence,
        &enable_transparency,
        &system_uses_light_theme,
        &dwm_color_prevalence,
    );
    let mut palette = palette;
    let mut color_prevalence = color_prevalence;
    let mut enable_transparency = enable_transparency;
    let mut system_uses_light_theme = system_uses_light_theme;
    let mut dwm_color_prevalence = dwm_color_prevalence;
    palette.already_applied = already;
    color_prevalence.already_applied = already;
    enable_transparency.already_applied = already;
    system_uses_light_theme.already_applied = already;
    dwm_color_prevalence.already_applied = already;
    if backup.accent_palette.is_none() {
        backup.accent_palette = Some(palette);
    }
    if backup.color_prevalence.is_none() {
        backup.color_prevalence = Some(color_prevalence);
    }
    if backup.enable_transparency.is_none() {
        backup.enable_transparency = Some(enable_transparency);
    }
    if backup.system_uses_light_theme.is_none() {
        backup.system_uses_light_theme = Some(system_uses_light_theme);
    }
    if backup.dwm_color_prevalence.is_none() {
        backup.dwm_color_prevalence = Some(dwm_color_prevalence);
    }
    save_backup(backup);
}

fn write_black_palette() -> Result<bool, String> {
    let current = read_binary(ACCENT, PALETTE_VALUE);
    let mut bytes = if current.present {
        current.bytes
    } else {
        // No palette yet: still write the three shell slots and leave the rest zero.
        vec![0u8; 32]
    };
    if bytes.len() < 32 {
        bytes.resize(32, 0);
    }
    if !blacken_shell_slots(&mut bytes) && slots_are_black(&bytes) {
        return Ok(false);
    }
    write_binary(ACCENT, PALETTE_VALUE, &bytes[..32])?;
    Ok(true)
}

/// Restore the saved original only while the value is still the one we wrote.
fn restore_named_if_ours(
    slot: &mut Option<DwordSlot>,
    key: &str,
    name: &str,
    written: u32,
) -> Result<(), String> {
    if let Some(value) = slot.clone() {
        if still_ours(&read_dword(key, name), written) {
            restore_dword(key, name, &value)?;
        } else {
            super::wide::log_line(
                "info",
                &format!("left {name} as it is: it was changed after AllBlack11 set it"),
            );
        }
        *slot = None;
    }
    Ok(())
}

fn restore_dword(key: &str, name: &str, slot: &DwordSlot) -> Result<(), String> {
    if slot.present {
        write_dword(key, name, slot.value)
    } else {
        delete_value(key, name)
    }
}

fn restore_binary(key: &str, name: &str, slot: &BinarySlot) -> Result<(), String> {
    if slot.present {
        write_binary(key, name, &slot.bytes)
    } else {
        delete_value(key, name)
    }
}

fn read_dword(key: &str, name: &str) -> DwordSlot {
    match query(key, name) {
        Some((REG_DWORD, bytes)) if bytes.len() >= 4 => DwordSlot {
            present: true,
            value: u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            already_applied: false,
        },
        Some(_) => DwordSlot {
            present: true,
            value: 0,
            already_applied: false,
        },
        None => DwordSlot {
            present: false,
            value: 0,
            already_applied: false,
        },
    }
}

fn read_binary(key: &str, name: &str) -> BinarySlot {
    match query(key, name) {
        Some((_, bytes)) => BinarySlot {
            present: true,
            bytes,
            already_applied: false,
        },
        None => BinarySlot {
            present: false,
            bytes: Vec::new(),
            already_applied: false,
        },
    }
}

fn query(key: &str, name: &str) -> Option<(u32, Vec<u8>)> {
    let Some(handle) = open_key(key, KEY_QUERY_VALUE) else {
        return None;
    };
    let name_w = to_wide(name);
    let mut kind = 0u32;
    let mut size = 0u32;
    let status = unsafe {
        RegQueryValueExW(
            handle,
            pcw(&name_w),
            null(),
            &mut kind,
            null_mut(),
            &mut size,
        )
    };
    if status != ERROR_SUCCESS {
        unsafe { RegCloseKey(handle) };
        return None;
    }
    let mut bytes = vec![0u8; size as usize];
    let mut got = size;
    let status = unsafe {
        RegQueryValueExW(
            handle,
            pcw(&name_w),
            null(),
            &mut kind,
            bytes.as_mut_ptr(),
            &mut got,
        )
    };
    unsafe { RegCloseKey(handle) };
    if status != ERROR_SUCCESS {
        return None;
    }
    bytes.truncate(got as usize);
    Some((kind, bytes))
}

fn write_dword(key: &str, name: &str, value: u32) -> Result<(), String> {
    let handle =
        open_key(key, KEY_SET_VALUE).ok_or_else(|| format!("could not open HKCU\\{key}"))?;
    let name_w = to_wide(name);
    let bytes = value.to_le_bytes();
    let status = unsafe {
        RegSetValueExW(
            handle,
            pcw(&name_w),
            0,
            REG_DWORD,
            bytes.as_ptr(),
            bytes.len() as u32,
        )
    };
    unsafe { RegCloseKey(handle) };
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!("could not set {name} ({status})"))
    }
}

fn write_binary(key: &str, name: &str, bytes: &[u8]) -> Result<(), String> {
    let handle =
        open_key(key, KEY_SET_VALUE).ok_or_else(|| format!("could not open HKCU\\{key}"))?;
    let name_w = to_wide(name);
    let status = unsafe {
        RegSetValueExW(
            handle,
            pcw(&name_w),
            0,
            REG_BINARY,
            bytes.as_ptr(),
            bytes.len() as u32,
        )
    };
    unsafe { RegCloseKey(handle) };
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!("could not set {name} ({status})"))
    }
}

fn delete_value(key: &str, name: &str) -> Result<(), String> {
    let Some(handle) = open_key(key, KEY_SET_VALUE) else {
        return Ok(());
    };
    let name_w = to_wide(name);
    let status = unsafe { RegDeleteValueW(handle, pcw(&name_w)) };
    unsafe { RegCloseKey(handle) };
    if status == ERROR_SUCCESS || status == 2 {
        Ok(())
    } else {
        Err(format!("could not delete {name} ({status})"))
    }
}

fn open_key(subkey: &str, access: u32) -> Option<HKEY> {
    let subkey = to_wide(subkey);
    let mut key = null_mut();
    let status = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, pcw(&subkey), 0, access, &mut key) };
    if status == ERROR_SUCCESS {
        Some(key)
    } else {
        None
    }
}
