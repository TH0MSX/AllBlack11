//! AllBlack11 x64 helper.
//!
//! Runs only on an ARM64 PC, started by the ARM64 AllBlack11.exe when an emulated x64 classic
//! dialog qualifies. It exists because a hook DLL must match the target's CPU type, and an
//! ARM64 process cannot load an x64 DLL into an x64-emulated one.
//!
//! What it will do is deliberately tiny: install and remove a `WH_CALLWNDPROCRET` hook (with
//! the x64 copy of the hook DLL that sits next to it) on a thread it has itself re-validated.
//! It takes four commands, over a pipe only the same user can open, from only its parent:
//!
//!   HOOK <thread id>    PRUNE    STOPALL    QUIT
//!
//! (parsed by `helper_protocol`, which the controller compiles too)
//!
//! There is no command to load another file, run a program, or hook a thread that does not
//! own a classic dialog in an eligible x64 process. Built with `panic = "abort"`; every
//! fallible step returns early.
//!
//! Evidence note: cross-compiled only. It has not been run on hardware.

#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(windows))]
fn main() {
    eprintln!("The AllBlack11 helper is a Windows program.");
    std::process::exit(1);
}

#[cfg(windows)]
#[path = "../../src/arch.rs"]
#[allow(dead_code)]
mod arch;
#[cfg(windows)]
#[path = "../../src/helper_protocol.rs"]
#[allow(dead_code)]
mod protocol;
#[cfg(windows)]
#[path = "../../src/legacy_policy.rs"]
#[allow(dead_code)]
mod policy;
#[cfg(windows)]
#[path = "../../src/target.rs"]
#[allow(dead_code)]
mod target;

#[cfg(windows)]
fn main() {
    std::process::exit(win::run());
}

#[cfg(windows)]
mod win {
    use std::collections::HashMap;
    use std::ptr::{null, null_mut};

    use windows_sys::Win32::Foundation::{
        CloseHandle, LocalFree, BOOL, FALSE, FILETIME, HANDLE, HMODULE, HWND, INVALID_HANDLE_VALUE, LPARAM,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, SECURITY_ATTRIBUTES, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::Storage::FileSystem::{ReadFile, WriteFile, PIPE_ACCESS_DUPLEX};
    use windows_sys::Win32::System::LibraryLoader::{
        GetModuleFileNameW, GetProcAddress, LoadLibraryW,
    };
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeClientProcessId, PIPE_READMODE_BYTE,
        PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
    };
    use windows_sys::Win32::System::Threading::{
        ExitProcess, GetProcessIdOfThread, GetProcessTimes, IsWow64Process2, OpenProcess, OpenProcessToken,
        OpenThread, QueryFullProcessImageNameW, WaitForSingleObject, INFINITE, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, THREAD_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumThreadWindows, GetClassNameW, RegisterWindowMessageW, SendMessageTimeoutW,
        SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, HWND_BROADCAST, SMTO_ABORTIFHUNG,
        WH_CALLWNDPROCRET,
    };

    use crate::arch::MACHINE_AMD64;
    use crate::policy::{host_kind, HostKind};
    use crate::protocol::{parse_command, Command, LineBuffer, Reply};
    use crate::target::{classify_target, pe_machine, route, Route};

    const FILE_FLAG_FIRST_PIPE_INSTANCE: u32 = 0x0008_0000;

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn run() -> i32 {
        let args: Vec<String> = std::env::args().collect();
        let value = |flag: &str| {
            args.iter()
                .position(|arg| arg == flag)
                .and_then(|index| args.get(index + 1))
                .cloned()
        };
        let (Some(parent), Some(pipe), Some(parent_created)) = (
            value("--parent").and_then(|text| text.parse::<u32>().ok()),
            value("--pipe"),
            value("--parent-created").and_then(|text| text.parse::<u64>().ok()),
        ) else {
            return 2;
        };
        // The pipe name must be ours. Anything else is refused before it is used.
        if !pipe.starts_with("\\\\.\\pipe\\AllBlack11.Helper.") || pipe.len() > 120 {
            return 2;
        }
        unsafe {
            let parent_handle = OpenProcess(
                PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                0,
                parent,
            );
            if parent_handle.is_null() {
                return 3;
            }
            // The id alone could name a different process by now. Its start time cannot.
            if creation_time(parent_handle) != Some(parent_created) {
                return 7;
            }
            // Die with the parent. A blocking wait, so it costs nothing while idle.
            let watch = parent_handle as isize;
            std::thread::spawn(move || {
                WaitForSingleObject(watch as HANDLE, INFINITE);
                ExitProcess(0);
            });
            let Some(server) = create_pipe(&pipe) else {
                return 4;
            };
            if ConnectNamedPipe(server, null_mut()) == 0
                && windows_sys::Win32::Foundation::GetLastError() != 535
            {
                // 535 = ERROR_PIPE_CONNECTED: the client got there first, which is fine.
                return 5;
            }
            let mut client_pid = 0u32;
            if GetNamedPipeClientProcessId(server, &mut client_pid) == 0 || client_pid != parent {
                return 6; // Only our parent may talk to us.
            }
            serve(server);
            CloseHandle(server);
        }
        0
    }

    unsafe fn creation_time(process: HANDLE) -> Option<u64> {
        let mut created: FILETIME = std::mem::zeroed();
        let mut exited: FILETIME = std::mem::zeroed();
        let mut kernel: FILETIME = std::mem::zeroed();
        let mut user: FILETIME = std::mem::zeroed();
        if GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user) == 0 {
            return None;
        }
        Some((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
    }

    /// First instance only, this user only, local clients only.
    unsafe fn create_pipe(name: &str) -> Option<HANDLE> {
        // Owner-only DACL: the account that started the helper, nobody else.
        let sddl = wide("D:P(A;;GA;;;OW)");
        let mut descriptor = null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            null_mut(),
        ) == 0
        {
            return None;
        }
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let name = wide(name);
        let handle = CreateNamedPipeW(
            name.as_ptr(),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            512,
            512,
            0,
            &attributes,
        );
        LocalFree(descriptor as _);
        if handle == INVALID_HANDLE_VALUE {
            None
        } else {
            Some(handle)
        }
    }

    unsafe fn serve(pipe: HANDLE) {
        let mut hooks: HashMap<u32, HHOOK> = HashMap::new();
        let mut module: Option<HMODULE> = None;
        let mut lines = LineBuffer::default();
        let mut buffer = [0u8; 128];
        loop {
            let mut read = 0u32;
            if ReadFile(pipe, buffer.as_mut_ptr(), buffer.len() as u32, &mut read, null_mut()) == 0 || read == 0 {
                break; // The controller went away.
            }
            let Ok(batch) = lines.push(&buffer[..read as usize]) else {
                break; // Not a protocol we speak: drop the connection.
            };
            for line in batch {
                let (reply, quit) = handle(&line, &mut hooks, &mut module);
                let mut out = reply.to_line().into_bytes();
                out.push(b'\n');
                let mut written = 0u32;
                WriteFile(pipe, out.as_ptr(), out.len() as u32, &mut written, null_mut());
                if quit {
                    return;
                }
            }
        }
        // Disconnected or hostile: leave nothing hooked behind.
        stop_all(&mut hooks);
    }

    unsafe fn handle(
        line: &str,
        hooks: &mut HashMap<u32, HHOOK>,
        module: &mut Option<HMODULE>,
    ) -> (Reply, bool) {
        match parse_command(line) {
            Ok(Command::Hook(thread)) => (hook(thread, hooks, module), false),
            Ok(Command::Prune) => {
                prune(hooks);
                (Reply::Hooks(hooks.len()), false)
            }
            Ok(Command::StopAll) => {
                stop_all(hooks);
                (Reply::Ok, false)
            }
            Ok(Command::Quit) => {
                stop_all(hooks);
                (Reply::Ok, true)
            }
            Err(reason) => (Reply::Err(reason.to_string()), false),
        }
    }

    unsafe fn hook(
        thread: u32,
        hooks: &mut HashMap<u32, HHOOK>,
        module: &mut Option<HMODULE>,
    ) -> Reply {
        if hooks.contains_key(&thread) {
            return Reply::Ok;
        }
        if hooks.len() >= 32 {
            return Reply::Err("too-many".into());
        }
        if let Err(reason) = validate(thread) {
            return Reply::Err(reason.to_string());
        }
        let library = match *module {
            Some(existing) => existing,
            None => {
                let Some(path) = hook_path() else {
                    return Reply::Err("no-hook-dll".into());
                };
                let loaded = LoadLibraryW(wide(&path).as_ptr());
                if loaded.is_null() {
                    return Reply::Err("hook-load".into());
                }
                *module = Some(loaded);
                loaded
            }
        };
        let Some(proc) = GetProcAddress(library, b"AllBlack11HookProcRet\0".as_ptr()) else {
            return Reply::Err("hook-export".into());
        };
        let handle = SetWindowsHookExW(
            WH_CALLWNDPROCRET,
            Some(std::mem::transmute::<
                unsafe extern "system" fn() -> isize,
                unsafe extern "system" fn(i32, usize, isize) -> isize,
            >(proc)),
            library,
            thread,
        );
        if handle.is_null() {
            return Reply::Err("hook-failed".into());
        }
        hooks.insert(thread, handle);
        Reply::Ok
    }

    /// The hook DLL is always the x64 copy next to this exe. Never a path from the pipe.
    fn hook_path() -> Option<String> {
        let mut buf = [0u16; 520];
        let len =
            unsafe { GetModuleFileNameW(null_mut(), buf.as_mut_ptr(), buf.len() as u32) } as usize;
        if len == 0 || len >= buf.len() {
            return None;
        }
        let exe = String::from_utf16_lossy(&buf[..len]);
        let dir = std::path::Path::new(&exe).parent()?;
        let dll = dir.join("AllBlack11Hook.x64.dll");
        dll.is_file().then(|| dll.display().to_string())
    }

    /// Re-check the target from scratch. The controller's opinion is not trusted.
    unsafe fn validate(thread: u32) -> Result<(), &'static str> {
        let thread_handle = OpenThread(THREAD_QUERY_LIMITED_INFORMATION, 0, thread);
        if thread_handle.is_null() {
            return Err("no-thread");
        }
        let pid = GetProcessIdOfThread(thread_handle);
        CloseHandle(thread_handle);
        if pid == 0 {
            return Err("no-process");
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return Err("protected");
        }
        let result = (|| {
            if is_elevated(process) {
                return Err("elevated");
            }
            let mut buf = [0u16; 520];
            let mut len = buf.len() as u32;
            if QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len)
                == 0
            {
                return Err("unknown-process");
            }
            let full = String::from_utf16_lossy(&buf[..len as usize]);
            let exe = full.rsplit(['\\', '/']).next().unwrap_or("").to_string();
            if host_kind(&exe) == HostKind::Denied {
                return Err("denied-process");
            }
            let (mut process_machine, mut native) = (0u16, 0u16);
            if IsWow64Process2(process, &mut process_machine, &mut native) == 0 {
                return Err("architecture");
            }
            let image = read_machine(&full);
            // This helper is x64: it may only touch targets an x64 controller could.
            match route(
                MACHINE_AMD64,
                classify_target(native, process_machine, image),
            ) {
                Route::Native => {}
                _ => return Err("architecture"),
            }
            if !owns_dialog(thread) {
                return Err("no-dialog");
            }
            Ok(())
        })();
        CloseHandle(process);
        result
    }

    fn read_machine(path: &str) -> Option<u16> {
        use std::io::Read;
        let mut header = [0u8; 1024];
        let read = std::fs::File::open(path).ok()?.read(&mut header).ok()?;
        pe_machine(&header[..read])
    }

    unsafe fn is_elevated(process: HANDLE) -> bool {
        let mut token = null_mut();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
            return true; // cannot tell: treat as privileged
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

    unsafe fn owns_dialog(thread: u32) -> bool {
        let mut found: BOOL = FALSE;
        EnumThreadWindows(
            thread,
            Some(dialog_probe),
            &mut found as *mut BOOL as LPARAM,
        );
        found != 0
    }

    unsafe extern "system" fn dialog_probe(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let mut buf = [0u16; 16];
        let len = GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if len > 0 && String::from_utf16_lossy(&buf[..len as usize]) == "#32770" {
            *(lparam as *mut BOOL) = 1;
            return 0; // stop
        }
        1
    }

    unsafe fn prune(hooks: &mut HashMap<u32, HHOOK>) {
        let dead: Vec<u32> = hooks
            .keys()
            .copied()
            .filter(|thread| {
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
            if let Some(hook) = hooks.remove(&thread) {
                UnhookWindowsHookEx(hook);
            }
        }
    }

    /// Ask every hooked thread to restore its windows, then unhook. The DLL stays mapped in
    /// those processes (it pinned itself), so this means "styling stopped", not "unloaded".
    unsafe fn stop_all(hooks: &mut HashMap<u32, HHOOK>) {
        let message = RegisterWindowMessageW(wide("AllBlack11.HookStop").as_ptr());
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
        remove_all(hooks);
    }

    unsafe fn remove_all(hooks: &mut HashMap<u32, HHOOK>) {
        for (_, hook) in hooks.drain() {
            UnhookWindowsHookEx(hook);
        }
    }

    #[allow(dead_code)]
    fn _unused() {
        let _ = (null::<u16>(),);
    }
}
