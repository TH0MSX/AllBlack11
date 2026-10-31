//! Controller side of the x64 helper (ARM64 builds only; on x64 this is inert).
//!
//! An ARM64 process cannot load an x64 hook DLL into an x64-emulated process, so a small x64
//! helper does the hooking. The helper is packaged next to this exe (never downloaded), is
//! started only when an eligible emulated-x64 dialog appears, and stops when styling is turned
//! off, when this program exits, or after a quiet spell with nothing left to hook.
//!
//! Trust model, stated plainly: the pipe is local-only, first-instance, and owner-only, and
//! each side checks the other's process id. That keeps other *users* and accidents out. It is
//! not a defense against malware running as you, which can already inject into your processes
//! by other means. The helper accepts four commands and re-validates every target itself.
//!
//! Evidence: cross-compiled only. Not run on hardware.

use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, HWND};
use windows_sys::Win32::Security::Cryptography::{
    BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG,
};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, ReadFile, WriteFile, OPEN_EXISTING};
use windows_sys::Win32::System::Pipes::{GetNamedPipeServerProcessId, WaitNamedPipeW};
use windows_sys::Win32::System::Threading::TerminateProcess;

use crate::BUILD_MACHINE;

use super::wide::{log_line, pcw, to_wide};

const HELPER_EXE: &str = "AllBlack11Helper.x64.exe";
const HELPER_HOOK: &str = "AllBlack11Hook.x64.dll";
const IDLE_PRUNE: Duration = Duration::from_secs(90);
const CONNECT_WAIT_MS: u32 = 5000;

enum Cmd {
    Hook(u32),
    StopAll,
    Quit,
}

struct State {
    tx: Option<Sender<Cmd>>,
    hooked: Vec<u32>,
    /// Do not retry a helper that failed to start until this time.
    retry_after: Option<Instant>,
    /// For forced termination when the worker is stuck at shutdown.
    child: Option<usize>,
}

static STATE: Mutex<State> = Mutex::new(State {
    tx: None,
    hooked: Vec::new(),
    retry_after: None,
    child: None,
});

/// The helper and its x64 hook are packaged next to this exe, and this is an ARM64 build.
pub fn installed() -> bool {
    if BUILD_MACHINE != crate::arch::MACHINE_ARM64 {
        return false;
    }
    let present =
        |name: &str| super::legacy::hook_dll_path(name).is_some_and(|path| path.is_file());
    present(HELPER_EXE) && present(HELPER_HOOK)
}

pub fn hook_thread(thread: u32, _hwnd: HWND) {
    if !installed() {
        return;
    }
    let mut state = STATE.lock().unwrap_or_else(|err| err.into_inner());
    if state.hooked.contains(&thread) {
        return;
    }
    if state.tx.is_none() {
        if state
            .retry_after
            .is_some_and(|until| Instant::now() < until)
        {
            return;
        }
        let (tx, rx) = mpsc::channel();
        state.tx = Some(tx);
        std::thread::spawn(move || worker(rx));
    }
    if let Some(tx) = &state.tx {
        if tx.send(Cmd::Hook(thread)).is_ok() {
            state.hooked.push(thread);
        }
    }
}

pub fn stop_all() {
    let mut state = STATE.lock().unwrap_or_else(|err| err.into_inner());
    state.hooked.clear();
    if let Some(tx) = &state.tx {
        let _ = tx.send(Cmd::StopAll);
    }
}

/// Ask the helper to quit, and make sure it is gone shortly after.
pub fn shutdown() {
    let (child, tx) = {
        let mut state = STATE.lock().unwrap_or_else(|err| err.into_inner());
        state.hooked.clear();
        (state.child.take(), state.tx.take())
    };
    if let Some(tx) = tx {
        let _ = tx.send(Cmd::Quit);
        // Give it a moment to unhook cleanly, then end it. Its parent-death watch also
        // ends it if this program disappears first.
        std::thread::sleep(Duration::from_millis(400));
    }
    if let Some(child) = child {
        unsafe {
            TerminateProcess(child as HANDLE, 0);
            CloseHandle(child as HANDLE);
        }
    }
}

struct Link {
    pipe: HANDLE,
}

impl Link {
    fn request(&self, command: &str) -> Option<String> {
        let mut line = command.as_bytes().to_vec();
        line.push(b'\n');
        let mut written = 0u32;
        let ok = unsafe {
            WriteFile(
                self.pipe,
                line.as_ptr(),
                line.len() as u32,
                &mut written,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return None;
        }
        let mut reply = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            let mut read = 0u32;
            let ok = unsafe {
                ReadFile(
                    self.pipe,
                    byte.as_mut_ptr(),
                    1,
                    &mut read,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 || read == 0 {
                return None;
            }
            if byte[0] == b'\n' {
                break;
            }
            reply.push(byte[0]);
            if reply.len() > 256 {
                return None;
            }
        }
        Some(String::from_utf8_lossy(&reply).to_string())
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.pipe) };
    }
}

fn random_hex() -> Option<String> {
    let mut bytes = [0u8; 16];
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            bytes.as_mut_ptr(),
            bytes.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status != 0 {
        return None;
    }
    Some(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn spawn_and_connect() -> Option<(Link, u32)> {
    use std::os::windows::io::IntoRawHandle;
    use std::os::windows::process::CommandExt;
    let exe = super::legacy::hook_dll_path(HELPER_EXE)?;
    let name = format!("\\\\.\\pipe\\AllBlack11.Helper.{}", random_hex()?);
    let child = std::process::Command::new(&exe)
        .args(["--parent", &std::process::id().to_string(), "--pipe", &name])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()
        .ok()?;
    let helper_pid = child.id();
    let process = child.into_raw_handle() as usize;
    STATE.lock().unwrap_or_else(|err| err.into_inner()).child = Some(process);

    let wide = to_wide(&name);
    if unsafe { WaitNamedPipeW(pcw(&wide), CONNECT_WAIT_MS) } == 0 {
        return None;
    }
    let pipe = unsafe {
        CreateFileW(
            pcw(&wide),
            GENERIC_READ | GENERIC_WRITE,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if pipe.is_null() || pipe == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
        return None;
    }
    let link = Link { pipe };
    // The other end must be the process we started, not something that grabbed the name.
    let mut server = 0u32;
    if unsafe { GetNamedPipeServerProcessId(pipe, &mut server) } == 0 || server != helper_pid {
        return None;
    }
    Some((link, helper_pid))
}

fn worker(rx: mpsc::Receiver<Cmd>) {
    let Some((link, pid)) = spawn_and_connect() else {
        log_line(
            "warn",
            "the x64 helper could not be started; emulated x64 dialogs are skipped for now",
        );
        let mut state = STATE.lock().unwrap_or_else(|err| err.into_inner());
        state.tx = None;
        state.hooked.clear();
        state.retry_after = Some(Instant::now() + Duration::from_secs(60));
        if let Some(child) = state.child.take() {
            unsafe {
                TerminateProcess(child as HANDLE, 0);
                CloseHandle(child as HANDLE);
            }
        }
        return;
    };
    log_line("info", &format!("x64 helper started (pid {pid})"));
    loop {
        match rx.recv_timeout(IDLE_PRUNE) {
            Ok(Cmd::Hook(thread)) => {
                if let Some(reply) = link.request(&format!("HOOK {thread}")) {
                    if reply != "OK" {
                        // The helper refused (for instance: elevated, or no dialog). Fine.
                        STATE
                            .lock()
                            .unwrap_or_else(|err| err.into_inner())
                            .hooked
                            .retain(|t| *t != thread);
                    }
                } else {
                    break;
                }
            }
            Ok(Cmd::StopAll) => {
                if link.request("STOPALL").is_none() {
                    break;
                }
            }
            Ok(Cmd::Quit) => {
                let _ = link.request("QUIT");
                break;
            }
            Err(RecvTimeoutError::Timeout) => {
                // Quiet for a while: let the helper drop hooks for dialogs that closed, and
                // leave entirely if nothing is left to hook.
                match link.request("PRUNE").as_deref() {
                    Some("HOOKS 0") => {
                        let _ = link.request("QUIT");
                        break;
                    }
                    Some(_) => {}
                    None => break,
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    let mut state = STATE.lock().unwrap_or_else(|err| err.into_inner());
    state.tx = None;
    state.hooked.clear();
    if let Some(child) = state.child.take() {
        unsafe {
            // Already gone in the normal case; this only matters if QUIT was not honored.
            TerminateProcess(child as HANDLE, 0);
            CloseHandle(child as HANDLE);
        }
    }
}
