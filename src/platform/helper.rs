//! x64 helper for emulated-x64 targets. Placeholder until M3 lands.
use windows_sys::Win32::Foundation::HWND;

pub fn installed() -> bool {
    false
}
pub fn hook_thread(_thread: u32, _hwnd: HWND) {}
pub fn stop_all() {}
pub fn shutdown() {}
