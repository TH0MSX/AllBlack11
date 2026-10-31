//! AllBlack11 Setup. One small native program per CPU type (ARM64 or x64) that
//! embeds the app, installs it for the current user, and registers an entry in
//! Settings > Apps > Installed apps. No administrator rights.
//!
//! Command line:
//!   AllBlack11-Setup.exe                    Wizard (install, update, or uninstall)
//!   AllBlack11-Setup.exe /S                 Silent install
//!     /NOAUTOSTART /NOSHORTCUT /NOLAUNCH    Silent-install options
//!   AllBlack11-Setup.exe --uninstall [/S] [/PURGE]
//!                                            Uninstall. /PURGE also deletes settings.

#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(windows))]
fn main() {
    eprintln!("AllBlack11 Setup is a Windows program.");
    std::process::exit(1);
}

#[cfg(windows)]
fn main() {
    std::process::exit(win::run());
}

#[cfg(windows)]
mod win {
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::ptr::{null, null_mut};
    use std::time::Duration;

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect,
        GetSysColorBrush, InvalidateRect, SetBkMode, SetTextColor, COLOR_WINDOW, HDC, HFONT, PAINTSTRUCT,
        TRANSPARENT,
    };
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegDeleteValueW, RegSetValueExW, HKEY,
        HKEY_CURRENT_USER, KEY_SET_VALUE, REG_DWORD, REG_OPTION_NON_VOLATILE, REG_SZ,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, IsWow64Process2};
    use windows_sys::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
        GetClientRect, GetMessageW, GetWindowLongPtrW, LoadCursorW, MessageBoxW, PostQuitMessage,
        RegisterClassExW, SendMessageW, SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow,
        TranslateMessage, CREATESTRUCTW, GWLP_USERDATA, IDC_ARROW, IDYES, MB_ICONERROR, MB_ICONINFORMATION,
        MB_ICONQUESTION, MB_OK, MB_YESNO, MSG, SWP_NOZORDER, SW_SHOW, WM_COMMAND, WM_CREATE,
        WM_CTLCOLORSTATIC, WM_DESTROY, WM_ERASEBKGND, WM_PAINT, WM_SETFONT, WNDCLASSEXW, WS_CAPTION,
        WS_CHILD, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
    };

    // Embedded payload for this CPU type. ALLBLACK11_PAYLOAD is set by build.sh and the release workflow.
    static APP_EXE: &[u8] = include_bytes!(concat!(env!("ALLBLACK11_PAYLOAD"), "/AllBlack11.exe"));
    static HOOK_DLL: &[u8] = include_bytes!(concat!(env!("ALLBLACK11_PAYLOAD"), "/AllBlack11Hook.dll"));
    static DEFAULT_CONFIG: &[u8] = include_bytes!(concat!(env!("ALLBLACK11_PAYLOAD"), "/config.default.json"));
    static LICENSE: &[u8] = include_bytes!(concat!(env!("ALLBLACK11_PAYLOAD"), "/LICENSE"));

    const VERSION: &str = env!("CARGO_PKG_VERSION");
    const NO_WINDOW: u32 = 0x0800_0000; // CREATE_NO_WINDOW
    const DETACHED: u32 = 0x0000_0008; // DETACHED_PROCESS
    const UNINSTALL_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\AllBlack11";
    const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";

    const MACHINE_ARM64: u16 = 0xAA64;
    const MACHINE_AMD64: u16 = 0x8664;
    const BUILD_MACHINE: u16 = if cfg!(target_arch = "aarch64") { MACHINE_ARM64 } else { MACHINE_AMD64 };

    // ---- small helpers ------------------------------------------------------

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn display_version() -> String {
        match VERSION.split_once("-preview") {
            Some((base, _)) => format!("{} Preview", base.trim_end_matches(".0")),
            None => VERSION.to_string(),
        }
    }

    fn dest_dir() -> PathBuf {
        PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_default()).join("Programs").join("AllBlack11")
    }

    fn data_dir() -> PathBuf {
        PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("AllBlack11")
    }

    fn shortcut_path() -> PathBuf {
        PathBuf::from(std::env::var("APPDATA").unwrap_or_default())
            .join("Microsoft\\Windows\\Start Menu\\Programs\\AllBlack11.lnk")
    }

    fn message(owner: HWND, title: &str, text: &str, flags: u32) -> i32 {
        let (title, text) = (wide(title), wide(text));
        unsafe { MessageBoxW(owner, text.as_ptr(), title.as_ptr(), flags) }
    }

    fn host_machine() -> u16 {
        let (mut process, mut native) = (0u16, 0u16);
        if unsafe { IsWow64Process2(GetCurrentProcess(), &mut process, &mut native) } == 0 {
            0
        } else {
            native
        }
    }

    fn machine_name(machine: u16) -> &'static str {
        match machine {
            MACHINE_ARM64 => "ARM64",
            MACHINE_AMD64 => "x64",
            _ => "unknown",
        }
    }

    /// Run a hidden helper and wait for it. Errors are ignored: every use is best effort.
    fn quiet(program: &str, args: &[&str]) -> bool {
        Command::new(program)
            .args(args)
            .creation_flags(NO_WINDOW)
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    // ---- registry -----------------------------------------------------------

    struct Key(HKEY);

    impl Key {
        fn create(path: &str) -> Result<Key, String> {
            let path = wide(path);
            let mut key: HKEY = null_mut();
            let status = unsafe {
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    path.as_ptr(),
                    0,
                    null(),
                    REG_OPTION_NON_VOLATILE,
                    KEY_SET_VALUE,
                    null(),
                    &mut key,
                    null_mut(),
                )
            };
            if status == 0 {
                Ok(Key(key))
            } else {
                Err(format!("registry error {status}"))
            }
        }

        fn set_string(&self, name: &str, value: &str) {
            let (name, data) = (wide(name), wide(value));
            unsafe {
                RegSetValueExW(self.0, name.as_ptr(), 0, REG_SZ, data.as_ptr() as *const u8, (data.len() * 2) as u32);
            }
        }

        fn set_dword(&self, name: &str, value: u32) {
            let name = wide(name);
            unsafe {
                RegSetValueExW(self.0, name.as_ptr(), 0, REG_DWORD, value.to_le_bytes().as_ptr(), 4);
            }
        }

        fn delete_value(&self, name: &str) {
            let name = wide(name);
            unsafe { RegDeleteValueW(self.0, name.as_ptr()) };
        }
    }

    impl Drop for Key {
        fn drop(&mut self) {
            unsafe { RegCloseKey(self.0) };
        }
    }

    // ---- install / uninstall -----------------------------------------------

    #[derive(Clone, Copy)]
    struct Options {
        autostart: bool,
        shortcut: bool,
        launch: bool,
    }

    fn installed() -> bool {
        dest_dir().join("AllBlack11.exe").is_file()
    }

    /// Replace `path` with `bytes`. A file in use cannot be overwritten but can be renamed,
    /// so a locked old copy is moved aside and removed on a later run.
    fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
        let tmp = path.with_extension("new");
        std::fs::write(&tmp, bytes).map_err(|err| format!("{}: {err}", path.display()))?;
        if path.exists() && std::fs::remove_file(path).is_err() {
            let aside = path.with_extension("old");
            let _ = std::fs::remove_file(&aside);
            std::fs::rename(path, &aside).map_err(|err| format!("{} is in use: {err}", path.display()))?;
        }
        std::fs::rename(&tmp, path).map_err(|err| format!("{}: {err}", path.display()))
    }

    fn stop_running_app(dir: &Path) {
        let exe = dir.join("AllBlack11.exe");
        if exe.is_file() {
            let _ = Command::new(&exe).arg("--exit").creation_flags(NO_WINDOW).status();
        }
        std::thread::sleep(Duration::from_millis(900));
        // Still there? End it. The app restores window frames when it receives --exit,
        // so this only matters if it hung.
        quiet("taskkill", &["/F", "/IM", "AllBlack11.exe"]);
        std::thread::sleep(Duration::from_millis(250));
    }

    fn retire_pepatheme() {
        quiet("taskkill", &["/F", "/IM", "PepaTheme.exe"]);
        if let Ok(key) = Key::create(RUN_KEY) {
            key.delete_value("PepaTheme");
        }
        let old = PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_default()).join("Programs\\PepaTheme");
        if old.exists() {
            let _ = std::fs::remove_dir_all(old);
        }
    }

    fn make_shortcut(target: &Path, link: &Path) -> bool {
        let script = format!(
            "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{link}');\
             $s.TargetPath='{target}';$s.WorkingDirectory='{dir}';$s.IconLocation='{target},0';\
             $s.Description='AllBlack11';$s.Save()",
            link = link.display().to_string().replace('\'', "''"),
            target = target.display().to_string().replace('\'', "''"),
            dir = target.parent().map(|p| p.display().to_string()).unwrap_or_default().replace('\'', "''"),
        );
        quiet("powershell", &["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
    }

    fn install(options: Options, report: &mut dyn FnMut(&str)) -> Result<(), String> {
        let dest = dest_dir();
        report("Closing AllBlack11 if it is running…");
        retire_pepatheme();
        stop_running_app(&dest);

        report("Copying files…");
        std::fs::create_dir_all(&dest).map_err(|err| format!("{}: {err}", dest.display()))?;
        write_file(&dest.join("AllBlack11.exe"), APP_EXE)?;
        write_file(&dest.join("AllBlack11Hook.dll"), HOOK_DLL)?;
        write_file(&dest.join("LICENSE"), LICENSE)?;
        write_file(&dest.join("config.default.json"), DEFAULT_CONFIG)?;
        for leftover in ["AllBlack11.old", "AllBlack11Hook.old"] {
            let _ = std::fs::remove_file(dest.join(leftover));
        }
        let me = std::env::current_exe().map_err(|err| err.to_string())?;
        let installed_setup = dest.join("AllBlack11-Setup.exe");
        if me != installed_setup {
            let _ = write_file(&installed_setup, &std::fs::read(&me).map_err(|err| err.to_string())?);
        }

        report("Preparing settings…");
        let data = data_dir();
        let _ = std::fs::create_dir_all(&data);
        let config = data.join("config.json");
        if !config.exists() {
            let legacy = PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join("PepaTheme");
            if legacy.join("config.json").is_file() {
                let _ = std::fs::copy(legacy.join("config.json"), &config);
                let snapshots = legacy.join("snapshots.json");
                if snapshots.is_file() {
                    let _ = std::fs::copy(snapshots, data.join("snapshots.json"));
                }
            } else {
                std::fs::write(&config, DEFAULT_CONFIG).map_err(|err| err.to_string())?;
            }
        }

        report("Registering with Windows…");
        let exe = dest.join("AllBlack11.exe");
        let run = Key::create(RUN_KEY)?;
        if options.autostart {
            run.set_string("AllBlack11", &format!("\"{}\"", exe.display()));
        } else {
            run.delete_value("AllBlack11");
        }
        let link = shortcut_path();
        if options.shortcut {
            if !make_shortcut(&exe, &link) {
                report("Could not create the Start menu shortcut. Continuing…");
            }
        } else {
            let _ = std::fs::remove_file(&link);
        }
        let key = Key::create(UNINSTALL_KEY)?;
        let setup = format!("\"{}\"", installed_setup.display());
        key.set_string("DisplayName", "AllBlack11");
        key.set_string("DisplayVersion", VERSION);
        key.set_string("Publisher", "Tomasz Pepliński (TH0MSX)");
        key.set_string("InstallLocation", &dest.display().to_string());
        key.set_string("DisplayIcon", &format!("{},0", exe.display()));
        key.set_string("UninstallString", &format!("{setup} --uninstall"));
        key.set_string("QuietUninstallString", &format!("{setup} --uninstall /S"));
        key.set_string("URLInfoAbout", "https://github.com/TH0MSX/AllBlack11");
        key.set_dword("NoModify", 1);
        key.set_dword("NoRepair", 1);
        key.set_dword("EstimatedSize", ((APP_EXE.len() + HOOK_DLL.len() + 2 * 1024 * 1024) / 1024) as u32);

        if options.launch {
            report("Starting AllBlack11…");
            let _ = Command::new(&exe).creation_flags(DETACHED).spawn();
        }
        Ok(())
    }

    fn uninstall(purge: bool, report: &mut dyn FnMut(&str)) -> Result<(), String> {
        let dest = dest_dir();
        let exe = dest.join("AllBlack11.exe");
        report("Restoring Windows settings…");
        if exe.is_file() {
            // Quits the app, puts back the shell values and CaptionHeight, keeps the config.
            let _ = Command::new(&exe).args(["--restore", "--quiet"]).creation_flags(NO_WINDOW).status();
        }
        quiet("taskkill", &["/F", "/IM", "AllBlack11.exe"]);

        report("Removing registrations…");
        if let Ok(run) = Key::create(RUN_KEY) {
            run.delete_value("AllBlack11");
        }
        let _ = std::fs::remove_file(shortcut_path());
        let sub = wide(UNINSTALL_KEY);
        unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, sub.as_ptr()) };

        if purge {
            report("Deleting your settings…");
            let _ = std::fs::remove_dir_all(data_dir());
        }

        report("Deleting files…");
        let me = std::env::current_exe().ok();
        let running_from_dest = me.as_ref().is_some_and(|me| me.starts_with(&dest));
        if running_from_dest {
            // Setup is inside the folder it is deleting: let a helper finish once it exits.
            let command = format!("ping -n 3 127.0.0.1 >nul & rmdir /s /q \"{}\"", dest.display());
            let _ = Command::new("cmd").args(["/C", &command]).creation_flags(NO_WINDOW | DETACHED).spawn();
        } else if dest.exists() {
            std::fs::remove_dir_all(&dest).map_err(|err| format!("{}: {err}", dest.display()))?;
        }
        Ok(())
    }

    // ---- entry point --------------------------------------------------------

    pub fn run() -> i32 {
        unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        let args: Vec<String> = std::env::args().skip(1).collect();
        let has = |flag: &str| args.iter().any(|arg| arg.eq_ignore_ascii_case(flag));
        let silent = has("/S") || has("--silent");

        if has("--version") {
            message(null_mut(), "AllBlack11 Setup", &format!("AllBlack11 {VERSION} ({})", machine_name(BUILD_MACHINE)), MB_OK | MB_ICONINFORMATION);
            return 0;
        }
        if has("--uninstall") {
            return run_uninstall(silent, has("/PURGE"));
        }
        if silent {
            let options = Options {
                autostart: !has("/NOAUTOSTART"),
                shortcut: !has("/NOSHORTCUT"),
                launch: !has("/NOLAUNCH"),
            };
            return match install(options, &mut |_| {}) {
                Ok(()) => 0,
                Err(err) => {
                    message(null_mut(), "AllBlack11 Setup", &err, MB_OK | MB_ICONERROR);
                    1
                }
            };
        }
        wizard()
    }

    fn run_uninstall(silent: bool, purge_flag: bool) -> i32 {
        let mut purge = purge_flag;
        if !silent {
            let sure = message(null_mut(), "Uninstall AllBlack11", "Remove AllBlack11 and put back the Windows settings it changed?", MB_YESNO | MB_ICONQUESTION);
            if sure != IDYES {
                return 0;
            }
            purge = message(
                null_mut(),
                "Uninstall AllBlack11",
                "Also delete your settings, rules and backups in %APPDATA%\\AllBlack11?\n\nChoose No to keep them for a future install.",
                MB_YESNO | MB_ICONQUESTION,
            ) == IDYES;
        }
        match uninstall(purge, &mut |_| {}) {
            Ok(()) => {
                if !silent {
                    message(null_mut(), "AllBlack11", "AllBlack11 was removed. If you changed the title bar height, sign out once to get the old height back.", MB_OK | MB_ICONINFORMATION);
                }
                0
            }
            Err(err) => {
                if !silent {
                    message(null_mut(), "AllBlack11", &format!("Uninstall did not finish: {err}"), MB_OK | MB_ICONERROR);
                }
                1
            }
        }
    }

    // ---- wizard window --------------------------------------------------------

    const ID_INSTALL: usize = 1;
    const ID_UNINSTALL: usize = 2;
    const ID_CLOSE: usize = 3;
    const ID_AUTOSTART: usize = 10;
    const ID_SHORTCUT: usize = 11;
    const ID_LAUNCH: usize = 12;
    const BM_GETCHECK: u32 = 0x00F0;
    const BM_SETCHECK: u32 = 0x00F1;
    const BS_AUTOCHECKBOX: u32 = 0x0003;
    const BS_DEFPUSHBUTTON: u32 = 0x0001;
    const SS_LEFT: u32 = 0x0000;
    const HEADER_H: i32 = 104;
    const W: i32 = 520;
    const H: i32 = 430;

    struct Wizard {
        font: HFONT,
        title_font: HFONT,
        small_font: HFONT,
        white: windows_sys::Win32::Graphics::Gdi::HBRUSH,
        black: windows_sys::Win32::Graphics::Gdi::HBRUSH,
        status: HWND,
        install_button: HWND,
        uninstall_button: HWND,
        close_button: HWND,
        boxes: [HWND; 3],
        k: f32,
        updating: bool,
        done: bool,
    }

    fn wizard() -> i32 {
        unsafe {
            let instance = GetModuleHandleW(null());
            let class = wide("AllBlack11.Setup");
            let mut info: WNDCLASSEXW = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
            info.lpfnWndProc = Some(wndproc);
            info.hInstance = instance;
            info.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
            info.hbrBackground = GetSysColorBrush(COLOR_WINDOW);
            info.lpszClassName = class.as_ptr();
            // Class icon comes from resource 1 (the same icon as the app).
            info.hIcon = windows_sys::Win32::UI::WindowsAndMessaging::LoadIconW(instance, 1 as *const u16);
            RegisterClassExW(&info);
            let title = wide(&format!("AllBlack11 {} Setup", display_version()));
            let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
            let hwnd = CreateWindowExW(0, class.as_ptr(), title.as_ptr(), style, 100, 100, 600, 500, null_mut(), null_mut(), instance, null());
            if hwnd.is_null() {
                message(null_mut(), "AllBlack11 Setup", "Could not open the Setup window.", MB_OK | MB_ICONERROR);
                return 1;
            }
            let dpi = GetDpiForWindow(hwnd).max(96);
            let k = dpi as f32 / 96.0;
            let mut rect = RECT { left: 0, top: 0, right: (W as f32 * k) as i32, bottom: (H as f32 * k) as i32 };
            AdjustWindowRectExForDpi(&mut rect, style, 0, 0, dpi);
            let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
            let screen_w = windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(0);
            let screen_h = windows_sys::Win32::UI::WindowsAndMessaging::GetSystemMetrics(1);
            SetWindowPos(hwnd, null_mut(), (screen_w - width) / 2, ((screen_h - height) / 2).max(0), width, height, SWP_NOZORDER);
            ShowWindow(hwnd, SW_SHOW);
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            0
        }
    }

    fn font(dpi: u32, px: i32, weight: i32) -> HFONT {
        let name = wide("Segoe UI");
        unsafe { CreateFontW(-((px * dpi as i32 + 48) / 96), 0, 0, 0, weight, 0, 0, 0, 1, 0, 0, 5, 0, name.as_ptr()) }
    }

    unsafe fn child(parent: HWND, class: &str, text: &str, style: u32, x: i32, y: i32, w: i32, h: i32, id: usize, font: HFONT, k: f32) -> HWND {
        let (class, text) = (wide(class), wide(text));
        let px = |v: i32| (v as f32 * k).round() as i32;
        let hwnd = CreateWindowExW(0, class.as_ptr(), text.as_ptr(), WS_CHILD | WS_VISIBLE | style, px(x), px(y), px(w), px(h), parent, id as *mut _, GetModuleHandleW(null()), null());
        SendMessageW(hwnd, WM_SETFONT, font as WPARAM, 1);
        hwnd
    }

    fn set_text(hwnd: HWND, text: &str) {
        let wide = wide(text);
        unsafe { SetWindowTextW(hwnd, wide.as_ptr()) };
    }

    unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if msg == WM_CREATE {
            let _ = &*(lparam as *const CREATESTRUCTW);
            let dpi = GetDpiForWindow(hwnd).max(96);
            let k = dpi as f32 / 96.0;
            let body = font(dpi, 15, 400);
            let small = font(dpi, 13, 400);
            let big = font(dpi, 26, 600);
            let updating = installed();
            let machine = host_machine();
            let mismatch = BUILD_MACHINE == MACHINE_AMD64 && machine == MACHINE_ARM64;
            let wrong = BUILD_MACHINE == MACHINE_ARM64 && machine != MACHINE_ARM64 && machine != 0;
            let cpu = if wrong {
                format!("This is the ARM64 installer, but this PC is {}. Download the x64 setup instead.", machine_name(machine))
            } else if mismatch {
                "This is the x64 installer on an ARM64 PC. It works through emulation. The ARM64 setup is faster and lighter.".to_string()
            } else {
                format!("Native {} build for this {} PC.", machine_name(BUILD_MACHINE), machine_name(machine))
            };
            child(hwnd, "STATIC", &format!("Installs for your account only, in {}. No administrator rights.", dest_dir().display()), SS_LEFT, 28, HEADER_H + 16, W - 56, 36, 0, small, k);
            child(hwnd, "STATIC", &cpu, SS_LEFT, 28, HEADER_H + 56, W - 56, 34, 0, small, k);
            let boxes = [
                child(hwnd, "BUTTON", "Start AllBlack11 when I sign in", WS_TABSTOP | BS_AUTOCHECKBOX, 28, HEADER_H + 106, W - 56, 24, ID_AUTOSTART, body, k),
                child(hwnd, "BUTTON", "Add a Start menu shortcut", WS_TABSTOP | BS_AUTOCHECKBOX, 28, HEADER_H + 138, W - 56, 24, ID_SHORTCUT, body, k),
                child(hwnd, "BUTTON", "Open AllBlack11 when Setup finishes", WS_TABSTOP | BS_AUTOCHECKBOX, 28, HEADER_H + 170, W - 56, 24, ID_LAUNCH, body, k),
            ];
            for check in boxes {
                SendMessageW(check, BM_SETCHECK, 1, 0);
            }
            child(hwnd, "STATIC", "Nothing is changed in Windows until you turn a feature on in Settings. Everything it changes can be undone from the same place, or by uninstalling.", SS_LEFT, 28, HEADER_H + 210, W - 56, 50, 0, small, k);
            let status = child(hwnd, "STATIC", "", SS_LEFT, 28, H - 94, W - 56, 22, 0, small, k);
            let install_button = child(hwnd, "BUTTON", if updating { "Update" } else { "Install" }, WS_TABSTOP | BS_DEFPUSHBUTTON, W - 28 - 110 - 96 - 12 - 110, H - 56, 110, 32, ID_INSTALL, body, k);
            let uninstall_button = child(hwnd, "BUTTON", "Uninstall", WS_TABSTOP, W - 28 - 96 - 12 - 110, H - 56, 110, 32, ID_UNINSTALL, body, k);
            let close_button = child(hwnd, "BUTTON", "Close", WS_TABSTOP, W - 28 - 96, H - 56, 96, 32, ID_CLOSE, body, k);
            if !updating {
                windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow(uninstall_button, 0);
            }
            let state = Box::new(Wizard {
                font: body,
                title_font: big,
                small_font: small,
                white: GetSysColorBrush(COLOR_WINDOW),
                black: CreateSolidBrush(0),
                status,
                install_button,
                uninstall_button,
                close_button,
                boxes,
                k,
                updating,
                done: false,
            });
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
            return 0;
        }
        let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Wizard;
        if state.is_null() {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let state = &mut *state;
        match msg {
            WM_ERASEBKGND => {
                let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                GetClientRect(hwnd, &mut rect);
                FillRect(wparam as HDC, &rect, state.white);
                1
            }
            WM_PAINT => {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let dc = BeginPaint(hwnd, &mut ps);
                let mut client = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                GetClientRect(hwnd, &mut client);
                let header = RECT { left: 0, top: 0, right: client.right, bottom: (HEADER_H as f32 * state.k) as i32 };
                FillRect(dc, &header, state.black);
                let teal = CreateSolidBrush(0x0074_8501);
                let line = RECT { left: 0, top: header.bottom - (3.0 * state.k) as i32, right: client.right, bottom: header.bottom };
                FillRect(dc, &line, teal);
                DeleteObject(teal);
                SetBkMode(dc, TRANSPARENT as i32);
                let draw = |text: &str, font: HFONT, color: u32, left: i32, top: i32, bottom: i32| {
                    windows_sys::Win32::Graphics::Gdi::SelectObject(dc, font);
                    SetTextColor(dc, color);
                    let mut rect = RECT { left: (left as f32 * state.k) as i32, top: (top as f32 * state.k) as i32, right: client.right, bottom: (bottom as f32 * state.k) as i32 };
                    let text = wide(text);
                    DrawTextW(dc, text.as_ptr(), -1, &mut rect, 0x0020 | 0x0800);
                };
                draw("AllBlack11", state.title_font, 0x00FF_FFFF, 28, 18, 58);
                draw(&format!("{} · true black for Windows 11", display_version()), state.small_font, 0x00B0_B0B0, 30, 60, 84);
                EndPaint(hwnd, &ps);
                0
            }
            WM_CTLCOLORSTATIC => {
                SetBkMode(wparam as HDC, TRANSPARENT as i32);
                state.white as LRESULT
            }
            WM_COMMAND => {
                let id = wparam & 0xFFFF;
                match id {
                    ID_CLOSE => {
                        DestroyWindow(hwnd);
                    }
                    ID_INSTALL if !state.done => {
                        let options = Options {
                            autostart: SendMessageW(state.boxes[0], BM_GETCHECK, 0, 0) == 1,
                            shortcut: SendMessageW(state.boxes[1], BM_GETCHECK, 0, 0) == 1,
                            launch: SendMessageW(state.boxes[2], BM_GETCHECK, 0, 0) == 1,
                        };
                        busy(state, hwnd, true);
                        let status = state.status;
                        let result = install(options, &mut |text| {
                            set_text(status, text);
                            InvalidateRect(status, null(), 1);
                            windows_sys::Win32::Graphics::Gdi::UpdateWindow(status);
                        });
                        match result {
                            Ok(()) => {
                                state.done = true;
                                set_text(state.status, &format!("{} complete. You can close Setup.", if state.updating { "Update" } else { "Install" }));
                                set_text(state.install_button, "Done");
                                windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow(state.install_button, 0);
                                windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow(state.uninstall_button, 1);
                                windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow(state.close_button, 1);
                            }
                            Err(err) => {
                                set_text(state.status, "Setup did not finish.");
                                message(hwnd, "AllBlack11 Setup", &format!("Setup did not finish.\n\n{err}"), MB_OK | MB_ICONERROR);
                                busy(state, hwnd, false);
                            }
                        }
                    }
                    ID_UNINSTALL => {
                        busy(state, hwnd, true);
                        let code = run_uninstall(false, false);
                        if code == 0 && !installed() {
                            DestroyWindow(hwnd);
                        } else {
                            busy(state, hwnd, false);
                        }
                    }
                    _ => {}
                }
                0
            }
            WM_DESTROY => {
                for font in [state.font, state.title_font, state.small_font] {
                    DeleteObject(font);
                }
                DeleteObject(state.black);
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    unsafe fn busy(state: &Wizard, _hwnd: HWND, working: bool) {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
        let enabled = i32::from(!working);
        EnableWindow(state.install_button, enabled);
        EnableWindow(state.uninstall_button, enabled);
        EnableWindow(state.close_button, enabled);
        for check in state.boxes {
            EnableWindow(check, enabled);
        }
    }
}
