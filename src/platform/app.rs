use std::collections::{HashMap, HashSet};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Mutex, OnceLock};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, BOOL, ERROR_ALREADY_EXISTS, HANDLE, HWND, LPARAM, LRESULT, RECT, TRUE,
    WAIT_OBJECT_0, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::SystemInformation::GetTickCount64;
use windows_sys::Win32::System::Threading::{
    CreateEventW, CreateMutexW, GetCurrentProcessId, GetProcessTimes, OpenEventW, OpenProcess,
    QueryFullProcessImageNameW, SetEvent, EVENT_MODIFY_STATE, INFINITE, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows_sys::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_ERROR, NIIF_INFO, NIM_ADD,
    NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyWindow, DispatchMessageW, EnumWindows,
    FindWindowW, GetAncestor, GetClassNameW, GetWindow, GetWindowLongPtrW, GetWindowRect,
    GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, KillTimer,
    MsgWaitForMultipleObjectsEx, PeekMessageW, PostMessageW, PostQuitMessage, RegisterClassExW,
    RegisterWindowMessageW, SetTimer, TranslateMessage, EVENT_OBJECT_SHOW, EVENT_OBJECT_UNCLOAKED,
    EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MOVESIZEEND, EVENT_SYSTEM_MOVESIZESTART, GA_ROOT,
    GWL_EXSTYLE, GWL_STYLE, GW_OWNER, MSG, MWMO_INPUTAVAILABLE, PM_REMOVE, QS_ALLINPUT,
    WINEVENT_INCONTEXT, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS, WM_APP, WM_CLOSE, WM_CONTEXTMENU, WM_DESTROY, WM_DRAWITEM, WM_ENDSESSION,
    WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_MEASUREITEM, WM_QUERYENDSESSION, WM_QUIT, WM_RBUTTONUP,
    WM_SETTINGCHANGE, WM_TIMER,
    MessageBoxW, IDYES, MB_DEFBUTTON2, MB_ICONQUESTION, MB_SETFOREGROUND, MB_TOPMOST, MB_YESNO,
    WNDCLASSEXW, WS_CHILD, WS_DLGFRAME, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP,
    WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
};

use windows_sys::Win32::System::Registry::{RegCloseKey, HKEY};

use crate::config::{
    acrylic_gradient, bundled_default_json, file_name, hex_rgb, normalize_caption_height, parse_config,
    set_enabled_flag, set_shell_flag, summarize, Backdrop, Style,
};
use crate::policy::{decide, rect_covers_monitor, Decision, WindowFacts};
use crate::snapshot::{format_snapshots, parse_snapshots, same_process, StoredSnapshot};
use crate::{
    WS_CHILD as POLICY_WS_CHILD, WS_DLGFRAME as POLICY_WS_DLGFRAME,
    WS_EX_LAYERED as POLICY_WS_EX_LAYERED, WS_EX_NOREDIRECTIONBITMAP as POLICY_WS_EX_NOREDIR,
    WS_EX_TOOLWINDOW as POLICY_WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT as POLICY_WS_EX_TRANSPARENT,
};

use super::composition;
use super::dwm;
use super::icon;
use super::legacy;
use super::menu::{self, Item, ItemKind};
use super::metrics;
use super::registry;
use super::settings;
use super::shellwin;
use super::sysinfo;
use super::cfgio;
use super::uxtheme;
use super::wide::{
    self, config_path, fill_wide, log_line, migrate_legacy_config, pcw, snapshot_path, to_wide,
};

const _: () = {
    assert!(WS_CHILD == POLICY_WS_CHILD);
    assert!(WS_DLGFRAME == POLICY_WS_DLGFRAME);
    assert!(WS_EX_TOOLWINDOW == POLICY_WS_EX_TOOLWINDOW);
    assert!(WS_EX_LAYERED == POLICY_WS_EX_LAYERED);
    assert!(WS_EX_TRANSPARENT == POLICY_WS_EX_TRANSPARENT);
    assert!(WS_EX_NOREDIRECTIONBITMAP == POLICY_WS_EX_NOREDIR);
};

const WM_TRAY: u32 = WM_APP + 1;
const WM_APPLY: u32 = WM_APP + 2;
const WM_ALREADY: u32 = WM_APP + 3;
const TIMER_SECOND: usize = 1;
const HOOK_FLAGS: u32 = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
const CMD_ENABLE: u32 = 1;
const CMD_RELOAD: u32 = 2;
const CMD_OPEN: u32 = 3;
const CMD_RESET: u32 = 4;
const CMD_AUTOSTART: u32 = 5;
const CMD_EXIT: u32 = 6;
const CMD_OLED: u32 = 7;
const CMD_BLACK_SHELL: u32 = 8;
const CMD_SETTINGS: u32 = 9;
const CMD_RESTART_EXPLORER: u32 = 10;
const CMD_OPEN_FOLDER: u32 = 11;
const CMD_DIAGNOSTICS: u32 = 12;
const CMD_PRESET_BASE: u32 = 20;
const WM_EXPLORER_DONE: u32 = WM_APP + 7;
const TIMER_ACCENT: usize = 3;
const TIMER_CLICK: usize = 4;

static SUPPRESS_MENU: AtomicBool = AtomicBool::new(false);

static APP_HWND: AtomicIsize = AtomicIsize::new(0);
static QUEUE: Mutex<Vec<(i64, u32)>> = Mutex::new(Vec::new());
static STATE: Mutex<Option<App>> = Mutex::new(None);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Startup,
    Show,
    Foreground,
    MoveSize,
    MoveStart,
    Second,
}

struct Due {
    hwnd: i64,
    at: u64,
}

struct PendingApply {
    hwnd: i64,
    pid: u32,
    exe: String,
    style: Style,
    move_blur: bool,
}

// Win32 handles are raw pointers. Every method runs on the UI thread that created them.
unsafe impl Send for App {}

struct App {
    hwnd: HWND,
    hooks: Vec<HWINEVENTHOOK>,
    icon_on: windows_sys::Win32::UI::WindowsAndMessaging::HICON,
    icon_off: windows_sys::Win32::UI::WindowsAndMessaging::HICON,
    config: crate::Config,
    config_file: std::path::PathBuf,
    accent_bgr: u32,
    apps_dark: Option<bool>,
    high_contrast: bool,
    build: u32,
    originals: HashMap<i64, StoredSnapshot>,
    hold: HashSet<i64>,
    exe_cache: HashMap<i64, (u32, u64, String)>,
    last_apply: HashMap<i64, u64>,
    due: Vec<Due>,
    seen_failures: HashSet<u32>,
    dirty: bool,
    shut_down: bool,
    taskbar_msg: u32,
    exit_event: HANDLE,
    reg_event: HANDLE,
    accent_key: HKEY,
    instance_mutex: HANDLE,
    logged_startup: bool,
    swca_failed: HashSet<u32>,
}

/// `--quiet` keeps the recovery commands from popping a message box when they have no
/// console, which is how Setup runs them.
static QUIET: AtomicBool = AtomicBool::new(false);

fn say(text: &str) {
    if !QUIET.load(Ordering::Relaxed) {
        wide::emit(text);
    }
}

pub fn start() -> i32 {
    std::panic::set_hook(Box::new(|info| {
        log_line("panic", &info.to_string());
    }));
    let args: Vec<String> = std::env::args().skip(1).collect();
    QUIET.store(args.iter().any(|arg| arg == "--quiet"), Ordering::Relaxed);
    if args
        .iter()
        .any(|arg| arg == "--help" || arg == "-h" || arg == "/?")
    {
        wide::emit(&help_text());
        return 0;
    }
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        wide::emit(&format!("AllBlack11 {}\n", env!("CARGO_PKG_VERSION")));
        return 0;
    }
    if args.iter().any(|arg| arg == "--check") {
        return cmd_check();
    }
    if args.iter().any(|arg| arg == "--exit") {
        return cmd_exit();
    }
    if args.iter().any(|arg| arg == "--restart-explorer") {
        return cmd_restart_explorer();
    }
    if args.iter().any(|arg| arg == "--restore") {
        return cmd_restore();
    }
    if let Some(unknown) = args.iter().find(|arg| arg.as_str() != "--quiet") {
        wide::emit(&format!("Unknown argument: {unknown}\n\n{}", help_text()));
        return 2;
    }
    match run() {
        Ok(()) => 0,
        Err(err) => {
            log_line("error", &err);
            wide::message_box("AllBlack11", &err, true);
            1
        }
    }
}

fn help_text() -> String {
    format!(
        "AllBlack11 {version}\n\
         Apply a dark OLED-glass frame to Windows 11 app windows.\n\
         \n\
         AllBlack11.exe           Start the tray app\n\
         AllBlack11.exe --exit    Restore themed windows and quit\n\
         Settings                 Tray menu, or double-click the icon\n\
         AllBlack11.exe --check   Validate config.json and print a diagnostics report\n\
                                  (also writes %APPDATA%\\AllBlack11\\check.txt)\n\
         AllBlack11.exe --restart-explorer  Restart the taskbar, Start and desktop\n\
         AllBlack11.exe --quiet   With --restore or --restart-explorer: no message box\n\
         AllBlack11.exe --restore Quit, undo the shell tweaks and CaptionHeight, keep the config\n\
         AllBlack11.exe --version Print the version\n\
         AllBlack11.exe --help    Show this help\n\
         \n\
         Config: %APPDATA%\\AllBlack11\\config.json\n",
        version = env!("CARGO_PKG_VERSION")
    )
}

fn cmd_check() -> i32 {
    let path = config_path();
    let config = if path.exists() {
        match std::fs::read_to_string(&path).and_then(|text| {
            parse_config(&text)
                .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))
        }) {
            Ok(config) => config,
            Err(err) => {
                return publish_check(&format!("Config error in {}:\n{err}\n", path.display()), 1);
            }
        }
    } else {
        parse_config(bundled_default_json()).expect("bundled config")
    };
    let accent = registry::accent_bgr();
    let build = dwm::windows_build();
    let report = format!(
        "AllBlack11 {version}\n\
         Config: {path}{missing}\n\
         {summary}\n\
         AccentColorMenu: 0x{accent:08X} ({rgb})\n\
         Windows build: {build}\n\
         High contrast: {hc}\n\
         Architecture: {arch}\n\
         Backdrop API (38): {backdrop}\n\
         \n\
         Cross-process DWM attributes this build will set:\n\
           20 DWMWA_USE_IMMERSIVE_DARK_MODE\n\
           33 DWMWA_WINDOW_CORNER_PREFERENCE\n\
           34 DWMWA_BORDER_COLOR\n\
           35 DWMWA_CAPTION_COLOR\n\
           36 DWMWA_TEXT_COLOR\n\
           38 DWMWA_SYSTEMBACKDROP_TYPE{backdrop_note}\n",
        version = env!("CARGO_PKG_VERSION"),
        path = path.display(),
        missing = if path.exists() {
            ""
        } else {
            " (not created yet)"
        },
        summary = summarize(&config, accent),
        rgb = hex_rgb(accent),
        hc = dwm::high_contrast_enabled(),
        arch = sysinfo::arch_line(),
        backdrop = if build >= 22621 {
            "yes"
        } else {
            "no, build < 22621"
        },
        backdrop_note = if build >= 22621 {
            ""
        } else {
            " (skipped on this build)"
        },
    );
    let mut report = report;
    if build != 0 && build < 22000 {
        report.push_str("This PC is older than Windows 11. The tray app will refuse to start.\n");
    }
    if let Some(advice) = sysinfo::arch_status().advice() {
        report.push_str(advice);
        report.push('\n');
    }
    publish_check(&report, 0)
}

fn publish_check(report: &str, code: i32) -> i32 {
    let dir = wide::config_dir();
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join("check.txt");
    let file_note = if std::fs::write(&file, report).is_ok() {
        format!("\nReport file: {}\n", file.display())
    } else {
        String::new()
    };
    wide::emit(&format!("{report}{file_note}"));
    code
}

/// Write the same report as `--check` and open it in the default editor.
fn cmd_check_to_file() {
    let path = wide::config_dir().join("check.txt");
    let config = cfgio::load();
    let accent = registry::accent_bgr();
    let build = dwm::windows_build();
    let report = format!(
        "AllBlack11 {version}\nArchitecture: {arch}\nWindows build: {build}\nHigh contrast: {hc}\n{summary}\nAccentColorMenu: 0x{accent:08X}\nConfig: {cfg}\nData folder: {dir}\n",
        version = env!("CARGO_PKG_VERSION"),
        arch = sysinfo::arch_line(),
        hc = dwm::high_contrast_enabled(),
        summary = summarize(&config, accent),
        cfg = config_path().display(),
        dir = wide::config_dir().display(),
    );
    if wide::write_atomic(&path, &report).is_ok() {
        let _ = registry::open_file(&path);
    }
}

fn cmd_exit() -> i32 {
    let name = to_wide("Local\\AllBlack11.Exit");
    let handle = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, pcw(&name)) };
    if handle.is_null() {
        return 0;
    }
    unsafe {
        SetEvent(handle);
        CloseHandle(handle);
    }
    0
}

fn cmd_restart_explorer() -> i32 {
    match sysinfo::restart_explorer_blocking() {
        Ok(text) => {
            say(&format!("{text}\n"));
            0
        }
        Err(err) => {
            say(&format!("{err}\n"));
            1
        }
    }
}

/// Recovery path that works without the tray: quit, put the shell registry values and
/// CaptionHeight back from their backups, and switch the shell flags off in the config.
fn cmd_restore() -> i32 {
    cmd_exit();
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let mut backup = shellwin::load_backup();
    let mut notes = Vec::new();
    match shellwin::disable_oled(&mut backup) {
        Ok(()) => notes.push("OLED taskbar: restored".to_string()),
        Err(err) => notes.push(format!("OLED taskbar: {err}")),
    }
    match shellwin::disable_black_shell(&mut backup) {
        Ok(()) => notes.push("Black shell: restored".to_string()),
        Err(err) => notes.push(format!("Black shell: {err}")),
    }
    match metrics::restore_caption_height() {
        Ok(()) => notes.push("CaptionHeight: restored (sign out to see it)".to_string()),
        Err(err) => notes.push(format!("CaptionHeight: {err}")),
    }
    let path = config_path();
    if let Ok(text) = std::fs::read_to_string(&path) {
        let off = set_shell_flag(&text, "oledTaskbar", false)
            .and_then(|text| set_shell_flag(&text, "blackShell", false));
        if let Ok(updated) = off {
            let _ = wide::write_atomic(&path, &format!("{}\n", updated.trim_end()));
        }
    }
    shellwin::broadcast_immersive_color();
    say(&format!("{}\n", notes.join("\n")));
    0
}

fn run() -> Result<(), String> {
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let build = dwm::windows_build();
    if !dwm::supports_theme(build) {
        return Err(format!(
            "AllBlack11 needs Windows 11 (build 22000 or newer). This PC reports build {build}."
        ));
    }

    let mutex_name = to_wide("Local\\AllBlack11");
    let instance_mutex = unsafe { CreateMutexW(null(), TRUE, pcw(&mutex_name)) };
    if instance_mutex.is_null() {
        return Err("Could not create the single-instance mutex.".into());
    }
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        let class = class_name();
        let existing = unsafe { FindWindowW(pcw(class), null()) };
        if !existing.is_null() {
            unsafe { PostMessageW(existing, WM_ALREADY, 0, 0) };
        }
        unsafe { CloseHandle(instance_mutex) };
        return Ok(());
    }

    let exit_name = to_wide("Local\\AllBlack11.Exit");
    let exit_event = unsafe { CreateEventW(null(), 0, 0, pcw(&exit_name)) };
    if exit_event.is_null() {
        unsafe { CloseHandle(instance_mutex) };
        return Err("Could not create the exit event.".into());
    }

    let instance = unsafe { GetModuleHandleW(null()) };
    let class = class_name();
    let mut window_class: WNDCLASSEXW = unsafe { std::mem::zeroed() };
    window_class.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
    window_class.lpfnWndProc = Some(wndproc);
    window_class.hInstance = instance;
    window_class.lpszClassName = pcw(class);
    unsafe { RegisterClassExW(&window_class) };
    let title = to_wide("AllBlack11");
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            pcw(class),
            pcw(&title),
            WS_POPUP,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            null(),
        )
    };
    if hwnd.is_null() {
        unsafe {
            CloseHandle(exit_event);
            CloseHandle(instance_mutex);
        }
        return Err("Could not create the message window.".into());
    }

    let (config, config_warning) = load_config();
    let tray_icon = icon::load_tray_icon(instance);
    let reg_event = shellwin::create_notify_event();
    let mut app = App {
        hwnd,
        hooks: Vec::new(),
        icon_on: tray_icon,
        icon_off: tray_icon,
        accent_bgr: registry::accent_bgr(),
        apps_dark: registry::apps_prefer_dark(),
        high_contrast: dwm::high_contrast_enabled(),
        build,
        originals: load_snapshots(),
        hold: HashSet::new(),
        exe_cache: HashMap::new(),
        last_apply: HashMap::new(),
        due: Vec::new(),
        seen_failures: HashSet::new(),
        dirty: false,
        shut_down: false,
        taskbar_msg: unsafe { RegisterWindowMessageW(pcw(&to_wide("TaskbarCreated"))) },
        exit_event,
        reg_event,
        accent_key: std::ptr::null_mut(),
        instance_mutex,
        logged_startup: false,
        swca_failed: HashSet::new(),
        config_file: config_path(),
        config,
    };
    app.prune_dead();
    APP_HWND.store(hwnd as isize, Ordering::Relaxed);
    uxtheme::allow_dark_menus(!app.high_contrast);
    uxtheme::allow_window(hwnd, true);
    if !app.add_tray() {
        unsafe {
            if !app.icon_on.is_null() {
                DestroyIcon(app.icon_on);
            }
            DestroyWindow(hwnd);
            CloseHandle(exit_event);
            CloseHandle(instance_mutex);
        }
        return Err("Could not add the tray icon. Is Explorer running?".into());
    }
    app.install_hooks();
    if let Some(warning) = config_warning {
        app.balloon("AllBlack11", &warning, true);
    }
    if app.high_contrast {
        app.restore_visuals();
        app.balloon("AllBlack11", "Paused while high contrast is on.", false);
    }
    if app.config.logging {
        log_line(
            "info",
            &format!(
                "AllBlack11 {} started pid={} build={build} accent={}",
                env!("CARGO_PKG_VERSION"),
                unsafe { GetCurrentProcessId() },
                hex_rgb(app.accent_bgr)
            ),
        );
    }
    app.apply_startup();
    if app.config.experimental_dark_legacy && !app.high_contrast {
        if let Err(err) = legacy::set_active(true) {
            log_line("error", &err);
        } else {
            app.scan_legacy();
        }
    }
    let shell_broadcast = app.apply_saved_shell();
    {
        let mut guard = STATE.lock().unwrap_or_else(|err| err.into_inner());
        *guard = Some(app);
    }
    if shell_broadcast {
        shellwin::broadcast_immersive_color();
    }

    unsafe { message_loop(hwnd) };
    shutdown();
    Ok(())
}

fn load_config() -> (crate::Config, Option<String>) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    migrate_legacy_config();
    let bundled = || parse_config(bundled_default_json()).expect("bundled config");
    if !path.exists() {
        if wide::write_atomic(&path, bundled_default_json()).is_err() {
            log_line("error", "could not write the default config");
        }
        return (bundled(), None);
    }
    match std::fs::read_to_string(&path) {
        Ok(text) => match parse_config(&text) {
            Ok(config) => (config, None),
            Err(err) => {
                log_line("error", &format!("config ignored, running disabled: {err}"));
                let mut config = bundled();
                config.enabled = false;
                (
                    config,
                    Some(format!(
                        "config.json was not applied, so theming is off. {err}"
                    )),
                )
            }
        },
        Err(err) => {
            log_line(
                "error",
                &format!("config unreadable, running disabled: {err}"),
            );
            let mut config = bundled();
            config.enabled = false;
            (
                config,
                Some(format!(
                    "Could not read config.json, so theming is off. {err}"
                )),
            )
        }
    }
}

fn load_snapshots() -> HashMap<i64, StoredSnapshot> {
    let text = std::fs::read_to_string(snapshot_path()).unwrap_or_default();
    match parse_snapshots(&text) {
        Ok(items) => items.into_iter().map(|item| (item.hwnd, item)).collect(),
        Err(err) => {
            log_line("error", &format!("snapshot file ignored: {err}"));
            HashMap::new()
        }
    }
}

fn class_name() -> &'static [u16] {
    static NAME: OnceLock<Vec<u16>> = OnceLock::new();
    NAME.get_or_init(|| to_wide("AllBlack11.Host"))
}

unsafe fn message_loop(hwnd: HWND) {
    loop {
        let (exit_event, reg_event) = with_app(|app| (app.exit_event, app.reg_event))
            .unwrap_or((std::ptr::null_mut(), std::ptr::null_mut()));
        if exit_event.is_null() {
            break;
        }
        let handles = [exit_event, reg_event];
        let count = if reg_event.is_null() { 1 } else { 2 };
        let wait = MsgWaitForMultipleObjectsEx(
            count,
            handles.as_ptr(),
            INFINITE,
            QS_ALLINPUT,
            MWMO_INPUTAVAILABLE,
        );
        if wait == WAIT_OBJECT_0 {
            PostMessageW(hwnd, WM_CLOSE, 0, 0);
        } else if count == 2 && wait == WAIT_OBJECT_0 + 1 {
            with_app(|app| app.on_accent_notify());
        }
        loop {
            let mut message: MSG = std::mem::zeroed();
            if PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) == 0 {
                break;
            }
            if message.message == WM_QUIT {
                return;
            }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        if wait == 0xFFFF_FFFF {
            PostMessageW(hwnd, WM_CLOSE, 0, 0);
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => {
            let mouse = lparam as u32;
            if mouse == WM_RBUTTONUP || mouse == WM_CONTEXTMENU {
                show_menu(hwnd);
            } else if mouse == WM_LBUTTONDBLCLK {
                unsafe { KillTimer(hwnd, TIMER_CLICK) };
                SUPPRESS_MENU.store(true, Ordering::Release);
                settings::open(hwnd);
            } else if mouse == WM_LBUTTONUP {
                if SUPPRESS_MENU.swap(false, Ordering::AcqRel) {
                    return 0;
                }
                unsafe { SetTimer(hwnd, TIMER_CLICK, GetDoubleClickTime(), None) };
            }
            return 0;
        }
        WM_MEASUREITEM => {
            menu::on_measure(lparam);
            return 1;
        }
        WM_DRAWITEM => {
            menu::on_draw(lparam);
            return 1;
        }
        settings::WM_SETTINGS_APPLIED => {
            // Broadcast outside the state lock: this window receives its own broadcast.
            if with_app(|app| app.apply_external_config()).unwrap_or(false) {
                shellwin::broadcast_immersive_color();
            }
            return 0;
        }
        settings::WM_RUN_COMMAND => {
            on_command(hwnd, wparam as u32);
            return 0;
        }
        settings::WM_RESTART_EXPLORER => {
            request_explorer_restart(hwnd);
            return 0;
        }
        WM_EXPLORER_DONE => {
            let ok = wparam != 0;
            let text = sysinfo::take_result();
            let repaint = with_app(|app| {
                app.balloon("AllBlack11", &text, !ok);
                app.reapply_black_palette()
            })
            .unwrap_or(false);
            if repaint {
                shellwin::broadcast_immersive_color();
            }
            return 0;
        }
        WM_APPLY => {
            dispatch_queue();
            return 0;
        }
        WM_TIMER => {
            if wparam == TIMER_CLICK {
                unsafe { KillTimer(hwnd, TIMER_CLICK) };
                show_menu(hwnd);
            } else if wparam == TIMER_SECOND {
                with_app(|app| app.fire_due());
            } else if wparam == TIMER_ACCENT {
                let wrote = with_app(|app| app.reapply_black_palette()).unwrap_or(false);
                if wrote {
                    shellwin::broadcast_immersive_color();
                }
            }
            return 0;
        }
        WM_CLOSE | WM_ENDSESSION => {
            shutdown();
            if msg == WM_CLOSE {
                DestroyWindow(hwnd);
            }
            return 0;
        }
        WM_DESTROY => {
            shutdown();
            PostQuitMessage(0);
            return 0;
        }
        WM_QUERYENDSESSION => return TRUE as LRESULT,
        WM_SETTINGCHANGE => {
            on_setting_change(lparam);
            return 0;
        }
        WM_ALREADY => {
            with_app(|app| app.balloon("AllBlack11", "Already running.", false));
            return 0;
        }
        _ => {
            let taskbar = with_app(|app| app.taskbar_msg).unwrap_or(0);
            if taskbar != 0 && msg == taskbar {
                with_app(|app| app.add_tray());
                return 0;
            }
        }
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn with_app<R>(body: impl FnOnce(&mut App) -> R) -> Option<R> {
    let mut guard = STATE.lock().unwrap_or_else(|err| err.into_inner());
    guard.as_mut().map(body)
}

fn dispatch_queue() {
    loop {
        let batch = {
            let mut queue = QUEUE.lock().unwrap_or_else(|err| err.into_inner());
            std::mem::take(&mut *queue)
        };
        if batch.is_empty() {
            break;
        }
        with_app(|app| app.handle_batch(batch));
    }
}

fn on_setting_change(lparam: LPARAM) {
    let what = wide::lparam_wide(lparam);
    let interesting = what.is_empty()
        || what.contains("ImmersiveColorSet")
        || what.eq_ignore_ascii_case("WindowsThemeElement");
    if !interesting {
        return;
    }
    with_app(|app| {
        let accent = registry::accent_bgr();
        let contrast = dwm::high_contrast_enabled();
        let accent_changed = accent != app.accent_bgr;
        let contrast_changed = contrast != app.high_contrast;
        app.accent_bgr = accent;
        app.apps_dark = registry::apps_prefer_dark();
        app.high_contrast = contrast;
        if contrast && contrast_changed {
            app.restore_visuals();
            let _ = legacy::set_active(false);
            app.balloon("AllBlack11", "Paused while high contrast is on.", false);
        } else if accent_changed || contrast_changed {
            app.hold.clear();
            app.last_apply.clear();
            app.apply_startup();
            if !contrast && app.config.experimental_dark_legacy && legacy::set_active(true).is_ok() {
                app.scan_legacy();
            }
        }
        uxtheme::allow_dark_menus(!contrast);
        app.update_tray();
    });
}

fn show_menu(hwnd: HWND) {
    let Some((enabled, autostart, contrast, oled, black_shell, dark)) = with_app(|app| {
        (
            app.config.enabled,
            registry::autostart_enabled(),
            app.high_contrast,
            app.config.shell.oled_taskbar,
            app.config.shell.black_shell,
            app.apps_dark.unwrap_or(true),
        )
    }) else {
        return;
    };
    uxtheme::allow_dark_menus(!contrast);
    let arch = match sysinfo::arch_status() {
        crate::ArchStatus::NativeArm64 => " · ARM64",
        crate::ArchStatus::X64OnArm64 => " · x64 emulated",
        crate::ArchStatus::NativeX64 => " · x64",
        crate::ArchStatus::Other => "",
    };
    let presets: Vec<Item> = crate::PRESETS
        .iter()
        .enumerate()
        .map(|(index, preset)| {
            hinted(
                CMD_PRESET_BASE + index as u32,
                preset.name,
                preset.blurb,
                ItemKind::Action,
                true,
            )
        })
        .collect();
    let items = vec![
        item(
            0,
            &format!("AllBlack11 {}{arch}", display_version()),
            ItemKind::Title,
            false,
        ),
        item(0, "", ItemKind::Separator, true),
        hinted(
            CMD_ENABLE,
            if contrast { "Enabled (high contrast)" } else { "Enabled" },
            "Master switch for dark title bars, borders and backdrops.",
            ItemKind::Checked(enabled),
            !contrast,
        ),
        hinted(
            CMD_SETTINGS,
            "Settings…",
            "Every option, with a live preview and an explanation.",
            ItemKind::Action,
            true,
        ),
        submenu("Presets", presets),
        item(0, "", ItemKind::Separator, true),
        submenu(
            "Shell",
            vec![
                hinted(
                    CMD_OLED,
                    "OLED taskbar",
                    "Pure black taskbar through Windows' own OLED flag.",
                    ItemKind::Checked(oled),
                    true,
                ),
                hinted(
                    CMD_BLACK_SHELL,
                    "Black Start, taskbar & notification center",
                    "Blacks out the accent palette. Undone when unchecked.",
                    ItemKind::Checked(black_shell),
                    true,
                ),
                item(0, "", ItemKind::Separator, true),
                hinted(
                    CMD_RESTART_EXPLORER,
                    "Restart Explorer…",
                    "Reloads taskbar, Start and desktop. File Explorer windows close.",
                    ItemKind::Action,
                    true,
                ),
            ],
        ),
        submenu(
            "Maintenance",
            vec![
                item(CMD_RELOAD, "Reload config", ItemKind::Action, true),
                item(CMD_OPEN, "Open config", ItemKind::Action, true),
                item(CMD_OPEN_FOLDER, "Open data folder", ItemKind::Action, true),
                item(CMD_DIAGNOSTICS, "Write diagnostics report", ItemKind::Action, true),
                item(0, "", ItemKind::Separator, true),
                hinted(
                    CMD_RESET,
                    "Reset all windows",
                    "Put every themed window back until the next reload.",
                    ItemKind::Action,
                    true,
                ),
            ],
        ),
        item(0, "", ItemKind::Separator, true),
        item(
            CMD_AUTOSTART,
            "Start with Windows",
            ItemKind::Checked(autostart),
            true,
        ),
        item(0, "", ItemKind::Separator, true),
        item(CMD_EXIT, "Exit", ItemKind::Action, true),
    ];
    let chosen = menu::popup(hwnd, dark, contrast, &items);
    on_command(hwnd, chosen);
    settings::notify_external_change();
}

/// Version for display: the Cargo version without the `.0` patch of a preview.
pub fn display_version() -> String {
    let version = env!("CARGO_PKG_VERSION");
    match version.split_once("-preview") {
        Some((base, _)) => format!("{} Preview", base.trim_end_matches(".0")),
        None => version.to_string(),
    }
}

fn item(id: u32, label: &str, kind: ItemKind, enabled: bool) -> Item {
    hinted(id, label, "", kind, enabled)
}

fn hinted(id: u32, label: &str, hint: &str, kind: ItemKind, enabled: bool) -> Item {
    Item {
        id,
        label: label.into(),
        hint: hint.into(),
        kind,
        enabled,
        children: Vec::new(),
    }
}

fn submenu(label: &str, children: Vec<Item>) -> Item {
    Item {
        id: 0,
        label: label.into(),
        hint: String::new(),
        kind: ItemKind::Submenu,
        enabled: true,
        children,
    }
}

/// Ask, then restart Explorer on a worker thread. Called from the tray menu and Settings.
fn request_explorer_restart(hwnd: HWND) {
    if sysinfo::restart_in_progress() {
        with_app(|app| app.balloon("AllBlack11", "Explorer is already restarting.", false));
        return;
    }
    let title = to_wide("Restart Explorer");
    let text = to_wide(
        "This restarts the taskbar, Start menu and desktop (explorer.exe).\n\n\
         Open File Explorer windows will close, so save anything in them first. \
         Other apps keep running. Windows brings Explorer back by itself in a few seconds.\n\n\
         Restart Explorer now?",
    );
    let answer = unsafe {
        MessageBoxW(
            null_mut(),
            pcw(&text),
            pcw(&title),
            MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2 | MB_TOPMOST | MB_SETFOREGROUND,
        )
    };
    if answer != IDYES {
        return;
    }
    with_app(|app| app.balloon("AllBlack11", "Restarting Explorer…", false));
    sysinfo::restart_explorer_async(hwnd, WM_EXPLORER_DONE);
}

fn on_command(hwnd: HWND, command: u32) {
    match command {
        CMD_ENABLE => {
            with_app(|app| app.toggle_enabled());
        }
        CMD_RELOAD => {
            with_app(|app| app.reload());
        }
        CMD_OPEN => {
            let path = with_app(|app| {
                app.ensure_config_file();
                app.config_file.clone()
            });
            if let Some(path) = path {
                if let Err(err) = registry::open_file(&path) {
                    with_app(|app| app.balloon("AllBlack11", &err, true));
                }
            }
        }
        CMD_RESET => {
            with_app(|app| app.reset_all());
        }
        CMD_SETTINGS => {
            settings::open(hwnd);
        }
        CMD_RESTART_EXPLORER => request_explorer_restart(hwnd),
        CMD_OPEN_FOLDER => {
            if let Err(err) = registry::open_file(&wide::config_dir()) {
                with_app(|app| app.balloon("AllBlack11", &err, true));
            }
        }
        CMD_DIAGNOSTICS => {
            cmd_check_to_file();
        }
        id if (CMD_PRESET_BASE..CMD_PRESET_BASE + crate::PRESETS.len() as u32).contains(&id) => {
            let broadcast = with_app(|app| app.apply_preset((id - CMD_PRESET_BASE) as usize))
                .unwrap_or(false);
            if broadcast {
                shellwin::broadcast_immersive_color();
            }
        }
        CMD_AUTOSTART => {
            let enabled = registry::autostart_enabled();
            match registry::set_autostart(!enabled) {
                Ok(()) => {
                    let text = if enabled {
                        "Start with Windows turned off."
                    } else {
                        "Start with Windows turned on."
                    };
                    with_app(|app| app.balloon("AllBlack11", text, false));
                }
                Err(err) => {
                    with_app(|app| app.balloon("AllBlack11", &err, true));
                }
            }
        }
        CMD_OLED => {
            let broadcast = with_app(|app| app.toggle_oled()).unwrap_or(false);
            if broadcast {
                shellwin::broadcast_immersive_color();
            }
        }
        CMD_BLACK_SHELL => {
            let broadcast = with_app(|app| app.toggle_black_shell()).unwrap_or(false);
            if broadcast {
                shellwin::broadcast_immersive_color();
            }
        }
        CMD_EXIT => unsafe {
            PostMessageW(hwnd, WM_CLOSE, 0, 0);
        },
        _ => {}
    }
}

fn shutdown() {
    let mut guard = STATE.lock().unwrap_or_else(|err| err.into_inner());
    let Some(app) = guard.as_mut() else {
        return;
    };
    if app.shut_down {
        return;
    }
    app.shut_down = true;
    APP_HWND.store(0, Ordering::Relaxed);
    legacy::shutdown();
    app.unhook();
    app.restore_and_forget();
    app.remove_tray();
    unsafe {
        KillTimer(app.hwnd, TIMER_SECOND);
        if !app.icon_on.is_null() {
            DestroyIcon(app.icon_on);
            app.icon_on = null_mut();
            app.icon_off = null_mut();
        }
        if !app.accent_key.is_null() {
            RegCloseKey(app.accent_key);
            app.accent_key = null_mut();
        }
        if !app.reg_event.is_null() {
            CloseHandle(app.reg_event);
            app.reg_event = null_mut();
        }
        if !app.exit_event.is_null() {
            CloseHandle(app.exit_event);
            app.exit_event = null_mut();
        }
        if !app.instance_mutex.is_null() {
            CloseHandle(app.instance_mutex);
            app.instance_mutex = null_mut();
        }
    }
    if app.config.logging {
        log_line("info", "exit, windows restored");
    }
}

impl App {
    fn install_hooks(&mut self) {
        debug_assert!(HOOK_FLAGS & WINEVENT_INCONTEXT == 0);
        let mut failed = false;
        for event in [
            EVENT_OBJECT_SHOW,
            EVENT_OBJECT_UNCLOAKED,
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_MOVESIZESTART,
            EVENT_SYSTEM_MOVESIZEEND,
        ] {
            let hook = unsafe {
                SetWinEventHook(
                    event,
                    event,
                    null_mut(),
                    Some(win_event_proc),
                    0,
                    0,
                    HOOK_FLAGS,
                )
            };
            if hook.is_null() {
                failed = true;
                log_line(
                    "error",
                    &format!("SetWinEventHook failed for event {event}"),
                );
            } else {
                self.hooks.push(hook);
            }
        }
        if failed {
            self.balloon(
                "AllBlack11",
                "Could not watch for new windows. Open windows were still scanned.",
                true,
            );
        }
    }

    fn unhook(&mut self) {
        for hook in self.hooks.drain(..) {
            unsafe { UnhookWinEvent(hook) };
        }
    }

    fn add_tray(&self) -> bool {
        let mut nid = blank_nid(self.hwnd);
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY;
        nid.hIcon = self.current_icon();
        fill_wide(&mut nid.szTip, &self.tip());
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &nid);
            Shell_NotifyIconW(NIM_ADD, &nid) != 0
        }
    }

    fn update_tray(&self) {
        let mut nid = blank_nid(self.hwnd);
        nid.uFlags = NIF_ICON | NIF_TIP;
        nid.hIcon = self.current_icon();
        fill_wide(&mut nid.szTip, &self.tip());
        unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) };
    }

    fn remove_tray(&self) {
        let nid = blank_nid(self.hwnd);
        unsafe { Shell_NotifyIconW(NIM_DELETE, &nid) };
    }

    fn current_icon(&self) -> windows_sys::Win32::UI::WindowsAndMessaging::HICON {
        if self.config.enabled && !self.high_contrast {
            self.icon_on
        } else {
            self.icon_off
        }
    }

    fn tip(&self) -> String {
        if self.high_contrast {
            "AllBlack11: paused, high contrast".into()
        } else if !self.config.enabled {
            "AllBlack11: off".into()
        } else {
            format!("AllBlack11: {} windows", self.themed_count())
        }
    }

    fn themed_count(&self) -> usize {
        self.originals
            .keys()
            .filter(|key| !self.hold.contains(key))
            .count()
    }

    fn balloon(&self, title: &str, text: &str, error: bool) {
        let mut nid = blank_nid(self.hwnd);
        nid.uFlags = NIF_INFO;
        fill_wide(&mut nid.szInfoTitle, title);
        fill_wide(&mut nid.szInfo, text);
        nid.dwInfoFlags = if error { NIIF_ERROR } else { NIIF_INFO };
        unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) };
        if error || self.config.logging {
            log_line(if error { "error" } else { "info" }, text);
        }
    }

    fn toggle_enabled(&mut self) {
        let turning_on = !self.config.enabled;
        if turning_on {
            self.ensure_config_file();
            match std::fs::read_to_string(&self.config_file) {
                Ok(text) => match parse_config(&text) {
                    Ok(mut config) => {
                        config.enabled = true;
                        self.config = config;
                        self.accent_bgr = registry::accent_bgr();
                        self.apps_dark = registry::apps_prefer_dark();
                    }
                    Err(err) => {
                        self.balloon(
                            "AllBlack11",
                            &format!("Fix config.json before enabling. {err}"),
                            true,
                        );
                        return;
                    }
                },
                Err(err) => {
                    self.balloon("AllBlack11", &format!("Could not read config: {err}"), true);
                    return;
                }
            }
        } else {
            self.config.enabled = false;
        }
        self.persist_enabled();
        if self.config.enabled {
            self.hold.clear();
            self.last_apply.clear();
            self.exe_cache.clear();
            self.apply_startup();
        } else {
            self.restore_visuals();
        }
        self.update_tray();
    }

    fn reload(&mut self) {
        let text = match std::fs::read_to_string(&self.config_file) {
            Ok(text) => text,
            Err(err) => {
                self.balloon("AllBlack11", &format!("Could not read config: {err}"), true);
                return;
            }
        };
        match parse_config(&text) {
            Ok(config) => {
                self.config = config;
                self.accent_bgr = registry::accent_bgr();
                self.apps_dark = registry::apps_prefer_dark();
                self.high_contrast = dwm::high_contrast_enabled();
                self.hold.clear();
                self.last_apply.clear();
                self.exe_cache.clear();
                if self.config.enabled && !self.high_contrast {
                    self.apply_startup();
                } else {
                    self.restore_visuals();
                }
                self.update_tray();
                self.balloon("AllBlack11", "Config reloaded.", false);
            }
            Err(err) => {
                self.balloon("AllBlack11", &format!("Config not applied: {err}"), true);
            }
        }
    }

    fn reset_all(&mut self) {
        let keys: Vec<i64> = self.originals.keys().copied().collect();
        for key in keys {
            self.restore_one(key, false);
            self.hold.insert(key);
        }
        self.update_tray();
        self.balloon(
            "AllBlack11",
            "Windows restored. Reload config to theme them again.",
            false,
        );
    }

    fn ensure_config_file(&self) {
        if self.config_file.exists() {
            return;
        }
        if let Some(parent) = self.config_file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = wide::write_atomic(&self.config_file, bundled_default_json());
    }

    fn persist_enabled(&self) {
        let text = std::fs::read_to_string(&self.config_file).unwrap_or_default();
        let updated = if text.trim().is_empty() {
            let mut config = self.config.clone();
            config.enabled = self.config.enabled;
            serde_json::to_string_pretty(&config).unwrap_or_else(|_| text)
        } else {
            set_enabled_flag(&text, self.config.enabled).unwrap_or_else(|_| text)
        };
        if wide::write_atomic(&self.config_file, &format!("{updated}\n")).is_err() {
            log_line("error", "could not update enabled in config.json");
        }
    }

    fn apply_startup(&mut self) {
        if self.high_contrast || !self.config.enabled {
            self.scan_legacy();
            return;
        }
        let hwnds = enum_hwnds();
        let mut pending = Vec::new();
        for key in hwnds {
            if let Some(item) = self.plan(key, Kind::Startup) {
                pending.push(item);
            }
        }
        self.commit(pending);
        if self.config.logging && !self.logged_startup {
            self.logged_startup = true;
            log_line(
                "info",
                &format!("startup pass, {} windows remembered", self.themed_count()),
            );
        }
        self.update_tray();
        self.scan_legacy();
    }

    fn scan_legacy(&self) {
        if !self.config.experimental_dark_legacy || self.high_contrast || self.shut_down {
            return;
        }
        for key in enum_hwnds() {
            self.note_legacy(key);
        }
    }

    fn note_legacy(&self, key: i64) {
        if !self.config.experimental_dark_legacy || self.high_contrast || self.shut_down {
            return;
        }
        legacy::consider(key_to_hwnd(key), &self.config.exclude);
    }

    /// Returns true when a shell value changed and Windows should be told.
    fn apply_external_config(&mut self) -> bool {
        let old = self.config.clone();
        let text = match std::fs::read_to_string(&self.config_file) {
            Ok(text) => text,
            Err(err) => {
                self.balloon("AllBlack11", &format!("Could not read config: {err}"), true);
                return false;
            }
        };
        let new = match parse_config(&text) {
            Ok(config) => config,
            Err(err) => {
                self.balloon("AllBlack11", &format!("Config not applied: {err}"), true);
                return false;
            }
        };
        self.config = new;
        self.config.caption_height = normalize_caption_height(self.config.caption_height);
        let shell_changed = self.reconcile_shell(&old);
        if normalize_caption_height(old.caption_height) != self.config.caption_height {
            if let Err(err) = metrics::apply_caption_height(self.config.caption_height) {
                self.balloon("AllBlack11", &err, true);
            } else if self.config.caption_height.is_some() {
                self.balloon(
                    "AllBlack11",
                    "Caption height saved. Sign out and back in for it to apply.",
                    false,
                );
            }
        }
        let want_hook = self.config.experimental_dark_legacy && !self.high_contrast;
        let had_hook = old.experimental_dark_legacy && !self.high_contrast;
        if want_hook != had_hook || (want_hook && old.exclude != self.config.exclude) {
            let _ = legacy::set_active(false);
            if want_hook {
                if let Err(err) = legacy::set_active(true) {
                    self.balloon("AllBlack11", &err, true);
                    self.config.experimental_dark_legacy = false;
                }
            }
        }
        self.accent_bgr = registry::accent_bgr();
        self.apps_dark = registry::apps_prefer_dark();
        self.high_contrast = dwm::high_contrast_enabled();
        self.hold.clear();
        self.last_apply.clear();
        self.exe_cache.clear();
        if self.config.enabled && !self.high_contrast {
            self.apply_startup();
        } else {
            self.restore_visuals();
        }
        self.update_tray();
        uxtheme::allow_dark_menus(!self.high_contrast);
        shell_changed
    }

    /// Apply a preset from the tray: write the config, then take the same path as Settings.
    fn apply_preset(&mut self, index: usize) -> bool {
        let mut config = cfgio::load();
        if !crate::apply_preset(&mut config, index) {
            return false;
        }
        if let Err(err) = cfgio::store(&config) {
            self.balloon("AllBlack11", &err, true);
            return false;
        }
        let changed = self.apply_external_config();
        if let Some(preset) = crate::PRESETS.get(index) {
            self.balloon("AllBlack11", &format!("Preset applied: {}.", preset.name), false);
        }
        changed
    }

    fn reconcile_shell(&mut self, old: &crate::Config) -> bool {
        let want_oled = self.config.shell.oled_taskbar;
        let want_black = self.config.shell.black_shell;
        let mut changed = false;
        if want_oled != old.shell.oled_taskbar {
            self.config.shell.oled_taskbar = old.shell.oled_taskbar;
            changed |= self.set_oled(want_oled);
        }
        if want_black != old.shell.black_shell {
            self.config.shell.black_shell = old.shell.black_shell;
            changed |= self.set_black_shell(want_black);
        }
        changed
    }

    fn handle_batch(&mut self, batch: Vec<(i64, u32)>) {
        if self.shut_down {
            return;
        }
        let mut kinds: HashMap<i64, Kind> = HashMap::new();
        for (key, event) in batch {
            let kind = kind_from_event(event);
            kinds
                .entry(key)
                .and_modify(|current| {
                    if rank(kind) > rank(*current) {
                        *current = kind;
                    }
                })
                .or_insert(kind);
        }
        let mut pending = Vec::new();
        for (key, kind) in kinds {
            self.note_legacy(key);
            if let Some(item) = self.plan(key, kind) {
                pending.push(item);
            }
        }
        self.commit(pending);
        self.update_tray();
    }

    fn fire_due(&mut self) {
        let now = unsafe { GetTickCount64() };
        let due: Vec<i64> = self
            .due
            .iter()
            .filter(|item| item.at <= now)
            .map(|item| item.hwnd)
            .collect();
        self.due.retain(|item| item.at > now);
        let mut pending = Vec::new();
        for key in due {
            if let Some(item) = self.plan(key, Kind::Second) {
                pending.push(item);
            }
        }
        self.commit(pending);
        self.arm_timer();
        self.update_tray();
    }

    fn plan(&mut self, key: i64, kind: Kind) -> Option<PendingApply> {
        let hwnd = key_to_hwnd(key);
        if unsafe { IsWindow(hwnd) } == 0 {
            if self.originals.remove(&key).is_some() {
                self.dirty = true;
            }
            self.hold.remove(&key);
            self.exe_cache.remove(&key);
            return None;
        }
        if kind == Kind::Startup
            && unsafe { IsWindowVisible(hwnd) } == 0
            && unsafe { IsIconic(hwnd) } == 0
        {
            return None;
        }
        let Some((pid, create, exe)) = self.identity(hwnd) else {
            if self.originals.contains_key(&key) {
                self.restore_one(key, false);
            }
            return None;
        };
        if self.hold.contains(&key) {
            if self
                .originals
                .get(&key)
                .is_some_and(|snap| same_process(snap, pid, create))
            {
                return None;
            }
            self.hold.remove(&key);
        }
        if self.high_contrast || !self.config.enabled {
            return None;
        }
        let facts = collect_facts(hwnd, &exe, pid);
        match decide(&facts, &self.config, self.accent_bgr) {
            Decision::Skip(_) => {
                if self.originals.contains_key(&key) {
                    self.restore_one(key, false);
                }
                None
            }
            Decision::Apply(style) => {
                if kind == Kind::MoveStart
                    && (style.backdrop != Backdrop::TintedAcrylic
                        || !style.tinted_acrylic_on_move_fallback)
                {
                    return None;
                }
                if kind == Kind::Show {
                    self.schedule_second(key);
                }
                if !self.should_apply(key, kind) {
                    return None;
                }
                self.ensure_original(key, hwnd, pid, create);
                Some(PendingApply {
                    hwnd: key,
                    pid,
                    exe,
                    style,
                    move_blur: kind == Kind::MoveStart,
                })
            }
        }
    }

    fn should_apply(&self, key: i64, kind: Kind) -> bool {
        let now = unsafe { GetTickCount64() };
        let last = self.last_apply.get(&key).copied().unwrap_or(0);
        match kind {
            Kind::Startup | Kind::Second | Kind::MoveStart | Kind::MoveSize => true,
            Kind::Show => now.saturating_sub(last) > 80,
            Kind::Foreground => self.config.reapply_on_foreground && now.saturating_sub(last) > 800,
        }
    }

    fn ensure_original(&mut self, key: i64, hwnd: HWND, pid: u32, create: u64) {
        if let Some(existing) = self.originals.get(&key) {
            if same_process(existing, pid, create) {
                return;
            }
        }
        let captured = dwm::capture(hwnd);
        self.originals.insert(
            key,
            StoredSnapshot {
                hwnd: key,
                pid,
                create,
                dark: captured.dark,
                backdrop: captured.backdrop,
                caption: captured.caption,
                border: captured.border,
                text: captured.text,
                corners: captured.corners,
                accent_policy: false,
            },
        );
        self.dirty = true;
    }

    fn commit(&mut self, pending: Vec<PendingApply>) {
        if self.dirty {
            self.write_snapshots();
        }
        let now = unsafe { GetTickCount64() };
        for item in pending {
            let hwnd = key_to_hwnd(item.hwnd);
            if unsafe { IsWindow(hwnd) } == 0 {
                continue;
            }
            if item.move_blur {
                self.apply_move_blur(item.hwnd, hwnd, item.pid, &item.exe, &item.style);
                continue;
            }
            let mut backdrop = item.style.backdrop.dwm_value();
            if item.style.backdrop == Backdrop::TintedAcrylic {
                let gradient = acrylic_gradient(
                    item.style.acrylic_tint,
                    item.style.acrylic_opacity,
                    self.accent_bgr,
                );
                if composition::set_accent(
                    hwnd,
                    composition::ACCENT_ENABLE_ACRYLICBLURBEHIND,
                    gradient,
                ) {
                    backdrop = Backdrop::None.dwm_value();
                    self.mark_accent(item.hwnd, true);
                } else {
                    self.note_swca_failure(item.pid, &item.exe);
                    let _ = composition::clear_accent(hwnd);
                    backdrop = Backdrop::Acrylic.dwm_value();
                }
            } else if self
                .originals
                .get(&item.hwnd)
                .is_some_and(|snap| snap.accent_policy)
            {
                if composition::clear_accent(hwnd) {
                    self.mark_accent(item.hwnd, false);
                }
            }
            for failure in dwm::apply(hwnd, &item.style, self.accent_bgr, self.build, backdrop) {
                if self.seen_failures.insert(failure.attribute) {
                    log_line(
                        "warn",
                        &format!(
                            "{} ({}) HRESULT 0x{:08X} on {}",
                            failure.name, failure.attribute, failure.hr as u32, item.exe
                        ),
                    );
                }
            }
            self.last_apply.insert(item.hwnd, now);
            legacy::nudge(hwnd);
        }
        if self.dirty {
            self.write_snapshots();
        }
    }

    fn schedule_second(&mut self, key: i64) {
        if self.due.iter().any(|item| item.hwnd == key) || self.due.len() > 256 {
            return;
        }
        self.due.push(Due {
            hwnd: key,
            at: unsafe { GetTickCount64() } + 300,
        });
        self.arm_timer();
    }

    fn arm_timer(&self) {
        let now = unsafe { GetTickCount64() };
        let Some(soonest) = self.due.iter().map(|item| item.at).min() else {
            unsafe { KillTimer(self.hwnd, TIMER_SECOND) };
            return;
        };
        let wait = soonest.saturating_sub(now).clamp(16, 2_000) as u32;
        unsafe { SetTimer(self.hwnd, TIMER_SECOND, wait, None) };
    }

    fn restore_visuals(&mut self) {
        let keys: Vec<i64> = self.originals.keys().copied().collect();
        for key in keys {
            self.restore_one(key, false);
        }
        self.update_tray();
    }

    fn restore_and_forget(&mut self) {
        let keys: Vec<i64> = self.originals.keys().copied().collect();
        for key in keys {
            self.restore_one(key, true);
        }
        self.dirty = true;
        self.write_snapshots();
    }

    fn restore_one(&mut self, key: i64, forget: bool) {
        let Some(snap) = self.originals.get(&key).cloned() else {
            return;
        };
        let hwnd = key_to_hwnd(key);
        if unsafe { IsWindow(hwnd) } != 0 {
            dwm::restore(hwnd, &snap, self.apps_dark, self.build);
            if snap.accent_policy {
                let _ = composition::clear_accent(hwnd);
            }
        }
        self.last_apply.remove(&key);
        if forget {
            self.originals.remove(&key);
            self.hold.remove(&key);
        }
    }

    fn write_snapshots(&mut self) {
        self.prune_dead();
        let items: Vec<StoredSnapshot> = self.originals.values().cloned().collect();
        let text = format_snapshots(&items);
        if wide::write_atomic(&snapshot_path(), &format!("{text}\n")).is_ok() {
            self.dirty = false;
        }
    }

    fn prune_dead(&mut self) {
        let keys: Vec<i64> = self.originals.keys().copied().collect();
        for key in keys {
            let hwnd = key_to_hwnd(key);
            if unsafe { IsWindow(hwnd) } == 0 {
                self.originals.remove(&key);
                self.hold.remove(&key);
                self.dirty = true;
                continue;
            }
            if let Some((pid, create, _)) = self.identity(hwnd) {
                if self
                    .originals
                    .get(&key)
                    .is_some_and(|snap| !same_process(snap, pid, create))
                {
                    self.originals.remove(&key);
                    self.hold.remove(&key);
                    self.dirty = true;
                }
            }
        }
    }

    fn apply_move_blur(&mut self, key: i64, hwnd: HWND, pid: u32, exe: &str, style: &Style) {
        let gradient = acrylic_gradient(style.acrylic_tint, style.acrylic_opacity, self.accent_bgr);
        if composition::set_accent(hwnd, composition::ACCENT_ENABLE_BLURBEHIND, gradient) {
            self.mark_accent(key, true);
        } else {
            self.note_swca_failure(pid, exe);
            let _ = composition::clear_accent(hwnd);
            let _ = dwm::set_backdrop(hwnd, Backdrop::Acrylic.dwm_value(), self.build);
        }
    }

    fn mark_accent(&mut self, key: i64, touched: bool) {
        if let Some(snap) = self.originals.get_mut(&key) {
            if snap.accent_policy != touched {
                snap.accent_policy = touched;
                self.dirty = true;
            }
        }
    }

    fn note_swca_failure(&mut self, pid: u32, exe: &str) {
        if self.swca_failed.insert(pid) {
            log_line(
                "warn",
                &format!("SetWindowCompositionAttribute failed for {exe}; using DWM acrylic"),
            );
        }
    }

    fn apply_saved_shell(&mut self) -> bool {
        let mut broadcast = false;
        if self.config.shell.oled_taskbar {
            let mut backup = shellwin::load_backup();
            if let Err(err) = shellwin::enable_oled(&mut backup) {
                log_line("error", &err);
            } else {
                broadcast = true;
            }
        }
        if self.config.shell.black_shell {
            let mut backup = shellwin::load_backup();
            match shellwin::enable_black_shell(&mut backup) {
                Ok(_) => {
                    self.start_accent_watch();
                    broadcast = true;
                }
                Err(err) => log_line("error", &err),
            }
        }
        broadcast
    }

    fn toggle_oled(&mut self) -> bool {
        self.set_oled(!self.config.shell.oled_taskbar)
    }

    fn set_oled(&mut self, enable: bool) -> bool {
        self.ensure_config_file();
        if self.config.shell.oled_taskbar == enable {
            return false;
        }
        let mut backup = shellwin::load_backup();
        let result = if enable {
            shellwin::enable_oled(&mut backup)
        } else {
            shellwin::disable_oled(&mut backup)
        };
        match result {
            Ok(()) => {
                self.config.shell.oled_taskbar = enable;
                self.persist_shell("oledTaskbar", enable);
                true
            }
            Err(err) => {
                self.balloon("AllBlack11", &err, true);
                false
            }
        }
    }

    fn toggle_black_shell(&mut self) -> bool {
        self.set_black_shell(!self.config.shell.black_shell)
    }

    fn set_black_shell(&mut self, enable: bool) -> bool {
        self.ensure_config_file();
        if self.config.shell.black_shell == enable {
            return false;
        }
        let mut backup = shellwin::load_backup();
        let result = if enable {
            shellwin::enable_black_shell(&mut backup).map(|_| ())
        } else {
            shellwin::disable_black_shell(&mut backup)
        };
        match result {
            Ok(()) => {
                self.config.shell.black_shell = enable;
                self.persist_shell("blackShell", enable);
                if enable {
                    self.start_accent_watch();
                } else {
                    self.stop_accent_watch();
                }
                true
            }
            Err(err) => {
                self.balloon("AllBlack11", &err, true);
                false
            }
        }
    }

    fn persist_shell(&self, field: &str, enabled: bool) {
        let text = std::fs::read_to_string(&self.config_file).unwrap_or_default();
        let updated = set_shell_flag(&text, field, enabled).unwrap_or(text);
        if wide::write_atomic(&self.config_file, &format!("{updated}\n")).is_err() {
            log_line("error", "could not save the shell toggle");
        }
    }

    fn start_accent_watch(&mut self) {
        if self.accent_key.is_null() {
            self.accent_key = shellwin::open_accent_key().unwrap_or(std::ptr::null_mut());
        }
        if !shellwin::arm_accent_watch(self.accent_key, self.reg_event) {
            log_line("warn", "could not watch the Accent registry key");
        }
    }

    fn stop_accent_watch(&mut self) {
        if !self.accent_key.is_null() {
            unsafe { RegCloseKey(self.accent_key) };
            self.accent_key = std::ptr::null_mut();
        }
    }

    fn on_accent_notify(&mut self) {
        if self.config.shell.black_shell {
            self.start_accent_watch();
            unsafe { SetTimer(self.hwnd, TIMER_ACCENT, 400, None) };
        }
    }

    fn reapply_black_palette(&mut self) -> bool {
        if !self.config.shell.black_shell {
            return false;
        }
        match shellwin::reapply_black_palette() {
            Ok(wrote) => wrote,
            Err(err) => {
                log_line("error", &err);
                false
            }
        }
    }

    fn identity(&mut self, hwnd: HWND) -> Option<(u32, u64, String)> {
        let mut pid = 0u32;
        let thread = unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        if thread == 0 || pid == 0 {
            return None;
        }
        let key = hwnd as isize as i64;
        if let Some((cached_pid, create, exe)) = self.exe_cache.get(&key) {
            if *cached_pid == pid {
                return Some((pid, *create, exe.clone()));
            }
        }
        let (create, exe) = query_process(pid)?;
        if self.exe_cache.len() > 1024 {
            self.exe_cache.clear();
        }
        self.exe_cache.insert(key, (pid, create, exe.clone()));
        Some((pid, create, exe))
    }
}

fn kind_from_event(event: u32) -> Kind {
    if event == EVENT_SYSTEM_FOREGROUND {
        Kind::Foreground
    } else if event == EVENT_SYSTEM_MOVESIZESTART {
        Kind::MoveStart
    } else if event == EVENT_SYSTEM_MOVESIZEEND {
        Kind::MoveSize
    } else {
        Kind::Show
    }
}

fn rank(kind: Kind) -> u8 {
    match kind {
        Kind::MoveSize => 6,
        Kind::MoveStart => 5,
        Kind::Second => 4,
        Kind::Show => 3,
        Kind::Startup => 2,
        Kind::Foreground => 1,
    }
}

fn blank_nid(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid
}

fn key_to_hwnd(key: i64) -> HWND {
    key as isize as HWND
}

unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if hwnd.is_null() || id_object != 0 || id_child != 0 {
        return;
    }
    let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
    if style & WS_CHILD != 0 {
        return;
    }
    if style & WS_DLGFRAME == 0 && class_of(hwnd) != "#32770" {
        return;
    }
    let exstyle = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
    if exstyle & WS_EX_TOOLWINDOW != 0 {
        return;
    }
    enqueue(hwnd as isize as i64, event);
}

fn enqueue(hwnd: i64, event: u32) {
    if APP_HWND.load(Ordering::Relaxed) == 0 {
        return;
    }
    let mut queue = QUEUE.lock().unwrap_or_else(|err| err.into_inner());
    if queue.len() >= 4096 {
        return;
    }
    let was_empty = queue.is_empty();
    queue.push((hwnd, event));
    drop(queue);
    if was_empty {
        let raw = APP_HWND.load(Ordering::Relaxed);
        if raw != 0 {
            unsafe { PostMessageW(raw as HWND, WM_APPLY, 0, 0) };
        }
    }
}

fn enum_hwnds() -> Vec<i64> {
    let mut hwnds = Vec::new();
    unsafe {
        EnumWindows(Some(enum_proc), &mut hwnds as *mut Vec<i64> as LPARAM);
    }
    hwnds
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let hwnds = &mut *(lparam as *mut Vec<i64>);
    hwnds.push(hwnd as isize as i64);
    1
}

fn collect_facts(hwnd: HWND, exe: &str, pid: u32) -> WindowFacts {
    let _ = pid;
    unsafe {
        let class_name = class_of(hwnd);
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        let exstyle = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let _ = GetWindowRect(hwnd, &mut rect);
        let width = (rect.right - rect.left).abs();
        let height = (rect.bottom - rect.top).abs();
        let root = GetAncestor(hwnd, GA_ROOT);
        let owner = GetWindow(hwnd, GW_OWNER);
        WindowFacts {
            exe: exe.to_string(),
            class_name,
            style,
            exstyle,
            width,
            height,
            top_level: !root.is_null() && root == hwnd,
            has_owner: !owner.is_null(),
            minimized: IsIconic(hwnd) != 0,
            covers_monitor: covers_monitor(hwnd, &rect),
            pid_known: true,
        }
    }
}

fn covers_monitor(hwnd: HWND, rect: &RECT) -> bool {
    unsafe {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if monitor.is_null() {
            return false;
        }
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return false;
        }
        rect_covers_monitor(
            (rect.left, rect.top, rect.right, rect.bottom),
            (
                info.rcMonitor.left,
                info.rcMonitor.top,
                info.rcMonitor.right,
                info.rcMonitor.bottom,
            ),
        )
    }
}

fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let length = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if length <= 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&buf[..length as usize])
    }
}

fn query_process(pid: u32) -> Option<(u64, String)> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut created = std::mem::zeroed();
        let mut exited = std::mem::zeroed();
        let mut kernel = std::mem::zeroed();
        let mut user = std::mem::zeroed();
        let timed = GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user);
        let create = if timed != 0 {
            ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64
        } else {
            0
        };
        let mut buf = [0u16; 1024];
        let mut length = buf.len() as u32;
        let ok =
            QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut length);
        CloseHandle(handle);
        if ok == 0 {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..length as usize]);
        Some((create, file_name(&path).to_string()))
    }
}
