//! Finding windows for the picker: a list of open windows and "the window under the cursor".
//!
//! An HWND is just a number Windows can reuse. Every entry therefore also records the owning
//! process id and that process's creation time, and anything that acts on a window later
//! checks all three before touching it.

use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT};
use windows_sys::Win32::Graphics::Dwm::DwmGetWindowAttribute;
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetAncestor, GetCursorPos, GetWindow, GetWindowLongPtrW, GetWindowRect,
    GetWindowThreadProcessId, IsWindowVisible, WindowFromPoint, GA_ROOT, GWLP_HINSTANCE,
    GWL_EXSTYLE, GW_OWNER,
};

use super::legacy::class_name;
use super::procinfo::window_title;
use crate::Fingerprint;

const WS_EX_TOOLWINDOW: u32 = 0x80;
const WS_EX_NOREDIRECTIONBITMAP: u32 = 0x0020_0000;
const DWMWA_CLOAKED: u32 = 14;

#[derive(Clone, Debug)]
pub struct WindowEntry {
    pub key: i64,
    pub pid: u32,
    pub exe: String,
    pub class: String,
    /// What was true of this window when the list was built. Anything that later acts on the
    /// entry must re-check it against the live window first (an HWND can be reused).
    pub fingerprint: Fingerprint,
    /// Kept only in memory for the list. Never written to disk or to a report by default.
    pub title: String,
}

fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked = 0u32;
    let hr = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut _,
            std::mem::size_of::<u32>() as u32,
        )
    };
    hr >= 0 && cloaked != 0
}

/// Facts about a window that do not change over its life. See `crate::identity`.
pub fn fingerprint_of(hwnd: HWND) -> Option<Fingerprint> {
    if hwnd.is_null() {
        return None;
    }
    let mut pid = 0u32;
    let thread = unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if thread == 0 || pid == 0 {
        return None;
    }
    let (create, _) = super::app::process_basic(pid)?;
    let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    Some(Fingerprint {
        pid,
        create,
        thread,
        class: class_name(hwnd),
        instance: unsafe { GetWindowLongPtrW(hwnd, GWLP_HINSTANCE) } as u64,
        owner: unsafe { GetWindow(hwnd, GW_OWNER) } as isize as i64,
        top_level: unsafe { GetAncestor(hwnd, GA_ROOT) } == hwnd,
        tool_window: ex_style & WS_EX_TOOLWINDOW != 0,
        no_redirection: ex_style & WS_EX_NOREDIRECTIONBITMAP != 0,
    })
}

pub fn entry_for(hwnd: HWND) -> Option<WindowEntry> {
    let fingerprint = fingerprint_of(hwnd)?;
    let (_, exe) = super::app::process_basic(fingerprint.pid)?;
    Some(WindowEntry {
        key: hwnd as isize as i64,
        pid: fingerprint.pid,
        exe,
        class: fingerprint.class.clone(),
        fingerprint,
        title: window_title(hwnd),
    })
}

/// Visible top-level windows, newest process first is not promised; sorted by exe name.
pub fn list_windows() -> Vec<WindowEntry> {
    let mut found: Vec<HWND> = Vec::new();
    unsafe {
        EnumWindows(Some(collect), &mut found as *mut Vec<HWND> as LPARAM);
    }
    let own = unsafe { GetCurrentProcessId() };
    let mut entries: Vec<WindowEntry> = found
        .into_iter()
        .filter_map(entry_for)
        .filter(|entry| entry.pid != own)
        .collect();
    entries.sort_by(|a, b| {
        a.exe
            .to_ascii_lowercase()
            .cmp(&b.exe.to_ascii_lowercase())
            .then(a.class.cmp(&b.class))
    });
    entries.truncate(120);
    entries
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let found = &mut *(lparam as *mut Vec<HWND>);
    if IsWindowVisible(hwnd) == 0 {
        return 1;
    }
    if GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW != 0 {
        return 1;
    }
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    if GetWindowRect(hwnd, &mut rect) == 0
        || rect.right - rect.left < 40
        || rect.bottom - rect.top < 24
    {
        return 1;
    }
    if is_cloaked(hwnd) {
        return 1;
    }
    found.push(hwnd);
    1
}

/// The top-level window under the mouse, if it is not one of ours.
pub fn window_under_cursor() -> Option<WindowEntry> {
    let mut point = POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut point) } == 0 {
        return None;
    }
    let hit = unsafe { WindowFromPoint(point) };
    if hit.is_null() {
        return None;
    }
    let root = unsafe { GetAncestor(hit, GA_ROOT) };
    let entry = entry_for(if root.is_null() { hit } else { root })?;
    if entry.pid == unsafe { GetCurrentProcessId() } {
        None
    } else {
        Some(entry)
    }
}
