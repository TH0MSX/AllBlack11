//! AllBlack11 control harness.
//!
//! A single classic dialog holding every control family the legacy layer knows about, in
//! every state it claims to handle (enabled, disabled, checked, read-only, selected, default,
//! owner-drawn, created late). Run it next to AllBlack11 with "Dark legacy windows" on, or
//! use **Self-hook** to attach the hook to this process without the tray app.
//!
//! The cycle test switches styling off and on 50 times and compares GDI and USER object
//! counts, which is the cheapest honest leak check available without a profiler.
//!
//! Evidence note: this program is built by CI but only exercises anything when someone runs
//! it on Windows. A passing build says nothing about how the controls look.

#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(windows))]
fn main() {
    eprintln!("The AllBlack11 harness is a Windows program.");
    std::process::exit(1);
}

#[cfg(windows)]
#[path = "../../src/legacy_policy.rs"]
#[allow(dead_code)]
mod policy;

#[cfg(windows)]
fn main() {
    win::run();
}

#[cfg(windows)]
mod win {
    use std::ptr::{null, null_mut};

    use windows_sys::Win32::Foundation::{HMODULE, HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        CreateFontW, CreateSolidBrush, FillRect, HBRUSH, HFONT,
    };
    use windows_sys::Win32::System::LibraryLoader::{
        GetModuleFileNameW, GetModuleHandleW, GetProcAddress, LoadLibraryW,
    };
    use windows_sys::Win32::System::Memory::{
        CreateFileMappingW, MapViewOfFile, FILE_MAP_ALL_ACCESS, PAGE_READWRITE,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentThreadId, GetGuiResources, IsWow64Process2,
    };
    use windows_sys::Win32::UI::Controls::{
        InitCommonControlsEx, DRAWITEMSTRUCT, ICC_BAR_CLASSES, ICC_LINK_CLASS,
        ICC_LISTVIEW_CLASSES, ICC_PROGRESS_CLASS, ICC_TAB_CLASSES, ICC_TREEVIEW_CLASSES,
        ICC_UPDOWN_CLASS, INITCOMMONCONTROLSEX, LVCOLUMNW, LVITEMW, TCITEMW, TVINSERTSTRUCTW,
    };
    use windows_sys::Win32::UI::HiDpi::{
        GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateDialogIndirectParamW, CreateWindowExW, DefWindowProcW, DispatchMessageW,
        GetClientRect, GetDlgItem, GetMessageW, GetWindowTextW, IsDialogMessageW, PostQuitMessage,
        RegisterWindowMessageW, SendMessageW, SetWindowLongPtrW, SetWindowPos, SetWindowTextW,
        SetWindowsHookExW, ShowWindow, TranslateMessage, UnhookWindowsHookEx, GWLP_USERDATA, HHOOK,
        MSG, SWP_NOZORDER, SW_SHOW, WH_CALLWNDPROCRET, WM_CLOSE, WM_COMMAND, WM_DESTROY,
        WM_DRAWITEM, WM_INITDIALOG, WM_SETFONT, WM_SHOWWINDOW, WS_CHILD, WS_TABSTOP, WS_VISIBLE,
    };

    use crate::policy::{palette, SharedState, FLAG_CLASSIC_BUTTONS, FLAG_THEME_PUSH_BUTTONS};

    // Button / control styles written out so the harness has no hidden dependencies.
    const BS_PUSHBUTTON: u32 = 0x0;
    const BS_DEFPUSHBUTTON: u32 = 0x1;
    const BS_AUTOCHECKBOX: u32 = 0x3;
    const BS_AUTO3STATE: u32 = 0x6;
    const BS_GROUPBOX: u32 = 0x7;
    const BS_AUTORADIOBUTTON: u32 = 0x9;
    const BS_OWNERDRAW: u32 = 0xB;
    const ES_MULTILINE: u32 = 0x4;
    const ES_PASSWORD: u32 = 0x20;
    const ES_AUTOVSCROLL: u32 = 0x40;
    const ES_AUTOHSCROLL: u32 = 0x80;
    const ES_READONLY: u32 = 0x800;
    const WS_BORDER: u32 = 0x0080_0000;
    const WS_VSCROLL: u32 = 0x0020_0000;
    const WS_DISABLED: u32 = 0x0800_0000;
    const WS_EX_CLIENTEDGE: u32 = 0x200;
    const SS_NOTIFY: u32 = 0x100;
    const CBS_DROPDOWN: u32 = 0x2;
    const CBS_DROPDOWNLIST: u32 = 0x3;
    const LBS_NOTIFY: u32 = 0x1;
    const LVS_REPORT: u32 = 0x1;
    const LVS_SHOWSELALWAYS: u32 = 0x8;
    const TVS_HASLINES: u32 = 0x2;
    const TVS_HASBUTTONS: u32 = 0x1;
    const TVS_LINESATROOT: u32 = 0x4;
    const TVS_SHOWSELALWAYS: u32 = 0x20;
    const TBS_AUTOTICKS: u32 = 0x1;

    const BM_SETCHECK: u32 = 0xF1;
    const EM_SETREADONLY: u32 = 0xCF;
    const CB_ADDSTRING: u32 = 0x143;
    const CB_SETCURSEL: u32 = 0x14E;
    const LB_ADDSTRING: u32 = 0x180;
    const LB_SETCURSEL: u32 = 0x186;
    const LVM_INSERTITEMW: u32 = 0x104D;
    const LVM_INSERTCOLUMNW: u32 = 0x1061;
    const LVM_SETITEMTEXTW: u32 = 0x1074;
    const LVM_SETEXTENDEDLISTVIEWSTYLE: u32 = 0x1036;
    const LVS_EX_CHECKBOXES: usize = 0x4;
    const LVS_EX_FULLROWSELECT: usize = 0x20;
    const TVM_INSERTITEMW: u32 = 0x1132;
    const TCM_INSERTITEMW: u32 = 0x133E;
    const PBM_SETPOS: u32 = 0x402;
    const TBM_SETPOS: u32 = 0x405;

    const GR_GDIOBJECTS: u32 = 0;
    const GR_USEROBJECTS: u32 = 1;

    const ID_SELF_HOOK: u16 = 900;
    const ID_PALETTE: u16 = 901;
    const ID_DISABLE: u16 = 902;
    const ID_READONLY: u16 = 903;
    const ID_LATE: u16 = 904;
    const ID_CYCLE: u16 = 905;
    const ID_REPORT: u16 = 906;
    const ID_FIRST_STATEFUL: u16 = 1000;

    struct SelfHook {
        module: HMODULE,
        hook: HHOOK,
        shared: *mut SharedState,
    }

    struct State {
        dialog: HWND,
        font: HFONT,
        k: f32,
        self_hook: Option<SelfHook>,
        black: bool,
        controls_disabled: bool,
        readonly: bool,
        late_count: i32,
        owner_brush: HBRUSH,
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn run() {
        unsafe {
            SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let icc = INITCOMMONCONTROLSEX {
                dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_LISTVIEW_CLASSES
                    | ICC_TREEVIEW_CLASSES
                    | ICC_TAB_CLASSES
                    | ICC_BAR_CLASSES
                    | ICC_PROGRESS_CLASS
                    | ICC_LINK_CLASS
                    | ICC_UPDOWN_CLASS,
            };
            InitCommonControlsEx(&icc);
            // Rich edit lives in its own DLL.
            LoadLibraryW(wide("Msftedit.dll").as_ptr());

            let template = dialog_template(&title_text());
            let instance = GetModuleHandleW(null());
            let dialog = CreateDialogIndirectParamW(
                instance,
                template.as_ptr() as *const _,
                null_mut(),
                Some(dialog_proc),
                0,
            );
            if dialog.is_null() {
                return;
            }
            ShowWindow(dialog, SW_SHOW);
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                if IsDialogMessageW(dialog, &msg) == 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
    }

    fn title_text() -> String {
        let (mut process, mut native) = (0u16, 0u16);
        unsafe { IsWow64Process2(GetCurrentProcess(), &mut process, &mut native) };
        let me = if cfg!(target_arch = "aarch64") {
            "ARM64"
        } else {
            "x64"
        };
        let host = match native {
            0xAA64 => "ARM64 PC",
            0x8664 => "x64 PC",
            _ => "unknown PC",
        };
        let mode = if cfg!(feature = "classic") {
            "classic (no visual styles)"
        } else {
            "manifested"
        };
        format!("AllBlack11 Harness - {me} build on {host} - {mode}")
    }

    /// A minimal DLGTEMPLATE: no controls, the real ones are created in WM_INITDIALOG so
    /// "created late" and "created at init" go through the same code.
    fn dialog_template(title: &str) -> Vec<u8> {
        const WS_POPUP: u32 = 0x8000_0000;
        const WS_CAPTION: u32 = 0x00C0_0000;
        const WS_SYSMENU: u32 = 0x0008_0000;
        const WS_THICKFRAME: u32 = 0x0004_0000;
        const WS_MINIMIZEBOX: u32 = 0x0002_0000;
        const WS_MAXIMIZEBOX: u32 = 0x0001_0000;
        const DS_MODALFRAME: u32 = 0x80;
        let mut bytes = Vec::new();
        let style = WS_POPUP
            | WS_CAPTION
            | WS_SYSMENU
            | WS_THICKFRAME
            | WS_MINIMIZEBOX
            | WS_MAXIMIZEBOX
            | DS_MODALFRAME;
        bytes.extend_from_slice(&style.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes()); // cdit
        for value in [10i16, 10, 200, 150] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&0u16.to_le_bytes()); // menu
        bytes.extend_from_slice(&0u16.to_le_bytes()); // class
        for unit in title.encode_utf16().chain(std::iter::once(0)) {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        while bytes.len() % 4 != 0 {
            bytes.push(0);
        }
        bytes
    }

    unsafe extern "system" fn dialog_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> isize {
        match msg {
            WM_INITDIALOG => {
                let dpi = GetDpiForWindow(hwnd).max(96);
                let k = dpi as f32 / 96.0;
                let name = wide("Segoe UI");
                let font = CreateFontW(
                    -((15 * dpi as i32 + 48) / 96),
                    0,
                    0,
                    0,
                    400,
                    0,
                    0,
                    0,
                    1,
                    0,
                    0,
                    5,
                    0,
                    name.as_ptr(),
                );
                let state = Box::new(State {
                    dialog: hwnd,
                    font,
                    k,
                    self_hook: None,
                    black: false,
                    controls_disabled: false,
                    readonly: false,
                    late_count: 0,
                    owner_brush: CreateSolidBrush(0x0000_80FF),
                });
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
                let state = &mut *(windows_sys::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
                    hwnd,
                    GWLP_USERDATA,
                ) as *mut State);
                // Self-hook first when asked for, so every control is created under it.
                if std::env::args().any(|arg| arg == "--self-hook") {
                    start_self_hook(state);
                }
                build_controls(state);
                SetWindowPos(
                    hwnd,
                    null_mut(),
                    80,
                    60,
                    (1000.0 * k) as i32,
                    (700.0 * k) as i32,
                    SWP_NOZORDER,
                );
                refresh_counts(state);
                1
            }
            WM_COMMAND => {
                let state = state_of(hwnd);
                if state.is_null() {
                    return 0;
                }
                let state = &mut *state;
                match (wparam & 0xFFFF) as u16 {
                    ID_SELF_HOOK => toggle_self_hook(state),
                    ID_PALETTE => {
                        state.black = !state.black;
                        apply_palette(state);
                    }
                    ID_DISABLE => toggle_disabled(state),
                    ID_READONLY => toggle_readonly(state),
                    ID_LATE => add_late_controls(state),
                    ID_CYCLE => cycle_test(state),
                    _ => {}
                }
                refresh_counts(state);
                0
            }
            WM_DRAWITEM => {
                // The owner-drawn button: painted by this program, which the legacy layer
                // must leave alone. Bright orange so any interference is obvious.
                let state = state_of(hwnd);
                if !state.is_null() {
                    let draw = &*(lparam as *const DRAWITEMSTRUCT);
                    FillRect(draw.hDC, &draw.rcItem, (*state).owner_brush);
                }
                1
            }
            WM_CLOSE => {
                let state = state_of(hwnd);
                if !state.is_null() {
                    stop_self_hook(&mut *state);
                }
                windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd);
                1
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                1
            }
            _ => 0,
        }
    }

    unsafe fn state_of(hwnd: HWND) -> *mut State {
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA)
            as *mut State
    }

    // ---- control construction ---------------------------------------------------------

    unsafe fn make(
        state: &State,
        class: &str,
        text: &str,
        style: u32,
        ex: u32,
        (x, y, w, h): (i32, i32, i32, i32),
        id: u16,
    ) -> HWND {
        let k = state.k;
        let px = |v: i32| (v as f32 * k).round() as i32;
        let (class, text) = (wide(class), wide(text));
        let hwnd = CreateWindowExW(
            ex,
            class.as_ptr(),
            text.as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            px(x),
            px(y),
            px(w),
            px(h),
            state.dialog,
            id as usize as *mut _,
            GetModuleHandleW(null()),
            null(),
        );
        if !hwnd.is_null() {
            SendMessageW(hwnd, WM_SETFONT, state.font as WPARAM, 1);
        }
        hwnd
    }

    unsafe fn build_controls(state: &mut State) {
        let tab = WS_TABSTOP;
        let mut id = ID_FIRST_STATEFUL;
        let mut next = || {
            id += 1;
            id
        };
        // ---- control bar ----
        for (index, (label, ctl)) in [
            ("Self-hook: off", ID_SELF_HOOK),
            ("Palette: graphite", ID_PALETTE),
            ("Disable controls", ID_DISABLE),
            ("Read-only on", ID_READONLY),
            ("Add controls late", ID_LATE),
            ("Cycle x50", ID_CYCLE),
        ]
        .iter()
        .enumerate()
        {
            make(
                state,
                "BUTTON",
                label,
                tab | BS_PUSHBUTTON,
                0,
                (12 + index as i32 * 156, 10, 150, 28),
                *ctl,
            );
        }
        make(state, "STATIC", "", 0, 0, (12, 44, 960, 20), ID_REPORT);

        // ---- column 1: buttons ----
        make(
            state,
            "BUTTON",
            "Buttons",
            BS_GROUPBOX,
            0,
            (12, 72, 300, 292),
            next(),
        );
        let b = |dy: i32| (28, 94 + dy, 268, 24);
        make(
            state,
            "BUTTON",
            "Push button",
            tab | BS_PUSHBUTTON,
            0,
            b(0),
            next(),
        );
        make(
            state,
            "BUTTON",
            "Default push button",
            tab | BS_DEFPUSHBUTTON,
            0,
            b(30),
            next(),
        );
        make(
            state,
            "BUTTON",
            "Disabled push button",
            tab | WS_DISABLED | BS_PUSHBUTTON,
            0,
            b(60),
            next(),
        );
        let c1 = make(
            state,
            "BUTTON",
            "Checkbox, checked",
            tab | BS_AUTOCHECKBOX,
            0,
            b(90),
            next(),
        );
        SendMessageW(c1, BM_SETCHECK, 1, 0);
        make(
            state,
            "BUTTON",
            "Checkbox, unchecked",
            tab | BS_AUTOCHECKBOX,
            0,
            b(114),
            next(),
        );
        let c3 = make(
            state,
            "BUTTON",
            "Disabled checkbox, checked",
            WS_DISABLED | BS_AUTOCHECKBOX,
            0,
            b(138),
            next(),
        );
        SendMessageW(c3, BM_SETCHECK, 1, 0);
        make(
            state,
            "BUTTON",
            "Three-state checkbox",
            tab | BS_AUTO3STATE,
            0,
            b(162),
            next(),
        );
        let r1 = make(
            state,
            "BUTTON",
            "Radio, selected",
            tab | BS_AUTORADIOBUTTON,
            0,
            b(186),
            next(),
        );
        SendMessageW(r1, BM_SETCHECK, 1, 0);
        make(
            state,
            "BUTTON",
            "Radio, not selected",
            BS_AUTORADIOBUTTON,
            0,
            b(210),
            next(),
        );
        make(
            state,
            "BUTTON",
            "Disabled radio",
            WS_DISABLED | BS_AUTORADIOBUTTON,
            0,
            b(234),
            next(),
        );
        make(
            state,
            "BUTTON",
            "Owner-drawn: left alone (orange)",
            tab | BS_OWNERDRAW,
            0,
            b(262),
            next(),
        );

        // ---- column 2: text ----
        make(
            state,
            "BUTTON",
            "Text and fields",
            BS_GROUPBOX,
            0,
            (324, 72, 320, 420),
            next(),
        );
        let t = |dy: i32| (340, 94 + dy, 288, 24);
        make(state, "STATIC", "Static label", 0, 0, t(0), next());
        make(
            state,
            "STATIC",
            "Disabled static label",
            WS_DISABLED,
            0,
            t(26),
            next(),
        );
        make(
            state,
            "STATIC",
            "Clickable static (SS_NOTIFY)",
            SS_NOTIFY,
            0,
            t(52),
            next(),
        );
        make(
            state,
            "SysLink",
            "A <a href=\"https://example.com\">link</a> in text",
            tab,
            0,
            t(78),
            next(),
        );
        make(
            state,
            "EDIT",
            "Edit, editable",
            tab | WS_BORDER | ES_AUTOHSCROLL,
            0,
            t(108),
            next(),
        );
        make(
            state,
            "EDIT",
            "Edit, read-only",
            tab | WS_BORDER | ES_AUTOHSCROLL | ES_READONLY,
            0,
            t(138),
            next(),
        );
        make(
            state,
            "EDIT",
            "Edit, disabled",
            WS_DISABLED | WS_BORDER | ES_AUTOHSCROLL,
            0,
            t(168),
            next(),
        );
        make(
            state,
            "EDIT",
            "password",
            tab | WS_BORDER | ES_PASSWORD | ES_AUTOHSCROLL,
            0,
            t(198),
            next(),
        );
        make(
            state,
            "EDIT",
            "Multiline edit.\r\nSecond line.\r\nThird line.\r\nFourth line.\r\nFifth line.",
            tab | WS_BORDER | WS_VSCROLL | ES_MULTILINE | ES_AUTOVSCROLL,
            0,
            (340, 228, 288, 76),
            next(),
        );
        let combo = make(
            state,
            "COMBOBOX",
            "",
            tab | CBS_DROPDOWN | WS_VSCROLL,
            0,
            (340, 312, 288, 120),
            next(),
        );
        let droplist = make(
            state,
            "COMBOBOX",
            "",
            tab | CBS_DROPDOWNLIST | WS_VSCROLL,
            0,
            (340, 342, 288, 120),
            next(),
        );
        for item in ["First item", "Second item", "Third item"] {
            let w = wide(item);
            SendMessageW(combo, CB_ADDSTRING, 0, w.as_ptr() as LPARAM);
            SendMessageW(droplist, CB_ADDSTRING, 0, w.as_ptr() as LPARAM);
        }
        SendMessageW(combo, CB_SETCURSEL, 1, 0);
        SendMessageW(droplist, CB_SETCURSEL, 0, 0);
        let list = make(
            state,
            "LISTBOX",
            "",
            tab | WS_BORDER | WS_VSCROLL | LBS_NOTIFY,
            0,
            (340, 374, 288, 70),
            next(),
        );
        for item in [
            "Listbox item one",
            "Listbox item two (selected)",
            "Listbox item three",
            "Item four",
            "Item five",
        ] {
            SendMessageW(list, LB_ADDSTRING, 0, wide(item).as_ptr() as LPARAM);
        }
        SendMessageW(list, LB_SETCURSEL, 1, 0);
        make(
            state,
            "RICHEDIT50W",
            "Rich edit: left as it is (partial support)",
            tab | WS_BORDER | ES_MULTILINE,
            0,
            (340, 450, 288, 34),
            next(),
        );

        // ---- column 3: common controls ----
        make(
            state,
            "BUTTON",
            "Lists, trees and bars",
            BS_GROUPBOX,
            0,
            (656, 72, 330, 600),
            next(),
        );
        let lv = make(
            state,
            "SysListView32",
            "",
            tab | WS_BORDER | LVS_REPORT | LVS_SHOWSELALWAYS,
            WS_EX_CLIENTEDGE,
            (672, 94, 298, 130),
            next(),
        );
        SendMessageW(
            lv,
            LVM_SETEXTENDEDLISTVIEWSTYLE,
            0,
            (LVS_EX_CHECKBOXES | LVS_EX_FULLROWSELECT) as LPARAM,
        );
        for (index, title) in ["Name", "Kind"].iter().enumerate() {
            let mut text = wide(title);
            let mut column: LVCOLUMNW = std::mem::zeroed();
            column.mask = 0x1 | 0x2 | 0x4; // LVCF_FMT | LVCF_WIDTH | LVCF_TEXT
            column.cx = (140.0 * state.k) as i32;
            column.pszText = text.as_mut_ptr();
            SendMessageW(lv, LVM_INSERTCOLUMNW, index, &column as *const _ as LPARAM);
        }
        for (row, (name, kind)) in [
            ("Alpha", "first"),
            ("Bravo (selected)", "second"),
            ("Charlie", "third"),
        ]
        .iter()
        .enumerate()
        {
            let mut text = wide(name);
            let mut item: LVITEMW = std::mem::zeroed();
            item.mask = 0x1; // LVIF_TEXT
            item.iItem = row as i32;
            item.pszText = text.as_mut_ptr();
            SendMessageW(lv, LVM_INSERTITEMW, 0, &item as *const _ as LPARAM);
            let mut sub = wide(kind);
            let mut subitem: LVITEMW = std::mem::zeroed();
            subitem.iSubItem = 1;
            subitem.pszText = sub.as_mut_ptr();
            SendMessageW(lv, LVM_SETITEMTEXTW, row, &subitem as *const _ as LPARAM);
        }
        let tv = make(
            state,
            "SysTreeView32",
            "",
            tab | WS_BORDER | TVS_HASLINES | TVS_HASBUTTONS | TVS_LINESATROOT | TVS_SHOWSELALWAYS,
            WS_EX_CLIENTEDGE,
            (672, 232, 298, 120),
            next(),
        );
        let mut root_text = wide("Tree root");
        let mut insert: TVINSERTSTRUCTW = std::mem::zeroed();
        insert.hInsertAfter = 0xFFFF_0001usize as _; // TVI_LAST
        insert.Anonymous.item.mask = 0x1; // TVIF_TEXT
        insert.Anonymous.item.pszText = root_text.as_mut_ptr();
        let root = SendMessageW(tv, TVM_INSERTITEMW, 0, &insert as *const _ as LPARAM);
        for name in ["Child one", "Child two"] {
            let mut text = wide(name);
            let mut child: TVINSERTSTRUCTW = std::mem::zeroed();
            child.hParent = root as _;
            child.hInsertAfter = 0xFFFF_0001usize as _;
            child.Anonymous.item.mask = 0x1;
            child.Anonymous.item.pszText = text.as_mut_ptr();
            SendMessageW(tv, TVM_INSERTITEMW, 0, &child as *const _ as LPARAM);
        }
        let tabs = make(
            state,
            "SysTabControl32",
            "",
            tab,
            0,
            (672, 362, 298, 70),
            next(),
        );
        for (index, title) in ["General", "Advanced"].iter().enumerate() {
            let mut text = wide(title);
            let mut item: TCITEMW = std::mem::zeroed();
            item.mask = 0x1; // TCIF_TEXT
            item.pszText = text.as_mut_ptr();
            SendMessageW(tabs, TCM_INSERTITEMW, index, &item as *const _ as LPARAM);
        }
        let progress = make(
            state,
            "msctls_progress32",
            "",
            0,
            0,
            (672, 444, 298, 18),
            next(),
        );
        SendMessageW(progress, PBM_SETPOS, 60, 0);
        let slider = make(
            state,
            "msctls_trackbar32",
            "",
            tab | TBS_AUTOTICKS,
            0,
            (672, 470, 298, 34),
            next(),
        );
        SendMessageW(slider, TBM_SETPOS, 1, 40);
        make(state, "STATIC", "Tab strip, rich edit and the progress bar keep their light look by design:\r\nthe capability table lists them as partial or untouched.", 0, 0, (672, 516, 298, 120), next());
    }

    // ---- state toggles ----------------------------------------------------------------

    unsafe fn each_stateful(state: &State, mut body: impl FnMut(HWND)) {
        for id in (ID_FIRST_STATEFUL + 1)..(ID_FIRST_STATEFUL + 80) {
            let hwnd = GetDlgItem(state.dialog, id as i32);
            if !hwnd.is_null() {
                body(hwnd);
            }
        }
    }

    unsafe fn toggle_disabled(state: &mut State) {
        state.controls_disabled = !state.controls_disabled;
        let enable = i32::from(!state.controls_disabled);
        // Only controls that start enabled; the ones that start disabled stay that way.
        // Offsets follow creation order in build_controls: push buttons, checkboxes, a radio,
        // the clickable static, the link, then edit, password, multiline, both combos, listbox.
        for offset in [2u16, 3, 5, 6, 8, 9, 10, 14, 16, 17, 18, 21, 22, 23, 24, 25] {
            let id = ID_FIRST_STATEFUL + offset;
            let hwnd = GetDlgItem(state.dialog, id as i32);
            if !hwnd.is_null() {
                EnableWindow(hwnd, enable);
            }
        }
        set_text(
            GetDlgItem(state.dialog, ID_DISABLE as i32),
            if state.controls_disabled {
                "Enable controls"
            } else {
                "Disable controls"
            },
        );
    }

    unsafe fn toggle_readonly(state: &mut State) {
        state.readonly = !state.readonly;
        let on = usize::from(state.readonly);
        each_stateful(state, |hwnd| {
            let mut class = [0u16; 16];
            windows_sys::Win32::UI::WindowsAndMessaging::GetClassNameW(
                hwnd,
                class.as_mut_ptr(),
                16,
            );
            if String::from_utf16_lossy(&class)
                .trim_end_matches('\0')
                .eq_ignore_ascii_case("Edit")
            {
                SendMessageW(hwnd, EM_SETREADONLY, on, 0);
            }
        });
        set_text(
            GetDlgItem(state.dialog, ID_READONLY as i32),
            if state.readonly {
                "Read-only off"
            } else {
                "Read-only on"
            },
        );
    }

    unsafe fn add_late_controls(state: &mut State) {
        state.late_count += 1;
        let y = 500 + (state.late_count - 1) % 4 * 30;
        let n = state.late_count;
        make(
            state,
            "BUTTON",
            &format!("Late checkbox #{n}"),
            WS_TABSTOP | BS_AUTOCHECKBOX,
            0,
            (28, y.min(640), 200, 24),
            2000 + n as u16,
        );
        make(
            state,
            "EDIT",
            &format!("Late edit #{n}"),
            WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL,
            0,
            (236, y.min(640), 76, 24),
            2100 + n as u16,
        );
    }

    unsafe fn apply_palette(state: &mut State) {
        set_text(
            GetDlgItem(state.dialog, ID_PALETTE as i32),
            if state.black {
                "Palette: black"
            } else {
                "Palette: graphite"
            },
        );
        if let Some(hook) = &state.self_hook {
            let chosen = palette(if state.black { "black" } else { "graphite" });
            let mut next =
                SharedState::from_palette(chosen, FLAG_CLASSIC_BUTTONS | FLAG_THEME_PUSH_BUTTONS);
            next.enabled = 1;
            std::ptr::write_volatile(hook.shared, next);
            windows_sys::Win32::Graphics::Gdi::InvalidateRect(state.dialog, null(), 1);
            // Repaint children too.
            each_stateful(state, |hwnd| {
                windows_sys::Win32::Graphics::Gdi::InvalidateRect(hwnd, null(), 1);
            });
        }
    }

    // ---- self-hook --------------------------------------------------------------------

    unsafe fn start_self_hook(state: &mut State) -> bool {
        if state.self_hook.is_some() {
            return true;
        }
        let mut buf = [0u16; 520];
        let len = GetModuleFileNameW(null_mut(), buf.as_mut_ptr(), buf.len() as u32) as usize;
        let exe = String::from_utf16_lossy(&buf[..len]);
        let dir = std::path::Path::new(&exe)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
        let dll = dir.join("AllBlack11Hook.dll");
        let module = LoadLibraryW(wide(&dll.display().to_string()).as_ptr());
        if module.is_null() {
            set_report(
                state,
                "Self-hook: AllBlack11Hook.dll was not found next to the harness.",
            );
            return false;
        }
        let Some(proc) = GetProcAddress(module, b"AllBlack11HookProcRet\0".as_ptr()) else {
            set_report(
                state,
                "Self-hook: the DLL has no AllBlack11HookProcRet export.",
            );
            return false;
        };
        let mapping = CreateFileMappingW(
            windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE,
            null(),
            PAGE_READWRITE,
            0,
            std::mem::size_of::<SharedState>() as u32,
            wide("Local\\AllBlack11.Hook").as_ptr(),
        );
        if mapping.is_null() {
            set_report(state, "Self-hook: could not create the shared state.");
            return false;
        }
        let view = MapViewOfFile(
            mapping,
            FILE_MAP_ALL_ACCESS,
            0,
            0,
            std::mem::size_of::<SharedState>(),
        );
        if view.Value.is_null() {
            set_report(state, "Self-hook: could not map the shared state.");
            return false;
        }
        let shared = view.Value as *mut SharedState;
        let mut value = SharedState::from_palette(
            palette(if state.black { "black" } else { "graphite" }),
            FLAG_CLASSIC_BUTTONS | FLAG_THEME_PUSH_BUTTONS,
        );
        value.enabled = 1;
        std::ptr::write_volatile(shared, value);
        let hook = SetWindowsHookExW(
            WH_CALLWNDPROCRET,
            Some(std::mem::transmute::<
                unsafe extern "system" fn() -> isize,
                unsafe extern "system" fn(i32, WPARAM, LPARAM) -> LRESULT,
            >(proc)),
            module,
            GetCurrentThreadId(),
        );
        if hook.is_null() {
            set_report(state, "Self-hook: SetWindowsHookEx failed.");
            return false;
        }
        state.self_hook = Some(SelfHook {
            module,
            hook,
            shared,
        });
        // Treat the dialog now: the hook only sees messages from here on.
        SendMessageW(state.dialog, WM_SHOWWINDOW, 1, 0);
        true
    }

    unsafe fn stop_self_hook(state: &mut State) {
        let Some(hook) = state.self_hook.take() else {
            return;
        };
        std::ptr::write_volatile(&mut (*hook.shared).enabled, 0);
        let message = RegisterWindowMessageW(wide("AllBlack11.HookStop").as_ptr());
        // Deliver the stop message to this thread's windows so the hook restores them, then
        // unhook. The DLL pinned itself, so it stays mapped: "styling off", not "unloaded".
        SendMessageW(state.dialog, message, 0, 0);
        UnhookWindowsHookEx(hook.hook);
        let _ = hook.module;
    }

    unsafe fn toggle_self_hook(state: &mut State) {
        if state.self_hook.is_some() {
            stop_self_hook(state);
            set_text(
                GetDlgItem(state.dialog, ID_SELF_HOOK as i32),
                "Self-hook: off",
            );
            windows_sys::Win32::Graphics::Gdi::InvalidateRect(state.dialog, null(), 1);
        } else if start_self_hook(state) {
            set_text(
                GetDlgItem(state.dialog, ID_SELF_HOOK as i32),
                "Self-hook: on",
            );
        }
    }

    // ---- cycle test and counts --------------------------------------------------------

    unsafe fn gui_counts() -> (u32, u32) {
        let process = GetCurrentProcess();
        (
            GetGuiResources(process, GR_GDIOBJECTS),
            GetGuiResources(process, GR_USEROBJECTS),
        )
    }

    unsafe fn cycle_test(state: &mut State) {
        let was_on = state.self_hook.is_some();
        if !was_on && !start_self_hook(state) {
            return;
        }
        // Warm up once so one-time allocations (brushes, themes) are not counted as leaks.
        stop_self_hook(state);
        start_self_hook(state);
        let (gdi_before, user_before) = gui_counts();
        for _ in 0..50 {
            stop_self_hook(state);
            if !start_self_hook(state) {
                set_report(state, "Cycle test aborted: could not restart the hook.");
                return;
            }
        }
        let (gdi_after, user_after) = gui_counts();
        let gdi_delta = gdi_after as i64 - gdi_before as i64;
        let user_delta = user_after as i64 - user_before as i64;
        let pass = gdi_delta <= 4 && user_delta <= 4;
        set_report(
            state,
            &format!(
                "Cycle x50: {}  GDI {gdi_before} -> {gdi_after} ({gdi_delta:+})  USER {user_before} -> {user_after} ({user_delta:+})  (pass: each delta <= 4)",
                if pass { "PASS" } else { "FAIL" }
            ),
        );
        if !was_on {
            stop_self_hook(state);
        }
    }

    unsafe fn refresh_counts(state: &State) {
        let (gdi, user) = gui_counts();
        let mut current = [0u16; 400];
        let len = GetWindowTextW(
            GetDlgItem(state.dialog, ID_REPORT as i32),
            current.as_mut_ptr(),
            400,
        );
        let existing = String::from_utf16_lossy(&current[..len.max(0) as usize]);
        if existing.is_empty() || existing.starts_with("GDI objects") {
            set_text(
                GetDlgItem(state.dialog, ID_REPORT as i32),
                &format!("GDI objects: {gdi}   USER objects: {user}   (press Cycle x50 for the leak check)"),
            );
        }
    }

    unsafe fn set_report(state: &State, text: &str) {
        set_text(GetDlgItem(state.dialog, ID_REPORT as i32), text);
    }

    unsafe fn set_text(hwnd: HWND, text: &str) {
        if !hwnd.is_null() {
            SetWindowTextW(hwnd, wide(text).as_ptr());
        }
    }

    #[allow(dead_code)]
    unsafe fn _unused(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        let mut rect: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut rect);
        DefWindowProcW(hwnd, msg, w, l)
    }
}
