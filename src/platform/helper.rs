//! Controller side of the x64 helper (ARM64 builds only; on x64 this is inert).
//!
//! An ARM64 process cannot load an x64 hook DLL into an x64-emulated process, so a small x64
//! helper does the hooking. The helper is packaged next to this exe (never downloaded), is
//! started only when an eligible emulated-x64 dialog appears *and the option is on*, and stops
//! when styling is turned off, when the option is turned off, when this program exits, or after
//! a quiet spell with nothing left to hook.
//!
//! Lifecycle rules (each one has a test in `helper_protocol` or is called out as unverified):
//!  * Nothing is sent and nothing is started unless the option is on, the helper is installed
//!    and no failure backoff is running (`may_use_helper`).
//!  * All blocking waits happen on one worker thread, never the UI thread.
//!  * The queue to the worker is bounded; when it is full new work is dropped, not buffered.
//!  * A reply wait is bounded (3 s). A helper that stops answering is ended, and the next try
//!    waits longer (1, 2, 5, then 10 minutes).
//!  * Turning the option off sends STOPALL then QUIT and drops the connection. If the helper
//!    does not leave on its own it is terminated.
//!
//! Trust model, stated plainly: the pipe is local-only, first-instance, and owner-only, and
//! each side checks the other's process id; the helper also checks that its parent is the
//! process *started at the expected time*, not merely the expected id. That keeps other
//! *users* and accidents out. It is not a defense against malware running as you, which can
//! already inject into your processes by other means. The helper accepts four commands and
//! re-validates every target itself.
//!
//! Evidence: cross-compiled, protocol unit-tested. Never run on hardware.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, HWND};
use windows_sys::Win32::Security::Cryptography::{
    BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG,
};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, ReadFile, WriteFile, OPEN_EXISTING};
use windows_sys::Win32::System::Pipes::{
    GetNamedPipeServerProcessId, PeekNamedPipe, WaitNamedPipeW,
};
use windows_sys::Win32::System::SystemInformation::GetTickCount64;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes, TerminateProcess};

use crate::helper_protocol::{may_use_helper, Backoff, Reply};
use crate::BUILD_MACHINE;

use super::wide::{log_line, pcw, to_wide};

const HELPER_EXE: &str = "AllBlack11Helper.x64.exe";
const HELPER_HOOK: &str = "AllBlack11Hook.x64.dll";
const IDLE_PRUNE: Duration = Duration::from_secs(90);
const CONNECT_WAIT_MS: u32 = 5000;
const REPLY_WAIT: Duration = Duration::from_secs(3);
const QUEUE_DEPTH: usize = 32;
const MAX_TRACKED_THREADS: usize = 64;

enum Cmd {
    Hook(u32),
    StopAll,
    Quit,
}

struct State {
    tx: Option<SyncSender<Cmd>>,
    hooked: Vec<u32>,
    backoff: Backoff,
    /// Bumped each time a worker is created or retired, so a worker that is on its way out
    /// can never overwrite the state of the one that replaced it.
    generation: u64,
    /// Process handles of helpers (generation, handle), for forced termination when one does
    /// not leave on its own. Each entry is closed exactly once, by whoever removes it.
    children: Vec<(u64, usize)>,
}

static STATE: Mutex<State> = Mutex::new(State {
    tx: None,
    hooked: Vec::new(),
    backoff: Backoff::new(),
    generation: 0,
    children: Vec::new(),
});

/// The user's "cover emulated x64 apps" switch. Off means nothing here does anything.
static ENABLED: AtomicBool = AtomicBool::new(false);

fn now_ms() -> u64 {
    unsafe { GetTickCount64() }
}

/// The helper and its x64 hook are packaged next to this exe, and this is an ARM64 build.
pub fn installed() -> bool {
    if BUILD_MACHINE != crate::arch::MACHINE_ARM64 {
        return false;
    }
    let present =
        |name: &str| super::legacy::hook_dll_path(name).is_some_and(|path| path.is_file());
    present(HELPER_EXE) && present(HELPER_HOOK)
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Acquire)
}

/// What Settings shows: running, idle, backing off, or not available.
pub fn status_text() -> String {
    if !installed() {
        return if BUILD_MACHINE == crate::arch::MACHINE_ARM64 {
            "Not installed: the x64 helper files are missing from the install folder.".to_string()
        } else {
            "Not applicable: this is not an ARM64 build.".to_string()
        };
    }
    if !is_enabled() {
        return "Off. Nothing is running.".to_string();
    }
    let state = STATE.lock().unwrap_or_else(|err| err.into_inner());
    if state.tx.is_some() {
        format!("Running, watching {} dialog thread(s).", state.hooked.len())
    } else if !state.backoff.allowed(now_ms()) {
        "Not running: it failed to start recently, so it is paused before the next try.".to_string()
    } else {
        "On, and idle. It starts only when an emulated x64 dialog appears.".to_string()
    }
}

/// Follow the user's switch. Turning it off ends the helper now.
pub fn set_enabled(on: bool) {
    let was = ENABLED.swap(on, Ordering::AcqRel);
    if was && !on {
        stop_and_quit();
    }
}

pub fn hook_thread(thread: u32, _hwnd: HWND) {
    let mut state = STATE.lock().unwrap_or_else(|err| err.into_inner());
    if !may_use_helper(is_enabled(), installed(), &state.backoff, now_ms()) {
        return;
    }
    if state.hooked.contains(&thread) || state.hooked.len() >= MAX_TRACKED_THREADS {
        return;
    }
    if state.tx.is_none() {
        let (tx, rx) = mpsc::sync_channel(QUEUE_DEPTH);
        state.tx = Some(tx);
        state.generation += 1;
        let generation = state.generation;
        std::thread::spawn(move || worker(rx, generation));
    }
    if let Some(tx) = &state.tx {
        match tx.try_send(Cmd::Hook(thread)) {
            Ok(()) => state.hooked.push(thread),
            // Full or gone: drop this one. The dialog simply stays as Windows draws it.
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
        }
    }
}

/// Styling off for good: tell the helper to stop and leave, and make sure it does.
pub fn stop_and_quit() {
    let (tx, retiring) = {
        let mut state = STATE.lock().unwrap_or_else(|err| err.into_inner());
        state.hooked.clear();
        let tx = state.tx.take();
        let retiring = state.generation;
        if tx.is_some() {
            // Whatever comes next is a new worker; this one is leaving.
            state.generation += 1;
        }
        (tx, retiring)
    };
    if let Some(tx) = tx {
        let _ = tx.try_send(Cmd::StopAll);
        let _ = tx.try_send(Cmd::Quit);
        drop(tx);
        // Do not wait here. If the helper has not left shortly, end it from a short-lived
        // thread so the UI thread never sleeps.
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(1500));
            let mut state = STATE.lock().unwrap_or_else(|err| err.into_inner());
            terminate_generation(&mut state, retiring);
        });
    }
}

/// Program exit: leave nothing behind, and accept a short wait to do it properly.
pub fn shutdown() {
    ENABLED.store(false, Ordering::Release);
    let had = STATE
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .tx
        .is_some();
    stop_and_quit();
    if had {
        std::thread::sleep(Duration::from_millis(300));
    }
    let mut state = STATE.lock().unwrap_or_else(|err| err.into_inner());
    let all: Vec<u64> = state
        .children
        .iter()
        .map(|(generation, _)| *generation)
        .collect();
    for generation in all {
        terminate_generation(&mut state, generation);
    }
}

/// End and release the helper process of one generation, if it is still being tracked.
fn terminate_generation(state: &mut State, generation: u64) {
    let mut kept = Vec::with_capacity(state.children.len());
    for (g, handle) in state.children.drain(..) {
        if g == generation {
            unsafe {
                // Already gone in the normal case; this only matters if QUIT was not honored.
                TerminateProcess(handle as HANDLE, 0);
                CloseHandle(handle as HANDLE);
            }
        } else {
            kept.push((g, handle));
        }
    }
    state.children = kept;
}

struct Link {
    pipe: HANDLE,
}

impl Link {
    /// One command, one reply, with a bounded wait. `None` means the helper failed to answer
    /// in time, closed the pipe, or sent something that is not part of the protocol.
    fn request(&self, command: &str) -> Option<Reply> {
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
        // Wait for bytes with a deadline instead of blocking forever in ReadFile. This runs
        // on the worker thread only, and normally returns within a millisecond or two.
        let deadline = Instant::now() + REPLY_WAIT;
        loop {
            let mut available = 0u32;
            let ok = unsafe {
                PeekNamedPipe(
                    self.pipe,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    &mut available,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 {
                return None;
            }
            if available > 0 {
                break;
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(5));
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
            if reply.len() > crate::helper_protocol::MAX_LINE {
                return None;
            }
        }
        Reply::parse(&String::from_utf8_lossy(&reply))
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

fn own_creation_time() -> Option<u64> {
    use windows_sys::Win32::Foundation::FILETIME;
    unsafe {
        let mut created: FILETIME = std::mem::zeroed();
        let mut exited: FILETIME = std::mem::zeroed();
        let mut kernel: FILETIME = std::mem::zeroed();
        let mut user: FILETIME = std::mem::zeroed();
        if GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        ) == 0
        {
            return None;
        }
        Some((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
    }
}

fn spawn_and_connect(generation: u64) -> Option<(Link, u32)> {
    use std::os::windows::io::IntoRawHandle;
    use std::os::windows::process::CommandExt;
    let exe = super::legacy::hook_dll_path(HELPER_EXE)?;
    let name = format!("\\\\.\\pipe\\AllBlack11.Helper.{}", random_hex()?);
    let child = std::process::Command::new(&exe)
        .args([
            "--parent",
            &std::process::id().to_string(),
            "--parent-created",
            &own_creation_time()?.to_string(),
            "--pipe",
            &name,
        ])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()
        .ok()?;
    let helper_pid = child.id();
    let process = child.into_raw_handle() as usize;
    {
        let mut state = STATE.lock().unwrap_or_else(|err| err.into_inner());
        if state.generation != generation && state.tx.is_none() {
            // The option was switched off while this was starting. End it right away.
            unsafe {
                TerminateProcess(process as HANDLE, 0);
                CloseHandle(process as HANDLE);
            }
            return None;
        }
        state.children.push((generation, process));
    }

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
    // The other end must be the process we started, not something that grabbed the name. We
    // hold the child's process handle, so its id cannot have been recycled.
    let mut server = 0u32;
    if unsafe { GetNamedPipeServerProcessId(pipe, &mut server) } == 0 || server != helper_pid {
        return None;
    }
    Some((link, helper_pid))
}

/// Release this worker's claim on the shared state. Only the current generation may clear
/// the sender or the hooked list; a retired worker just ends its own process.
fn retire(generation: u64, failed: bool) {
    let mut state = STATE.lock().unwrap_or_else(|err| err.into_inner());
    if state.generation == generation {
        state.tx = None;
        state.hooked.clear();
        if failed {
            state.backoff.fail(now_ms());
        }
    }
    terminate_generation(&mut state, generation);
}

fn worker(rx: mpsc::Receiver<Cmd>, generation: u64) {
    let Some((link, pid)) = spawn_and_connect(generation) else {
        log_line(
            "warn",
            "the x64 helper could not be started; emulated x64 dialogs are skipped for now",
        );
        // A start that was cancelled by switching the option off is not a failure.
        let cancelled = !is_enabled();
        retire(generation, !cancelled);
        return;
    };
    {
        let mut state = STATE.lock().unwrap_or_else(|err| err.into_inner());
        if state.generation == generation {
            state.backoff.succeed();
        }
    }
    log_line("info", &format!("x64 helper started (pid {pid})"));
    let mut failed = false;
    loop {
        match rx.recv_timeout(IDLE_PRUNE) {
            Ok(Cmd::Hook(thread)) => match link.request(&format!("HOOK {thread}")) {
                Some(Reply::Ok) => {}
                // The helper declined (elevated, no dialog, ...). That is a normal answer.
                Some(_) => {
                    let mut state = STATE.lock().unwrap_or_else(|err| err.into_inner());
                    if state.generation == generation {
                        state.hooked.retain(|t| *t != thread);
                    }
                }
                None => {
                    failed = true;
                    break;
                }
            },
            Ok(Cmd::StopAll) => {
                if link.request("STOPALL").is_none() {
                    failed = true;
                    break;
                }
            }
            Ok(Cmd::Quit) => {
                let _ = link.request("QUIT");
                break;
            }
            Err(RecvTimeoutError::Timeout) => {
                // Quiet for a while: drop hooks for dialogs that closed, and leave entirely if
                // nothing is left to hook.
                match link.request("PRUNE") {
                    Some(Reply::Hooks(0)) => {
                        let _ = link.request("QUIT");
                        break;
                    }
                    Some(_) => {}
                    None => {
                        failed = true;
                        break;
                    }
                }
            }
            // The controller dropped its sender: that is "stop and leave".
            Err(RecvTimeoutError::Disconnected) => {
                let _ = link.request("QUIT");
                break;
            }
        }
    }
    if failed {
        log_line("warn", "the x64 helper stopped answering and was ended");
    }
    retire(generation, failed);
}
