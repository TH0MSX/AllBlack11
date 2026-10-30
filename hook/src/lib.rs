#![allow(non_snake_case)]

//! Tiny hook loaded only into processes AllBlack11 has already decided are safe.
//!
//! It subclasses classic `#32770` dialogs so `WM_CTLCOLOR*` can paint a dark
//! background and light text, and it asks common controls to use
//! `DarkMode_Explorer` / `DarkMode_CFD`. Nothing here runs unless the main
//! program mapped `Local\AllBlack11.Hook` and set `enabled`.
//!
//! The code avoids panics: a panic in this DLL would take down the other process.

#![cfg_attr(not(windows), allow(dead_code))]

#[cfg(windows)]
mod win {
    use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicU8, AtomicUsize, Ordering};

    use windows_sys::Win32::Foundation::{BOOL, FALSE, HINSTANCE, HWND, LPARAM, LRESULT, TRUE, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        CreateSolidBrush, DeleteObject, FillRect, GetCurrentObject, GetObjectW, GetStockObject,
        SetBkColor, SetTextColor,
        HBRUSH, HDC, LOGFONTW, OBJ_FONT,
    };
    use windows_sys::Win32::System::LibraryLoader::{DisableThreadLibraryCalls, GetModuleFileNameW};
    use windows_sys::Win32::System::Memory::{MapViewOfFile, OpenFileMappingW, FILE_MAP_READ};
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::Controls::SetWindowTheme;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, CallWindowProcW, DefWindowProcW, EnumChildWindows, GetAncestor, GetClassNameW,
        GetClientRect, GetPropW, GetWindowLongPtrW, GetWindowTextW, RegisterWindowMessageW,
        RemovePropW, SendMessageW, SetPropW, SetWindowLongPtrW, CWPSTRUCT, GA_ROOT, GWL_STYLE,
        GWLP_WNDPROC, HC_ACTION, HWND_BROADCAST, WNDPROC, WM_CREATE, WM_CTLCOLORBTN, WM_CTLCOLORDLG,
        WM_CTLCOLOREDIT, WM_CTLCOLORLISTBOX, WM_CTLCOLORMSGBOX, WM_CTLCOLORSCROLLBAR,
        WM_CTLCOLORSTATIC, WM_ERASEBKGND, WM_INITDIALOG, WM_NCDESTROY, WM_SHOWWINDOW,
    };

    const MAGIC: u32 = 0xAB11_0001;
    const MAX_SUB: usize = 48;
    const MAX_THEMED: usize = 96;
    const CLR_NONE: usize = 0xFFFF_FFFF;
    const LVM_FIRST: u32 = 0x1000;
    const LVM_SETBKCOLOR: u32 = LVM_FIRST + 1;
    const LVM_SETTEXTCOLOR: u32 = LVM_FIRST + 36;
    const LVM_SETTEXTBKCOLOR: u32 = LVM_FIRST + 38;
    const TV_FIRST: u32 = 0x1100;
    const TVM_SETBKCOLOR: u32 = TV_FIRST + 29;
    const TVM_SETTEXTCOLOR: u32 = TV_FIRST + 30;
    const SS_NOTIFY: u32 = 0x0100;
    const DLL_PROCESS_DETACH: u32 = 0;
    const DLL_PROCESS_ATTACH: u32 = 1;

    #[repr(C)]
    struct SharedState {
        magic: u32,
        enabled: u32,
        bg: u32,
        text: u32,
        link: u32,
        heading: u32,
        edit_bg: u32,
    }

    static STARTED: AtomicBool = AtomicBool::new(false);
    static BRUSH: AtomicIsize = AtomicIsize::new(0);
    static EDIT_BRUSH: AtomicIsize = AtomicIsize::new(0);
    static SHARED: AtomicUsize = AtomicUsize::new(0);
    static STOP_MSG: AtomicU32 = AtomicU32::new(0);
    static HOST: AtomicU8 = AtomicU8::new(0);
    static SUB_HWND: [AtomicUsize; MAX_SUB] = [const { AtomicUsize::new(0) }; MAX_SUB];
    static SUB_ORIG: [AtomicIsize; MAX_SUB] = [const { AtomicIsize::new(0) }; MAX_SUB];
    static THEMED: [AtomicUsize; MAX_THEMED] = [const { AtomicUsize::new(0) }; MAX_THEMED];

    const HOST_UNKNOWN: u8 = 0;
    const HOST_OTHER: u8 = 1;
    const HOST_EXPLORER: u8 = 2;
    const HOST_MMC: u8 = 3;
    const HOST_DENIED: u8 = 4;

    #[no_mangle]
    pub unsafe extern "system" fn DllMain(
        module: HINSTANCE,
        reason: u32,
        reserved: *mut core::ffi::c_void,
    ) -> i32 {
        if reason == DLL_PROCESS_ATTACH {
            DisableThreadLibraryCalls(module);
        } else if reason == DLL_PROCESS_DETACH && reserved.is_null() {
            unsubclass_all();
            free_brushes();
        }
        1
    }

    #[no_mangle]
    pub unsafe extern "system" fn AllBlack11HookProc(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if code < 0 {
            return CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam);
        }
        if code == HC_ACTION as i32 && lparam != 0 {
            let info = &*(lparam as *const CWPSTRUCT);
            on_message(info.hwnd, info.message);
        }
        CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
    }

    unsafe fn on_message(hwnd: HWND, message: u32) {
        if host_kind() == HOST_DENIED {
            return;
        }
        let stop = stop_message();
        if !hook_enabled() || (stop != 0 && message == stop) {
            unsubclass_all();
            clear_themes();
            return;
        }
        let first = !STARTED.swap(true, Ordering::AcqRel);
        let interesting = first
            || message == WM_CREATE
            || message == WM_INITDIALOG
            || message == WM_SHOWWINDOW;
        if interesting {
            consider(hwnd);
        }
    }

    unsafe fn consider(hwnd: HWND) {
        if hwnd.is_null() {
            return;
        }
        let root = GetAncestor(hwnd, GA_ROOT);
        let root = if root.is_null() { hwnd } else { root };
        let host = host_kind();
        let class = class_name(hwnd);
        let root_class = if root == hwnd {
            class.clone()
        } else {
            class_name(root)
        };
        let dialog = class == "#32770" || root_class == "#32770";
        let panel = host == HOST_MMC || (host == HOST_EXPLORER && control_panel_title(root));
        if host == HOST_EXPLORER && !dialog && !panel {
            return;
        }
        if dialog || panel || host == HOST_MMC {
            theme_window(hwnd, 0);
        }
    }

    unsafe fn theme_window(hwnd: HWND, depth: i32) {
        if hwnd.is_null() || depth > 6 {
            return;
        }
        let class = class_name(hwnd);
        if class == "#32770" {
            subclass(hwnd);
            set_theme(hwnd, wide("DarkMode_CFD").as_ptr());
            EnumChildWindows(hwnd, Some(child_proc), (depth + 1) as LPARAM);
            return;
        }
        if class == "CabinetWClass" || class == "MMCMainFrame" {
            set_theme(hwnd, wide("DarkMode_Explorer").as_ptr());
            EnumChildWindows(hwnd, Some(child_proc), (depth + 1) as LPARAM);
            return;
        }
        theme_control(hwnd, &class);
        if class == "DirectUIHWND" || class == "SHELLDLL_DefView" {
            EnumChildWindows(hwnd, Some(child_proc), (depth + 1) as LPARAM);
        }
    }

    unsafe extern "system" fn child_proc(hwnd: HWND, depth: LPARAM) -> BOOL {
        theme_window(hwnd, depth as i32);
        TRUE
    }

    unsafe fn theme_control(hwnd: HWND, class: &str) {
        if class == "SysListView32" {
            set_theme(hwnd, wide("DarkMode_Explorer").as_ptr());
            let bg = color_bg();
            let text = color_text();
            SendMessageW(hwnd, LVM_SETBKCOLOR, 0, bg as LPARAM);
            SendMessageW(hwnd, LVM_SETTEXTCOLOR, 0, text as LPARAM);
            SendMessageW(hwnd, LVM_SETTEXTBKCOLOR, 0, bg as LPARAM);
        } else if class == "SysTreeView32" {
            set_theme(hwnd, wide("DarkMode_Explorer").as_ptr());
            SendMessageW(hwnd, TVM_SETBKCOLOR, 0, color_bg() as LPARAM);
            SendMessageW(hwnd, TVM_SETTEXTCOLOR, 0, color_text() as LPARAM);
        } else if class == "Edit" || class == "ComboBox" || class == "RichEdit20W" || class == "RichEdit50W"
        {
            set_theme(hwnd, wide("DarkMode_CFD").as_ptr());
        } else if class == "Button"
            || class == "Static"
            || class == "SysLink"
            || class == "ScrollBar"
            || class == "SysHeader32"
            || class == "SysTabControl32"
            || class == "DirectUIHWND"
            || class == "SHELLDLL_DefView"
        {
            set_theme(hwnd, wide("DarkMode_Explorer").as_ptr());
        }
    }

    unsafe fn set_theme(hwnd: HWND, name: *const u16) {
        if !GetPropW(hwnd, prop_themed().as_ptr()).is_null() {
            return;
        }
        SetWindowTheme(hwnd, name, std::ptr::null());
        SetPropW(
            hwnd,
            prop_themed().as_ptr(),
            1 as windows_sys::Win32::Foundation::HANDLE,
        );
        remember_themed(hwnd);
    }

    unsafe fn subclass(hwnd: HWND) {
        if lookup_orig(hwnd).is_some() {
            return;
        }
        let previous = GetWindowLongPtrW(hwnd, GWLP_WNDPROC);
        let ours = wndproc_bits(Some(dialog_proc));
        if previous == 0 || previous == ours {
            return;
        }
        if !store_orig(hwnd, previous) {
            return;
        }
        SetWindowLongPtrW(hwnd, GWLP_WNDPROC, ours);
        remember_themed(hwnd);
    }

    unsafe extern "system" fn dialog_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if !hook_enabled() {
            return detach_and_forward(hwnd, msg, wparam, lparam);
        }
        match msg {
            WM_ERASEBKGND => {
                let mut rect = zero_rect();
                GetClientRect(hwnd, &mut rect);
                FillRect(wparam as HDC, &rect, brush(color_bg()));
                return 1;
            }
            WM_CTLCOLORDLG | WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLOREDIT
            | WM_CTLCOLORLISTBOX | WM_CTLCOLORSCROLLBAR | WM_CTLCOLORMSGBOX => {
                let hdc = wparam as HDC;
                let (text, bg) = ctl_colors(msg, lparam as HWND, hdc);
                SetTextColor(hdc, text);
                SetBkColor(hdc, bg);
                return brush(bg) as LRESULT;
            }
            WM_NCDESTROY => {
                return detach_and_forward(hwnd, msg, wparam, lparam);
            }
            _ => {}
        }
        forward(hwnd, msg, wparam, lparam)
    }

    unsafe fn ctl_colors(msg: u32, control: HWND, hdc: HDC) -> (u32, u32) {
        let bg = if msg == WM_CTLCOLOREDIT || msg == WM_CTLCOLORLISTBOX {
            color_edit_bg()
        } else {
            color_bg()
        };
        let class = class_name(control);
        let style = GetWindowLongPtrW(control, GWL_STYLE) as u32;
        let text = if class == "SysLink" || style & SS_NOTIFY != 0 {
            color_link()
        } else if heading_font(hdc) {
            color_heading()
        } else {
            color_text()
        };
        (text, bg)
    }

    unsafe fn heading_font(hdc: HDC) -> bool {
        let font = GetStockObject(OBJ_FONT as i32);
        let current = GetCurrentObject(hdc, OBJ_FONT as u32);
        if current.is_null() || current == font {
            return false;
        }
        let mut lf: LOGFONTW = std::mem::zeroed();
        let wrote = GetObjectW(
            current,
            std::mem::size_of::<LOGFONTW>() as i32,
            &mut lf as *mut LOGFONTW as *mut _,
        );
        wrote != 0 && lf.lfHeight.unsigned_abs() >= 18
    }

    unsafe fn detach_and_forward(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let orig = take_orig(hwnd);
        SetWindowTheme(hwnd, std::ptr::null(), std::ptr::null());
        RemovePropW(hwnd, prop_themed().as_ptr());
        if let Some(orig) = orig {
            SetWindowLongPtrW(hwnd, GWLP_WNDPROC, wndproc_bits(orig));
            return CallWindowProcW(orig, hwnd, msg, wparam, lparam);
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    unsafe fn forward(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if let Some(orig) = lookup_orig(hwnd) {
            CallWindowProcW(orig, hwnd, msg, wparam, lparam)
        } else {
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }

    unsafe fn unsubclass_all() {
        for index in 0..MAX_SUB {
            let key = SUB_HWND[index].swap(0, Ordering::AcqRel);
            let orig = SUB_ORIG[index].swap(0, Ordering::AcqRel);
            if key == 0 || orig == 0 {
                continue;
            }
            let hwnd = key as HWND;
            SetWindowLongPtrW(hwnd, GWLP_WNDPROC, orig);
            SetWindowTheme(hwnd, std::ptr::null(), std::ptr::null());
            RemovePropW(hwnd, prop_themed().as_ptr());
        }
    }

    unsafe fn clear_themes() {
        for index in 0..MAX_THEMED {
            let key = THEMED[index].swap(0, Ordering::AcqRel);
            if key == 0 {
                continue;
            }
            let hwnd = key as HWND;
            let class = class_name(hwnd);
            SetWindowTheme(hwnd, std::ptr::null(), std::ptr::null());
            RemovePropW(hwnd, prop_themed().as_ptr());
            if class == "SysListView32" {
                SendMessageW(hwnd, LVM_SETBKCOLOR, 0, CLR_NONE as LPARAM);
                SendMessageW(hwnd, LVM_SETTEXTCOLOR, 0, CLR_NONE as LPARAM);
                SendMessageW(hwnd, LVM_SETTEXTBKCOLOR, 0, CLR_NONE as LPARAM);
            } else if class == "SysTreeView32" {
                SendMessageW(hwnd, TVM_SETBKCOLOR, 0, CLR_NONE as LPARAM);
                SendMessageW(hwnd, TVM_SETTEXTCOLOR, 0, CLR_NONE as LPARAM);
            }
        }
        free_brushes();
    }

    fn wndproc_bits(proc: WNDPROC) -> isize {
        unsafe { std::mem::transmute(proc) }
    }

    fn lookup_orig(hwnd: HWND) -> Option<WNDPROC> {
        let key = hwnd as usize;
        for index in 0..MAX_SUB {
            if SUB_HWND[index].load(Ordering::Acquire) == key {
                let orig = SUB_ORIG[index].load(Ordering::Acquire);
                if orig != 0 {
                    return Some(unsafe { std::mem::transmute::<isize, WNDPROC>(orig) });
                }
            }
        }
        None
    }

    fn take_orig(hwnd: HWND) -> Option<WNDPROC> {
        let key = hwnd as usize;
        for index in 0..MAX_SUB {
            if SUB_HWND[index].load(Ordering::Acquire) == key {
                let orig = SUB_ORIG[index].swap(0, Ordering::AcqRel);
                SUB_HWND[index].store(0, Ordering::Release);
                if orig != 0 {
                    return Some(unsafe { std::mem::transmute::<isize, WNDPROC>(orig) });
                }
            }
        }
        None
    }

    fn store_orig(hwnd: HWND, orig: isize) -> bool {
        let key = hwnd as usize;
        for index in 0..MAX_SUB {
            if SUB_HWND[index]
                .compare_exchange(0, key, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                SUB_ORIG[index].store(orig, Ordering::Release);
                return true;
            }
        }
        false
    }

    fn remember_themed(hwnd: HWND) {
        let key = hwnd as usize;
        for index in 0..MAX_THEMED {
            let current = THEMED[index].load(Ordering::Acquire);
            if current == key {
                return;
            }
            if current == 0
                && THEMED[index]
                    .compare_exchange(0, key, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
            {
                return;
            }
        }
    }

    unsafe fn hook_enabled() -> bool {
        shared().is_some_and(|state| state.magic == MAGIC && state.enabled != 0)
    }

    unsafe fn shared() -> Option<&'static SharedState> {
        let cached = SHARED.load(Ordering::Acquire);
        if cached != 0 {
            return Some(&*(cached as *const SharedState));
        }
        let name = wide("Local\\AllBlack11.Hook");
        let handle = OpenFileMappingW(FILE_MAP_READ, FALSE, name.as_ptr());
        if handle.is_null() {
            return None;
        }
        let view = MapViewOfFile(handle, FILE_MAP_READ, 0, 0, std::mem::size_of::<SharedState>());
        if view.Value.is_null() {
            return None;
        }
        SHARED.store(view.Value as usize, Ordering::Release);
        Some(&*(view.Value as *const SharedState))
    }

    unsafe fn color_bg() -> u32 {
        shared().map(|state| state.bg).filter(|v| *v != 0).unwrap_or(0x001C_1C1C)
    }
    unsafe fn color_text() -> u32 {
        shared().map(|state| state.text).filter(|v| *v != 0).unwrap_or(0x00F2_F2F2)
    }
    unsafe fn color_link() -> u32 {
        shared().map(|state| state.link).filter(|v| *v != 0).unwrap_or(0x00FF_B56E)
    }
    unsafe fn color_heading() -> u32 {
        shared().map(|state| state.heading).filter(|v| *v != 0).unwrap_or(0x00FF_FFFF)
    }
    unsafe fn color_edit_bg() -> u32 {
        shared().map(|state| state.edit_bg).filter(|v| *v != 0).unwrap_or(0x002A_2A2A)
    }

    unsafe fn brush(color: u32) -> HBRUSH {
        let slot = if color == color_edit_bg() {
            &EDIT_BRUSH
        } else {
            &BRUSH
        };
        let existing = slot.load(Ordering::Acquire);
        if existing != 0 {
            return existing as HBRUSH;
        }
        let created = CreateSolidBrush(color);
        if created.is_null() {
            return std::ptr::null_mut();
        }
        if slot
            .compare_exchange(0, created as isize, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            DeleteObject(created);
            return slot.load(Ordering::Acquire) as HBRUSH;
        }
        created
    }

    unsafe fn free_brushes() {
        for slot in [&BRUSH, &EDIT_BRUSH] {
            let existing = slot.swap(0, Ordering::AcqRel);
            if existing != 0 {
                DeleteObject(existing as HBRUSH);
            }
        }
    }

    unsafe fn host_kind() -> u8 {
        let current = HOST.load(Ordering::Acquire);
        if current != HOST_UNKNOWN {
            return current;
        }
        let mut buf = [0u16; 260];
        let length = GetModuleFileNameW(std::ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32);
        let name = if length == 0 {
            String::new()
        } else {
            String::from_utf16_lossy(&buf[..length as usize])
        };
        let file = name.rsplit(['\\', '/']).next().unwrap_or("");
        let kind = if denied(file) {
            HOST_DENIED
        } else if file.eq_ignore_ascii_case("explorer.exe") {
            HOST_EXPLORER
        } else if file.eq_ignore_ascii_case("mmc.exe") {
            HOST_MMC
        } else {
            HOST_OTHER
        };
        HOST.store(kind, Ordering::Release);
        kind
    }

    fn denied(file: &str) -> bool {
        const NAMES: &[&str] = &[
            "csrss.exe",
            "dwm.exe",
            "lsass.exe",
            "lsaiso.exe",
            "services.exe",
            "smss.exe",
            "wininit.exe",
            "winlogon.exe",
            "logonui.exe",
            "allblack11.exe",
            "sihost.exe",
            "fontdrvhost.exe",
        ];
        NAMES.iter().any(|name| file.eq_ignore_ascii_case(name))
    }

    unsafe fn control_panel_title(hwnd: HWND) -> bool {
        let mut buf = [0u16; 128];
        let length = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if length <= 0 {
            return false;
        }
        let title = String::from_utf16_lossy(&buf[..length as usize]);
        let title = title.trim();
        title.eq_ignore_ascii_case("Control Panel")
            || title.to_ascii_lowercase().starts_with("control panel\\")
            || title.to_ascii_lowercase().starts_with("control panel ")
    }

    unsafe fn class_name(hwnd: HWND) -> String {
        let mut buf = [0u16; 64];
        let length = GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if length <= 0 {
            String::new()
        } else {
            String::from_utf16_lossy(&buf[..length as usize])
        }
    }

    unsafe fn stop_message() -> u32 {
        let cached = STOP_MSG.load(Ordering::Acquire);
        if cached != 0 {
            return cached;
        }
        let name = wide("AllBlack11.HookStop");
        let message = RegisterWindowMessageW(name.as_ptr());
        if message != 0 {
            STOP_MSG.store(message, Ordering::Release);
        }
        message
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn prop_themed() -> Vec<u16> {
        wide("AllBlack11.Themed")
    }

    fn zero_rect() -> windows_sys::Win32::Foundation::RECT {
        windows_sys::Win32::Foundation::RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        }
    }

    #[allow(dead_code)]
    fn thread_id() -> u32 {
        unsafe { GetCurrentThreadId() }
    }

    #[allow(dead_code)]
    const _BROADCAST: HWND = HWND_BROADCAST;
}

#[cfg(not(windows))]
#[no_mangle]
pub extern "C" fn allblack11_hook_stub() {}
