#![allow(non_snake_case)]

//! AllBlack11 legacy hook, v2. Loaded only into threads AllBlack11 has already decided are
//! safe (see `legacy_policy` and `inject`). It does nothing unless the main program mapped
//! `Local\AllBlack11.Hook` with a matching `MAGIC` and `enabled` set.
//!
//! How it differs from v1:
//! * It observes messages *after* the target handled them (`WH_CALLWNDPROCRET`), so a control
//!   is never poked before its own `WM_CREATE` has run.
//! * Every window it changes gets an entry holding the original state and the owning thread.
//!   Restoring only touches entries owned by the calling thread, because subclass and theme
//!   calls are only valid on the owner's thread.
//! * What it does per control family comes from the shared capability table. Owner-drawn
//!   controls, shell surfaces, tab strips and rich edits are left alone.
//! * The DLL pins itself into the process. Turning styling off removes subclasses and resets
//!   themes; it does not unload code that window procedures might still point at.
//!
//! The crate is built with `panic = "abort"`: a panic here would end someone else's process.
//! Every fallible step therefore returns early instead of unwrapping.

#![cfg_attr(not(windows), allow(dead_code))]

#[path = "../../src/legacy_policy.rs"]
#[allow(dead_code)]
mod policy;

#[cfg(windows)]
mod win {
    use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicU8, AtomicUsize, Ordering};
    use std::sync::Mutex;

    use windows_sys::Win32::Foundation::{
        BOOL, FALSE, HINSTANCE, HWND, LPARAM, LRESULT, TRUE, WPARAM,
    };
    use windows_sys::Win32::Graphics::Gdi::{
        CreateSolidBrush, FillRect, GetCurrentObject, GetObjectW, GetStockObject, InvalidateRect,
        SetBkColor, SetTextColor, HBRUSH, HDC, LOGFONTW, OBJ_FONT,
    };
    use windows_sys::Win32::System::LibraryLoader::{
        DisableThreadLibraryCalls, GetModuleFileNameW, GetModuleHandleExW,
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_PIN,
    };
    use windows_sys::Win32::System::Memory::{MapViewOfFile, OpenFileMappingW, FILE_MAP_READ};
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::Controls::SetWindowTheme;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, CallWindowProcW, DefWindowProcW, EnumChildWindows, GetClassNameW,
        GetClientRect, GetParent, GetWindowLongPtrW, RegisterWindowMessageW, SendMessageW,
        SetWindowLongPtrW, CWPRETSTRUCT, GWLP_WNDPROC, GWL_STYLE, HC_ACTION, WM_CREATE,
        WM_CTLCOLORBTN, WM_CTLCOLORDLG, WM_CTLCOLOREDIT, WM_CTLCOLORLISTBOX, WM_CTLCOLORMSGBOX,
        WM_CTLCOLORSCROLLBAR, WM_CTLCOLORSTATIC, WM_ERASEBKGND, WM_INITDIALOG, WM_NCDESTROY,
        WM_SHOWWINDOW, WNDPROC,
    };

    use crate::policy::SharedState;
    use crate::policy::{
        capability, host_kind, window_verdict, HostKind, Tech, Verdict, FLAG_CLASSIC_BUTTONS,
        FLAG_THEME_PUSH_BUTTONS, MAGIC,
    };

    // ---- constants --------------------------------------------------------------------

    const MAX_ENTRIES: usize = 256;
    const MAX_DEPTH: u32 = 8;
    const CLR_NONE: u32 = 0xFFFF_FFFF;
    const WM_ENABLE: u32 = 0x000A;

    const LVM_FIRST: u32 = 0x1000;
    const LVM_GETBKCOLOR: u32 = LVM_FIRST;
    const LVM_SETBKCOLOR: u32 = LVM_FIRST + 1;
    const LVM_GETHEADER: u32 = LVM_FIRST + 31;
    const LVM_GETTEXTCOLOR: u32 = LVM_FIRST + 35;
    const LVM_SETTEXTCOLOR: u32 = LVM_FIRST + 36;
    const LVM_GETTEXTBKCOLOR: u32 = LVM_FIRST + 37;
    const LVM_SETTEXTBKCOLOR: u32 = LVM_FIRST + 38;
    const TV_FIRST: u32 = 0x1100;
    const TVM_GETBKCOLOR: u32 = TV_FIRST + 31;
    const TVM_GETTEXTCOLOR: u32 = TV_FIRST + 32;
    const TVM_SETBKCOLOR: u32 = TV_FIRST + 29;
    const TVM_SETTEXTCOLOR: u32 = TV_FIRST + 30;

    const SS_NOTIFY: u32 = 0x0100;
    const WS_DISABLED: u32 = 0x0800_0000;

    // ---- per-window entries -----------------------------------------------------------

    #[derive(Clone, Copy, PartialEq, Eq)]
    #[repr(u8)]
    enum Kind {
        Free = 0,
        /// A dialog we subclassed.
        Dialog = 1,
        /// A dialog we looked at and left alone, so late children skip it too.
        Refused = 2,
        /// A control given a visual style by name.
        Themed = 3,
        /// A button painted classically while enabled.
        Classic = 4,
        ListView = 5,
        TreeView = 6,
    }

    struct Entry {
        hwnd: AtomicUsize,
        thread: AtomicU32,
        kind: AtomicU8,
        /// Dialog subclass is still installed but forwards only.
        passthrough: AtomicBool,
        orig_proc: AtomicIsize,
        colors: [AtomicU32; 3],
    }

    impl Entry {
        const fn new() -> Self {
            Self {
                hwnd: AtomicUsize::new(0),
                thread: AtomicU32::new(0),
                kind: AtomicU8::new(0),
                passthrough: AtomicBool::new(false),
                orig_proc: AtomicIsize::new(0),
                colors: [const { AtomicU32::new(0) }; 3],
            }
        }
    }

    static ENTRIES: [Entry; MAX_ENTRIES] = [const { Entry::new() }; MAX_ENTRIES];
    static PINNED: AtomicBool = AtomicBool::new(false);
    static SHARED: AtomicUsize = AtomicUsize::new(0);
    static STOP_MSG: AtomicU32 = AtomicU32::new(0);
    static HOST: AtomicU8 = AtomicU8::new(0);
    static BRUSHES: Mutex<[(u32, isize); 12]> = Mutex::new([(0, 0); 12]);

    fn find(hwnd: HWND) -> Option<usize> {
        let key = hwnd as usize;
        (0..MAX_ENTRIES).find(|&index| ENTRIES[index].hwnd.load(Ordering::Acquire) == key)
    }

    fn claim(hwnd: HWND, kind: Kind) -> Option<usize> {
        if let Some(index) = find(hwnd) {
            ENTRIES[index].kind.store(kind as u8, Ordering::Release);
            return Some(index);
        }
        let key = hwnd as usize;
        for index in 0..MAX_ENTRIES {
            if ENTRIES[index]
                .hwnd
                .compare_exchange(0, key, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                let entry = &ENTRIES[index];
                entry
                    .thread
                    .store(unsafe { GetCurrentThreadId() }, Ordering::Release);
                entry.kind.store(kind as u8, Ordering::Release);
                entry.passthrough.store(false, Ordering::Release);
                entry.orig_proc.store(0, Ordering::Release);
                for color in &entry.colors {
                    color.store(0, Ordering::Release);
                }
                return Some(index);
            }
        }
        None
    }

    fn release(index: usize) {
        let entry = &ENTRIES[index];
        entry.kind.store(Kind::Free as u8, Ordering::Release);
        entry.orig_proc.store(0, Ordering::Release);
        entry.hwnd.store(0, Ordering::Release);
    }

    fn kind_of(index: usize) -> Kind {
        match ENTRIES[index].kind.load(Ordering::Acquire) {
            1 => Kind::Dialog,
            2 => Kind::Refused,
            3 => Kind::Themed,
            4 => Kind::Classic,
            5 => Kind::ListView,
            6 => Kind::TreeView,
            _ => Kind::Free,
        }
    }

    // ---- entry points -----------------------------------------------------------------

    #[no_mangle]
    pub unsafe extern "system" fn DllMain(
        module: HINSTANCE,
        reason: u32,
        _reserved: *mut core::ffi::c_void,
    ) -> i32 {
        if reason == 1 {
            DisableThreadLibraryCalls(module);
        }
        1
    }

    #[no_mangle]
    pub unsafe extern "system" fn AllBlack11HookProcRet(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if code == HC_ACTION as i32 && lparam != 0 {
            pin_module();
            let info = &*(lparam as *const CWPRETSTRUCT);
            on_message(info.hwnd, info.message, info.wParam);
        }
        CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
    }

    /// Never let this module unload while a window procedure of ours may be installed.
    unsafe fn pin_module() {
        if PINNED.swap(true, Ordering::AcqRel) {
            return;
        }
        let mut module = std::ptr::null_mut();
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_PIN | GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
            AllBlack11HookProcRet as *const u16,
            &mut module,
        );
    }

    unsafe fn on_message(hwnd: HWND, message: u32, wparam: WPARAM) {
        if hwnd.is_null() || host_kind_cached() == HostKind::Denied {
            return;
        }
        let stop = stop_message();
        if message == stop && stop != 0 {
            restore_thread();
            return;
        }
        if !hook_enabled() {
            restore_thread();
            return;
        }
        match message {
            WM_INITDIALOG => {
                if class_name(hwnd) == "#32770" {
                    treat_dialog(hwnd, 0);
                }
            }
            WM_SHOWWINDOW => {
                if wparam != 0 && class_name(hwnd) == "#32770" && find(hwnd).is_none() {
                    treat_dialog(hwnd, 0);
                }
            }
            WM_CREATE => {
                // A control created after its dialog was set up.
                if !GetParent(hwnd).is_null() && accepted_ancestor(hwnd) {
                    treat_control(hwnd);
                }
            }
            WM_ENABLE => reapply_button(hwnd, wparam != 0),
            _ => {}
        }
    }

    // ---- dialogs ----------------------------------------------------------------------

    unsafe fn treat_dialog(hwnd: HWND, depth: u32) {
        if hwnd.is_null() || depth > MAX_DEPTH {
            return;
        }
        if let Some(index) = find(hwnd) {
            if kind_of(index) == Kind::Dialog {
                // Re-enabled after being switched off.
                ENTRIES[index].passthrough.store(false, Ordering::Release);
                treat_descendants(hwnd, depth);
                InvalidateRect(hwnd, std::ptr::null(), 1);
            }
            return;
        }
        // A page of a refused sheet is refused too.
        if refused_ancestor(hwnd) {
            return;
        }
        let children = descendant_classes(hwnd);
        let refs: Vec<&str> = children.iter().map(String::as_str).collect();
        match window_verdict(host_kind_cached(), &class_name(hwnd), &refs) {
            Verdict::Skip(_) => {
                claim(hwnd, Kind::Refused);
            }
            Verdict::Full | Verdict::Partial(_) => {
                let Some(index) = claim(hwnd, Kind::Dialog) else {
                    return;
                };
                let previous = GetWindowLongPtrW(hwnd, GWLP_WNDPROC);
                let ours = wndproc_bits(Some(dialog_proc));
                if previous == 0 || previous == ours {
                    release(index);
                    return;
                }
                ENTRIES[index].orig_proc.store(previous, Ordering::Release);
                SetWindowLongPtrW(hwnd, GWLP_WNDPROC, ours);
                treat_descendants(hwnd, depth);
                InvalidateRect(hwnd, std::ptr::null(), 1);
            }
        }
    }

    unsafe fn treat_descendants(root: HWND, depth: u32) {
        let mut found: Vec<HWND> = Vec::new();
        EnumChildWindows(
            root,
            Some(collect_hwnd),
            &mut found as *mut Vec<HWND> as LPARAM,
        );
        for child in found {
            if class_name(child) == "#32770" {
                treat_dialog(child, depth + 1);
            } else {
                treat_control(child);
            }
        }
    }

    unsafe extern "system" fn collect_hwnd(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let found = &mut *(lparam as *mut Vec<HWND>);
        if found.len() < 512 {
            found.push(hwnd);
            TRUE
        } else {
            FALSE
        }
    }

    unsafe fn descendant_classes(root: HWND) -> Vec<String> {
        let mut found: Vec<HWND> = Vec::new();
        EnumChildWindows(
            root,
            Some(collect_hwnd),
            &mut found as *mut Vec<HWND> as LPARAM,
        );
        found.into_iter().map(|child| class_name(child)).collect()
    }

    /// Some ancestor is a dialog we treat.
    unsafe fn accepted_ancestor(hwnd: HWND) -> bool {
        let mut current = GetParent(hwnd);
        let mut hops = 0;
        while !current.is_null() && hops < MAX_DEPTH {
            if let Some(index) = find(current) {
                match kind_of(index) {
                    Kind::Dialog => return !ENTRIES[index].passthrough.load(Ordering::Acquire),
                    Kind::Refused => return false,
                    _ => {}
                }
            }
            current = GetParent(current);
            hops += 1;
        }
        false
    }

    unsafe fn refused_ancestor(hwnd: HWND) -> bool {
        let mut current = GetParent(hwnd);
        let mut hops = 0;
        while !current.is_null() && hops < MAX_DEPTH {
            if let Some(index) = find(current) {
                if kind_of(index) == Kind::Refused {
                    return true;
                }
            }
            current = GetParent(current);
            hops += 1;
        }
        false
    }

    unsafe extern "system" fn dialog_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        let Some(index) = find(hwnd) else {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        };
        let entry = &ENTRIES[index];
        let orig = entry.orig_proc.load(Ordering::Acquire);
        if msg == WM_NCDESTROY {
            let result = forward(orig, hwnd, msg, wparam, lparam);
            release(index);
            return result;
        }
        if entry.passthrough.load(Ordering::Acquire) || !hook_enabled() {
            return forward(orig, hwnd, msg, wparam, lparam);
        }
        match msg {
            WM_ERASEBKGND => {
                let mut rect = zero_rect();
                GetClientRect(hwnd, &mut rect);
                FillRect(wparam as HDC, &rect, brush(color_bg()));
                return 1;
            }
            WM_CTLCOLORDLG | WM_CTLCOLORMSGBOX | WM_CTLCOLORBTN | WM_CTLCOLORSTATIC
            | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX | WM_CTLCOLORSCROLLBAR => {
                let hdc = wparam as HDC;
                let (text, bg) = ctl_colors(msg, lparam as HWND, hdc);
                SetTextColor(hdc, text);
                SetBkColor(hdc, bg);
                return brush(bg) as LRESULT;
            }
            _ => {}
        }
        forward(orig, hwnd, msg, wparam, lparam)
    }

    unsafe fn forward(
        orig: isize,
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if orig == 0 {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let proc: WNDPROC = std::mem::transmute::<isize, WNDPROC>(orig);
        CallWindowProcW(proc, hwnd, msg, wparam, lparam)
    }

    // ---- colors for WM_CTLCOLOR* ------------------------------------------------------

    /// (text color, background color) for a control. Every pair is chosen to stay readable
    /// in every state this layer can see: normal, read-only, disabled.
    unsafe fn ctl_colors(msg: u32, control: HWND, hdc: HDC) -> (u32, u32) {
        let text = color_text();
        match msg {
            WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => (text, color_edit_bg()),
            WM_CTLCOLORSTATIC => {
                let class = class_name(control);
                let style = GetWindowLongPtrW(control, GWL_STYLE) as u32;
                let disabled = style & WS_DISABLED != 0;
                if class.eq_ignore_ascii_case("Edit") {
                    // Read-only and disabled edits are reported as "static".
                    return (
                        if disabled { color_disabled() } else { text },
                        color_readonly_bg(),
                    );
                }
                if class.eq_ignore_ascii_case("Button") {
                    return (if disabled { color_disabled() } else { text }, color_bg());
                }
                if class.eq_ignore_ascii_case("SysLink") || style & SS_NOTIFY != 0 {
                    return (color_link(), color_bg());
                }
                if heading_font(hdc) {
                    return (color_heading(), color_bg());
                }
                (text, color_bg())
            }
            _ => (text, color_bg()),
        }
    }

    unsafe fn heading_font(hdc: HDC) -> bool {
        let stock = GetStockObject(OBJ_FONT as i32);
        let current = GetCurrentObject(hdc, OBJ_FONT as u32);
        if current.is_null() || current == stock {
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

    // ---- controls ---------------------------------------------------------------------

    unsafe fn treat_control(hwnd: HWND) {
        if hwnd.is_null() || find(hwnd).is_some() {
            return;
        }
        let class = class_name(hwnd);
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let capability = capability(&class, style);
        match capability.tech {
            Tech::Leave | Tech::CtlColor => {}
            Tech::ThemeCfd => {
                if claim(hwnd, Kind::Themed).is_some() {
                    SetWindowTheme(hwnd, wide("DarkMode_CFD").as_ptr(), std::ptr::null());
                }
            }
            Tech::ThemeExplorer => {
                let is_button = class.eq_ignore_ascii_case("Button");
                if is_button && !flag(FLAG_THEME_PUSH_BUTTONS) {
                    return;
                }
                if claim(hwnd, Kind::Themed).is_some() {
                    SetWindowTheme(hwnd, wide("DarkMode_Explorer").as_ptr(), std::ptr::null());
                }
            }
            Tech::Classic => {
                if !flag(FLAG_CLASSIC_BUTTONS) {
                    return;
                }
                if claim(hwnd, Kind::Classic).is_some() {
                    // A disabled button keeps its visual style: classic disabled text is drawn
                    // embossed with a white highlight, which is ugly and hard to read on dark.
                    reapply_button(hwnd, style & WS_DISABLED == 0);
                }
            }
            Tech::ListView => treat_list(hwnd),
            Tech::TreeView => treat_tree(hwnd),
            Tech::Header => {
                if claim(hwnd, Kind::Themed).is_some() {
                    SetWindowTheme(hwnd, wide("DarkMode_ItemsView").as_ptr(), std::ptr::null());
                }
            }
        }
    }

    unsafe fn reapply_button(hwnd: HWND, enabled: bool) {
        let Some(index) = find(hwnd) else {
            return;
        };
        if kind_of(index) != Kind::Classic {
            return;
        }
        if enabled {
            SetWindowTheme(hwnd, wide("").as_ptr(), wide("").as_ptr());
        } else {
            SetWindowTheme(hwnd, std::ptr::null(), std::ptr::null());
        }
    }

    unsafe fn treat_list(hwnd: HWND) {
        let Some(index) = claim(hwnd, Kind::ListView) else {
            return;
        };
        let entry = &ENTRIES[index];
        entry.colors[0].store(
            SendMessageW(hwnd, LVM_GETBKCOLOR, 0, 0) as u32,
            Ordering::Release,
        );
        entry.colors[1].store(
            SendMessageW(hwnd, LVM_GETTEXTCOLOR, 0, 0) as u32,
            Ordering::Release,
        );
        entry.colors[2].store(
            SendMessageW(hwnd, LVM_GETTEXTBKCOLOR, 0, 0) as u32,
            Ordering::Release,
        );
        SetWindowTheme(hwnd, wide("DarkMode_Explorer").as_ptr(), std::ptr::null());
        let bg = color_edit_bg();
        SendMessageW(hwnd, LVM_SETBKCOLOR, 0, bg as LPARAM);
        SendMessageW(hwnd, LVM_SETTEXTCOLOR, 0, color_text() as LPARAM);
        SendMessageW(hwnd, LVM_SETTEXTBKCOLOR, 0, bg as LPARAM);
        let header = SendMessageW(hwnd, LVM_GETHEADER, 0, 0) as HWND;
        if !header.is_null() && claim(header, Kind::Themed).is_some() {
            SetWindowTheme(
                header,
                wide("DarkMode_ItemsView").as_ptr(),
                std::ptr::null(),
            );
        }
    }

    unsafe fn treat_tree(hwnd: HWND) {
        let Some(index) = claim(hwnd, Kind::TreeView) else {
            return;
        };
        let entry = &ENTRIES[index];
        entry.colors[0].store(
            SendMessageW(hwnd, TVM_GETBKCOLOR, 0, 0) as u32,
            Ordering::Release,
        );
        entry.colors[1].store(
            SendMessageW(hwnd, TVM_GETTEXTCOLOR, 0, 0) as u32,
            Ordering::Release,
        );
        SetWindowTheme(hwnd, wide("DarkMode_Explorer").as_ptr(), std::ptr::null());
        SendMessageW(hwnd, TVM_SETBKCOLOR, 0, color_edit_bg() as LPARAM);
        SendMessageW(hwnd, TVM_SETTEXTCOLOR, 0, color_text() as LPARAM);
    }

    // ---- restore ----------------------------------------------------------------------

    /// Undo everything this thread's windows had done to them. Safe to call repeatedly.
    unsafe fn restore_thread() {
        let thread = GetCurrentThreadId();
        for index in 0..MAX_ENTRIES {
            let entry = &ENTRIES[index];
            let key = entry.hwnd.load(Ordering::Acquire);
            if key == 0 || entry.thread.load(Ordering::Acquire) != thread {
                continue;
            }
            let hwnd = key as HWND;
            match kind_of(index) {
                Kind::Dialog => {
                    let ours = wndproc_bits(Some(dialog_proc));
                    let orig = entry.orig_proc.load(Ordering::Acquire);
                    if GetWindowLongPtrW(hwnd, GWLP_WNDPROC) == ours && orig != 0 {
                        SetWindowLongPtrW(hwnd, GWLP_WNDPROC, orig);
                        release(index);
                    } else {
                        // Someone subclassed after us, so unlinking would cut them off.
                        // Stay in place and only forward.
                        entry.passthrough.store(true, Ordering::Release);
                    }
                    InvalidateRect(hwnd, std::ptr::null(), 1);
                }
                Kind::Themed | Kind::Classic => {
                    SetWindowTheme(hwnd, std::ptr::null(), std::ptr::null());
                    release(index);
                }
                Kind::ListView => {
                    SendMessageW(
                        hwnd,
                        LVM_SETBKCOLOR,
                        0,
                        entry.colors[0].load(Ordering::Acquire) as LPARAM,
                    );
                    SendMessageW(
                        hwnd,
                        LVM_SETTEXTCOLOR,
                        0,
                        entry.colors[1].load(Ordering::Acquire) as LPARAM,
                    );
                    SendMessageW(
                        hwnd,
                        LVM_SETTEXTBKCOLOR,
                        0,
                        entry.colors[2].load(Ordering::Acquire) as LPARAM,
                    );
                    SetWindowTheme(hwnd, std::ptr::null(), std::ptr::null());
                    release(index);
                }
                Kind::TreeView => {
                    SendMessageW(
                        hwnd,
                        TVM_SETBKCOLOR,
                        0,
                        entry.colors[0].load(Ordering::Acquire) as LPARAM,
                    );
                    SendMessageW(
                        hwnd,
                        TVM_SETTEXTCOLOR,
                        0,
                        entry.colors[1].load(Ordering::Acquire) as LPARAM,
                    );
                    SetWindowTheme(hwnd, std::ptr::null(), std::ptr::null());
                    release(index);
                }
                Kind::Refused => release(index),
                Kind::Free => {}
            }
        }
    }

    // ---- shared state -----------------------------------------------------------------

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
        let view = MapViewOfFile(
            handle,
            FILE_MAP_READ,
            0,
            0,
            std::mem::size_of::<SharedState>(),
        );
        if view.Value.is_null() {
            return None;
        }
        SHARED.store(view.Value as usize, Ordering::Release);
        Some(&*(view.Value as *const SharedState))
    }

    unsafe fn hook_enabled() -> bool {
        let Some(state) = shared() else {
            return false;
        };
        std::ptr::read_volatile(&state.magic) == MAGIC
            && std::ptr::read_volatile(&state.enabled) != 0
    }

    unsafe fn flag(bit: u32) -> bool {
        shared().is_some_and(|state| std::ptr::read_volatile(&state.flags) & bit != 0)
    }

    unsafe fn color(pick: impl Fn(&SharedState) -> u32, fallback: u32) -> u32 {
        shared()
            .map(pick)
            .filter(|v| *v != CLR_NONE)
            .unwrap_or(fallback)
    }

    unsafe fn color_bg() -> u32 {
        color(|s| std::ptr::read_volatile(&s.bg), 0x001C_1C1C)
    }
    unsafe fn color_text() -> u32 {
        color(|s| std::ptr::read_volatile(&s.text), 0x00F0_F0F0)
    }
    unsafe fn color_link() -> u32 {
        color(|s| std::ptr::read_volatile(&s.link), 0x00FF_B56E)
    }
    unsafe fn color_heading() -> u32 {
        color(|s| std::ptr::read_volatile(&s.heading), 0x00FF_FFFF)
    }
    unsafe fn color_edit_bg() -> u32 {
        color(|s| std::ptr::read_volatile(&s.edit_bg), 0x002B_2B2B)
    }
    unsafe fn color_readonly_bg() -> u32 {
        color(|s| std::ptr::read_volatile(&s.readonly_bg), 0x0024_2424)
    }
    unsafe fn color_disabled() -> u32 {
        color(|s| std::ptr::read_volatile(&s.disabled_text), 0x008C_8C8C)
    }

    /// One brush per color, kept for the life of the process. A brush returned from
    /// `WM_CTLCOLOR*` can still be selected into a DC after the call, so it is never deleted.
    unsafe fn brush(color: u32) -> HBRUSH {
        let mut table = BRUSHES.lock().unwrap_or_else(|err| err.into_inner());
        for slot in table.iter() {
            if slot.1 != 0 && slot.0 == color {
                return slot.1 as HBRUSH;
            }
        }
        let created = CreateSolidBrush(color);
        if created.is_null() {
            return GetStockObject(0) as HBRUSH; // WHITE_BRUSH: never null
        }
        for slot in table.iter_mut() {
            if slot.1 == 0 {
                *slot = (color, created as isize);
                return created;
            }
        }
        // Table full (the palette changed many times): hand back a fresh, uncached brush.
        created
    }

    // ---- host ---------------------------------------------------------------------------

    unsafe fn host_kind_cached() -> HostKind {
        let current = HOST.load(Ordering::Acquire);
        if current != 0 {
            return match current {
                2 => HostKind::Explorer,
                3 => HostKind::Mmc,
                4 => HostKind::Denied,
                _ => HostKind::Other,
            };
        }
        let mut buf = [0u16; 260];
        let length = GetModuleFileNameW(std::ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32);
        let name = if length == 0 {
            String::new()
        } else {
            String::from_utf16_lossy(&buf[..length as usize])
        };
        let file = name.rsplit(['\\', '/']).next().unwrap_or("");
        let kind = host_kind(file);
        HOST.store(
            match kind {
                HostKind::Other => 1,
                HostKind::Explorer => 2,
                HostKind::Mmc => 3,
                HostKind::Denied => 4,
            },
            Ordering::Release,
        );
        kind
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

    unsafe fn class_name(hwnd: HWND) -> String {
        let mut buf = [0u16; 96];
        let length = GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if length <= 0 {
            String::new()
        } else {
            String::from_utf16_lossy(&buf[..length as usize])
        }
    }

    fn wndproc_bits(proc: WNDPROC) -> isize {
        unsafe { std::mem::transmute(proc) }
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn zero_rect() -> windows_sys::Win32::Foundation::RECT {
        windows_sys::Win32::Foundation::RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        }
    }
}

#[cfg(not(windows))]
#[no_mangle]
pub extern "C" fn allblack11_hook_stub() {}
