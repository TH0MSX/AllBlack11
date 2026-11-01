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

/// A frame hosted by `ApplicationFrameHost.exe` (Store, Calculator and other UWP apps) belongs to
/// the host process, which has no package of its own. The app that owns the window is the process
/// of the `Windows.UI.Core.CoreWindow` child inside the frame. Returns its process id, or None
/// when the frame has no such child (yet) or the child is in the host's own process.
pub fn hosted_app_pid(frame: HWND, host_pid: u32) -> Option<u32> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{EnumChildWindows, GetWindowThreadProcessId};
    struct Found {
        host_pid: u32,
        pid: Option<u32>,
    }
    unsafe extern "system" fn visit(child: HWND, lparam: isize) -> i32 {
        let found = &mut *(lparam as *mut Found);
        if super::legacy::class_name(child) != "Windows.UI.Core.CoreWindow" {
            return 1;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(child, &mut pid);
        if pid != 0 && pid != found.host_pid {
            found.pid = Some(pid);
            return 0;
        }
        1
    }
    let mut found = Found {
        host_pid,
        pid: None,
    };
    unsafe { EnumChildWindows(frame, Some(visit), &mut found as *mut Found as isize) };
    found.pid
}

/// `path_and_package` for a window: for an `ApplicationFrameHost` frame the package is the hosted
/// app's, so a rule or pack can name Calculator or the Store and not every UWP window at once.
/// The path stays the host's (the file that is actually running the frame).
pub fn path_and_package_of_window(
    hwnd: HWND,
    pid: u32,
    exe: &str,
    want_package: bool,
) -> (String, String) {
    let (path, mut package) = path_and_package(pid, want_package);
    if want_package && package.is_empty() && exe.eq_ignore_ascii_case("ApplicationFrameHost.exe") {
        if let Some(inner) = hosted_app_pid(hwnd, pid) {
            package = path_and_package(inner, true).1;
        }
    }
    (path, package)
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
