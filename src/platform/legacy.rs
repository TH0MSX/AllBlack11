//! Legacy dialogs.
//!
//! Without injection, AllBlack11 still asks each dialog and its children to use
//! `DarkMode_Explorer` / `DarkMode_CFD`. That call is cross-process and often
//! ignored; the dark caption from DWM is the part that reliably sticks.
//!
//! Full background and text recoloring needs a same-architecture hook DLL. It is
//! loaded only while `experimentalDarkLegacy` is on, and only into processes
//! `inject_allowed` accepts.

use std::collections::HashMap;
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{CloseHandle, FreeLibrary, BOOL, HANDLE, HMODULE, HWND, LPARAM, WPARAM};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::System::Memory::{
    CreateFileMappingW, MapViewOfFile, UnmapViewOfFile, FILE_MAP_ALL_ACCESS, MEMORY_MAPPED_VIEW_ADDRESS,
    PAGE_READWRITE,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Controls::SetWindowTheme;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, GetClassNameW, GetWindowTextW, GetWindowThreadProcessId, SendMessageTimeoutW,
    SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, HWND_BROADCAST, SMTO_ABORTIFHUNG, WH_CALLWNDPROC,
    WM_NULL,
};

use crate::config::{exe_matches, file_name};
use crate::inject::{inject_allowed, InjectTarget};

use super::wide::{exe_path, pcw, to_wide};

const MAGIC: u32 = 0xAB11_0001;
const IMAGE_FILE_MACHINE_UNKNOWN: u16 = 0;
#[cfg_attr(not(target_arch = "x86_64"), allow(dead_code))]
const IMAGE_FILE_MACHINE_AMD64: u16 = 0x8664;
#[cfg_attr(not(target_arch = "aarch64"), allow(dead_code))]
const IMAGE_FILE_MACHINE_ARM64: u16 = 0xAA64;

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

enum Gate {
    Deny,
    Open,
}

struct Live {
    module: HMODULE,
    shared: *mut SharedState,
    mapping: HANDLE,
    hooks: HashMap<u32, HHOOK>,
    gates: HashMap<u32, Gate>,
}

unsafe impl Send for Live {}

static LIVE: Mutex<Option<Live>> = Mutex::new(None);

pub fn set_active(active: bool) -> Result<(), String> {
    if active {
        ensure_loaded()?;
        if let Some(live) = LIVE.lock().unwrap_or_else(|err| err.into_inner()).as_mut() {
            unsafe {
                if !live.shared.is_null() {
                    (*live.shared).enabled = 1;
                }
            }
        }
        Ok(())
    } else {
        stop();
        Ok(())
    }
}

pub fn consider(hwnd: HWND, exclude: &[String]) {
    let Some(mut guard) = LIVE.lock().ok() else {
        return;
    };
    let Some(live) = guard.as_mut() else {
        return;
    };
    if unsafe { live.shared.is_null() || (*live.shared).enabled == 0 } {
        return;
    }
    if hwnd.is_null() || live.hooks.len() >= 48 {
        return;
    }
    let mut pid = 0u32;
    let thread = unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if thread == 0 || pid == 0 || live.hooks.contains_key(&thread) {
        return;
    }
    if let Some(Gate::Deny) = live.gates.get(&pid) {
        return;
    }
    let class = class_name(hwnd);
    let title = window_title(hwnd);
    // Explorer-hosted Control Panel is deliberately not a candidate. Its address band and
    // toolbar are common controls that turn into black blocks when themed dark over a light
    // DirectUI page, and the page itself cannot be recolored from here.
    let control_panel = false;
    let _ = &title;
    let candidate = class == "#32770" || class.eq_ignore_ascii_case("MMCMainFrame");
    if !candidate {
        return;
    }
    let decision = match process_gate(pid, &class, control_panel, exclude) {
        Ok(()) => Gate::Open,
        Err(_) => Gate::Deny,
    };
    if matches!(decision, Gate::Deny) {
        live.gates.insert(pid, Gate::Deny);
        return;
    }
    live.gates.insert(pid, Gate::Open);
    let module = live.module;
    let proc = unsafe { hook_proc(module) };
    let Some(proc) = proc else {
        return;
    };
    let hook = unsafe { SetWindowsHookExW(WH_CALLWNDPROC, Some(proc), module, thread) };
    if hook.is_null() {
        super::wide::log_line("warn", "SetWindowsHookEx failed for a legacy window");
        return;
    }
    live.hooks.insert(thread, hook);
    drop(guard);
    poke(hwnd);
}

pub fn nudge(hwnd: HWND) {
    if hwnd.is_null() {
        return;
    }
    // Classic dialogs only. Never Explorer windows, and never MMC frames: their toolbars and
    // address bands break when themed from outside the process.
    let class = class_name(hwnd);
    if class != "#32770" {
        return;
    }
    unsafe {
        apply_theme(hwnd, &class);
        EnumChildWindows(hwnd, Some(nudge_child), 0);
    }
}

pub fn shutdown() {
    stop();
    let mut guard = LIVE.lock().unwrap_or_else(|err| err.into_inner());
    if let Some(live) = guard.take() {
        unsafe {
            if !live.shared.is_null() {
                UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                    Value: live.shared as *mut _,
                });
            }
            if !live.mapping.is_null() {
                CloseHandle(live.mapping);
            }
            if !live.module.is_null() {
                FreeLibrary(live.module);
            }
        }
    }
}

fn stop() {
    let mut guard = LIVE.lock().unwrap_or_else(|err| err.into_inner());
    let Some(live) = guard.as_mut() else {
        return;
    };
    unsafe {
        if !live.shared.is_null() {
            (*live.shared).enabled = 0;
        }
        let name = to_wide("AllBlack11.HookStop");
        let message = windows_sys::Win32::UI::WindowsAndMessaging::RegisterWindowMessageW(pcw(&name));
        if message != 0 {
            let mut result = 0usize;
            SendMessageTimeoutW(
                HWND_BROADCAST,
                message,
                0,
                0,
                SMTO_ABORTIFHUNG,
                300,
                &mut result,
            );
        }
        for hook in live.hooks.drain() {
            UnhookWindowsHookEx(hook.1);
        }
        live.gates.clear();
    }
}

fn ensure_loaded() -> Result<(), String> {
    let mut guard = LIVE.lock().unwrap_or_else(|err| err.into_inner());
    if guard.is_some() {
        return Ok(());
    }
    let module = load_hook_module()?;
    let mapping = create_mapping()?;
    let view = unsafe {
        MapViewOfFile(
            mapping,
            FILE_MAP_ALL_ACCESS,
            0,
            0,
            std::mem::size_of::<SharedState>(),
        )
    };
    if view.Value.is_null() {
        unsafe {
            CloseHandle(mapping);
            FreeLibrary(module);
        }
        return Err("could not map the hook state".into());
    }
    unsafe {
        let state = &mut *(view.Value as *mut SharedState);
        *state = SharedState {
            magic: MAGIC,
            enabled: 1,
            bg: 0x001C_1C1C,
            text: 0x00F2_F2F2,
            link: 0x00FF_B56E,
            heading: 0x00FF_FFFF,
            edit_bg: 0x002A_2A2A,
        };
    }
    *guard = Some(Live {
        module,
        shared: view.Value as *mut SharedState,
        mapping,
        hooks: HashMap::new(),
        gates: HashMap::new(),
    });
    Ok(())
}

fn load_hook_module() -> Result<HMODULE, String> {
    let Some(exe) = exe_path() else {
        return Err("could not find AllBlack11.exe".into());
    };
    let dir = std::path::Path::new(&exe)
        .parent()
        .ok_or_else(|| "could not find the program folder".to_string())?;
    let dll = dir.join("AllBlack11Hook.dll");
    if !dll.exists() {
        return Err("AllBlack11Hook.dll is not next to AllBlack11.exe".into());
    }
    let wide = to_wide(&dll.display().to_string());
    let module = unsafe { LoadLibraryW(pcw(&wide)) };
    if module.is_null() {
        Err("could not load AllBlack11Hook.dll".into())
    } else {
        Ok(module)
    }
}

fn create_mapping() -> Result<HANDLE, String> {
    let name = to_wide("Local\\AllBlack11.Hook");
    let handle = unsafe {
        CreateFileMappingW(
            windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE,
            std::ptr::null(),
            PAGE_READWRITE,
            0,
            std::mem::size_of::<SharedState>() as u32,
            pcw(&name),
        )
    };
    if handle.is_null() {
        Err("could not create the hook mapping".into())
    } else {
        Ok(handle)
    }
}

unsafe fn hook_proc(
    module: HMODULE,
) -> Option<unsafe extern "system" fn(i32, WPARAM, LPARAM) -> isize> {
    let name = b"AllBlack11HookProc\0";
    let address = GetProcAddress(module, name.as_ptr())?;
    Some(std::mem::transmute::<
        unsafe extern "system" fn() -> isize,
        unsafe extern "system" fn(i32, WPARAM, LPARAM) -> isize,
    >(address))
}

fn process_gate(pid: u32, class_name: &str, control_panel: bool, exclude: &[String]) -> Result<(), &'static str> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return inject_allowed(&InjectTarget {
                exe: "",
                class_name,
                excluded: false,
                elevated: false,
                protected: true,
                same_arch: false,
                control_panel,
            });
        }
        let elevated = token_is_elevated(handle);
        let same_arch = process_machine(handle).is_some_and(|machine| machine == our_machine());
        let exe = process_exe(handle).unwrap_or_default();
        CloseHandle(handle);
        let excluded = exclude.iter().any(|item| exe_matches(item, &exe));
        inject_allowed(&InjectTarget {
            exe: &exe,
            class_name,
            excluded,
            elevated,
            protected: false,
            same_arch,
            control_panel,
        })
    }
}

fn process_exe(handle: HANDLE) -> Option<String> {
    use windows_sys::Win32::System::Threading::{QueryFullProcessImageNameW, PROCESS_NAME_WIN32};
    let mut buf = [0u16; 520];
    let mut len = buf.len() as u32;
    let ok = unsafe {
        QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len)
    };
    if ok == 0 || len == 0 {
        return None;
    }
    let full = String::from_utf16_lossy(&buf[..len as usize]);
    Some(file_name(&full).to_string())
}

unsafe fn token_is_elevated(process: HANDLE) -> bool {
    use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    let mut token = std::ptr::null_mut();
    if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
        return true;
    }
    let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let mut written = 0u32;
    let ok = GetTokenInformation(
        token,
        TokenElevation,
        &mut elevation as *mut TOKEN_ELEVATION as *mut _,
        std::mem::size_of::<TOKEN_ELEVATION>() as u32,
        &mut written,
    );
    CloseHandle(token);
    ok == 0 || elevation.TokenIsElevated != 0
}

unsafe fn process_machine(process: HANDLE) -> Option<u16> {
    use windows_sys::Win32::System::Threading::IsWow64Process2;
    let mut process_machine = 0u16;
    let mut native_machine = 0u16;
    if IsWow64Process2(process, &mut process_machine, &mut native_machine) == 0 {
        return None;
    }
    if process_machine == IMAGE_FILE_MACHINE_UNKNOWN {
        Some(native_machine)
    } else {
        Some(process_machine)
    }
}

fn our_machine() -> u16 {
    #[cfg(target_arch = "aarch64")]
    {
        IMAGE_FILE_MACHINE_ARM64
    }
    #[cfg(target_arch = "x86_64")]
    {
        IMAGE_FILE_MACHINE_AMD64
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        0
    }
}

fn poke(hwnd: HWND) {
    unsafe {
        let mut result = 0usize;
        SendMessageTimeoutW(hwnd, WM_NULL, 0, 0, SMTO_ABORTIFHUNG, 100, &mut result);
    }
}

unsafe extern "system" fn nudge_child(hwnd: HWND, _: LPARAM) -> BOOL {
    let class = class_name(hwnd);
    apply_theme(hwnd, &class);
    1
}

unsafe fn apply_theme(hwnd: HWND, class: &str) {
    let name = if class == "#32770" || class == "Edit" || class == "ComboBox" {
        "DarkMode_CFD"
    } else {
        "DarkMode_Explorer"
    };
    let wide = to_wide(name);
    SetWindowTheme(hwnd, pcw(&wide), std::ptr::null());
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 64];
    let length = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if length <= 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&buf[..length as usize])
    }
}

fn window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 160];
    let length = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if length <= 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&buf[..length as usize])
    }
}
