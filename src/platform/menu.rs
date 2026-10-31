//! Owner-drawn tray menu. Item metrics follow a Windows 11 context menu (taller rows,
//! horizontal padding, rounded outer window) and the colors follow AppsUseLightTheme.
//! A thread-local CBT hook only watches this process, so it does not inject anywhere.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::{
    CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, FillRect, GetStockObject,
    GetTextExtentPoint32W, MonitorFromPoint, MONITOR_DEFAULTTONEAREST, RoundRect, SelectObject, SetBkMode, SetTextColor, HDC, HFONT, NULL_BRUSH,
    PS_SOLID, TRANSPARENT,
};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::Controls::{
    DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODA_SELECT, ODS_DISABLED, ODS_GRAYED, ODS_SELECTED, ODT_MENU,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CallNextHookEx, CreatePopupMenu, DestroyMenu, DrawMenuBar, GetClassNameW,
    GetCursorPos, GetMenuItemCount, PostMessageW, SetForegroundWindow, SetMenuInfo, SetMenuItemInfoW,
    SetWindowsHookExW, TrackPopupMenuEx, UnhookWindowsHookEx, HMENU, MENUINFO, MENUITEMINFOW,
    MF_CHECKED, MF_DISABLED, MF_GRAYED, MF_OWNERDRAW, MF_POPUP, MF_SEPARATOR, MF_STRING,
    MIM_APPLYTOSUBMENUS, MIM_BACKGROUND, MIIM_DATA, MSG, TPM_BOTTOMALIGN, TPM_NONOTIFY, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, WH_CBT, WM_NULL, WM_UNINITMENUPOPUP,
};

use super::wide::{pcw, to_wide};

const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWCP_ROUND: u32 = 2;
const HCBT_CREATEWND: i32 = 3;
const HCBT_ACTIVATE: i32 = 5;
const FW_NORMAL: i32 = 400;
const DEFAULT_CHARSET: u32 = 1;
const CLEARTYPE_QUALITY: u32 = 5;
const DT_SINGLELINE: u32 = 0x0020;
const DT_VCENTER: u32 = 0x0004;
const DT_NOPREFIX: u32 = 0x0800;
const DT_TOP: u32 = 0x0000;
const DT_BOTTOM: u32 = 0x0008;
const DT_END_ELLIPSIS: u32 = 0x8000;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Title,
    Action,
    Checked(bool),
    Submenu,
    Separator,
}

pub struct Item {
    pub id: u32,
    pub label: String,
    /// Optional second line that says what the item does.
    pub hint: String,
    pub kind: ItemKind,
    pub enabled: bool,
    pub children: Vec<Item>,
}

struct Drawn {
    label: String,
    hint: String,
    kind: ItemKind,
    enabled: bool,
}

thread_local! {
    static DRAWN: RefCell<Vec<Drawn>> = const { RefCell::new(Vec::new()) };
}

static MENU_DARK: AtomicBool = AtomicBool::new(true);

pub fn popup(owner: HWND, dark: bool, high_contrast: bool, items: &[Item]) -> u32 {
    if high_contrast {
        return system_popup(owner, items);
    }
    MENU_DARK.store(dark, Ordering::Relaxed);
    DRAWN.with(|drawn| drawn.borrow_mut().clear());
    unsafe {
        let menu = build_themed(items);
        if menu.is_null() {
            return 0;
        }
        let brush = CreateSolidBrush(if dark { 0x002C_2C2C } else { 0x00FB_FBFB });
        if !brush.is_null() {
            let mut info: MENUINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MENUINFO>() as u32;
            info.fMask = MIM_BACKGROUND | MIM_APPLYTOSUBMENUS;
            info.hbrBack = brush;
            SetMenuInfo(menu, &info);
        }
        let hook = SetWindowsHookExW(WH_CBT, Some(cbt_proc), std::ptr::null_mut(), GetCurrentThreadId());
        let mut point = POINT { x: 0, y: 0 };
        GetCursorPos(&mut point);
        let mut flags = TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY;
        if point.y > 80 {
            flags |= TPM_BOTTOMALIGN;
        }
        SetForegroundWindow(owner);
        let chosen = TrackPopupMenuEx(menu, flags, point.x, point.y, owner, std::ptr::null());
        PostMessageW(owner, WM_NULL, 0, 0);
        if !hook.is_null() {
            UnhookWindowsHookEx(hook);
        }
        DestroyMenu(menu);
        if !brush.is_null() {
            DeleteObject(brush);
        }
        chosen as u32
    }
}

pub fn on_measure(lparam: LPARAM) -> bool {
    unsafe {
        if lparam == 0 {
            return false;
        }
        let measure = &mut *(lparam as *mut MEASUREITEMSTRUCT);
        if measure.CtlType != ODT_MENU {
            return false;
        }
        let Some(item) = item_at(measure.itemData) else {
            return false;
        };
        let dpi = current_dpi();
        if item.kind == ItemKind::Separator {
            measure.itemHeight = scale(dpi, 8) as u32;
            measure.itemWidth = scale(dpi, 160) as u32;
            return true;
        }
        let mut dc_size = (scale(dpi, 160), 0i32);
        if let Some(dc) = paint_dc() {
            let text_size = |font: HFONT, text: &str| -> (i32, i32) {
                let previous = SelectObject(dc, font);
                let mut size = windows_sys::Win32::Foundation::SIZE { cx: 0, cy: 0 };
                let wide = to_wide(text);
                GetTextExtentPoint32W(dc, wide.as_ptr(), (wide.len() - 1) as i32, &mut size);
                if !previous.is_null() {
                    SelectObject(dc, previous);
                }
                (size.cx, size.cy)
            };
            dc_size = text_size(menu_font(dpi, false), &item.label);
            if !item.hint.is_empty() {
                let hint = text_size(menu_font(dpi, true), &item.hint);
                dc_size.0 = dc_size.0.max(hint.0);
            }
            windows_sys::Win32::Graphics::Gdi::ReleaseDC(std::ptr::null_mut(), dc);
        }
        let height = if item.kind == ItemKind::Title {
            scale(dpi, 28)
        } else if item.hint.is_empty() {
            scale(dpi, 34).max(dc_size.1 + scale(dpi, 12))
        } else {
            scale(dpi, 52).max(dc_size.1 * 2 + scale(dpi, 14))
        };
        measure.itemHeight = height as u32;
        // Keep a long hint from stretching the menu across the screen.
        let widest = dc_size.0.min(scale(dpi, 330));
        measure.itemWidth = (widest + scale(dpi, 28 + 16 + 18)) as u32;
        true
    }
}

pub fn on_draw(lparam: LPARAM) -> bool {
    unsafe {
        if lparam == 0 {
            return false;
        }
        let draw = &*(lparam as *const DRAWITEMSTRUCT);
        if draw.CtlType != ODT_MENU {
            return false;
        }
        let Some(item) = item_at(draw.itemData) else {
            return false;
        };
        paint_item(draw, item);
        true
    }
}

fn system_popup(owner: HWND, items: &[Item]) -> u32 {
    unsafe {
        let menu = build_system(items);
        if menu.is_null() {
            return 0;
        }
        let mut point = POINT { x: 0, y: 0 };
        GetCursorPos(&mut point);
        let mut flags = TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY;
        if point.y > 80 {
            flags |= TPM_BOTTOMALIGN;
        }
        SetForegroundWindow(owner);
        let chosen = TrackPopupMenuEx(menu, flags, point.x, point.y, owner, std::ptr::null());
        PostMessageW(owner, WM_NULL, 0, 0);
        DestroyMenu(menu);
        chosen as u32
    }
}

fn build_themed(items: &[Item]) -> HMENU {
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return menu;
        }
        fill_themed(menu, items);
        menu
    }
}

fn fill_themed(menu: HMENU, items: &[Item]) {
    for item in items {
        let index = DRAWN.with(|drawn| {
            let mut drawn = drawn.borrow_mut();
            drawn.push(Drawn {
                label: item.label.clone(),
                hint: item.hint.clone(),
                kind: item.kind,
                enabled: item.enabled,
            });
            drawn.len() - 1
        });
        let mut flags = MF_OWNERDRAW;
        if !item.enabled || item.kind == ItemKind::Title {
            flags |= MF_GRAYED | MF_DISABLED;
        }
        if item.kind == ItemKind::Separator {
            flags = MF_OWNERDRAW | MF_SEPARATOR;
        }
        unsafe {
            if item.kind == ItemKind::Submenu {
                let child = CreatePopupMenu();
                fill_themed(child, &item.children);
                AppendMenuW(menu, flags | MF_POPUP, child as usize, std::ptr::null());
            } else {
                AppendMenuW(menu, flags, item.id as usize, std::ptr::null());
            }
            let mut info: MENUITEMINFOW = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MENUITEMINFOW>() as u32;
            info.fMask = MIIM_DATA;
            info.dwItemData = index;
            SetMenuItemInfoW(
                menu,
                if item.kind == ItemKind::Submenu {
                    GetMenuItemCount(menu) as u32 - 1
                } else {
                    item.id
                },
                i32::from(item.kind == ItemKind::Submenu),
                &info,
            );
        }
    }
}

fn build_system(items: &[Item]) -> HMENU {
    unsafe {
        let menu = CreatePopupMenu();
        if !menu.is_null() {
            fill_system(menu, items);
        }
        menu
    }
}

fn fill_system(menu: HMENU, items: &[Item]) {
    for item in items {
        let wide = to_wide(&item.label.replace('&', "&&"));
        let mut flags = MF_STRING;
        if item.kind == ItemKind::Separator {
            flags = MF_SEPARATOR;
        }
        if !item.enabled || item.kind == ItemKind::Title {
            flags |= MF_GRAYED | MF_DISABLED;
        }
        if let ItemKind::Checked(true) = item.kind {
            flags |= MF_CHECKED;
        }
        unsafe {
            if item.kind == ItemKind::Submenu {
                let child = CreatePopupMenu();
                fill_system(child, &item.children);
                AppendMenuW(menu, flags | MF_POPUP, child as usize, pcw(&wide));
            } else if item.kind == ItemKind::Separator {
                AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
            } else {
                AppendMenuW(menu, flags, item.id as usize, pcw(&wide));
            }
        }
    }
}

fn item_at(index: usize) -> Option<Drawn> {
    DRAWN.with(|drawn| drawn.borrow().get(index).cloned())
}

impl Clone for Drawn {
    fn clone(&self) -> Self {
        Self {
            label: self.label.clone(),
            hint: self.hint.clone(),
            kind: self.kind,
            enabled: self.enabled,
        }
    }
}

fn paint_item(draw: &DRAWITEMSTRUCT, item: Drawn) {
    unsafe {
        let dark = MENU_DARK.load(Ordering::Relaxed);
        let selected = draw.itemState & ODS_SELECTED != 0 && item.enabled && item.kind != ItemKind::Title;
        let disabled = !item.enabled || draw.itemState & (ODS_DISABLED | ODS_GRAYED) != 0 || item.kind == ItemKind::Title;
        let bg = if dark { 0x002C_2C2C } else { 0x00FB_FBFB };
        let hover = if dark { 0x003A_3A3A } else { 0x00E8_E8E8 };
        let text = if disabled {
            if dark { 0x008A_8A8A } else { 0x0088_8888 }
        } else if dark {
            0x00F5_F5F5
        } else {
            0x001A_1A1A
        };
        let mut rect = draw.rcItem;
        let brush = CreateSolidBrush(if selected { hover } else { bg });
        if item.kind == ItemKind::Separator {
            FillRect(draw.hDC, &rect, brush);
            let y = rect.top + (rect.bottom - rect.top) / 2;
            let pen = CreatePen(PS_SOLID, 1, if dark { 0x003F_3F3F } else { 0x00E0_E0E0 });
            let old = SelectObject(draw.hDC, pen);
            windows_sys::Win32::Graphics::Gdi::MoveToEx(draw.hDC, rect.left + 12, y, std::ptr::null_mut());
            windows_sys::Win32::Graphics::Gdi::LineTo(draw.hDC, rect.right - 12, y);
            SelectObject(draw.hDC, old);
            DeleteObject(pen);
            DeleteObject(brush);
            return;
        }
        if selected {
            let pad = 4;
            let hover_rect = RECT {
                left: rect.left + pad,
                top: rect.top + 1,
                right: rect.right - pad,
                bottom: rect.bottom - 1,
            };
            let hollow = GetStockObject(NULL_BRUSH);
            let old_brush = SelectObject(draw.hDC, brush);
            let pen = CreatePen(PS_SOLID, 1, hover);
            let old_pen = SelectObject(draw.hDC, pen);
            RoundRect(draw.hDC, hover_rect.left, hover_rect.top, hover_rect.right, hover_rect.bottom, 8, 8);
            SelectObject(draw.hDC, old_pen);
            SelectObject(draw.hDC, old_brush);
            DeleteObject(pen);
            let _ = hollow;
        } else if !brush.is_null() {
            FillRect(draw.hDC, &rect, brush);
        }
        DeleteObject(brush);
        let dpi = current_dpi();
        let font = menu_font(dpi, false);
        let previous = SelectObject(draw.hDC, font);
        SetBkMode(draw.hDC, TRANSPARENT as i32);
        SetTextColor(draw.hDC, text);
        rect.left += scale(dpi, 16);
        if let ItemKind::Checked(on) = item.kind {
            if on {
                let mark = to_wide("✓");
                let mut mark_rect = rect;
                mark_rect.right = mark_rect.left + scale(dpi, 16);
                DrawTextW(
                    draw.hDC,
                    mark.as_ptr(),
                    -1,
                    &mut mark_rect,
                    DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
                );
            }
        }
        rect.left += scale(dpi, 18);
        if item.kind == ItemKind::Submenu {
            rect.right -= scale(dpi, 18);
        }
        let wide = to_wide(&item.label);
        if item.hint.is_empty() {
            DrawTextW(
                draw.hDC,
                wide.as_ptr(),
                -1,
                &mut rect,
                DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
            );
        } else {
            // Label on top, the muted explanation underneath.
            let mut top = rect;
            top.bottom = rect.top + (rect.bottom - rect.top) * 11 / 20;
            DrawTextW(
                draw.hDC,
                wide.as_ptr(),
                -1,
                &mut top,
                DT_SINGLELINE | DT_BOTTOM | DT_NOPREFIX,
            );
            let mut low = rect;
            low.top = top.bottom - scale(dpi, 1);
            low.right -= scale(dpi, 4);
            SelectObject(draw.hDC, menu_font(dpi, true));
            SetTextColor(draw.hDC, if dark { 0x0099_9999 } else { 0x0066_6666 });
            let hint = to_wide(&item.hint);
            DrawTextW(
                draw.hDC,
                hint.as_ptr(),
                -1,
                &mut low,
                DT_SINGLELINE | DT_TOP | DT_NOPREFIX | DT_END_ELLIPSIS,
            );
            SelectObject(draw.hDC, font);
            SetTextColor(draw.hDC, text);
        }
        if item.kind == ItemKind::Submenu {
            let arrow = to_wide("›");
            let mut arrow_rect = draw.rcItem;
            arrow_rect.left = arrow_rect.right - scale(dpi, 22);
            DrawTextW(
                draw.hDC,
                arrow.as_ptr(),
                -1,
                &mut arrow_rect,
                DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
            );
        }
        if !previous.is_null() {
            SelectObject(draw.hDC, previous);
        }
        let _ = ODA_SELECT;
    }
}

fn scale(dpi: u32, px: i32) -> i32 {
    let dpi = if dpi == 0 { 96 } else { dpi as i32 };
    (px * dpi + 48) / 96
}

/// DPI of the monitor the menu opens on. The foreground window can sit on another
/// monitor with a different scale.
fn current_dpi() -> u32 {
    unsafe {
        let mut point = POINT { x: 0, y: 0 };
        GetCursorPos(&mut point);
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        let (mut x, mut y) = (0u32, 0u32);
        if !monitor.is_null() && GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y) == 0 && x != 0 {
            return x;
        }
        let hwnd = windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow();
        let dpi = GetDpiForWindow(hwnd);
        if dpi == 0 {
            96
        } else {
            dpi
        }
    }
}

fn menu_font(dpi: u32, small: bool) -> HFONT {
    use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
    static FONTS: [AtomicIsize; 2] = [AtomicIsize::new(0), AtomicIsize::new(0)];
    static FONT_DPI: [AtomicU32; 2] = [AtomicU32::new(0), AtomicU32::new(0)];
    let slot = usize::from(small);
    let cached = FONTS[slot].load(Ordering::Acquire);
    if cached != 0 && FONT_DPI[slot].load(Ordering::Acquire) == dpi {
        return cached as HFONT;
    }
    unsafe {
        let height = -scale(dpi, if small { 12 } else { 14 });
        let name = to_wide("Segoe UI");
        let font = CreateFontW(
            height,
            0,
            0,
            0,
            FW_NORMAL,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            0,
            0,
            CLEARTYPE_QUALITY,
            0,
            pcw(&name),
        );
        if font.is_null() {
            return GetStockObject(17) as HFONT; // DEFAULT_GUI_FONT
        }
        let previous = FONTS[slot].swap(font as isize, Ordering::AcqRel);
        FONT_DPI[slot].store(dpi, Ordering::Release);
        if previous != 0 {
            DeleteObject(previous as HFONT);
        }
        font
    }
}

fn paint_dc() -> Option<HDC> {
    unsafe {
        let dc = windows_sys::Win32::Graphics::Gdi::GetDC(std::ptr::null_mut());
        if dc.is_null() {
            None
        } else {
            Some(dc)
        }
    }
}

unsafe extern "system" fn cbt_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HCBT_CREATEWND || code == HCBT_ACTIVATE {
        let hwnd = wparam as HWND;
        if is_menu_class(hwnd) {
            let round = DWMWCP_ROUND;
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &round as *const u32 as *const _,
                4,
            );
            let dark: i32 = i32::from(MENU_DARK.load(Ordering::Relaxed));
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &dark as *const i32 as *const _,
                4,
            );
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

unsafe fn is_menu_class(hwnd: HWND) -> bool {
    let mut buf = [0u16; 16];
    let length = GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
    length == 6 && buf[..6] == ['#' as u16, '3' as u16, '2' as u16, '7' as u16, '6' as u16, '8' as u16]
}

#[allow(dead_code)]
fn _keep(msg: MSG, uninit: u32) {
    let _ = (msg, uninit, WM_UNINITMENUPOPUP, DrawMenuBar);
}
