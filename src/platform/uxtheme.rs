//! Undocumented uxtheme ordinals that opt this process into the Windows 11 menu theme.
//!
//! Ordinals (Windows 10 1903 and later):
//! - 133 `AllowDarkModeForWindow`
//! - 135 `SetPreferredAppMode` (`AllowDark` = 1 follows the system app theme)
//! - 136 `FlushMenuThemes`

use std::sync::OnceLock;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};

use super::wide::{pcw, to_wide};

const ALLOW_DARK: i32 = 1;
const APP_MODE_DEFAULT: i32 = 0;

type SetPreferredAppMode = unsafe extern "system" fn(i32) -> i32;
type AllowDarkModeForWindow = unsafe extern "system" fn(HWND, i32) -> i32;
type FlushMenuThemes = unsafe extern "system" fn();

struct Ordinals {
    set_mode: SetPreferredAppMode,
    allow_window: AllowDarkModeForWindow,
    flush: FlushMenuThemes,
}

static ORDINALS: OnceLock<Option<Ordinals>> = OnceLock::new();

pub fn allow_dark_menus(allow: bool) {
    let Some(api) = ordinals() else {
        return;
    };
    unsafe {
        (api.set_mode)(if allow { ALLOW_DARK } else { APP_MODE_DEFAULT });
        (api.flush)();
    }
}

pub fn allow_window(hwnd: HWND, allow: bool) {
    if hwnd.is_null() {
        return;
    }
    let Some(api) = ordinals() else {
        return;
    };
    unsafe {
        (api.allow_window)(hwnd, i32::from(allow));
    }
}

fn ordinals() -> Option<&'static Ordinals> {
    ORDINALS.get_or_init(load).as_ref()
}

fn load() -> Option<Ordinals> {
    unsafe {
        let name = to_wide("uxtheme.dll");
        let module = LoadLibraryExW(pcw(&name), std::ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
        if module.is_null() {
            return None;
        }
        let set_mode = proc(module, 135)?;
        let allow_window = proc(module, 133)?;
        let flush = proc(module, 136)?;
        Some(Ordinals {
            set_mode,
            allow_window,
            flush,
        })
    }
}

unsafe fn proc<T>(module: windows_sys::Win32::Foundation::HMODULE, ordinal: usize) -> Option<T> {
    let address = GetProcAddress(module, ordinal as *const u8)?;
    Some(std::mem::transmute_copy(&address))
}
