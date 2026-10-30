//! Native settings window. No WebView and no extra crate: one Win32 window, GDI
//! painting, and a handful of EDIT controls.
//!
//! The frame uses Mica and immersive dark mode. The client is a solid near-black
//! fill so text stays sharp on OLED and the window does not need a composition
//! buffer of its own. Controls apply by rewriting `config.json` and posting
//! `WM_SETTINGS_APPLIED` back to the tray window. Sliders commit when the thumb
//! is released so a drag does not restyle every window on each pixel.

use std::sync::atomic::{AtomicIsize, Ordering};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::{
    CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, Ellipse, FillRect,
    GetStockObject, InvalidateRect, MonitorFromPoint, RoundRect, SelectObject, SetBkColor, SetBkMode,
    SetTextColor, HBRUSH, HFONT, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    NULL_PEN, PS_SOLID, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::SetWindowTheme;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetFocus;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetCursorPos,
    GetWindowTextW, LoadCursorW, PostMessageW, RegisterClassExW, SetForegroundWindow, SetWindowPos,
    SetWindowTextW, ShowWindow, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, EN_KILLFOCUS,
    GWL_STYLE, HMENU, IDC_ARROW, SW_SHOW, SWP_NOZORDER, WM_CLOSE, WM_COMMAND, WM_CREATE,
    WM_CTLCOLOREDIT,
    WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL,
    WM_PAINT, WM_SETFONT, WNDCLASSEXW, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_MINIMIZEBOX,
    WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};

use crate::config::{
    base_style, file_name, normalize_caption_height, parse_color, Backdrop, Color, Config, Corners,
    Opacity, PartialStyle, Rule, Style,
};

use super::registry;
use super::uxtheme;
use super::wide::{config_path, pcw, to_wide};

pub const WM_SETTINGS_APPLIED: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 4;
const WM_REFRESH: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 5;

const ID_CAPTION: u16 = 101;
const ID_BORDER: u16 = 102;
const ID_TEXT: u16 = 103;
const ID_TINT: u16 = 104;
const ID_RULE: u16 = 105;
const ID_EXCLUDE: u16 = 106;

const BG: u32 = 0x001B_1B1B;
const CARD: u32 = 0x0026_2626;
const TEXT: u32 = 0x00F3_F3F3;
const MUTED: u32 = 0x00A6_A6A6;
const TRACK: u32 = 0x003C_3C3C;
const ACCENT: u32 = 0x0074_8501;
const EDIT_BG: u32 = 0x002E_2E2E;

static SETTINGS: AtomicIsize = AtomicIsize::new(0);

struct Edits {
    caption: HWND,
    border: HWND,
    text: HWND,
    tint: HWND,
    rule: HWND,
    exclude: HWND,
}

struct Ui {
    owner: HWND,
    config: Config,
    autostart: bool,
    rule: Option<usize>,
    exclude_sel: Option<usize>,
    scroll: i32,
    drag: Option<Slider>,
    status: String,
    edits: Edits,
    font: HFONT,
    bg_brush: HBRUSH,
    edit_brush: HBRUSH,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Slider {
    Opacity,
    Caption,
}

enum Row {
    Header(&'static str),
    Note(&'static str),
    Toggle(Toggle, &'static str),
    Choice(Choice, &'static str),
    Slider(Slider, &'static str),
    Color(u16, &'static str),
    RuleList,
    ExcludeList,
    Button(Button, &'static str),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Toggle {
    Theme,
    DefaultOn,
    RuleOn,
    Dark,
    Blur,
    Oled,
    Black,
    Autostart,
    Reapply,
    Logging,
    Experimental,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Choice {
    Backdrop,
    Corners,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Button {
    UseDefault,
    AddRule,
    RemoveRule,
    AddExclude,
    RemoveExclude,
}

struct Laid {
    y: i32,
    h: i32,
    row: Row,
}

pub fn open(owner: HWND) {
    let existing = SETTINGS.load(Ordering::Acquire);
    if existing != 0 {
        unsafe {
            ShowWindow(existing as HWND, SW_SHOW);
            SetForegroundWindow(existing as HWND);
        }
        return;
    }
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        register(instance);
        let class = to_wide("AllBlack11.Settings");
        let title = to_wide("AllBlack11 Settings");
        let (x, y, w, h) = place(460, 700);
        let hwnd = CreateWindowExW(
            0,
            pcw(&class),
            pcw(&title),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_VISIBLE | WS_CLIPCHILDREN,
            x,
            y,
            w,
            h,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            owner as *const _ as *const _,
        );
        if hwnd.is_null() {
            super::wide::log_line("error", "could not open Settings");
        }
    }
}

pub fn notify_external_change() {
    let hwnd = SETTINGS.load(Ordering::Acquire);
    if hwnd != 0 {
        unsafe { PostMessageW(hwnd as HWND, WM_REFRESH, 0, 0) };
    }
}

fn register(instance: windows_sys::Win32::Foundation::HINSTANCE) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| unsafe {
        let class = to_wide("AllBlack11.Settings");
        let mut info: WNDCLASSEXW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        info.style = CS_HREDRAW | CS_VREDRAW;
        info.lpfnWndProc = Some(wndproc);
        info.hInstance = instance;
        info.hCursor = LoadCursorW(std::ptr::null_mut(), IDC_ARROW);
        info.lpszClassName = pcw(&class);
        RegisterClassExW(&info);
    });
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_CREATE {
        let create = &*(lparam as *const CREATESTRUCTW);
        let owner = create.lpCreateParams as HWND;
        let ui = Box::new(Ui::new(hwnd, owner));
        set_ui(hwnd, Box::into_raw(ui));
        SETTINGS.store(hwnd as isize, Ordering::Release);
        style_window(hwnd);
        return 0;
    }
    let ui = get_ui(hwnd);
    if ui.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let ui = &mut *ui;
    match msg {
        WM_ERASEBKGND => {
            let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            GetClientRect(hwnd, &mut rect);
            FillRect(wparam as windows_sys::Win32::Graphics::Gdi::HDC, &rect, ui.bg_brush);
            return 1;
        }
        WM_PAINT => {
            paint(hwnd, ui);
            return 0;
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam >> 16) as u16 as i16) as i32;
            ui.scroll = (ui.scroll - delta / 120 * ui.s(48)).max(0);
            ui.clamp_scroll(hwnd);
            ui.position_edits(hwnd);
            InvalidateRect(hwnd, std::ptr::null(), 1);
            return 0;
        }
        WM_LBUTTONDOWN => {
            let y = lparam_y(lparam) + ui.scroll;
            let x = lparam_x(lparam);
            if let Some(slider) = ui.hit_slider(x, y) {
                ui.drag = Some(slider);
                ui.set_slider(slider, x);
                InvalidateRect(hwnd, std::ptr::null(), 1);
            }
            return 0;
        }
        WM_MOUSEMOVE => {
            if let Some(slider) = ui.drag {
                ui.set_slider(slider, lparam_x(lparam));
                InvalidateRect(hwnd, std::ptr::null(), 1);
            }
            return 0;
        }
        WM_LBUTTONUP => {
            let y = lparam_y(lparam) + ui.scroll;
            let x = lparam_x(lparam);
            if ui.drag.take().is_some() {
                ui.commit();
            } else {
                ui.click(x, y);
            }
            InvalidateRect(hwnd, std::ptr::null(), 1);
            return 0;
        }
        WM_COMMAND => {
            let id = (wparam & 0xFFFF) as u16;
            let code = ((wparam >> 16) & 0xFFFF) as u16;
            if code == EN_KILLFOCUS as u16 {
                ui.edit_committed(id);
                InvalidateRect(hwnd, std::ptr::null(), 1);
            }
            return 0;
        }
        WM_CTLCOLOREDIT => {
            let dc = wparam as windows_sys::Win32::Graphics::Gdi::HDC;
            SetTextColor(dc, TEXT);
            SetBkColor(dc, EDIT_BG);
            return ui.edit_brush as LRESULT;
        }
        WM_KEYDOWN => {
            if wparam == 0x1B {
                DestroyWindow(hwnd);
            }
            return 0;
        }
        WM_REFRESH => {
            ui.reload_from_disk();
            InvalidateRect(hwnd, std::ptr::null(), 1);
            return 0;
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            return 0;
        }
        WM_DESTROY => {
            SETTINGS.store(0, Ordering::Release);
            let ui = Box::from_raw(take_ui(hwnd));
            if !ui.font.is_null() {
                DeleteObject(ui.font);
            }
            if !ui.bg_brush.is_null() {
                DeleteObject(ui.bg_brush);
            }
            if !ui.edit_brush.is_null() {
                DeleteObject(ui.edit_brush);
            }
            return 0;
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn style_window(hwnd: HWND) {
    unsafe {
        let dark: i32 = 1;
        DwmSetWindowAttribute(hwnd, 20, &dark as *const i32 as *const _, 4);
        let mica: i32 = 2;
        DwmSetWindowAttribute(hwnd, 38, &mica as *const i32 as *const _, 4);
        let round: u32 = 2;
        DwmSetWindowAttribute(hwnd, 33, &round as *const u32 as *const _, 4);
        uxtheme::allow_window(hwnd, true);
    }
}

fn set_ui(hwnd: HWND, ui: *mut Ui) {
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
            hwnd,
            windows_sys::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
            ui as isize,
        );
    }
}

fn get_ui(hwnd: HWND) -> *mut Ui {
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
            hwnd,
            windows_sys::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
        ) as *mut Ui
    }
}

fn take_ui(hwnd: HWND) -> *mut Ui {
    let ui = get_ui(hwnd);
    set_ui(hwnd, std::ptr::null_mut());
    ui
}

fn lparam_x(lparam: LPARAM) -> i32 {
    (lparam as u16 as i16) as i32
}
fn lparam_y(lparam: LPARAM) -> i32 {
    ((lparam >> 16) as u16 as i16) as i32
}

impl Ui {
    fn new(hwnd: HWND, owner: HWND) -> Self {
        let config = load_config();
        let dpi = dpi_of(hwnd);
        let font = make_font(dpi);
        let edits = Edits {
            caption: make_edit(hwnd, ID_CAPTION, font),
            border: make_edit(hwnd, ID_BORDER, font),
            text: make_edit(hwnd, ID_TEXT, font),
            tint: make_edit(hwnd, ID_TINT, font),
            rule: make_edit(hwnd, ID_RULE, font),
            exclude: make_edit(hwnd, ID_EXCLUDE, font),
        };
        let ui = Self {
            owner,
            autostart: registry::autostart_enabled(),
            rule: None,
            exclude_sel: None,
            scroll: 0,
            drag: None,
            status: String::new(),
            config,
            edits,
            font,
            bg_brush: unsafe { CreateSolidBrush(BG) },
            edit_brush: unsafe { CreateSolidBrush(EDIT_BG) },
        };
        ui.push_colors_to_edits();
        ui
    }

    fn s(&self, px: i32) -> i32 {
        let dpi = 96i32;
        let _ = dpi;
        px
    }

    fn dpi(&self, hwnd: HWND) -> u32 {
        let _ = hwnd;
        dpi_of(hwnd)
    }

    fn scale(&self, hwnd: HWND, px: i32) -> i32 {
        let dpi = self.dpi(hwnd) as i32;
        (px * dpi.max(96) + 48) / 96
    }

    fn reload_from_disk(&mut self) {
        let focus = unsafe { GetFocus() };
        let editing = focus == self.edits.caption
            || focus == self.edits.border
            || focus == self.edits.text
            || focus == self.edits.tint
            || focus == self.edits.rule
            || focus == self.edits.exclude;
        self.config = load_config();
        self.autostart = registry::autostart_enabled();
        if self.rule.is_some_and(|index| index >= self.config.rules.len()) {
            self.rule = None;
        }
        if !editing {
            self.push_colors_to_edits();
        }
    }

    fn push_colors_to_edits(&self) {
        let style = self.current_style();
        set_text(self.edits.caption, &color_token(style.caption));
        set_text(self.edits.border, &color_token(style.border));
        set_text(self.edits.text, &color_token(style.text));
        set_text(self.edits.tint, &color_token(style.acrylic_tint));
    }

    fn current_style(&self) -> Style {
        let base = self.config.defaults.merge_onto(base_style());
        match self.rule.and_then(|index| self.config.rules.get(index)) {
            Some(rule) => rule.style.merge_onto(base),
            None => base,
        }
    }

    fn write_style(&mut self, style: Style) {
        let mut partial = PartialStyle {
            enabled: None,
            dark_mode: Some(style.dark_mode),
            backdrop: Some(style.backdrop),
            caption_color: Some(style.caption),
            border_color: Some(style.border),
            text_color: Some(style.text),
            corners: Some(style.corners),
            acrylic_tint: Some(style.acrylic_tint),
            acrylic_opacity: Some(Opacity(style.acrylic_opacity)),
            tinted_acrylic_on_move_fallback: Some(style.tinted_acrylic_on_move_fallback),
        };
        if let Some(index) = self.rule {
            if let Some(rule) = self.config.rules.get_mut(index) {
                partial.enabled = rule.style.enabled.or(Some(true));
                rule.style = partial;
            }
        } else {
            partial.enabled = self.config.defaults.enabled;
            self.config.defaults = partial;
        }
    }

    fn commit(&mut self) {
        self.status.clear();
        if let Err(err) = store_config(&self.config) {
            self.status = err;
            return;
        }
        unsafe { PostMessageW(self.owner, WM_SETTINGS_APPLIED, 0, 0) };
    }

    fn click(&mut self, x: i32, y: i32) {
        let rows = self.rows_at_width(520);
        for row in &rows {
            if y < row.y || y >= row.y + row.h {
                continue;
            }
            match &row.row {
                Row::Toggle(id, _) => self.toggle(*id),
                Row::Choice(id, _) => self.cycle(*id),
                Row::Button(id, _) => self.button(*id),
                Row::RuleList => self.pick_rule(y - row.y),
                Row::ExcludeList => self.pick_exclude(y - row.y),
                _ => {}
            }
            let _ = x;
            return;
        }
    }

    fn toggle(&mut self, id: Toggle) {
        match id {
            Toggle::Theme => self.config.enabled = !self.config.enabled,
            Toggle::DefaultOn => {
                let on = !self.config.defaults.enabled.unwrap_or(true);
                self.config.defaults.enabled = Some(on);
            }
            Toggle::RuleOn => {
                if let Some(index) = self.rule {
                    if let Some(rule) = self.config.rules.get_mut(index) {
                        let on = !rule.style.enabled.unwrap_or(true);
                        rule.style.enabled = Some(on);
                    }
                }
            }
            Toggle::Dark => {
                let mut style = self.current_style();
                style.dark_mode = !style.dark_mode;
                self.write_style(style);
            }
            Toggle::Blur => {
                let mut style = self.current_style();
                style.tinted_acrylic_on_move_fallback = !style.tinted_acrylic_on_move_fallback;
                self.write_style(style);
            }
            Toggle::Oled => self.config.shell.oled_taskbar = !self.config.shell.oled_taskbar,
            Toggle::Black => self.config.shell.black_shell = !self.config.shell.black_shell,
            Toggle::Autostart => {
                let next = !self.autostart;
                if let Err(err) = registry::set_autostart(next) {
                    self.status = err;
                    return;
                }
                self.autostart = next;
                self.status = if next {
                    "Start with Windows is on.".into()
                } else {
                    "Start with Windows is off.".into()
                };
                return;
            }
            Toggle::Reapply => self.config.reapply_on_foreground = !self.config.reapply_on_foreground,
            Toggle::Logging => self.config.logging = !self.config.logging,
            Toggle::Experimental => {
                self.config.experimental_dark_legacy = !self.config.experimental_dark_legacy;
                self.status = if self.config.experimental_dark_legacy {
                    "Experimental dark dialogs are on. Classic dialogs may be recolored in-process.".into()
                } else {
                    "Experimental dark dialogs are off. Injected hooks are removed.".into()
                };
            }
        }
        self.commit();
    }

    fn cycle(&mut self, id: Choice) {
        let mut style = self.current_style();
        match id {
            Choice::Backdrop => style.backdrop = next_backdrop(style.backdrop),
            Choice::Corners => style.corners = next_corners(style.corners),
        }
        self.write_style(style);
        self.commit();
    }

    fn button(&mut self, id: Button) {
        match id {
            Button::UseDefault => {
                self.rule = None;
                self.push_colors_to_edits();
            }
            Button::AddRule => {
                let name = file_name(&get_text(self.edits.rule)).trim().to_string();
                if name.is_empty() {
                    self.status = "Type an exe name, such as notepad.exe.".into();
                    return;
                }
                if let Some(index) = self
                    .config
                    .rules
                    .iter()
                    .position(|rule| rule.process.eq_ignore_ascii_case(&name))
                {
                    self.rule = Some(index);
                } else {
                    self.config.rules.push(Rule {
                        process: name,
                        style: PartialStyle {
                            enabled: Some(true),
                            ..PartialStyle::default()
                        },
                    });
                    self.rule = Some(self.config.rules.len() - 1);
                }
                set_text(self.edits.rule, "");
                self.push_colors_to_edits();
                self.commit();
            }
            Button::RemoveRule => {
                if let Some(index) = self.rule {
                    if index < self.config.rules.len() {
                        self.config.rules.remove(index);
                    }
                    self.rule = None;
                    self.push_colors_to_edits();
                    self.commit();
                }
            }
            Button::AddExclude => {
                let name = file_name(&get_text(self.edits.exclude)).trim().to_string();
                if name.is_empty() {
                    self.status = "Type an exe name to exclude.".into();
                    return;
                }
                if !self.config.exclude.iter().any(|item| item.eq_ignore_ascii_case(&name)) {
                    self.config.exclude.push(name);
                }
                set_text(self.edits.exclude, "");
                self.commit();
            }
            Button::RemoveExclude => {
                if let Some(index) = self.exclude_sel {
                    if index < self.config.exclude.len() {
                        self.config.exclude.remove(index);
                    }
                    self.exclude_sel = None;
                    self.commit();
                }
            }
        }
    }

    fn edit_committed(&mut self, id: u16) {
        let hwnd = match id {
            ID_CAPTION => self.edits.caption,
            ID_BORDER => self.edits.border,
            ID_TEXT => self.edits.text,
            ID_TINT => self.edits.tint,
            _ => return,
        };
        let text = get_text(hwnd);
        match parse_color(text.trim()) {
            Ok(color) => {
                let mut style = self.current_style();
                match id {
                    ID_CAPTION => style.caption = color,
                    ID_BORDER => style.border = color,
                    ID_TEXT => style.text = color,
                    ID_TINT => style.acrylic_tint = color,
                    _ => {}
                }
                self.write_style(style);
                self.status.clear();
                self.commit();
            }
            Err(err) => {
                self.status = err;
                self.push_colors_to_edits();
            }
        }
    }

    fn set_slider(&mut self, slider: Slider, x: i32) {
        let fraction = ((x - 150) as f32 / 280.0).clamp(0.0, 1.0);
        match slider {
            Slider::Opacity => {
                let mut style = self.current_style();
                style.acrylic_opacity = (fraction * 255.0).round() as u8;
                self.write_style(style);
            }
            Slider::Caption => {
                if fraction < 0.04 {
                    self.config.caption_height = None;
                } else {
                    let pixels = 16.0 + fraction * 48.0;
                    self.config.caption_height = Some(pixels.round() as u32);
                }
            }
        }
    }

    fn hit_slider(&self, x: i32, y: i32) -> Option<Slider> {
        for row in self.rows_at_width(520) {
            if y >= row.y && y < row.y + row.h {
                if let Row::Slider(id, _) = row.row {
                    if x > 140 {
                        return Some(id);
                    }
                }
            }
        }
        None
    }

    fn pick_rule(&mut self, local_y: i32) {
        let index = (local_y / 24) as usize;
        if index < self.config.rules.len() {
            self.rule = Some(index);
            self.push_colors_to_edits();
        }
    }

    fn pick_exclude(&mut self, local_y: i32) {
        let index = (local_y / 24) as usize;
        if index < self.config.exclude.len() {
            self.exclude_sel = Some(index);
        }
    }

    fn position_edits(&self, hwnd: HWND) {
        let rows = self.rows_at_width(client_width(hwnd));
        for row in &rows {
            if let Row::Color(id, _) = row.row {
                let edit = self.edit_hwnd(id);
                place_edit(edit, client_width(hwnd) - self.scale(hwnd, 128), row.y - self.scroll + 6, self.scale(hwnd, 112), self.scale(hwnd, 22));
            }
            if let Row::Button(Button::AddRule, _) = row.row {
                place_edit(self.edits.rule, self.scale(hwnd, 16), row.y - self.scroll + 4, self.scale(hwnd, 220), self.scale(hwnd, 22));
            }
            if let Row::Button(Button::AddExclude, _) = row.row {
                place_edit(self.edits.exclude, self.scale(hwnd, 16), row.y - self.scroll + 4, self.scale(hwnd, 220), self.scale(hwnd, 22));
            }
        }
    }

    fn edit_hwnd(&self, id: u16) -> HWND {
        match id {
            ID_CAPTION => self.edits.caption,
            ID_BORDER => self.edits.border,
            ID_TEXT => self.edits.text,
            _ => self.edits.tint,
        }
    }

    fn clamp_scroll(&mut self, hwnd: HWND) {
        let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        unsafe { GetClientRect(hwnd, &mut rect) };
        let view = (rect.bottom - rect.top).max(1);
        let content = self.content_height().max(view);
        let max_scroll = content - view;
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
        if self.scroll < 0 {
            self.scroll = 0;
        }
    }

    fn content_height(&self) -> i32 {
        self.rows_at_width(520).last().map(|row| row.y + row.h + 16).unwrap_or(0)
    }

    fn rows_at_width(&self, _width: i32) -> Vec<Laid> {
        let mut y = 12;
        let mut out = Vec::new();
        let push = |out: &mut Vec<Laid>, y: &mut i32, h: i32, row: Row| {
            out.push(Laid { y: *y, h, row });
            *y += h;
        };
        let editing = match self.rule.and_then(|index| self.config.rules.get(index)) {
            Some(rule) => format!("Frames — {}", rule.process),
            None => "Frames — all apps".into(),
        };
        let _ = editing;
        push(&mut out, &mut y, 28, Row::Header(if self.rule.is_some() { "Frames for one app" } else { "Frames" }));
        push(&mut out, &mut y, 32, Row::Toggle(Toggle::Theme, "Theme windows"));
        if self.rule.is_some() {
            push(&mut out, &mut y, 32, Row::Toggle(Toggle::RuleOn, "This rule is on"));
            push(&mut out, &mut y, 28, Row::Button(Button::UseDefault, "Edit the default instead"));
        } else {
            push(&mut out, &mut y, 32, Row::Toggle(Toggle::DefaultOn, "Default frames on"));
        }
        push(&mut out, &mut y, 32, Row::Toggle(Toggle::Dark, "Dark title bar"));
        push(&mut out, &mut y, 32, Row::Choice(Choice::Backdrop, "Backdrop"));
        push(&mut out, &mut y, 32, Row::Choice(Choice::Corners, "Corners"));
        push(&mut out, &mut y, 32, Row::Color(ID_CAPTION, "Caption"));
        push(&mut out, &mut y, 32, Row::Color(ID_BORDER, "Border"));
        push(&mut out, &mut y, 32, Row::Color(ID_TEXT, "Text"));
        push(&mut out, &mut y, 32, Row::Color(ID_TINT, "Acrylic tint"));
        push(&mut out, &mut y, 40, Row::Slider(Slider::Opacity, "Acrylic opacity"));
        push(&mut out, &mut y, 32, Row::Toggle(Toggle::Blur, "Blur while dragging"));
        push(&mut out, &mut y, 28, Row::Header("Legacy title bar"));
        push(&mut out, &mut y, 40, Row::Slider(Slider::Caption, "Caption height"));
        push(&mut out, &mut y, 36, Row::Note("Sign out and back in for CaptionHeight. System restores the previous value."));
        push(&mut out, &mut y, 28, Row::Header("Shell"));
        push(&mut out, &mut y, 32, Row::Toggle(Toggle::Oled, "OLED taskbar"));
        push(&mut out, &mut y, 32, Row::Toggle(Toggle::Black, "Black Start, taskbar and notification center"));
        push(&mut out, &mut y, 28, Row::Header("Startup"));
        push(&mut out, &mut y, 32, Row::Toggle(Toggle::Autostart, "Start with Windows"));
        push(&mut out, &mut y, 32, Row::Toggle(Toggle::Reapply, "Reapply when a window is focused"));
        push(&mut out, &mut y, 32, Row::Toggle(Toggle::Logging, "Write a log"));
        push(&mut out, &mut y, 28, Row::Header("Per-app rules"));
        push(&mut out, &mut y, 24 * 4, Row::RuleList);
        push(&mut out, &mut y, 32, Row::Button(Button::AddRule, "Add"));
        push(&mut out, &mut y, 28, Row::Button(Button::RemoveRule, "Remove selected rule"));
        push(&mut out, &mut y, 28, Row::Header("Exclusions"));
        push(&mut out, &mut y, 24 * 4, Row::ExcludeList);
        push(&mut out, &mut y, 32, Row::Button(Button::AddExclude, "Add"));
        push(&mut out, &mut y, 28, Row::Button(Button::RemoveExclude, "Remove selected exclusion"));
        push(&mut out, &mut y, 28, Row::Header("Experimental"));
        push(&mut out, &mut y, 32, Row::Toggle(Toggle::Experimental, "Dark legacy windows"));
        push(
            &mut out,
            &mut y,
            64,
            Row::Note("Off by default. Uses a same-architecture hook DLL. Not for games, launchers, or elevated processes. Antivirus products may flag it."),
        );
        if !self.status.is_empty() {
            push(&mut out, &mut y, 40, Row::Note("Status"));
        }
        out
    }
}

fn paint(hwnd: HWND, ui: &mut Ui) {
    unsafe {
        let mut paint: windows_sys::Win32::Graphics::Gdi::PAINTSTRUCT = std::mem::zeroed();
        let dc = windows_sys::Win32::Graphics::Gdi::BeginPaint(hwnd, &mut paint);
        let mut client = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        GetClientRect(hwnd, &mut client);
        FillRect(dc, &client, ui.bg_brush);
        let previous = SelectObject(dc, ui.font);
        SetBkMode(dc, TRANSPARENT as i32);
        let width = client.right - client.left;
        ui.position_edits(hwnd);
        for row in ui.rows_at_width(width) {
            let top = row.y - ui.scroll;
            if top > client.bottom || top + row.h < 0 {
                continue;
            }
            paint_row(dc, ui, &row, top, width);
        }
        if !previous.is_null() {
            SelectObject(dc, previous);
        }
        windows_sys::Win32::Graphics::Gdi::EndPaint(hwnd, &paint);
    }
}

unsafe fn paint_row(dc: windows_sys::Win32::Graphics::Gdi::HDC, ui: &Ui, row: &Laid, top: i32, width: i32) {
        let rect = RECT { left: 16, top, right: width - 16, bottom: top + row.h };
        match &row.row {
            Row::Header(text) => draw_label(dc, &rect, text, TEXT),
            Row::Note(text) => {
                if *text == "Status" {
                    draw_label(dc, &rect, &ui.status, ACCENT);
                } else {
                    draw_label(dc, &rect, text, MUTED);
                }
            }
            Row::Toggle(id, label) => {
                draw_label(dc, &rect, label, TEXT);
                draw_toggle(dc, width - 64, top + 6, toggle_on(ui, *id));
            }
            Row::Choice(id, label) => {
                draw_label(dc, &rect, label, TEXT);
                let value = choice_text(ui, *id);
                let mut value_rect = rect;
                value_rect.left = width - 180;
                draw_label(dc, &value_rect, &value, ACCENT);
            }
            Row::Slider(id, label) => {
                draw_label(dc, &rect, label, TEXT);
                let (fraction, caption) = slider_state(ui, *id);
                draw_slider(dc, 160, top + 16, width - 180, fraction);
                let mut value_rect = rect;
                value_rect.left = width - 70;
                draw_label(dc, &value_rect, &caption, MUTED);
            }
            Row::Color(_, label) => {
                let style = ui.current_style();
                let color = match *label {
                    "Caption" => style.caption,
                    "Border" => style.border,
                    "Text" => style.text,
                    _ => style.acrylic_tint,
                };
                draw_label(dc, &rect, label, TEXT);
                draw_swatch(dc, width - 156, top + 8, color);
            }
            Row::RuleList => draw_list(dc, &rect, ui.config.rules.iter().map(|rule| rule.process.as_str()), ui.rule),
            Row::ExcludeList => draw_list(dc, &rect, ui.config.exclude.iter().map(|item| item.as_str()), ui.exclude_sel),
            Row::Button(_, label) => draw_label(dc, &rect, label, ACCENT),
        }
}

fn toggle_on(ui: &Ui, id: Toggle) -> bool {
    let style = ui.current_style();
    match id {
        Toggle::Theme => ui.config.enabled,
        Toggle::DefaultOn => ui.config.defaults.enabled.unwrap_or(true),
        Toggle::RuleOn => ui
            .rule
            .and_then(|index| ui.config.rules.get(index))
            .and_then(|rule| rule.style.enabled)
            .unwrap_or(true),
        Toggle::Dark => style.dark_mode,
        Toggle::Blur => style.tinted_acrylic_on_move_fallback,
        Toggle::Oled => ui.config.shell.oled_taskbar,
        Toggle::Black => ui.config.shell.black_shell,
        Toggle::Autostart => ui.autostart,
        Toggle::Reapply => ui.config.reapply_on_foreground,
        Toggle::Logging => ui.config.logging,
        Toggle::Experimental => ui.config.experimental_dark_legacy,
    }
}

fn choice_text(ui: &Ui, id: Choice) -> String {
    let style = ui.current_style();
    match id {
        Choice::Backdrop => backdrop_label(style.backdrop).into(),
        Choice::Corners => corners_label(style.corners).into(),
    }
}

fn slider_state(ui: &Ui, id: Slider) -> (f32, String) {
    match id {
        Slider::Opacity => {
            let byte = ui.current_style().acrylic_opacity;
            let fraction = f32::from(byte) / 255.0;
            (fraction, format!("{:.0}%", fraction * 100.0))
        }
        Slider::Caption => match normalize_caption_height(ui.config.caption_height) {
            None => (0.0, "System".into()),
            Some(px) => ((px as f32 - 16.0) / 48.0, format!("{px} px")),
        },
    }
}

fn draw_label(dc: windows_sys::Win32::Graphics::Gdi::HDC, rect: &RECT, text: &str, color: u32) {
    unsafe {
        SetTextColor(dc, color);
        let mut rect = *rect;
        let wide = to_wide(text);
        DrawTextW(dc, wide.as_ptr(), -1, &mut rect, 0x0020 | 0x0004 | 0x0800 | 0x0010);
    }
}

fn draw_toggle(dc: windows_sys::Win32::Graphics::Gdi::HDC, x: i32, y: i32, on: bool) {
    unsafe {
        let fill = CreateSolidBrush(if on { ACCENT } else { TRACK });
        let old = SelectObject(dc, fill);
        let pen = CreatePen(PS_SOLID, 1, if on { ACCENT } else { TRACK });
        let old_pen = SelectObject(dc, pen);
        RoundRect(dc, x, y, x + 40, y + 20, 20, 20);
        SelectObject(dc, old_pen);
        SelectObject(dc, old);
        DeleteObject(pen);
        DeleteObject(fill);
        let knob = CreateSolidBrush(0x00FF_FFFF);
        let old = SelectObject(dc, knob);
        let null_pen = GetStockObject(NULL_PEN);
        let old_pen = SelectObject(dc, null_pen);
        let cx = if on { x + 24 } else { x + 4 };
        Ellipse(dc, cx, y + 3, cx + 14, y + 17);
        SelectObject(dc, old_pen);
        SelectObject(dc, old);
        DeleteObject(knob);
    }
}

fn draw_slider(dc: windows_sys::Win32::Graphics::Gdi::HDC, x: i32, y: i32, width: i32, fraction: f32) {
    unsafe {
        let track = CreateSolidBrush(TRACK);
        let rect = RECT { left: x, top: y, right: x + width, bottom: y + 4 };
        FillRect(dc, &rect, track);
        DeleteObject(track);
        let thumb_x = x + (fraction.clamp(0.0, 1.0) * width as f32) as i32;
        let knob = CreateSolidBrush(ACCENT);
        let old = SelectObject(dc, knob);
        let pen = CreatePen(PS_SOLID, 1, ACCENT);
        let old_pen = SelectObject(dc, pen);
        Ellipse(dc, thumb_x - 6, y - 5, thumb_x + 6, y + 9);
        SelectObject(dc, old_pen);
        SelectObject(dc, old);
        DeleteObject(pen);
        DeleteObject(knob);
        let _ = rect;
    }
}

fn draw_swatch(dc: windows_sys::Win32::Graphics::Gdi::HDC, x: i32, y: i32, color: Color) {
    let rgb = match color {
        Color::Rgb { r, g, b } => (b as u32) << 16 | (g as u32) << 8 | r as u32,
        Color::Accent => ACCENT,
        Color::None => 0x0044_4444,
        Color::Default => 0x0088_8888,
    };
    unsafe {
        let brush = CreateSolidBrush(rgb);
        let rect = RECT { left: x, top: y, right: x + 18, bottom: y + 14 };
        FillRect(dc, &rect, brush);
        DeleteObject(brush);
    }
}

fn draw_list<'a>(
    dc: windows_sys::Win32::Graphics::Gdi::HDC,
    rect: &RECT,
    items: impl Iterator<Item = &'a str>,
    selected: Option<usize>,
) {
    unsafe {
        let card = CreateSolidBrush(CARD);
        FillRect(dc, rect, card);
        DeleteObject(card);
        for (index, item) in items.enumerate().take(4) {
            let mut line = RECT {
                left: rect.left + 8,
                top: rect.top + index as i32 * 24,
                right: rect.right - 8,
                bottom: rect.top + index as i32 * 24 + 24,
            };
            if selected == Some(index) {
                let hi = CreateSolidBrush(0x0032_3232);
                FillRect(dc, &line, hi);
                DeleteObject(hi);
            }
            draw_label(dc, &line, item, TEXT);
            let _ = &mut line;
        }
    }
}

fn next_backdrop(value: Backdrop) -> Backdrop {
    match value {
        Backdrop::Auto => Backdrop::None,
        Backdrop::None => Backdrop::Mica,
        Backdrop::Mica => Backdrop::MicaAlt,
        Backdrop::MicaAlt => Backdrop::Acrylic,
        Backdrop::Acrylic => Backdrop::TintedAcrylic,
        Backdrop::TintedAcrylic => Backdrop::Auto,
    }
}

fn next_corners(value: Corners) -> Corners {
    match value {
        Corners::Default => Corners::Round,
        Corners::Round => Corners::Small,
        Corners::Small => Corners::Square,
        Corners::Square => Corners::Default,
    }
}

fn backdrop_label(value: Backdrop) -> &'static str {
    match value {
        Backdrop::Auto => "Auto",
        Backdrop::None => "None",
        Backdrop::Mica => "Mica",
        Backdrop::MicaAlt => "Mica alt",
        Backdrop::Acrylic => "Acrylic",
        Backdrop::TintedAcrylic => "Tinted acrylic",
    }
}

fn corners_label(value: Corners) -> &'static str {
    match value {
        Corners::Default => "Default",
        Corners::Round => "Round",
        Corners::Small => "Small",
        Corners::Square => "Square",
    }
}

fn color_token(color: Color) -> String {
    match color {
        Color::Default => "default".into(),
        Color::None => "none".into(),
        Color::Accent => "accent".into(),
        Color::Rgb { r, g, b } => format!("#{r:02X}{g:02X}{b:02X}"),
    }
}

fn load_config() -> Config {
    let path = config_path();
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(config) = crate::config::parse_config(&text) {
            return config;
        }
    }
    crate::config::parse_config(crate::config::bundled_default_json()).expect("bundled config")
}

fn store_config(config: &Config) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let text = serde_json::to_string_pretty(config).map_err(|err| err.to_string())?;
    std::fs::write(path, format!("{text}\n")).map_err(|err| err.to_string())
}

fn make_edit(parent: HWND, id: u16, font: HFONT) -> HWND {
    unsafe {
        let class = to_wide("EDIT");
        let hwnd = CreateWindowExW(
            0,
            pcw(&class),
            std::ptr::null(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | 0x0080,
            0,
            -40,
            10,
            10,
            parent,
            id as HMENU,
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        );
        let theme = to_wide("DarkMode_CFD");
        SetWindowTheme(hwnd, pcw(&theme), std::ptr::null());
        uxtheme::allow_window(hwnd, true);
        if !font.is_null() {
            windows_sys::Win32::UI::WindowsAndMessaging::SendMessageW(hwnd, WM_SETFONT, font as WPARAM, 1);
        }
        hwnd
    }
}

fn make_font(dpi: u32) -> HFONT {
    unsafe {
        let height = -((15 * dpi.max(96) as i32 + 48) / 96);
        let name = to_wide("Segoe UI");
        CreateFontW(height, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, pcw(&name))
    }
}

fn place_edit(hwnd: HWND, x: i32, y: i32, w: i32, h: i32) {
    unsafe {
        SetWindowPos(hwnd, std::ptr::null_mut(), x, y, w, h, SWP_NOZORDER);
    }
}

fn set_text(hwnd: HWND, text: &str) {
    let wide = to_wide(text);
    unsafe { SetWindowTextW(hwnd, pcw(&wide)) };
}

fn get_text(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    let length = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if length <= 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&buf[..length as usize])
    }
}

fn client_width(hwnd: HWND) -> i32 {
    let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe { GetClientRect(hwnd, &mut rect) };
    (rect.right - rect.left).max(320)
}

fn dpi_of(hwnd: HWND) -> u32 {
    unsafe {
        let dpi = GetDpiForWindow(hwnd);
        if dpi == 0 {
            96
        } else {
            dpi
        }
    }
}

fn place(width: i32, height: i32) -> (i32, i32, i32, i32) {
    unsafe {
        let mut cursor = POINT { x: 0, y: 0 };
        GetCursorPos(&mut cursor);
        let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if windows_sys::Win32::Graphics::Gdi::GetMonitorInfoW(monitor, &mut info) == 0 {
            return (CW_USEDEFAULT, CW_USEDEFAULT, width, height);
        }
        let work = info.rcWork;
        let x = work.left + ((work.right - work.left) - width) / 2;
        let y = work.top + ((work.bottom - work.top) - height) / 2;
        (x, y, width, height)
    }
}

#[allow(dead_code)]
fn _style(style: u32) -> u32 {
    style & GWL_STYLE as u32
}
