//! What this PC and this build are, plus the Explorer restart.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use windows_sys::Win32::Foundation::{CloseHandle, HWND};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, IsWow64Process2, OpenProcess, TerminateProcess, WaitForSingleObject,
    PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetShellWindow, GetWindowThreadProcessId, PostMessageW, SW_SHOWNORMAL,
};

use crate::{classify, machine_name, ArchStatus, BUILD_MACHINE};

use super::wide::{pcw, to_wide};

/// Machine type of the PC itself, even when this process is emulated.
pub fn host_machine() -> u16 {
    let mut process_machine = 0u16;
    let mut native_machine = 0u16;
    let ok = unsafe {
        IsWow64Process2(GetCurrentProcess(), &mut process_machine, &mut native_machine)
    };
    if ok == 0 {
        0
    } else {
        native_machine
    }
}

pub fn arch_status() -> ArchStatus {
    classify(BUILD_MACHINE, host_machine())
}

/// "Native ARM64 build on an ARM64 PC (build ARM64, PC ARM64)".
pub fn arch_line() -> String {
    format!(
        "{} (build {}, PC {})",
        arch_status().headline(),
        machine_name(BUILD_MACHINE),
        machine_name(host_machine())
    )
}

static RESTARTING: AtomicBool = AtomicBool::new(false);

fn shell_pid() -> u32 {
    let shell = unsafe { GetShellWindow() };
    if shell.is_null() {
        return 0;
    }
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(shell, &mut pid) };
    pid
}

/// Ends the shell process. Windows starts a new one by itself; if it has not done so
/// after a few seconds, start `explorer.exe` here. Runs on the calling thread.
pub fn restart_explorer_blocking() -> Result<String, String> {
    let old = shell_pid();
    if old != 0 {
        let handle = unsafe { OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, 0, old) };
        if handle.is_null() {
            return Err("Windows would not let AllBlack11 close Explorer.".into());
        }
        unsafe {
            TerminateProcess(handle, 1);
            WaitForSingleObject(handle, 4000);
            CloseHandle(handle);
        }
    }
    let wait_for_shell = |tries: u32| -> bool {
        for _ in 0..tries {
            std::thread::sleep(Duration::from_millis(250));
            let pid = shell_pid();
            if pid != 0 && pid != old {
                return true;
            }
        }
        false
    };
    if wait_for_shell(24) {
        return Ok("Explorer restarted.".into());
    }
    let verb = to_wide("open");
    let file = to_wide("explorer.exe");
    unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            pcw(&verb),
            pcw(&file),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        );
    }
    if wait_for_shell(40) {
        Ok("Explorer restarted.".into())
    } else {
        Err("Explorer did not come back. Press Ctrl+Shift+Esc, choose Run new task, and run explorer.exe.".into())
    }
}

/// Restart on a worker thread and post the outcome to `notify` as `message`
/// (`wparam` 1 = success). The text is read with [`take_result`].
pub fn restart_explorer_async(notify: HWND, message: u32) {
    if RESTARTING.swap(true, Ordering::AcqRel) {
        return;
    }
    let target = notify as isize;
    std::thread::spawn(move || {
        let outcome = restart_explorer_blocking();
        let ok = outcome.is_ok();
        let text = outcome.unwrap_or_else(|err| err);
        *RESULT.lock().unwrap_or_else(|err| err.into_inner()) = Some(text);
        RESTARTING.store(false, Ordering::Release);
        unsafe { PostMessageW(target as HWND, message, usize::from(ok), 0) };
    });
}

static RESULT: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

pub fn take_result() -> String {
    RESULT
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .take()
        .unwrap_or_default()
}

pub fn restart_in_progress() -> bool {
    RESTARTING.load(Ordering::Acquire)
}
