//! Legacy dialogs: the optional, per-window compatibility layer (layer B).
//!
//! Nothing here runs unless `experimentalDarkLegacy` is on. When it is, a small hook DLL of
//! the target's own CPU type is loaded into the UI thread of a *classic dialog* in a process
//! that passed every gate in `inject::inject_allowed`. The DLL treats supported control
//! families through the capability table in `legacy_policy`.
//!
//! Decisions are made by one function, [`assess`], which the hooking path and the window
//! inspector both call, so "why was this not themed?" always has the same answer as "did it
//! get themed?".
//!
//! Lifetime rule: the hook DLL pins itself into every process it enters and never unloads
//! there. Turning the layer off therefore means "styling stopped and subclasses removed",
//! not "module physically unloaded". That is deliberate: unloading a DLL that owns window
//! procedures is how you crash another application.

use std::collections::HashMap;
use std::sync::Mutex;

use windows_sys::Win32::Foundation::{
    CloseHandle, FreeLibrary, BOOL, FILETIME, HANDLE, HMODULE, HWND, LPARAM, WPARAM,
};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::System::Memory::{
    CreateFileMappingW, MapViewOfFile, UnmapViewOfFile, FILE_MAP_ALL_ACCESS,
    MEMORY_MAPPED_VIEW_ADDRESS, PAGE_READWRITE,
};
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, IsWow64Process2, OpenProcess, OpenProcessToken, OpenThread,
    QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    THREAD_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, GetClassNameW, GetWindowThreadProcessId, SendMessageTimeoutW,
    SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, HWND_BROADCAST, SMTO_ABORTIFHUNG,
    WH_CALLWNDPROCRET, WM_NULL,
};

use crate::config::{exe_matches, file_name};
use crate::legacy_policy::{
    host_kind, palette, window_verdict, SharedState, Verdict, FLAG_CLASSIC_BUTTONS,
    FLAG_THEME_PUSH_BUTTONS,
};
use crate::target::{classify_target, pe_machine, Route, TargetArch};
use crate::{inject_allowed, InjectTarget, BUILD_MACHINE};

use super::wide::{exe_path, pcw, to_wide};

/// What the user chose for the legacy layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyOptions {
    pub palette: String,
    /// Allow the packaged x64 helper to treat emulated x64 targets (ARM64 only).
    pub emulated_x64: bool,
}

impl Default for LegacyOptions {
    fn default() -> Self {
        Self {
            palette: "graphite".into(),
            emulated_x64: false,
        }
    }
}

/// Everything known about one window for the legacy layer, and the reason for the verdict.
#[derive(Clone, Debug)]
pub struct Assessment {
    pub arch: TargetArch,
    pub verdict: Verdict,
    /// `Ok(route)`: the window may be treated. `Err(reason)`: stable text for diagnostics.
    pub gate: Result<Route, &'static str>,
}

enum Gate {
    Deny,
    Open,
}

struct Live {
    module: HMODULE,
    shared: *mut SharedState,
    mapping: HANDLE,
    options: LegacyOptions,
    hooks: HashMap<u32, HHOOK>,
    /// Keyed by (pid, process creation time) so a reused PID never inherits a verdict.
    gates: HashMap<(u32, u64), Gate>,
}

unsafe impl Send for Live {}

static LIVE: Mutex<Option<Live>> = Mutex::new(None);

pub fn set_active(active: bool, options: &LegacyOptions) -> Result<(), String> {
    // The x64 helper follows the user's "cover emulated x64 apps" switch exactly: off means
    // it is told to stop and quit now, and nothing new is sent to it.
    super::helper::set_enabled(active && options.emulated_x64);
    if active {
        ensure_loaded(options)?;
        if let Some(live) = LIVE.lock().unwrap_or_else(|err| err.into_inner()).as_mut() {
            live.options = options.clone();
            write_state(live, 1);
        }
        Ok(())
    } else {
        stop();
        Ok(())
    }
}

fn state_flags() -> u32 {
    FLAG_CLASSIC_BUTTONS | FLAG_THEME_PUSH_BUTTONS
}

fn write_state(live: &Live, enabled: u32) {
    if live.shared.is_null() {
        return;
    }
    unsafe {
        let mut state = SharedState::from_palette(palette(&live.options.palette), state_flags());
        state.enabled = 0;
        std::ptr::write_volatile(live.shared, state);
        // `enabled` last, so a reader never sees a half-written palette as enabled.
        std::ptr::write_volatile(&mut (*live.shared).enabled, enabled);
    }
}

/// Assess a window without changing anything.
pub fn assess(hwnd: HWND, exclude: &[String], options: &LegacyOptions) -> Option<Assessment> {
    if hwnd.is_null() {
        return None;
    }
    let mut pid = 0u32;
    let thread = unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if thread == 0 || pid == 0 {
        return None;
    }
    let class = class_name(hwnd);
    let children = child_classes(hwnd);
    let info = query_process(pid);
    let exe = info
        .as_ref()
        .map(|info| info.exe.clone())
        .unwrap_or_default();
    let host = host_kind(&exe);
    let child_refs: Vec<&str> = children.iter().map(String::as_str).collect();
    let verdict = window_verdict(host, &class, &child_refs);
    let (arch, elevated, protected) = match &info {
        Some(info) => (info.arch, info.elevated, false),
        None => (TargetArch::Unknown, false, true),
    };
    let excluded = exclude.iter().any(|item| exe_matches(item, &exe));
    let mut gate = inject_allowed(&InjectTarget {
        exe: &exe,
        class_name: &class,
        excluded,
        elevated,
        protected,
        controller_machine: BUILD_MACHINE,
        arch,
        helper_available: options.emulated_x64 && helper_installed(),
    });
    // A window the policy would leave alone is not "allowed" just because the process is.
    if let (Ok(_), Verdict::Skip(reason)) = (&gate, &verdict) {
        gate = Err(reason);
    }
    Some(Assessment {
        arch,
        verdict,
        gate,
    })
}

/// A new top-level window appeared. Hook its thread when it qualifies.
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
    if hwnd.is_null() {
        return;
    }
    let mut pid = 0u32;
    let thread = unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if thread == 0 || pid == 0 || live.hooks.contains_key(&thread) {
        return;
    }
    // Cheap class check first so ordinary windows never trigger a process query.
    if class_name(hwnd) != "#32770" {
        return;
    }
    let key = (pid, creation_time(pid));
    if let Some(Gate::Deny) = live.gates.get(&key) {
        return;
    }
    let options = live.options.clone();
    drop(guard);
    let Some(assessment) = assess(hwnd, exclude, &options) else {
        return;
    };
    let mut guard = LIVE.lock().unwrap_or_else(|err| err.into_inner());
    let Some(live) = guard.as_mut() else {
        return;
    };
    match assessment.gate {
        Ok(Route::Native) => {
            live.gates.insert(key, Gate::Open);
        }
        Ok(Route::X64Helper) => {
            live.gates.insert(key, Gate::Open);
            drop(guard);
            super::helper::hook_thread(thread, hwnd);
            return;
        }
        Ok(Route::Unsupported(_)) | Err(_) => {
            // Window-level refusals (a file dialog, say) must not condemn the whole process.
            if !matches!(assessment.verdict, Verdict::Skip(_)) || process_level_refusal(&assessment)
            {
                live.gates.insert(key, Gate::Deny);
            }
            return;
        }
    }
    if live.hooks.len() >= 48 {
        prune_dead_threads(live);
        if live.hooks.len() >= 48 {
            return;
        }
    }
    let module = live.module;
    let Some(proc) = (unsafe { hook_proc(module) }) else {
        return;
    };
    // After-the-fact hook: the hook sees each message once the window procedure has handled
    // it, so a control is never poked before its own WM_CREATE has run.
    let hook = unsafe { SetWindowsHookExW(WH_CALLWNDPROCRET, Some(proc), module, thread) };
    if hook.is_null() {
        super::wide::log_line("warn", "SetWindowsHookEx failed for a legacy window");
        return;
    }
    live.hooks.insert(thread, hook);
    drop(guard);
    poke(hwnd);
}

fn process_level_refusal(assessment: &Assessment) -> bool {
    matches!(
        assessment.gate,
        Err("protected")
            | Err("elevated")
            | Err("excluded")
            | Err("unknown-process")
            | Err("arm64ec")
            | Err("x86-process")
            | Err("needs-arm64-build")
            | Err("architecture")
    )
}

fn prune_dead_threads(live: &mut Live) {
    let dead: Vec<u32> = live
        .hooks
        .keys()
        .copied()
        .filter(|thread| unsafe {
            let handle = OpenThread(THREAD_QUERY_LIMITED_INFORMATION, 0, *thread);
            if handle.is_null() {
                true
            } else {
                CloseHandle(handle);
                false
            }
        })
        .collect();
    for thread in dead {
        if let Some(hook) = live.hooks.remove(&thread) {
            unsafe { UnhookWindowsHookEx(hook) };
        }
    }
}

pub fn shutdown() {
    stop();
    super::helper::shutdown();
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
            // Only drops this process's own reference. Each target pinned the DLL itself.
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
    write_state(live, 0);
    super::helper::stop_and_quit();
    unsafe {
        // Each hooked thread removes its own subclasses and restores its own controls when
        // it handles this message, because those calls are only valid on the owning thread.
        let name = to_wide("AllBlack11.HookStop");
        let message =
            windows_sys::Win32::UI::WindowsAndMessaging::RegisterWindowMessageW(pcw(&name));
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
    }
    live.gates.clear();
}

fn ensure_loaded(options: &LegacyOptions) -> Result<(), String> {
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
    let live = Live {
        module,
        shared: view.Value as *mut SharedState,
        mapping,
        options: options.clone(),
        hooks: HashMap::new(),
        gates: HashMap::new(),
    };
    write_state(&live, 1);
    *guard = Some(live);
    Ok(())
}

pub fn hook_dll_path(name: &str) -> Option<std::path::PathBuf> {
    let exe = exe_path()?;
    let dir = std::path::Path::new(&exe).parent()?.to_path_buf();
    Some(dir.join(name))
}

/// Stop the x64 helper and the hooks it holds, without touching native hooks.
pub fn stop_helper() {
    super::helper::stop_and_quit();
}

/// Whether the packaged x64 helper and x64 hook are installed next to the exe.
pub fn helper_installed() -> bool {
    super::helper::installed()
}

fn load_hook_module() -> Result<HMODULE, String> {
    let dll = hook_dll_path("AllBlack11Hook.dll")
        .ok_or_else(|| "could not find the program folder".to_string())?;
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

pub fn mapping_name() -> &'static str {
    "Local\\AllBlack11.Hook"
}

fn create_mapping() -> Result<HANDLE, String> {
    let name = to_wide(mapping_name());
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
    let name = b"AllBlack11HookProcRet\0";
    let address = GetProcAddress(module, name.as_ptr())?;
    Some(std::mem::transmute::<
        unsafe extern "system" fn() -> isize,
        unsafe extern "system" fn(i32, WPARAM, LPARAM) -> isize,
    >(address))
}

// ---------------------------------------------------------------------------
// Process facts
// ---------------------------------------------------------------------------

struct ProcessInfo {
    exe: String,
    arch: TargetArch,
    elevated: bool,
}

fn query_process(pid: u32) -> Option<ProcessInfo> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let elevated = token_is_elevated(handle);
        let (mut process_machine, mut native_machine) = (0u16, 0u16);
        let have_machine = IsWow64Process2(handle, &mut process_machine, &mut native_machine) != 0;
        let (exe, full_path) = process_image(handle).unwrap_or_default();
        CloseHandle(handle);
        let image_machine = if full_path.is_empty() {
            None
        } else {
            read_image_machine(&full_path)
        };
        let arch = if have_machine {
            classify_target(native_machine, process_machine, image_machine)
        } else {
            TargetArch::Unknown
        };
        Some(ProcessInfo {
            exe,
            arch,
            elevated,
        })
    }
}

fn read_image_machine(path: &str) -> Option<u16> {
    use std::io::Read;
    let mut header = [0u8; 1024];
    let mut file = std::fs::File::open(path).ok()?;
    let read = file.read(&mut header).ok()?;
    pe_machine(&header[..read])
}

fn creation_time(pid: u32) -> u64 {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return 0;
        }
        let mut created: FILETIME = std::mem::zeroed();
        let mut exited: FILETIME = std::mem::zeroed();
        let mut kernel: FILETIME = std::mem::zeroed();
        let mut user: FILETIME = std::mem::zeroed();
        let ok = GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user);
        CloseHandle(handle);
        if ok == 0 {
            0
        } else {
            (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime)
        }
    }
}

/// (file name, full path)
fn process_image(handle: HANDLE) -> Option<(String, String)> {
    let mut buf = [0u16; 520];
    let mut len = buf.len() as u32;
    let ok = unsafe {
        QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len)
    };
    if ok == 0 || len == 0 {
        return None;
    }
    let full = String::from_utf16_lossy(&buf[..len as usize]);
    Some((file_name(&full).to_string(), full))
}

unsafe fn token_is_elevated(process: HANDLE) -> bool {
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    let mut token = std::ptr::null_mut();
    if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
        // Cannot read the token: assume privileged and stay out.
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

fn poke(hwnd: HWND) {
    unsafe {
        let mut result = 0usize;
        SendMessageTimeoutW(hwnd, WM_NULL, 0, 0, SMTO_ABORTIFHUNG, 100, &mut result);
    }
}

fn child_classes(hwnd: HWND) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    unsafe {
        EnumChildWindows(
            hwnd,
            Some(collect_class),
            &mut found as *mut Vec<String> as LPARAM,
        );
    }
    found
}

unsafe extern "system" fn collect_class(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let found = &mut *(lparam as *mut Vec<String>);
    if found.len() < 256 {
        found.push(class_name(hwnd));
    }
    1
}

pub fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    let length = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if length <= 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&buf[..length as usize])
    }
}
