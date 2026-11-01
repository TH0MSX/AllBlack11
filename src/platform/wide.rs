use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::time::{SystemTime, UNIX_EPOCH};

use windows_sys::core::PCWSTR;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ACCESS_DENIED, GENERIC_READ, GENERIC_WRITE, HANDLE,
    INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, WriteFile, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Console::{
    AttachConsole, FreeConsole, GetConsoleMode, SetConsoleMode, SetStdHandle, WriteConsoleW,
    ATTACH_PARENT_PROCESS, ENABLE_PROCESSED_OUTPUT, ENABLE_WRAP_AT_EOL_OUTPUT, STD_ERROR_HANDLE,
    STD_OUTPUT_HANDLE,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_TOPMOST,
};

use crate::format_unix_utc;

pub fn to_wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn fill_wide(dest: &mut [u16], text: &str) {
    dest.fill(0);
    let limit = dest.len().saturating_sub(1);
    for (index, unit) in text.encode_utf16().take(limit).enumerate() {
        dest[index] = unit;
    }
}

pub fn pcw(wide: &[u16]) -> PCWSTR {
    wide.as_ptr()
}

pub fn config_dir() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        if !appdata.is_empty() {
            return PathBuf::from(appdata).join("AllBlack11");
        }
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        return PathBuf::from(profile)
            .join("AppData")
            .join("Roaming")
            .join("AllBlack11");
    }
    PathBuf::from("AllBlack11")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub fn snapshot_path() -> PathBuf {
    config_dir().join("snapshots.json")
}

pub fn shell_backup_path() -> PathBuf {
    config_dir().join("shell-backup.json")
}

pub fn legacy_config_dir() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        if !appdata.is_empty() {
            return PathBuf::from(appdata).join("PepaTheme");
        }
    }
    PathBuf::from("PepaTheme")
}

/// First launch after the rename: keep the user's PepaTheme config and window snapshots.
pub fn migrate_legacy_config() {
    let new_dir = config_dir();
    let new_config = new_dir.join("config.json");
    if new_config.exists() {
        return;
    }
    let old_dir = legacy_config_dir();
    let old_config = old_dir.join("config.json");
    if !old_config.exists() {
        return;
    }
    let _ = std::fs::create_dir_all(&new_dir);
    if std::fs::copy(&old_config, &new_config).is_err() {
        return;
    }
    let old_snapshots = old_dir.join("snapshots.json");
    let new_snapshots = snapshot_path();
    if old_snapshots.exists() && !new_snapshots.exists() {
        let _ = std::fs::copy(&old_snapshots, &new_snapshots);
    }
    log_line(
        "info",
        "copied config and snapshots from %APPDATA%\\PepaTheme",
    );
}

/// Write `text` so a crash or a full disk never leaves a half-written file (see `atomic`).
pub fn write_atomic(path: &std::path::Path, text: &str) -> Result<(), String> {
    crate::write_atomic(path, text)
}

pub fn lkg_path() -> PathBuf {
    config_dir().join("config.lkg.json")
}

pub fn notebook_path() -> PathBuf {
    config_dir().join("compat.json")
}

pub fn onboarding_path() -> PathBuf {
    config_dir().join("onboarding.json")
}

pub fn accents_path() -> PathBuf {
    config_dir().join("accents.json")
}

pub fn lookbook_path() -> PathBuf {
    config_dir().join("lookbook.json")
}

pub fn recipes_dir() -> PathBuf {
    config_dir().join("recipes")
}

pub fn log_path() -> PathBuf {
    config_dir().join("allblack11.log")
}

pub fn exe_path() -> Option<String> {
    let mut buf = [0u16; 1024];
    let length = unsafe { GetModuleFileNameW(null_mut(), buf.as_mut_ptr(), buf.len() as u32) };
    if length == 0 || length as usize >= buf.len() {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..length as usize]))
}

pub fn log_line(level: &str, message: &str) {
    let dir = config_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = log_path();
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > 256 * 1024 {
            let _ = std::fs::rename(&path, dir.join("allblack11.log.1"));
        }
    }
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{} {level} {message}", format_unix_utc(secs));
    }
}

pub fn message_box(title: &str, text: &str, error: bool) {
    let title_w = to_wide(title);
    let text_w = to_wide(text);
    let flags = MB_OK
        | MB_TOPMOST
        | if error {
            MB_ICONERROR
        } else {
            MB_ICONINFORMATION
        };
    unsafe {
        MessageBoxW(null_mut(), pcw(&text_w), pcw(&title_w), flags);
    }
}

/// Attach to the parent console (PowerShell, Windows Terminal, cmd) and write `text`.
/// A GUI-subsystem process is not born with that console, so stdout starts out closed.
/// Returns false when there is no parent console, such as a double-click from Explorer.
pub fn write_console(text: &str) -> bool {
    unsafe {
        let attached_by_us = AttachConsole(ATTACH_PARENT_PROCESS) != 0;
        if !attached_by_us && GetLastError() != ERROR_ACCESS_DENIED {
            return false;
        }
        let name = to_wide("CONOUT$");
        let handle: HANDLE = CreateFileW(
            pcw(&name),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        );
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            if attached_by_us {
                FreeConsole();
            }
            return false;
        }
        // AttachConsole does not update this process's standard handles.
        SetStdHandle(STD_OUTPUT_HANDLE, handle);
        SetStdHandle(STD_ERROR_HANDLE, handle);
        let mut mode = 0u32;
        if GetConsoleMode(handle, &mut mode) != 0 {
            SetConsoleMode(
                handle,
                mode | ENABLE_PROCESSED_OUTPUT | ENABLE_WRAP_AT_EOL_OUTPUT,
            );
        }
        let wrote = write_all(handle, text);
        // Release the console so the parent's prompt is not left waiting on our attachment.
        CloseHandle(handle);
        if attached_by_us {
            FreeConsole();
        }
        wrote
    }
}

unsafe fn write_all(handle: HANDLE, text: &str) -> bool {
    let units: Vec<u16> = text.encode_utf16().collect();
    if !units.is_empty() {
        let mut written = 0u32;
        if WriteConsoleW(
            handle,
            units.as_ptr(),
            units.len() as u32,
            &mut written,
            null(),
        ) != 0
            && written == units.len() as u32
        {
            return true;
        }
    }
    // WriteConsole fails when the handle is a pipe (redirection). WriteFile covers that.
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return true;
    }
    let mut written = 0u32;
    WriteFile(
        handle,
        bytes.as_ptr(),
        bytes.len() as u32,
        &mut written,
        null_mut(),
    ) != 0
        && written == bytes.len() as u32
}

pub fn emit(text: &str) {
    if write_console(text) {
        return;
    }
    log_line("info", text.trim_end());
    let check = config_dir().join("check.txt");
    let shown = if check.is_file() {
        format!(
            "Could not attach to the parent console.\nThe report was saved to:\n{}",
            check.display()
        )
    } else if text.chars().count() > 700 {
        "Details were written to %APPDATA%\\AllBlack11\\allblack11.log".to_string()
    } else {
        text.to_string()
    };
    message_box("AllBlack11", &shown, false);
}

pub fn lparam_wide(value: isize) -> String {
    if value == 0 {
        return String::new();
    }
    unsafe {
        let ptr = value as *const u16;
        let mut units = Vec::new();
        for index in 0..160 {
            let unit = *ptr.add(index);
            if unit == 0 {
                break;
            }
            units.push(unit);
        }
        String::from_utf16_lossy(&units)
    }
}
