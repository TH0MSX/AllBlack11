//! Public DWM attributes applied to another process's top-level HWND.
//!
//! `DwmSetWindowAttribute` is an LPC into `dwm.exe`. DWM stores the attribute on the
//! window, so the caller does not need to own the HWND and does not load code into
//! the target. These are the attributes MicaForEveryone and the force-dark-titlebar
//! utilities set the same way. They paint only what DWM itself paints:
//! caption, caption text, border, corner clipping, immersive dark mode, and the
//! system backdrop. Pixels the app draws in its client area stay as the app drew them.
//!
//! Minimum builds (Windows SDK):
//! - 20, 33, 34, 35, 36: Windows 11 build 22000
//! - 38 `DWM_SYSTEMBACKDROP_TYPE`: Windows 11 build 22621

use std::mem::{size_of, zeroed};

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DwmSetWindowAttribute, DWMSBT_AUTO, DWMWA_BORDER_COLOR,
    DWMWA_CAPTION_COLOR, DWMWA_COLOR_DEFAULT, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_TEXT_COLOR,
    DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE,
};
use windows_sys::Win32::System::SystemInformation::OSVERSIONINFOW;
use windows_sys::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETHIGHCONTRAST};

use crate::config::{Corners, Style};
use crate::snapshot::StoredSnapshot;

const ATTR_DARK: u32 = DWMWA_USE_IMMERSIVE_DARK_MODE as u32;
const ATTR_CORNERS: u32 = DWMWA_WINDOW_CORNER_PREFERENCE as u32;
const ATTR_BORDER: u32 = DWMWA_BORDER_COLOR as u32;
const ATTR_CAPTION: u32 = DWMWA_CAPTION_COLOR as u32;
const ATTR_TEXT: u32 = DWMWA_TEXT_COLOR as u32;
const ATTR_BACKDROP: u32 = DWMWA_SYSTEMBACKDROP_TYPE as u32;
const BUILD_CAPTION: u32 = 22000;
const BUILD_BACKDROP: u32 = 22621;

const _: () = {
    assert!(DWMWA_USE_IMMERSIVE_DARK_MODE == 20);
    assert!(DWMWA_WINDOW_CORNER_PREFERENCE == 33);
    assert!(DWMWA_BORDER_COLOR == 34);
    assert!(DWMWA_CAPTION_COLOR == 35);
    assert!(DWMWA_TEXT_COLOR == 36);
    assert!(DWMWA_SYSTEMBACKDROP_TYPE == 38);
    assert!(DWMSBT_AUTO == 0);
};

#[link(name = "ntdll")]
unsafe extern "system" {
    fn RtlGetVersion(info: *mut OSVERSIONINFOW) -> i32;
}

pub fn windows_build() -> u32 {
    unsafe {
        let mut info: OSVERSIONINFOW = zeroed();
        info.dwOSVersionInfoSize = size_of::<OSVERSIONINFOW>() as u32;
        if RtlGetVersion(&mut info) < 0 {
            return 0;
        }
        info.dwBuildNumber
    }
}

pub fn high_contrast_enabled() -> bool {
    unsafe {
        let mut info: HIGHCONTRASTW = zeroed();
        info.cbSize = size_of::<HIGHCONTRASTW>() as u32;
        if SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            info.cbSize,
            &mut info as *mut HIGHCONTRASTW as *mut _,
            0,
        ) == 0
        {
            return false;
        }
        info.dwFlags & HCF_HIGHCONTRASTON != 0
    }
}

pub struct Captured {
    pub dark: Option<i32>,
    pub backdrop: Option<i32>,
    pub caption: Option<u32>,
    pub border: Option<u32>,
    pub text: Option<u32>,
    pub corners: Option<u32>,
}

pub fn capture(hwnd: HWND) -> Captured {
    Captured {
        dark: get_i32(hwnd, ATTR_DARK),
        backdrop: get_i32(hwnd, ATTR_BACKDROP),
        caption: get_u32(hwnd, ATTR_CAPTION),
        border: get_u32(hwnd, ATTR_BORDER),
        text: get_u32(hwnd, ATTR_TEXT),
        corners: get_u32(hwnd, ATTR_CORNERS),
    }
}

pub struct Failure {
    pub attribute: u32,
    pub name: &'static str,
    pub hr: i32,
}

pub fn apply(
    hwnd: HWND,
    style: &Style,
    accent_bgr: u32,
    build: u32,
    backdrop: i32,
) -> Vec<Failure> {
    let mut failed = Vec::new();
    let dark: i32 = if style.dark_mode { 1 } else { 0 };
    push_fail(
        &mut failed,
        ATTR_DARK,
        "DWMWA_USE_IMMERSIVE_DARK_MODE",
        set_i32(hwnd, ATTR_DARK, dark),
    );
    if build >= BUILD_BACKDROP {
        push_fail(
            &mut failed,
            ATTR_BACKDROP,
            "DWMWA_SYSTEMBACKDROP_TYPE",
            set_i32(hwnd, ATTR_BACKDROP, backdrop),
        );
    }
    push_fail(
        &mut failed,
        ATTR_CORNERS,
        "DWMWA_WINDOW_CORNER_PREFERENCE",
        set_u32(hwnd, ATTR_CORNERS, style.corners.dwm_value()),
    );
    push_fail(
        &mut failed,
        ATTR_CAPTION,
        "DWMWA_CAPTION_COLOR",
        set_u32(hwnd, ATTR_CAPTION, style.caption.to_colorref(accent_bgr)),
    );
    push_fail(
        &mut failed,
        ATTR_BORDER,
        "DWMWA_BORDER_COLOR",
        set_u32(hwnd, ATTR_BORDER, style.border.to_colorref(accent_bgr)),
    );
    push_fail(
        &mut failed,
        ATTR_TEXT,
        "DWMWA_TEXT_COLOR",
        set_u32(hwnd, ATTR_TEXT, style.text.to_colorref(accent_bgr)),
    );
    failed
}

pub fn restore(hwnd: HWND, snap: &StoredSnapshot, apps_dark: Option<bool>, build: u32) {
    if let Some(value) = snap
        .dark
        .or_else(|| apps_dark.map(|dark| if dark { 1 } else { 0 }))
    {
        let _ = set_i32(hwnd, ATTR_DARK, value);
    }
    if build >= BUILD_BACKDROP {
        let _ = set_i32(hwnd, ATTR_BACKDROP, snap.backdrop.unwrap_or(DWMSBT_AUTO));
    }
    let _ = set_u32(
        hwnd,
        ATTR_CORNERS,
        snap.corners.unwrap_or(Corners::Default.dwm_value()),
    );
    let _ = set_u32(
        hwnd,
        ATTR_CAPTION,
        snap.caption.unwrap_or(DWMWA_COLOR_DEFAULT),
    );
    let _ = set_u32(
        hwnd,
        ATTR_BORDER,
        snap.border.unwrap_or(DWMWA_COLOR_DEFAULT),
    );
    let _ = set_u32(hwnd, ATTR_TEXT, snap.text.unwrap_or(DWMWA_COLOR_DEFAULT));
}

pub fn set_backdrop(hwnd: HWND, backdrop: i32, build: u32) -> i32 {
    if build < BUILD_BACKDROP {
        return 0;
    }
    set_i32(hwnd, ATTR_BACKDROP, backdrop)
}

pub fn supports_theme(build: u32) -> bool {
    build >= BUILD_CAPTION
}

fn push_fail(failed: &mut Vec<Failure>, attribute: u32, name: &'static str, hr: i32) {
    if hr < 0 {
        failed.push(Failure {
            attribute,
            name,
            hr,
        });
    }
}

fn set_u32(hwnd: HWND, attribute: u32, value: u32) -> i32 {
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            attribute,
            &value as *const u32 as *const _,
            size_of::<u32>() as u32,
        )
    }
}

fn set_i32(hwnd: HWND, attribute: u32, value: i32) -> i32 {
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            attribute,
            &value as *const i32 as *const _,
            size_of::<i32>() as u32,
        )
    }
}

fn get_u32(hwnd: HWND, attribute: u32) -> Option<u32> {
    let mut value = 0u32;
    let hr = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            attribute,
            &mut value as *mut u32 as *mut _,
            size_of::<u32>() as u32,
        )
    };
    if hr >= 0 {
        Some(value)
    } else {
        None
    }
}

fn get_i32(hwnd: HWND, attribute: u32) -> Option<i32> {
    let mut value = 0i32;
    let hr = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            attribute,
            &mut value as *mut i32 as *mut _,
            size_of::<i32>() as u32,
        )
    };
    if hr >= 0 {
        Some(value)
    } else {
        None
    }
}
