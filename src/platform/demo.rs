//! Materials demonstration: a small window AllBlack11 owns, so a backdrop can be shown
//! rendered correctly. It applies the look currently being edited to *itself*, through the
//! same code path as any other window, and draws three panels:
//!
//!  * left: a transparent client, where the material shows through
//!  * middle: an opaque client, which is what most apps draw, and which hides the material
//!  * right: a half-transparent card
//!
//! This is a demonstration of how Windows materials behave. It is not a preview of how any
//! particular app will look: that app decides what its client area draws.

use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, Ordering};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, EndPaint,
    InvalidateRect, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HDC,
    PAINTSTRUCT, SRCCOPY,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::{
    CloseThemeData, DrawThemeTextEx, OpenThemeData, DTTOPTS, MARGINS,
};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetWindowTextW, LoadCursorW,
    RegisterClassExW, SetForegroundWindow, ShowWindow, CS_HREDRAW, CS_VREDRAW, IDC_ARROW, SW_SHOW,
    WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN, WM_PAINT, WNDCLASSEXW, WS_OVERLAPPEDWINDOW,
    WS_VISIBLE,
};

use crate::config::{acrylic_gradient, Backdrop, Style};

use super::composition;
use super::dwm;
use super::wide::{pcw, to_wide};

static DEMO: AtomicIsize = AtomicIsize::new(0);
/// The untouched half of a Before / After pair.
static BEFORE: AtomicIsize = AtomicIsize::new(0);

const DTT_TEXTCOLOR: u32 = 0x1;
const DTT_GLOWSIZE: u32 = 0x800;
const DTT_COMPOSITED: u32 = 0x2000;

fn close_existing() {
    for slot in [&DEMO, &BEFORE] {
        let existing = slot.load(Ordering::Acquire);
        if existing != 0 {
            unsafe { DestroyWindow(existing as HWND) };
        }
    }
}

fn create(title: &str, x: i32, y: i32, w: i32, h: i32) -> HWND {
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        register(instance);
        let class = to_wide("AllBlack11.Demo");
        let title = to_wide(title);
        CreateWindowExW(
            0,
            pcw(&class),
            pcw(&title),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            x,
            y,
            w,
            h,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    }
}

/// Let the material reach the whole client area; `paint` then draws only what should hide it.
fn extend_frame(hwnd: HWND) {
    let margins = MARGINS {
        cxLeftWidth: -1,
        cxRightWidth: -1,
        cyTopHeight: -1,
        cyBottomHeight: -1,
    };
    unsafe {
        DwmExtendFrameIntoClientArea(hwnd, &margins);
        ShowWindow(hwnd, SW_SHOW);
        InvalidateRect(hwnd, std::ptr::null(), 0);
    }
}

fn local_time_text() -> String {
    let mut now: windows_sys::Win32::Foundation::SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut now) };
    format!("{:02}:{:02}:{:02}", now.wHour, now.wMinute, now.wSecond)
}

/// Two windows AllBlack11 owns, with identical sample content: the left is left exactly as
/// Windows would draw it, the right gets the current look through the normal pipeline.
/// Nothing here is a capture of, or a claim about, any other app: it shows what the frame
/// styling does on a window we control. The time is when the pair was made, so a screenshot
/// taken of it can be told apart later.
pub fn open_pair(style: Style, accent_bgr: u32, build: u32, look_name: &str) {
    close_existing();
    let stamp = local_time_text();
    let (x, y, w, h) = pair_geometry();
    let before = create(
        &format!("Before: untouched by AllBlack11 ({stamp})"),
        x,
        y,
        w,
        h,
    );
    if before.is_null() {
        return;
    }
    BEFORE.store(before as isize, Ordering::Release);
    extend_frame(before);
    let after = create(&format!("After: {look_name} ({stamp})"), x + w + 8, y, w, h);
    if after.is_null() {
        return;
    }
    DEMO.store(after as isize, Ordering::Release);
    unsafe { apply_look(after, &style, accent_bgr, build) };
    extend_frame(after);
    unsafe { SetForegroundWindow(after) };
}

/// Two windows side by side, centered on the monitor under the cursor, scaled to its DPI.
fn pair_geometry() -> (i32, i32, i32, i32) {
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    unsafe {
        let mut cursor = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
        windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut cursor);
        let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return (100, 100, 520, 380);
        }
        let (mut dx, mut dy) = (0u32, 0u32);
        let dpi = if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) == 0 && dx != 0
        {
            dx as i32
        } else {
            96
        };
        let work = info.rcWork;
        let avail_w = work.right - work.left;
        let avail_h = work.bottom - work.top;
        let w = (520 * dpi / 96).min((avail_w - 40) / 2);
        let h = (380 * dpi / 96).min(avail_h - 60);
        let x = work.left + (avail_w - (2 * w + 8)) / 2;
        let y = work.top + (avail_h - h) / 2;
        (x, y, w, h)
    }
}

pub fn open(style: Style, accent_bgr: u32, build: u32) {
    close_existing();
    unsafe {
        let hwnd = create("Materials demonstration", 200, 160, 760, 420);
        if hwnd.is_null() {
            return;
        }
        DEMO.store(hwnd as isize, Ordering::Release);
        apply_look(hwnd, &style, accent_bgr, build);
        extend_frame(hwnd);
        SetForegroundWindow(hwnd);
    }
}

/// The same pipeline every other window goes through.
unsafe fn apply_look(hwnd: HWND, style: &Style, accent_bgr: u32, build: u32) {
    let style = *style;
    {
        let mut backdrop = style.backdrop.dwm_value();
        if style.backdrop == Backdrop::TintedAcrylic {
            let gradient = acrylic_gradient(style.acrylic_tint, style.acrylic_opacity, accent_bgr);
            if composition::set_accent(hwnd, composition::ACCENT_ENABLE_ACRYLICBLURBEHIND, gradient)
            {
                backdrop = Backdrop::None.dwm_value();
            } else {
                backdrop = Backdrop::Acrylic.dwm_value();
            }
        }
        dwm::apply(hwnd, &style, accent_bgr, build, backdrop);
    }
}

fn register(instance: windows_sys::Win32::Foundation::HINSTANCE) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| unsafe {
        let class = to_wide("AllBlack11.Demo");
        let mut info: WNDCLASSEXW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        info.style = CS_HREDRAW | CS_VREDRAW;
        info.lpfnWndProc = Some(proc);
        info.hInstance = instance;
        info.hCursor = LoadCursorW(std::ptr::null_mut(), IDC_ARROW);
        info.lpszClassName = pcw(&class);
        RegisterClassExW(&info);
    });
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            paint(hwnd);
            0
        }
        WM_KEYDOWN if wparam == 0x1B => {
            DestroyWindow(hwnd);
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            for slot in [&DEMO, &BEFORE] {
                if slot.load(Ordering::Acquire) == hwnd as isize {
                    slot.store(0, Ordering::Release);
                }
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Premultiplied ARGB.
fn argb(alpha: u8, r: u8, g: u8, b: u8) -> u32 {
    let scale = |c: u8| (u32::from(c) * u32::from(alpha) / 255) as u8;
    (u32::from(alpha) << 24)
        | (u32::from(scale(r)) << 16)
        | (u32::from(scale(g)) << 8)
        | u32::from(scale(b))
}

unsafe fn paint(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let screen = BeginPaint(hwnd, &mut ps);
    let mut client = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    GetClientRect(hwnd, &mut client);
    let (w, h) = (
        (client.right - client.left).max(1),
        (client.bottom - client.top).max(1),
    );
    let dc = CreateCompatibleDC(screen);
    let mut header: BITMAPINFOHEADER = std::mem::zeroed();
    header.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    header.biWidth = w;
    header.biHeight = -h; // top-down
    header.biPlanes = 1;
    header.biBitCount = 32;
    header.biCompression = BI_RGB;
    let info = BITMAPINFO {
        bmiHeader: header,
        bmiColors: [std::mem::zeroed(); 1],
    };
    let mut bits: *mut c_void = std::ptr::null_mut();
    let dib = CreateDIBSection(
        dc,
        &info,
        DIB_RGB_COLORS,
        &mut bits,
        std::ptr::null_mut(),
        0,
    );
    if dib.is_null() || bits.is_null() {
        DeleteDC(dc);
        EndPaint(hwnd, &ps);
        return;
    }
    let old = SelectObject(dc, dib);
    let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, (w * h) as usize);
    pixels.fill(0); // fully transparent: the material shows

    let dpi = GetDpiForWindow(hwnd).max(96) as i32;
    let px = |v: i32| v * dpi / 96;
    let margin = px(16);
    let top = px(64);
    let panel_w = (w - margin * 4) / 3;
    let panel_h = (h - top - margin).max(px(60));
    let fill = |pixels: &mut [u32], left: i32, value: u32| {
        for y in top..(top + panel_h).min(h) {
            for x in left..(left + panel_w).min(w) {
                pixels[(y * w + x) as usize] = value;
            }
        }
    };
    let left_a = margin;
    let left_b = margin * 2 + panel_w;
    let left_c = margin * 3 + panel_w * 2;
    // Left: nothing drawn. Middle: opaque. Right: a 25% white card.
    fill(pixels, left_b, argb(255, 32, 32, 36));
    fill(pixels, left_c, argb(64, 255, 255, 255));

    let theme = OpenThemeData(hwnd, to_wide("CompositedWindow::Window").as_ptr());
    if theme != 0 {
        let mut options: DTTOPTS = std::mem::zeroed();
        options.dwSize = std::mem::size_of::<DTTOPTS>() as u32;
        options.dwFlags = DTT_COMPOSITED | DTT_TEXTCOLOR | DTT_GLOWSIZE;
        options.crText = 0x00FF_FFFF;
        options.iGlowSize = px(8);
        let draw = |text: &str, rect: RECT, dc: HDC| {
            let wide = to_wide(text);
            let mut rect = rect;
            DrawThemeTextEx(
                theme,
                dc,
                0,
                0,
                wide.as_ptr(),
                -1,
                0x0010 | 0x0800,
                &mut rect,
                &options,
            );
        };
        let mut title = [0u16; 160];
        let length = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32).max(0) as usize;
        let heading = String::from_utf16_lossy(&title[..length]);
        draw(
            &format!(
                "{heading}\nDemonstration only: sample content drawn by AllBlack11 on a window it owns. Another app can look different: it decides what its own client area draws."
            ),
            RECT { left: margin, top: px(6), right: w - margin, bottom: top - px(4) },
            dc,
        );
        let caption = |left: i32| RECT {
            left: left + px(8),
            top: top + px(8),
            right: left + panel_w - px(8),
            bottom: top + panel_h,
        };
        draw(
            "Transparent client\nThe material shows through.",
            caption(left_a),
            dc,
        );
        draw(
            "Opaque client\nWhat most apps draw. It hides the material.",
            caption(left_b),
            dc,
        );
        draw(
            "Half-transparent card\nA little of the material shows.",
            caption(left_c),
            dc,
        );
        CloseThemeData(theme);
    }

    BitBlt(screen, 0, 0, w, h, dc, 0, 0, SRCCOPY);
    SelectObject(dc, old);
    DeleteObject(dib);
    DeleteDC(dc);
    EndPaint(hwnd, &ps);
}
