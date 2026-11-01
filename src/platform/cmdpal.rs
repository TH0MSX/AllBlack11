//! The command palette window: a search box and a short list, opened with Ctrl+K in
//! Settings, from the tray menu, or from an optional global hotkey.
//!
//! It is a child module of `settings` so it shares that window's colors, fonts and painting.
//! It draws what `crate::commands` produces and sends the chosen action to the host window;
//! every action is something the tray or Settings can already do.
//!
//! It closes itself as soon as it loses focus, so it can never be left open behind other
//! windows, and it does nothing in the background: no timers, no polling, no hooks.
//!
//! Evidence: cross-compiled only. Never run on Windows hardware.

use super::*;
use crate::{PaletteAction, PaletteCommand};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowLongPtrW, GWLP_USERDATA, WA_INACTIVE, WM_ACTIVATE, WM_CHAR,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

static PALETTE: AtomicIsize = AtomicIsize::new(0);

const WIDTH: i32 = 560;
const INPUT_H: i32 = 54;
const ROW_H: i32 = 34;
const MAX_ROWS: usize = 9;
const ID_QUERY: u16 = 301;
const EN_CHANGE_CODE: u16 = 0x0300;
/// Sent by the edit box's subclass for navigation keys: wparam is the virtual-key code.
const WM_PAL_KEY: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 20;

struct Pal {
    owner: HWND,
    edit: HWND,
    all: Vec<PaletteCommand>,
    shown: Vec<usize>,
    selected: usize,
    k: f32,
    fonts: Fonts,
    bg_brush: HBRUSH,
    edit_brush: HBRUSH,
    accent: u32,
}

/// Open the palette, or close it if it is already open.
pub fn open(owner: HWND) {
    let existing = PALETTE.load(Ordering::Acquire);
    if existing != 0 {
        unsafe { DestroyWindow(existing as HWND) };
        return;
    }
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        register(instance);
        let class = to_wide("AllBlack11.Palette");
        let (x, y, w, _) = place(WIDTH, 100);
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            pcw(&class),
            std::ptr::null(),
            WS_POPUP | WS_VISIBLE | WS_CLIPCHILDREN,
            x,
            // A little above center, where the eye already is.
            y - 120,
            w,
            100,
            owner,
            std::ptr::null_mut(),
            instance,
            owner as *const _ as *const _,
        );
        if hwnd.is_null() {
            super::super::wide::log_line("error", "could not open the command palette");
        }
    }
}

fn register(instance: windows_sys::Win32::Foundation::HINSTANCE) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| unsafe {
        let class = to_wide("AllBlack11.Palette");
        let mut info: WNDCLASSEXW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        info.style = CS_HREDRAW | CS_VREDRAW;
        info.lpfnWndProc = Some(palette_proc);
        info.hInstance = instance;
        info.hCursor = LoadCursorW(std::ptr::null_mut(), IDC_ARROW);
        info.lpszClassName = pcw(&class);
        RegisterClassExW(&info);
    });
}

fn context() -> crate::PaletteContext {
    crate::PaletteContext {
        paused: super::super::app::is_paused(),
        can_undo_scene: super::super::app::can_undo_scene(),
        scenes: crate::scene_catalog().into_iter().map(|s| s.name).collect(),
        looks: super::cfgio::load_lookbook()
            .looks
            .into_iter()
            .map(|l| l.name)
            .collect(),
    }
}

unsafe fn state<'a>(hwnd: HWND) -> Option<&'a mut Pal> {
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Pal;
    pointer.as_mut()
}

fn refilter(pal: &mut Pal) {
    let query = get_text(pal.edit);
    pal.shown = crate::palette_search(&pal.all, &query, MAX_ROWS);
    pal.selected = 0;
}

fn resize(hwnd: HWND, pal: &Pal) {
    let rows = pal.shown.len().max(1) as i32;
    let height = (INPUT_H + rows * ROW_H + 10) as f32 * pal.k;
    unsafe {
        let mut rect = std::mem::zeroed();
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut rect);
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            rect.left,
            rect.top,
            rect.right - rect.left,
            height as i32,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

fn run(hwnd: HWND, pal: &Pal) {
    let Some(command) = pal.shown.get(pal.selected).and_then(|i| pal.all.get(*i)) else {
        return;
    };
    let action = command.action.clone();
    let owner = pal.owner;
    // Close first: some actions (Restart Explorer) show their own dialog.
    unsafe { DestroyWindow(hwnd) };
    use super::super::app::{
        CMD_DIAGNOSTICS, CMD_OPEN_FOLDER, CMD_PAUSE, CMD_RESET, CMD_RESTART_EXPLORER,
        CMD_SCENE_BASE, CMD_SCENE_UNDO,
    };
    let send = |command: u32| unsafe {
        PostMessageW(owner, WM_RUN_COMMAND, command as WPARAM, 0);
    };
    match action {
        PaletteAction::Page(index) => open_page(owner, index),
        PaletteAction::Scene(index) => send(CMD_SCENE_BASE + index as u32),
        PaletteAction::TogglePause => send(CMD_PAUSE),
        PaletteAction::UndoScene => send(CMD_SCENE_UNDO),
        PaletteAction::ResetWindows => send(CMD_RESET),
        PaletteAction::Diagnostics => send(CMD_DIAGNOSTICS),
        PaletteAction::OpenFolder => send(CMD_OPEN_FOLDER),
        PaletteAction::RestartExplorer => send(CMD_RESTART_EXPLORER),
        PaletteAction::RestoreLastKnownGood => {
            super::super::cmdq::push(super::super::cmdq::AppCommand::RestoreLastKnownGood);
            unsafe { PostMessageW(owner, super::super::cmdq::WM_APP_COMMAND, 0, 0) };
        }
    }
}

unsafe extern "system" fn edit_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    palette: usize,
) -> LRESULT {
    match msg {
        WM_KEYDOWN => {
            // Up, Down, Enter, Escape go to the list; everything else is typing.
            if matches!(wparam, 0x26 | 0x28 | 0x0D | 0x1B) {
                SendMessageW(palette as HWND, WM_PAL_KEY, wparam, 0);
                return 0;
            }
        }
        // Swallow the character form of Enter/Escape so the box does not beep.
        WM_CHAR if wparam == 0x0D || wparam == 0x1B => return 0,
        _ => {}
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

unsafe extern "system" fn palette_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let create = &*(lparam as *const CREATESTRUCTW);
            let owner = create.lpCreateParams as HWND;
            let dpi = dpi_of(hwnd);
            let k = dpi as f32 / 96.0;
            let fonts = Fonts::make(dpi);
            let edit = make_edit(hwnd, ID_QUERY, fonts.body);
            let all = crate::palette_commands(&context());
            let shown = crate::palette_search(&all, "", MAX_ROWS);
            let pal = Box::new(Pal {
                owner,
                edit,
                all,
                shown,
                selected: 0,
                k,
                fonts,
                bg_brush: CreateSolidBrush(th().card),
                edit_brush: CreateSolidBrush(th().edit_bg),
                accent: theme_accent().0,
            });
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(pal) as isize);
            SetWindowSubclass(edit, Some(edit_proc), 1, hwnd as usize);
            place_edit(
                edit,
                (20.0 * k) as i32,
                (14.0 * k) as i32,
                ((WIDTH - 40) as f32 * k) as i32,
                (26.0 * k) as i32,
                true,
            );
            PALETTE.store(hwnd as isize, Ordering::Release);
            uxtheme::allow_window(hwnd, true);
            // Overlays round at 8 px (Microsoft: geometry); the system draws the corners.
            let round: u32 = 2;
            DwmSetWindowAttribute(hwnd, 33, &round as *const u32 as *const _, 4);
            if let Some(pal) = state(hwnd) {
                resize(hwnd, pal);
            }
            SetForegroundWindow(hwnd);
            windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(edit);
            0
        }
        WM_ACTIVATE => {
            // Clicking anywhere else dismisses it.
            if (wparam & 0xFFFF) as u32 == WA_INACTIVE {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_COMMAND => {
            let code = ((wparam >> 16) & 0xFFFF) as u16;
            let id = (wparam & 0xFFFF) as u16;
            if code == EN_CHANGE_CODE && id == ID_QUERY {
                if let Some(pal) = state(hwnd) {
                    refilter(pal);
                    resize(hwnd, pal);
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                }
            }
            0
        }
        WM_PAL_KEY => {
            if let Some(pal) = state(hwnd) {
                match wparam {
                    0x26 if pal.selected > 0 => pal.selected -= 1,
                    0x28 if pal.selected + 1 < pal.shown.len() => pal.selected += 1,
                    0x0D => {
                        run(hwnd, pal);
                        return 0;
                    }
                    0x1B => {
                        DestroyWindow(hwnd);
                        return 0;
                    }
                    _ => {}
                }
                InvalidateRect(hwnd, std::ptr::null(), 0);
            }
            0
        }
        WM_LBUTTONDOWN => {
            if let Some(pal) = state(hwnd) {
                let y = (lparam_y(lparam) as f32 / pal.k) as i32 - INPUT_H;
                if y >= 0 {
                    let row = (y / ROW_H) as usize;
                    if row < pal.shown.len() {
                        pal.selected = row;
                        run(hwnd, pal);
                    }
                }
            }
            0
        }
        WM_CTLCOLOREDIT => {
            let dc = wparam as HDC;
            SetTextColor(dc, th().text);
            SetBkColor(dc, th().edit_bg);
            state(hwnd).map_or(0, |pal| pal.edit_brush as LRESULT)
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            if let Some(pal) = state(hwnd) {
                paint_palette(hwnd, pal);
            }
            0
        }
        WM_DESTROY => {
            PALETTE.store(0, Ordering::Release);
            let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Pal;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            if !pointer.is_null() {
                let pal = Box::from_raw(pointer);
                pal.fonts.delete();
                DeleteObject(pal.bg_brush);
                DeleteObject(pal.edit_brush);
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn paint_palette(hwnd: HWND, pal: &Pal) {
    let mut ps: windows_sys::Win32::Graphics::Gdi::PAINTSTRUCT = std::mem::zeroed();
    let screen = windows_sys::Win32::Graphics::Gdi::BeginPaint(hwnd, &mut ps);
    let (w, h) = client_size(hwnd);
    let dc = CreateCompatibleDC(screen);
    let bitmap = CreateCompatibleBitmap(screen, w.max(1), h.max(1));
    let old_bitmap = SelectObject(dc, bitmap);
    SetBkMode(dc, TRANSPARENT as i32);
    let full = RECT {
        left: 0,
        top: 0,
        right: w,
        bottom: h,
    };
    FillRect(dc, &full, pal.bg_brush);
    let c = Canvas { dc, k: pal.k };
    let lw = (w as f32 / pal.k) as i32;
    let lh = (h as f32 / pal.k) as i32;
    c.round(0, 0, lw, lh, 0, None, Some(th().edge));
    // The search box sits on its own darker strip so it reads as the input.
    c.fill(10, 10, lw - 10, INPUT_H - 10, th().edit_bg);
    if pal.shown.is_empty() {
        c.text(
            24,
            INPUT_H,
            lw - 24,
            INPUT_H + ROW_H,
            "Nothing matches. Try fewer words.",
            th().muted,
            pal.fonts.small,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
    }
    for (row, index) in pal.shown.iter().enumerate() {
        let Some(command) = pal.all.get(*index) else {
            continue;
        };
        let top = INPUT_H + row as i32 * ROW_H;
        if row == pal.selected {
            c.round(
                8,
                top,
                lw - 8,
                top + ROW_H - 2,
                5,
                Some(th().card_hi),
                Some(pal.accent),
            );
        }
        c.text(
            20,
            top,
            lw / 2 + 40,
            top + ROW_H - 2,
            &command.title,
            th().text,
            pal.fonts.body,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
        c.text(
            lw / 2 + 48,
            top,
            lw - 18,
            top + ROW_H - 2,
            &command.hint,
            th().muted,
            pal.fonts.small,
            DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
    }
    BitBlt(screen, 0, 0, w, h, dc, 0, 0, SRCCOPY);
    SelectObject(dc, old_bitmap);
    DeleteObject(bitmap);
    DeleteDC(dc);
    windows_sys::Win32::Graphics::Gdi::EndPaint(hwnd, &ps);
}
