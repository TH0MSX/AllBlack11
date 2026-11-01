//! Facts about a process or window that only some rules need. Everything here is lazy:
//! the path, the package family and the window title are read only when a rule (or the
//! inspector the user opened) asks for them.

use windows_sys::Win32::Foundation::{CloseHandle, HWND};
use windows_sys::Win32::Storage::Packaging::Appx::GetPackageFamilyName;
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowTextW;

/// (full executable path, package family name). Either is empty when unknown. A process
/// that is not packaged has no family name, which is the normal case.
pub fn path_and_package(pid: u32, want_package: bool) -> (String, String) {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return (String::new(), String::new());
        }
        let mut buf = [0u16; 1024];
        let mut length = buf.len() as u32;
        let path = if QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            buf.as_mut_ptr(),
            &mut length,
        ) != 0
        {
            String::from_utf16_lossy(&buf[..length as usize])
        } else {
            String::new()
        };
        let mut package = String::new();
        if want_package {
            let mut name = [0u16; 128];
            let mut len = name.len() as u32;
            // 0 is success. APPMODEL_ERROR_NO_PACKAGE (15700) means "not a packaged app".
            if GetPackageFamilyName(handle, &mut len, name.as_mut_ptr()) == 0 && len > 1 {
                package = String::from_utf16_lossy(&name[..(len as usize - 1).min(name.len())]);
            }
        }
        CloseHandle(handle);
        (path, package)
    }
}

pub fn window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let length = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if length <= 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&buf[..length as usize])
    }
}
