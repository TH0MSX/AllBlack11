//! The full-screen wallpaper preview: one black window over the display Settings is on, showing the
//! picture placed the way Windows will place it with the chosen fit. Enter applies, Esc or a click
//! closes, and switching to another window closes it too, so it can never get stuck on top.
//!
//! When the full image has not been downloaded, the small preview picture is shown instead, larger;
//! the hint says so. Nothing is applied or saved by looking.
//!
//! Evidence: cross-compiled; shown under Wine only. Not verified on Windows hardware.

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreateCompatibleDC, CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint,
    FillRect, GetMonitorInfoW, GetStockObject, MonitorFromWindow, SelectObject, SetBkMode,
    SetStretchBltMode, SetTextColor, StretchBlt, DEFAULT_GUI_FONT, HALFTONE, HBITMAP, MONITORINFO,
    MONITOR_DEFAULTTONEAREST, PAINTSTRUCT, SRCCOPY, TRANSPARENT,
};
use windows_sys::Win32::Graphics::GdiPlus::{GdiplusShutdown, GdiplusStartup, GdiplusStartupInput};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SetFocus, VK_ESCAPE, VK_RETURN, VK_SPACE};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, FindWindowW, GetClientRect, GetWindowLongPtrW,
    LoadCursorW, PostMessageW, RegisterClassW, SetForegroundWindow, SetWindowLongPtrW, ShowWindow,
    CREATESTRUCTW, GWLP_USERDATA, IDC_ARROW, SW_SHOW, WA_INACTIVE, WM_ACTIVATE, WM_CLOSE,
    WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_NCCREATE, WM_PAINT, WM_RBUTTONDOWN,
    WNDCLASSW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use crate::wallpaper::{placement, Entry, Fit};

use super::gallery::{downloaded, hbitmap_from_file, image_path, preview_file};
use super::wide::{pcw, to_wide};

const CLASS: &str = "AllBlack11.WallpaperPreview";

struct Preview {
    bitmap: HBITMAP,
    bitmap_size: (i32, i32),
    /// The size of the real wallpaper, which the placement is computed for.
    image: (u32, u32),
    fit: Fit,
    owner: HWND,
    hint: Vec<u16>,
    activated: bool,
}

pub fn open(owner: HWND, entry: &Entry, fit: Fit) {
    unsafe {
        // One preview at a time.
        let class = to_wide(CLASS);
        let existing = FindWindowW(pcw(&class), std::ptr::null());
        if !existing.is_null() {
            PostMessageW(existing, WM_CLOSE, 0, 0);
        }
        let full = downloaded(entry);
        let path = if full {
            image_path(entry)
        } else {
            preview_file(entry)
        };
        let Some(bitmap) = load(&path.to_string_lossy()) else {
            return;
        };
        let mut info: windows_sys::Win32::Graphics::Gdi::BITMAP = std::mem::zeroed();
        windows_sys::Win32::Graphics::Gdi::GetObjectW(
            bitmap,
            std::mem::size_of::<windows_sys::Win32::Graphics::Gdi::BITMAP>() as i32,
            &mut info as *mut _ as *mut _,
        );
        let hint = if full {
            "Enter applies this wallpaper. Esc closes."
        } else {
            "Preview from the small picture (download for the full image). Enter applies once it is downloaded. Esc closes."
        };
        let state = Box::new(Preview {
            bitmap,
            bitmap_size: (info.bmWidth.max(1), info.bmHeight.max(1)),
            image: (entry.width, entry.height),
            fit,
            owner,
            hint: to_wide(hint),
            activated: false,
        });
        register();
        let mut monitor: MONITORINFO = std::mem::zeroed();
        monitor.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        let handle = MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST);
        let area = if GetMonitorInfoW(handle, &mut monitor) != 0 {
            monitor.rcMonitor
        } else {
            RECT {
                left: 0,
                top: 0,
                right: 1280,
                bottom: 720,
            }
        };
        // The title is what a screen reader announces.
        let title = to_wide(&format!(
            "Preview of {}. {}",
            entry.name,
            if full {
                "Press Enter to apply, Escape to close."
            } else {
                "Small preview. Press Escape to close."
            }
        ));
        let raw = Box::into_raw(state);
        let window = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            pcw(&class),
            pcw(&title),
            WS_POPUP,
            area.left,
            area.top,
            area.right - area.left,
            area.bottom - area.top,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            raw as *const _,
        );
        if window.is_null() {
            drop(Box::from_raw(raw));
            DeleteObject(bitmap);
            return;
        }
        ShowWindow(window, SW_SHOW);
        SetForegroundWindow(window);
        SetFocus(window);
    }
}

unsafe fn load(path: &str) -> Option<HBITMAP> {
    let mut token = 0usize;
    let input = GdiplusStartupInput {
        GdiplusVersion: 1,
        DebugEventCallback: 0,
        SuppressBackgroundThread: 0,
        SuppressExternalCodecs: 0,
    };
    if GdiplusStartup(&mut token, &input, std::ptr::null_mut()) != 0 {
        return None;
    }
    let bitmap = hbitmap_from_file(path);
    GdiplusShutdown(token);
    bitmap
}

unsafe fn register() {
    let class = to_wide(CLASS);
    let wc = WNDCLASSW {
        style: 0,
        lpfnWndProc: Some(proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: GetModuleHandleW(std::ptr::null()),
        hIcon: std::ptr::null_mut(),
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        hbrBackground: std::ptr::null_mut(),
        lpszMenuName: std::ptr::null(),
        lpszClassName: pcw(&class),
    };
    // A second registration fails harmlessly.
    RegisterClassW(&wc);
}

unsafe fn state<'a>(hwnd: HWND) -> Option<&'a mut Preview> {
    let raw = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Preview;
    raw.as_mut()
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let create = lparam as *const CREATESTRUCTW;
            if let Some(create) = create.as_ref() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut client: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut client);
            let black = CreateSolidBrush(0);
            FillRect(dc, &client, black);
            DeleteObject(black);
            if let Some(preview) = state(hwnd) {
                let (mw, mh) = (
                    (client.right - client.left).max(1) as u32,
                    (client.bottom - client.top).max(1) as u32,
                );
                let (dest, src) = placement(preview.fit, preview.image, (mw, mh));
                // The source rectangle is in the real wallpaper's pixels; the bitmap may be smaller.
                let sx = preview.bitmap_size.0 as f64 / preview.image.0.max(1) as f64;
                let sy = preview.bitmap_size.1 as f64 / preview.image.1.max(1) as f64;
                let mem = CreateCompatibleDC(dc);
                let old = SelectObject(mem, preview.bitmap);
                SetStretchBltMode(dc, HALFTONE);
                StretchBlt(
                    dc,
                    dest.0,
                    dest.1,
                    dest.2,
                    dest.3,
                    mem,
                    (src.0 as f64 * sx) as i32,
                    (src.1 as f64 * sy) as i32,
                    ((src.2 as f64 * sx) as i32).max(1),
                    ((src.3 as f64 * sy) as i32).max(1),
                    SRCCOPY,
                );
                SelectObject(mem, old);
                DeleteDC(mem);
                // The hint, small and dim, along the bottom.
                SelectObject(dc, GetStockObject(DEFAULT_GUI_FONT));
                SetBkMode(dc, TRANSPARENT as i32);
                SetTextColor(dc, 0x0090_9090);
                let mut line = RECT {
                    left: client.left + 16,
                    top: client.bottom - 40,
                    right: client.right - 16,
                    bottom: client.bottom - 12,
                };
                // DT_CENTER | DT_VCENTER | DT_SINGLELINE
                DrawTextW(
                    dc,
                    preview.hint.as_ptr(),
                    -1,
                    &mut line,
                    0x0001 | 0x0004 | 0x0020,
                );
            }
            EndPaint(hwnd, &ps);
            0
        }
        WM_KEYDOWN => {
            let key = wparam as u16;
            if key == VK_ESCAPE {
                DestroyWindow(hwnd);
            } else if key == VK_RETURN || key == VK_SPACE {
                if let Some(preview) = state(hwnd) {
                    PostMessageW(preview.owner, super::settings::WM_WALL_APPLY, 0, 0);
                }
                DestroyWindow(hwnd);
            }
            0
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
            DestroyWindow(hwnd);
            0
        }
        WM_ACTIVATE => {
            if let Some(preview) = state(hwnd) {
                if (wparam & 0xFFFF) as u32 == WA_INACTIVE {
                    if preview.activated {
                        DestroyWindow(hwnd);
                    }
                } else {
                    preview.activated = true;
                }
            }
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            let raw = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Preview;
            if !raw.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                let preview = Box::from_raw(raw);
                DeleteObject(preview.bitmap);
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
