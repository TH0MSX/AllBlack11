use std::path::Path;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ, RRF_RT_REG_DWORD,
};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::config::{file_name, FALLBACK_ACCENT_BGR};

use super::wide::{exe_path, pcw, to_wide};

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE: &str = "AllBlack11";

fn reg_dword(subkey: &str, value: &str) -> Option<u32> {
    let subkey = to_wide(subkey);
    let value = to_wide(value);
    let mut data = 0u32;
    let mut size = 4u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            pcw(&subkey),
            pcw(&value),
            RRF_RT_REG_DWORD,
            null_mut(),
            &mut data as *mut u32 as *mut _,
            &mut size,
        )
    };
    if status == ERROR_SUCCESS {
        Some(data)
    } else {
        None
    }
}

pub fn accent_bgr() -> u32 {
    let dword = reg_dword(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Accent",
        "AccentColorMenu",
    )
    .or_else(|| {
        reg_dword(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Accent",
            "AccentColor",
        )
    });
    dword
        .map(|value| value & 0x00FF_FFFF)
        .filter(|value| *value != 0)
        .unwrap_or(FALLBACK_ACCENT_BGR)
}

/// Windows' "Transparency effects" switch. Missing means on.
pub fn transparency_enabled() -> bool {
    reg_dword(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
        "EnableTransparency",
    )
    .map(|value| value != 0)
    .unwrap_or(true)
}

/// Windows' "Text size" setting (Settings > Accessibility > Text size) as a percentage, 100 to
/// 225. `None` when it has never been changed.
pub fn text_scale_percent() -> Option<u32> {
    reg_dword("Software\\Microsoft\\Accessibility", "TextScaleFactor")
}

/// `Some(true)` when apps are in dark mode (`AppsUseLightTheme == 0`).
pub fn apps_prefer_dark() -> Option<bool> {
    reg_dword(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
        "AppsUseLightTheme",
    )
    .map(|value| value == 0)
}

/// `Some(true)` when the taskbar and notification area are dark (`SystemUsesLightTheme == 0`).
/// The notification area follows the *system* theme, which can differ from the app theme.
pub fn system_prefers_dark() -> Option<bool> {
    reg_dword(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
        "SystemUsesLightTheme",
    )
    .map(|value| value == 0)
}

fn open_run_key(access: u32) -> Option<HKEY> {
    let subkey = to_wide(RUN_KEY);
    let mut key = null_mut();
    let status = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, pcw(&subkey), 0, access, &mut key) };
    if status == ERROR_SUCCESS {
        Some(key)
    } else {
        None
    }
}

pub fn autostart_enabled() -> bool {
    let Some(key) = open_run_key(KEY_QUERY_VALUE) else {
        return false;
    };
    let name = to_wide(RUN_VALUE);
    let mut kind = 0u32;
    let mut size = 0u32;
    let status = unsafe {
        windows_sys::Win32::System::Registry::RegQueryValueExW(
            key,
            pcw(&name),
            std::ptr::null(),
            &mut kind,
            null_mut(),
            &mut size,
        )
    };
    unsafe { RegCloseKey(key) };
    status == ERROR_SUCCESS && size > 2
}

pub fn set_autostart(enabled: bool) -> Result<(), String> {
    let key = open_run_key(KEY_SET_VALUE | KEY_QUERY_VALUE)
        .ok_or_else(|| "could not open HKCU Run key".to_string())?;
    let name = to_wide(RUN_VALUE);
    let status = if enabled {
        let path = exe_path().ok_or_else(|| "could not resolve this exe path".to_string())?;
        let quoted = to_wide(&format!("\"{path}\""));
        let bytes =
            unsafe { std::slice::from_raw_parts(quoted.as_ptr() as *const u8, quoted.len() * 2) };
        unsafe {
            RegSetValueExW(
                key,
                pcw(&name),
                0,
                REG_SZ,
                bytes.as_ptr(),
                bytes.len() as u32,
            )
        }
    } else {
        unsafe { RegDeleteValueW(key, pcw(&name)) }
    };
    unsafe { RegCloseKey(key) };
    // Deleting a missing value is fine.
    if status == ERROR_SUCCESS || (!enabled && status == 2) {
        Ok(())
    } else {
        Err(format!("registry update failed ({status})"))
    }
}

/// Open this repository's release pages in the default browser (`update::is_release_page_url`).
/// Call this only in response to a click or a key press.
pub fn open_release_url(url: &str) -> Result<(), String> {
    if !crate::update::is_release_page_url(url) {
        return Err("that address is not one AllBlack11 opens".into());
    }
    shell_open(url)
}

fn shell_open(url: &str) -> Result<(), String> {
    let op = to_wide("open");
    let target = to_wide(url);
    let code = unsafe {
        ShellExecuteW(
            null_mut(),
            pcw(&op),
            pcw(&target),
            null_mut(),
            null_mut(),
            SW_SHOWNORMAL,
        )
    };
    if (code as isize) > 32 {
        Ok(())
    } else {
        Err(format!(
            "could not open the browser (ShellExecute {})",
            code as isize
        ))
    }
}

/// Open a web address in the default browser. Only the creator's GitHub profile address passes
/// (`brand::is_safe_profile_url`); call this only in response to a click or key press.
pub fn open_url(url: &str) -> Result<(), String> {
    if !crate::is_safe_profile_url(url) {
        return Err("that address is not one AllBlack11 opens".into());
    }
    let op = to_wide("open");
    let target = to_wide(url);
    let code = unsafe {
        ShellExecuteW(
            null_mut(),
            pcw(&op),
            pcw(&target),
            null_mut(),
            null_mut(),
            SW_SHOWNORMAL,
        )
    };
    if (code as isize) > 32 {
        Ok(())
    } else {
        Err(format!(
            "could not open the browser (ShellExecute {})",
            code as isize
        ))
    }
}

pub fn open_file(path: &Path) -> Result<(), String> {
    let op = to_wide("open");
    let target = to_wide(&path.display().to_string());
    let code = unsafe {
        ShellExecuteW(
            null_mut(),
            pcw(&op),
            pcw(&target),
            null_mut(),
            null_mut(),
            SW_SHOWNORMAL,
        )
    };
    if (code as isize) > 32 {
        Ok(())
    } else {
        Err(format!(
            "could not open {} (ShellExecute {})",
            file_name(&path.display().to_string()),
            code as isize
        ))
    }
}
