//! The one place AllBlack11's own windows get their colors, fonts and scale from.
//!
//! Settings, the command palette and the setup flow are child modules of `settings`, so they
//! all read the same thread-local theme through `th()` and build fonts through `make_fonts`.
//! Nothing else in the UI picks a color or a font by hand.
//!
//! Evidence: cross-compiled; rendered under Wine for layout only. Segoe UI Variable, Segoe
//! Fluent Icons, dark/light switching, high contrast and text scaling are unverified on
//! Windows.

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::Mutex;

use windows_sys::Win32::Graphics::Gdi::{
    CreateFontW, EnumFontFamiliesExW, GetSysColor, DEFAULT_CHARSET, HFONT, LOGFONTW,
};

use super::super::wide::{pcw, to_wide};
use super::super::{dwm, registry};
use crate::{
    accent_on, face_candidates, on_accent, resolve_appearance, ui_palette, Appearance, SysColors,
    TextStyle, UiMode, UiPalette, ICON_FONTS,
};

thread_local! {
    static THEME: Cell<UiPalette> = Cell::new(ui_palette(UiMode::Dark, &SysColors::default()));
    static MODE: Cell<UiMode> = const { Cell::new(UiMode::Dark) };
    static ACCENT: Cell<(u32, u32)> = const { Cell::new((0x00B5_9A3C, 0)) };
}

/// The current palette. Cheap: a copy of a dozen integers.
pub fn th() -> UiPalette {
    THEME.with(Cell::get)
}

pub fn mode() -> UiMode {
    MODE.with(Cell::get)
}

/// The accent color adjusted to be visible on this theme, and the text color for on top of it.
pub fn accent() -> (u32, u32) {
    ACCENT.with(Cell::get)
}

fn sys_colors() -> SysColors {
    // COLOR_WINDOW 5, COLOR_WINDOWTEXT 8, COLOR_HIGHLIGHT 13, COLOR_HIGHLIGHTTEXT 14,
    // COLOR_GRAYTEXT 17, COLOR_HOTLIGHT 26.
    unsafe {
        SysColors {
            window: GetSysColor(5),
            window_text: GetSysColor(8),
            gray_text: GetSysColor(17),
            highlight: GetSysColor(13),
            highlight_text: GetSysColor(14),
            hotlight: GetSysColor(26),
        }
    }
}

/// Re-read everything the theme depends on: the user's choice, Windows' light/dark setting,
/// high contrast, and the accent color. Returns the resolved mode.
pub fn refresh(choice: Appearance) -> UiMode {
    let high_contrast = dwm::high_contrast_enabled();
    let system_dark = registry::apps_prefer_dark().unwrap_or(true);
    let mode = resolve_appearance(choice, system_dark, high_contrast);
    let palette = ui_palette(mode, &sys_colors());
    let accent = accent_on(&palette, registry::accent_bgr() & 0x00FF_FFFF, mode);
    let accent_text = if mode == UiMode::HighContrast {
        sys_colors().highlight_text
    } else {
        on_accent(accent)
    };
    THEME.with(|cell| cell.set(palette));
    MODE.with(|cell| cell.set(mode));
    ACCENT.with(|cell| cell.set((accent, accent_text)));
    mode
}

// ---------------------------------------------------------------------------
// Text scale
// ---------------------------------------------------------------------------

/// Windows' "Text size" (Settings > Accessibility > Text size), 100 to 225 percent.
pub fn text_scale_percent() -> u32 {
    registry::text_scale_percent()
        .unwrap_or(100)
        .clamp(100, 225)
}

// ---------------------------------------------------------------------------
// Fonts
// ---------------------------------------------------------------------------

static INSTALLED: Mutex<Option<HashMap<String, bool>>> = Mutex::new(None);

unsafe extern "system" fn found(
    _font: *const LOGFONTW,
    _metric: *const windows_sys::Win32::Graphics::Gdi::TEXTMETRICW,
    _kind: u32,
    flag: isize,
) -> i32 {
    *(flag as *mut bool) = true;
    0
}

/// Is a font family installed? Cached, because the answer does not change while we run.
pub fn font_installed(name: &str) -> bool {
    let mut guard = INSTALLED.lock().unwrap_or_else(|err| err.into_inner());
    let cache = guard.get_or_insert_with(HashMap::new);
    if let Some(known) = cache.get(name) {
        return *known;
    }
    let mut present = false;
    unsafe {
        let mut query: LOGFONTW = std::mem::zeroed();
        query.lfCharSet = DEFAULT_CHARSET;
        for (slot, unit) in query
            .lfFaceName
            .iter_mut()
            .zip(name.encode_utf16().take(31))
        {
            *slot = unit;
        }
        let screen = windows_sys::Win32::Graphics::Gdi::GetDC(std::ptr::null_mut());
        EnumFontFamiliesExW(
            screen,
            &query,
            Some(found),
            &mut present as *mut bool as isize,
            0,
        );
        windows_sys::Win32::Graphics::Gdi::ReleaseDC(std::ptr::null_mut(), screen);
    }
    cache.insert(name.to_string(), present);
    present
}

/// The first installed family from a candidate list; the last entry is always accepted so a
/// font is always chosen (Segoe UI ships with every supported Windows).
pub fn first_installed(candidates: &[&str]) -> String {
    for name in candidates {
        if font_installed(name) {
            return (*name).to_string();
        }
    }
    candidates.last().copied().unwrap_or("Segoe UI").to_string()
}

fn create_font(face: &str, effective_dpi: u32, size: i32, weight: i32) -> HFONT {
    unsafe {
        let height = -((size * effective_dpi.max(96) as i32 + 48) / 96);
        let name = to_wide(face);
        // CLEARTYPE_QUALITY (5), default pitch and family.
        CreateFontW(height, 0, 0, 0, weight, 0, 0, 0, 1, 0, 0, 5, 0, pcw(&name))
    }
}

/// A font for a style from the type ramp. `effective_dpi` is 96 times the layout scale, which
/// already includes the user's text size.
pub fn style_font(style: TextStyle, effective_dpi: u32) -> HFONT {
    let family = first_installed(face_candidates(style.face));
    create_font(&family, effective_dpi, style.size, style.weight)
}

/// Is an icon font installed at all? Without one the navigation pane shows labels only.
pub fn icon_font_available() -> bool {
    ICON_FONTS.iter().any(|name| font_installed(name))
}

/// The icon font, or null when neither Segoe Fluent Icons nor Segoe MDL2 Assets is installed.
pub fn icon_font(effective_dpi: u32, size: i32) -> HFONT {
    for name in ICON_FONTS {
        if font_installed(name) {
            return create_font(name, effective_dpi, size, 400);
        }
    }
    std::ptr::null_mut()
}

/// Segoe Fluent Icons code points used by the navigation and a few buttons. Verified against
/// Microsoft's icon list (see UI-GUIDELINES.md). A glyph never stands alone: every use is next
/// to a text label or has an accessible name.
pub mod glyph {
    pub const HOME: char = '\u{E80F}';
    pub const SETTINGS: char = '\u{E713}';
    pub const INFO: char = '\u{E946}';
    pub const COLOR: char = '\u{E790}';
    pub const SEARCH: char = '\u{E721}';
    pub const SHIELD: char = '\u{EA18}';
    pub const PHOTO: char = '\u{E91B}';
    pub const TAG: char = '\u{E8EC}';
}
