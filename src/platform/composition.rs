//! `SetWindowCompositionAttribute` is an undocumented export of user32.
//! TranslucentTB calls it on other processes' windows (including `Shell_TrayWnd`).
//! The call stays in this process: user32 forwards the accent policy to DWM.
//! It is not injection.

use std::mem::size_of;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{BOOL, HWND};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};

use super::wide::{pcw, to_wide};

pub const ACCENT_DISABLED: i32 = 0;
pub const ACCENT_ENABLE_BLURBEHIND: i32 = 3;
pub const ACCENT_ENABLE_ACRYLICBLURBEHIND: i32 = 4;
const WCA_ACCENT_POLICY: u32 = 19;
/// Bit that makes `GradientColor` the acrylic tint. Without it the color is ignored.
const ACCENT_FLAG_COLOR: u32 = 2;

#[repr(C)]
struct AccentPolicy {
    state: i32,
    flags: u32,
    gradient: u32,
    animation_id: u32,
}

#[repr(C)]
struct WindowCompositionAttribData {
    attrib: u32,
    data: *mut core::ffi::c_void,
    size: usize,
}

type SetWindowCompositionAttribute =
    unsafe extern "system" fn(HWND, *mut WindowCompositionAttribData) -> BOOL;

pub fn set_accent(hwnd: HWND, state: i32, gradient: u32) -> bool {
    let Some(set) = load() else {
        return false;
    };
    let colored = state != ACCENT_DISABLED;
    let mut policy = AccentPolicy {
        state,
        flags: if colored { ACCENT_FLAG_COLOR } else { 0 },
        gradient: if colored { gradient } else { 0 },
        animation_id: 0,
    };
    let mut data = WindowCompositionAttribData {
        attrib: WCA_ACCENT_POLICY,
        data: &mut policy as *mut AccentPolicy as *mut _,
        size: size_of::<AccentPolicy>(),
    };
    unsafe { set(hwnd, &mut data) != 0 }
}

pub fn clear_accent(hwnd: HWND) -> bool {
    set_accent(hwnd, ACCENT_DISABLED, 0)
}

fn load() -> Option<SetWindowCompositionAttribute> {
    static ADDR: OnceLock<usize> = OnceLock::new();
    let addr = *ADDR.get_or_init(|| unsafe {
        let module_name = to_wide("user32.dll");
        let module = GetModuleHandleW(pcw(&module_name));
        if module.is_null() {
            return 0;
        }
        match GetProcAddress(module, b"SetWindowCompositionAttribute\0".as_ptr()) {
            Some(func) => func as usize,
            None => 0,
        }
    });
    if addr == 0 {
        None
    } else {
        Some(unsafe { std::mem::transmute::<usize, SetWindowCompositionAttribute>(addr) })
    }
}
