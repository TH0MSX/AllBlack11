//! Native Settings window. No WebView and no extra crate: one Win32 window, GDI
//! painting, and a handful of EDIT controls.
//!
//! Layout rule: every coordinate in this file is *logical* (96 dpi) and is scaled
//! exactly once, in `Canvas`, `Ui::px`, and the mouse mapping. Painting, hit-testing,
//! and the EDIT controls all read the same logical rectangles, so they cannot drift
//! apart on a 150% or 200% ARM laptop panel.
//!
//! The top of the window is a pinned live preview (a mock window, Start menu and
//! taskbar redrawn from the current config) and a status line. Everything under it
//! scrolls. Controls apply by rewriting `config.json` and posting
//! `WM_SETTINGS_APPLIED` to the tray window. Sliders commit on release.

use std::sync::atomic::{AtomicIsize, Ordering};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreatePen, CreateRectRgn,
    CreateRoundRectRgn, CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, Ellipse, FillRect,
    GetStockObject, IntersectClipRect, InvalidateRect, LineTo, MonitorFromPoint, MoveToEx,
    RoundRect, SelectClipRgn, SelectObject, SetBkColor, SetBkMode, SetTextColor, SetViewportOrgEx,
    HBRUSH, HDC, HFONT, MONITORINFO, MONITOR_DEFAULTTONEAREST, NULL_BRUSH, NULL_PEN, PS_SOLID,
    SRCCOPY, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::SetWindowTheme;
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetFocus, ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetCursorPos, GetWindowTextW,
    LoadCursorW, PostMessageW, RegisterClassExW, SendMessageW, SetForegroundWindow, SetWindowPos,
    SetWindowTextW, ShowWindow, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, HMENU,
    IDC_ARROW, MINMAXINFO, SWP_HIDEWINDOW, SWP_NOACTIVATE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_SHOW,
    WM_CLOSE, WM_COMMAND, WM_CREATE, WM_CTLCOLOREDIT, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_SETFONT, WM_SIZE,
    WNDCLASSEXW, WS_CHILD, WS_CLIPCHILDREN, WS_OVERLAPPEDWINDOW, WS_TABSTOP, WS_VISIBLE,
};

use crate::config::{
    base_style, file_name, normalize_caption_height, parse_color, Backdrop, Color, Config, Corners,
    Opacity, PartialStyle, Rule, Style,
};
use crate::{ArchStatus, PRESETS};

use super::cfgio;
use super::registry;
use super::sysinfo;
use super::uxtheme;
use super::wide::{config_dir, log_path, packs_dir, pcw, recipes_dir, to_wide, write_atomic};
use windows_sys::Win32::UI::WindowsAndMessaging::WM_TIMER;

#[path = "controls.rs"]
mod controls;
#[path = "settings_pages.rs"]
mod pages;
#[path = "cmdpal.rs"]
mod palette_ui;
#[path = "uia.rs"]
mod uia;
#[path = "uistyle.rs"]
mod uistyle;

use uistyle::{accent as theme_accent, th};

static PENDING_PAGE: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(usize::MAX);
const WM_GOTO_PAGE: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 21;

/// Open the command palette (or close it if it is open).
pub fn open_palette(owner: HWND) {
    palette_ui::open(owner);
}

/// Open Settings on page `index` (0 Overview .. 5 Safety).
pub fn open_page(owner: HWND, index: usize) {
    PENDING_PAGE.store(index, Ordering::Release);
    open(owner);
    let hwnd = SETTINGS.load(Ordering::Acquire);
    if hwnd != 0 {
        unsafe { PostMessageW(hwnd as HWND, WM_GOTO_PAGE, 0, 0) };
    }
}

pub const WM_SETTINGS_APPLIED: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 4;
const WM_REFRESH: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 5;
pub const WM_RESTART_EXPLORER: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 6;
/// `wparam` is a tray command id the host should run, such as "write diagnostics".
pub const WM_RUN_COMMAND: u32 = windows_sys::Win32::UI::WindowsAndMessaging::WM_APP + 8;
pub const CMD_DIAGNOSTICS: u32 = 12;

const WM_DPICHANGED: u32 = 0x02E0;
const WM_GETMINMAXINFO: u32 = 0x0024;
const WM_MOUSELEAVE: u32 = 0x02A3;
const EN_CHANGE: u16 = 0x0300;
const EN_KILLFOCUS: u16 = 0x0200;

const ID_CAPTION: u16 = 101;
const ID_BORDER: u16 = 102;
const ID_TEXT: u16 = 103;
const ID_TINT: u16 = 104;
const ID_RULE: u16 = 105;
const ID_EXCLUDE: u16 = 106;

// Colors are COLORREF (0x00BBGGRR).
/// Gap between cards, and card inner padding, in logical pixels.
const GAP: i32 = crate::ui_metrics::CARD_GAP;
const PAD: i32 = crate::ui_metrics::CARD_PAD;

// Logical layout.
const PREVIEW_H: i32 = 176;
const STATUS_H: i32 = 40;
const ID_FILTER: u16 = 107;
const ID_LOOK: u16 = 108;
const ID_HELP: u16 = 109;
const TIMER_UI: usize = 1;
const MARGIN: i32 = crate::ui_metrics::PAGE_MARGIN;
const MIN_W: i32 = 480;
/// Content stops growing here (logical px); wider windows get margin instead of long lines.
const MAX_CONTENT: i32 = 880;
const MIN_H: i32 = 520;
const LIST_LINE: i32 = 24;

static SETTINGS: AtomicIsize = AtomicIsize::new(0);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Home,
    Fix,
    Appearance,
    Scenes,
    Rules,
    Safety,
    Diagnostics,
    Help,
}

struct Edits {
    filter: HWND,
    caption: HWND,
    border: HWND,
    text: HWND,
    tint: HWND,
    rule: HWND,
    exclude: HWND,
    look: HWND,
    help: HWND,
}

/// One font per style in the type ramp, plus the icon font (null when none is installed).
struct Fonts {
    /// Body, 14/20.
    body: HFONT,
    /// Caption, 12/16.
    small: HFONT,
    /// Body strong (semibold 14/20): section headings and the selected nav item.
    head: HFONT,
    /// Subtitle, 20/28 semibold: card and dialog headings.
    subtitle: HFONT,
    /// Title, 28/36 semibold: page titles.
    title: HFONT,
    icon: HFONT,
}

struct Ui {
    hwnd: HWND,
    owner: HWND,
    page: Page,
    ext: pages::Ext,
    config: Config,
    autostart: bool,
    rule: Option<usize>,
    exclude_sel: Option<usize>,
    preset_sel: Option<usize>,
    scroll: i32,
    drag: Option<Slider>,
    /// Pointer position and the control under it, for hover and pressed states.
    pointer: controls::Pointer,
    /// The tooltip window for the icon-only navigation rail, and the label text it points at.
    nav_tips: HWND,
    nav_tip_text: Vec<Vec<u16>>,
    /// Width of the navigation pane in logical px (0 while the setup flow is showing).
    nav_w: i32,
    /// Left edge of the content in client logical px: the navigation pane's width, plus centering
    /// when the content is capped (the setup flow).
    content_x: i32,
    /// Client height in logical px, for anchoring the footer navigation item.
    client_lh: i32,
    status: String,
    edits: Edits,
    fonts: Fonts,
    bg_brush: HBRUSH,
    edit_brush: HBRUSH,
    /// Device pixels per logical pixel.
    k: f32,
    accent: u32,
    build: u32,
    arch_line: String,
    arch: ArchStatus,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Slider {
    Opacity,
    Caption,
}

enum Row {
    Header(&'static str),
    Note(&'static str),
    /// A teaching tip: a short heading and one or two sentences, shown inline on the page.
    Tip(Vec<String>),
    /// The search box on the Help page.
    SearchField,
    /// Page title (28/36 semibold) and a one-line description under it.
    PageTitle(&'static str, String),
    Toggle(Toggle),
    Choice(Choice),
    Slider(Slider),
    Color(u16),
    Presets,
    RuleList,
    ExcludeList,
    Action(Button),
    AddField(Button),
    About(Vec<String>),
    Lines(Vec<String>),
    /// Paragraphs in body size, for the text a step or page is mainly about (setup).
    Prose(Vec<String>),
    /// The brand header: the mark, the name and one line of what AllBlack11 does.
    Brand,
    /// The creator credit with its link. One keyboard stop; the link is the control.
    Credit,
    Buttons(Vec<Button>),
    WindowList,
    RecipeList,
    /// Scenes first, then the user's Look Book, with a color chip for each.
    LookList,
    Filter,
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
    FocusLook,
    ReduceBattery,
    ReduceRemote,
    ReduceTransparency,
    EmulatedX64,
    ShowTitles,
    IdentityAccents,
    DayNight,
    PaletteHotkey,
    AutoUpdateCheck,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Choice {
    Backdrop,
    Corners,
    Palette,
    RuleScope,
    DayScene,
    NightScene,
    DayStarts,
    NightStarts,
    AppAppearance,
    UpdateChannel,
    /// Which compatibility pack the buttons below act on.
    Pack,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Button {
    UseDefault,
    AddRule,
    RemoveRule,
    AddExclude,
    RemoveExclude,
    RestartExplorer,
    OpenFolder,
    OpenLog,
    Diagnostics,
    PickWindow,
    RefreshWindows,
    CreateRule,
    TryLook,
    ExcludeApp,
    RestoreWindow,
    CopyReport,
    MarkWorks,
    MarkPartial,
    MarkFails,
    MarkReset,
    RaisePriority,
    LowerPriority,
    PreferAppDefaults,
    TryRecipe,
    ImportRecipe,
    ExportRecipe,
    PackInstall,
    PackRemove,
    ImportPack,
    TestRule,
    TestPicked,
    UndoLook,
    CheckUpdates,
    DownloadUpdate,
    CancelUpdate,
    InstallUpdate,
    DiscardUpdate,
    SkipUpdate,
    UnskipUpdate,
    OpenReleasePage,
    GoUpdates,
    RestoreLkg,
    ClearNotebook,
    ResetAllWindows,
    UndoShell,
    OpenHarness,
    DemoMaterials,
    SetupNext,
    SetupBack,
    SetupSkip,
    SetupFinish,
    SetupFinishHelp,
    SetupKeepCurrent,
    SetupTry,
    SetupContinue,
    SetupReplay,
    TipBack,
    TipNext,
    TipDone,
    TipClose,
    /// Start walkthrough `n` (index into `crate::TUTORIALS`).
    Tutorial(u8),
    OpenPalette,
    GoPalette,
    GoFix,
    GoScenes,
    GoHelp,
    DemoCompare,
    NextAccent,
    ClearAccent,
    PaletteFromImage,
    TryScene,
    SaveLook,
    RenameLook,
    DeleteLook,
    MarkCompare,
    PauseAll,
    ResumeAll,
    LeaveSafe,
    StopExperimental,
}

struct Laid {
    y: i32,
    h: i32,
    row: Row,
}

// ---------------------------------------------------------------------------
// Copy for every option: what it is, and what it will do.
// ---------------------------------------------------------------------------

fn toggle_meta(id: Toggle) -> (&'static str, &'static str) {
    match id {
        Toggle::Theme => (
            "Theme windows",
            "Master switch. Off puts every window back exactly as Windows drew it.",
        ),
        Toggle::DefaultOn => (
            "Default look on",
            "Applies the look below to every app that has no rule of its own.",
        ),
        Toggle::RuleOn => (
            "This rule is on",
            "Off leaves this one app untouched even when the default look is on.",
        ),
        Toggle::Dark => (
            "Dark title bar",
            "Dark caption buttons and system menu. Keeps light-theme apps from showing a white bar.",
        ),
        Toggle::Blur => (
            "Blur while dragging",
            "Tinted acrylic drops to a cheaper blur during a drag or resize, then returns. Smoother moves.",
        ),
        Toggle::Oled => (
            "OLED taskbar",
            "Sets Windows' own OLED taskbar flag so the taskbar paints pure black. Restored when you turn it off.",
        ),
        Toggle::Black => (
            "Black Start, taskbar and notification center",
            "Writes black into the accent palette and turns on transparency and the dark system theme. Your old values come back when you turn it off.",
        ),
        Toggle::Autostart => (
            "Start with Windows",
            "Launches AllBlack11 when you sign in. One value under HKCU Run. No administrator rights.",
        ),
        Toggle::Reapply => (
            "Reapply when a window is focused",
            "Re-checks a window each time you switch to it, so apps that repaint their frame stay black.",
        ),
        Toggle::Logging => (
            "Write a log",
            "Writes allblack11.log in the data folder, rotated at 256 KB. Handy when reporting a bug.",
        ),
        Toggle::Experimental => (
            "Dark legacy windows",
            "Recolors classic dialogs such as Run by loading a small DLL into non-elevated apps of the same CPU type. Off by default. Antivirus or anti-cheat software may object.",
        ),
        Toggle::FocusLook => (
            "Focus Look",
            "The window you are in gets an accent border; other windows go quiet. Changes only on focus changes, never animated. A color a rule sets itself always wins.",
        ),
        Toggle::ReduceBattery => (
            "Reduce effects on battery saver",
            "Drops backdrop materials (Mica, Acrylic) to plain colors while battery saver is on. Colors and corners stay.",
        ),
        Toggle::ReduceRemote => (
            "Reduce effects in a remote session",
            "Drops backdrop materials in a Remote Desktop session, where they are expensive and often not drawn.",
        ),
        Toggle::ReduceTransparency => (
            "Reduce effects when Windows transparency is off",
            "Respects your Windows setting: with Transparency effects off, no backdrop material is applied.",
        ),
        Toggle::EmulatedX64 => (
            "Cover emulated x64 apps (experimental)",
            "ARM64 only. Uses a separate x64 helper to reach x64 programs running under emulation. Off by default; the helper starts only when needed and stops when you turn this off.",
        ),
        Toggle::IdentityAccents => (
            "Identity accents (optional)",
            "Gives each app its own thin border color so windows are easy to tell apart. Only the border changes, and only where no rule sets one. Off by default.",
        ),
        Toggle::AutoUpdateCheck => (
            "Check for updates once a day (optional)",
            "Asks GitHub (api.github.com) at most once a day whether a newer release exists, and tells you once if so. Nothing is downloaded or installed unless you press a button. Off by default. Checking by button always works.",
        ),
        Toggle::PaletteHotkey => (
            "Command palette hotkey (optional)",
            "Ctrl+Alt+Space opens the command palette from anywhere. It registers one system hotkey and nothing else: no keyboard hook, no input is read. Off by default. The tray item and Ctrl+K in Settings always work.",
        ),
        Toggle::DayNight => (
            "Day / Night scenes (optional)",
            "Switches scene at set local times. It checks the clock only when a boundary passes, after sleep and when the time or time zone changes. It never wakes the PC. Off by default.",
        ),
        Toggle::ShowTitles => (
            "Show window titles in this page",
            "Titles can contain document names. They are shown here only if you turn this on, and are never saved. Copied reports leave them out unless this is on.",
        ),
    }
}

fn choice_desc(ui: &Ui, id: Choice) -> &'static str {
    let style = ui.current_style();
    match id {
        Choice::Backdrop => match style.backdrop {
            Backdrop::Auto => "Let Windows pick the material.",
            Backdrop::None => "No material. The app's own background shows.",
            Backdrop::Mica => "Soft, wallpaper-tinted surface. Needs Windows 11 22621 or newer.",
            Backdrop::MicaAlt => "Mica with a stronger, darker tint.",
            Backdrop::Acrylic => "Windows' frosted glass. Its tint cannot be changed.",
            Backdrop::TintedAcrylic => "Frosted glass in your own tint and opacity, set below.",
        },
        Choice::Palette => match ui.config.legacy.palette.as_str() {
            "black" => "True black dialogs. Best on OLED. Text stays above 7:1 contrast.",
            _ => "Dark gray dialogs. Easier on the eyes in a bright room. Text stays above 7:1 contrast.",
        },
        Choice::RuleScope => match ui.ext.rule_scope {
            0 => "A new rule matches the executable. The most common choice.",
            1 => "A new rule matches the executable and this window class.",
            2 => "A new rule matches the package (Store apps). Falls back to the executable.",
            _ => "A new rule matches only the window class, across every app that uses it.",
        },
        Choice::Pack => "Pick a compatibility pack, read what it can and cannot do below, then Install. A pack adds ordinary rules named \"Pack: …\" that your own rules override. Click to step to the next pack.",
        Choice::UpdateChannel => "Which releases to offer. Automatic follows the kind of build you run: a preview build is offered previews, a stable build only stable releases. Click to change.",
        Choice::AppAppearance => "Changes only AllBlack11's own windows, never the apps it styles. Click to switch between Windows setting, Light and Dark.",
        Choice::DayScene => "Used during the day. Click to step through the scenes and your saved looks.",
        Choice::NightScene => "Used at night. Click to step through the scenes and your saved looks.",
        Choice::DayStarts => "Local time the day scene begins. Click to move it by 30 minutes.",
        Choice::NightStarts => "Local time the night scene begins. Click to move it by 30 minutes.",
        Choice::Corners => match style.corners {
            Corners::Default => "Windows decides, which is rounded on Windows 11.",
            Corners::Round => "Standard rounded corners.",
            Corners::Small => "A smaller corner radius.",
            Corners::Square => "Sharp corners, like Windows 10.",
        },
    }
}

fn choice_title(id: Choice) -> &'static str {
    match id {
        Choice::Backdrop => "Backdrop",
        Choice::Corners => "Corners",
        Choice::Palette => "Dialog palette",
        Choice::RuleScope => "New rule matches",
        Choice::AppAppearance => "AllBlack11 theme",
        Choice::UpdateChannel => "Update channel",
        Choice::Pack => "Pack",
        Choice::DayScene => "Day scene",
        Choice::NightScene => "Night scene",
        Choice::DayStarts => "Day starts",
        Choice::NightStarts => "Night starts",
    }
}

fn color_label(id: u16) -> &'static str {
    match id {
        ID_CAPTION => "Title bar color",
        ID_BORDER => "Border color",
        ID_TEXT => "Title text color",
        _ => "Acrylic tint",
    }
}

fn slider_meta(id: Slider) -> (&'static str, &'static str) {
    match id {
        Slider::Opacity => (
            "Acrylic opacity",
            "How strongly the tint covers the blur. Only used by the tinted acrylic backdrop.",
        ),
        Slider::Caption => (
            "Legacy title bar height",
            "Changes the classic title bar height. Windows applies it at your next sign-in. System restores your value.",
        ),
    }
}

fn button_meta(id: Button) -> (&'static str, &'static str, &'static str) {
    match id {
        Button::UseDefault => (
            "Editing one app",
            "The look above changes only this app's rule.",
            "Edit default look",
        ),
        Button::AddRule => ("", "", "Add rule"),
        Button::RemoveRule => ("Remove the selected rule", "Pick a rule in the list first.", "Remove"),
        Button::AddExclude => ("", "", "Add"),
        Button::RemoveExclude => ("Remove the selected app", "Pick an app in the list first.", "Remove"),
        Button::RestartExplorer => (
            "Restart Explorer",
            "Reloads the taskbar, Start menu and desktop. Open File Explorer windows close; other apps keep running. Use it when a shell change does not show up.",
            "Restart Explorer…",
        ),
        Button::OpenFolder => ("Data folder", "Config, backups and the log live here.", "Open"),
        Button::OpenLog => ("Log", "Exists once Write a log has been on.", "Open"),
        Button::Diagnostics => (
            "Diagnostics report",
            "Version, CPU type, Windows build and your settings in one text file. Attach it to bug reports.",
            "Write report",
        ),
        Button::PickWindow => ("", "", "Pick a window"),
        Button::RefreshWindows => ("", "", "Refresh list"),
        Button::CreateRule => ("", "", "Save as rule"),
        Button::TryLook => ("", "", "Preview here (20 s)"),
        Button::ExcludeApp => ("", "", "Exclude app"),
        Button::RestoreWindow => ("", "", "Restore this window"),
        Button::CopyReport => ("", "", "Copy style receipt"),
        Button::MarkWorks => ("", "", "Works"),
        Button::MarkPartial => ("", "", "Partly"),
        Button::MarkFails => ("", "", "Doesn't work"),
        Button::MarkReset => ("", "", "Reset"),
        Button::RaisePriority => ("", "", "Raise priority"),
        Button::LowerPriority => ("", "", "Lower priority"),
        Button::PreferAppDefaults => (
            "Prefer app defaults",
            "Leave this target exactly as the apps draw themselves: no colors, no material, no corners. Undo it by turning the look back on above.",
            "Use app defaults",
        ),
        Button::TryRecipe => ("", "", "Try this look"),
        Button::ImportRecipe => ("", "", "Import…"),
        Button::PackInstall => ("", "", "Install pack"),
        Button::PackRemove => ("", "", "Remove pack"),
        Button::ImportPack => ("", "", "Import pack…"),
        Button::ExportRecipe => ("", "", "Export current…"),
        Button::CheckUpdates => ("", "", "Check for updates"),
        Button::DownloadUpdate => ("", "", "Download"),
        Button::CancelUpdate => ("", "", "Cancel download"),
        Button::InstallUpdate => ("", "", "Install (closes AllBlack11)"),
        Button::DiscardUpdate => ("", "", "Discard download"),
        Button::SkipUpdate => ("", "", "Skip this version"),
        Button::UnskipUpdate => ("", "", "Stop skipping"),
        Button::OpenReleasePage => ("", "", "Release page"),
        Button::GoUpdates => ("", "", "See the update"),
        Button::UndoLook => (
            "Undo last change",
            "Puts back the look from before the most recent change listed above, then restyles windows from it. Repeat to go further back.",
            "Undo last change",
        ),
        Button::TestRule => ("", "", "Test typed app"),
        Button::TestPicked => ("", "", "Test picked window"),
        Button::RestoreLkg => ("", "", "Restore last known good"),
        Button::ClearNotebook => ("", "", "Clear notebook"),
        Button::ResetAllWindows => (
            "Reset all windows",
            "Puts every themed window back as Windows draws it, until you reload or toggle Theme windows.",
            "Reset windows",
        ),
        Button::UndoShell => (
            "Undo shell changes",
            "Turns off the OLED taskbar and black Start, restoring the values saved before AllBlack11 changed them. A value you changed yourself since then is left alone.",
            "Undo shell changes",
        ),
        Button::DemoMaterials => (
            "Materials demonstration",
            "Opens a window that applies this look to itself, with a transparent, an opaque and a half-transparent panel. It shows how Windows materials behave. It does not show how another app will look.",
            "Open demonstration",
        ),
        Button::DemoCompare => (
            "Before / after",
            "Opens two windows AllBlack11 owns with the same sample content: one left as Windows draws it, one with the look selected above (or your current look). It compares frame styling on windows we control. It does not capture or predict any other app.",
            "Compare",
        ),
        Button::SetupNext => ("", "", "Next"),
        Button::SetupBack => ("", "", "Back"),
        Button::SetupSkip => ("", "", "Skip setup"),
        Button::SetupFinish => ("", "", "Go to Home"),
        Button::SetupFinishHelp => ("", "", "Open Help"),
        Button::SetupKeepCurrent => ("", "", "Keep my current look"),
        Button::SetupTry => ("", "", "Try the chosen look (15 s)"),
        Button::SetupContinue => ("", "", "Continue setup"),
        Button::SetupReplay => (
            "Guided setup",
            "Walk through choosing a look, trying it safely, picking an app and your preferences. Nothing is turned on unless you choose it.",
            "Run setup",
        ),
        Button::TipBack => ("", "", "Back"),
        Button::TipNext => ("", "", "Next tip"),
        Button::TipDone => ("", "", "Done"),
        Button::TipClose => ("", "", "Close tip"),
        Button::Tutorial(index) => {
            let tutorial = &crate::TUTORIALS[(index as usize).min(crate::TUTORIALS.len() - 1)];
            (tutorial.title, tutorial.blurb, "Start")
        }
        Button::GoPalette => ("", "", "Command palette"),
        Button::GoFix => ("", "", "Fix an app"),
        Button::GoScenes => ("", "", "Browse scenes"),
        Button::GoHelp => ("", "", "Help and tutorials"),
        Button::OpenPalette => (
            "Command palette",
            "Search every action: pick a window, switch scenes, pause, restore, open a page. Also Ctrl+K in Settings and the tray menu.",
            "Open",
        ),
        Button::NextAccent => ("", "", "Change accent"),
        Button::ClearAccent => ("", "", "Automatic"),
        Button::PaletteFromImage => (
            "Build a look from a picture",
            "Reads a few sample points from a picture you choose, on this PC only, and builds a dark frame look whose text stays readable. Nothing is applied until you Try it.",
            "Choose picture…",
        ),
        Button::TryScene => ("", "", "Try (15 s)"),
        Button::SaveLook => ("", "", "Save look"),
        Button::RenameLook => ("", "", "Rename"),
        Button::DeleteLook => ("", "", "Delete"),
        Button::MarkCompare => ("", "", "Compare…"),
        Button::PauseAll => (
            "Pause all styling",
            "Emergency stop. Puts every window back as Windows draws it and stops the legacy layer and the x64 helper right now. Not saved: restarting AllBlack11 starts from your saved settings.",
            "Pause all styling",
        ),
        Button::ResumeAll => (
            "Resume styling",
            "Applies your saved settings to open windows again.",
            "Resume styling",
        ),
        Button::LeaveSafe => (
            "Leave safe mode",
            "Applies your saved settings now, experimental features included.",
            "Leave safe mode",
        ),
        Button::StopExperimental => (
            "Turn off experimental features",
            "Turns off dark legacy windows and emulated x64 coverage, saves that, and stops the helper. Window styling itself keeps working.",
            "Turn off experimental",
        ),
        Button::OpenHarness => (
            "Control harness",
            "A small window with every control type and state, for checking the legacy layer without relying on another app. It ships in the portable zip.",
            "Open harness",
        ),
    }
}

// ---------------------------------------------------------------------------
// Row heights: measured, not guessed
// ---------------------------------------------------------------------------

/// Which font a block of text is painted with.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Face {
    Body,
    Small,
}

/// What row layout needs to measure text: a device context and the fonts the rows are painted
/// with, the DPI scale and the current content width. Set for the duration of one layout or
/// paint pass by [`MeasureScope`].
#[derive(Clone, Copy)]
struct Measure {
    dc: HDC,
    k: f32,
    lw: i32,
    body: HFONT,
    small: HFONT,
}

thread_local! {
    static MEASURE: std::cell::Cell<Option<Measure>> = const { std::cell::Cell::new(None) };
}

struct MeasureScope {
    own_dc: Option<HDC>,
    previous: Option<Measure>,
}

impl MeasureScope {
    /// Measure with `ui`'s fonts at content width `lw`. Without a `dc`, a throwaway memory DC
    /// is used (layout outside painting).
    fn new(ui: &Ui, dc: Option<HDC>, lw: i32) -> Self {
        let own_dc = match dc {
            Some(_) => None,
            None => Some(unsafe { CreateCompatibleDC(std::ptr::null_mut()) }),
        };
        let measure = Measure {
            dc: dc.or(own_dc).unwrap_or(std::ptr::null_mut()),
            k: ui.k,
            lw,
            body: ui.fonts.body,
            small: ui.fonts.small,
        };
        let previous = MEASURE.with(|cell| cell.replace(Some(measure)));
        Self { own_dc, previous }
    }
}

impl Drop for MeasureScope {
    fn drop(&mut self) {
        MEASURE.with(|cell| cell.set(self.previous));
        if let Some(dc) = self.own_dc {
            unsafe { DeleteDC(dc) };
        }
    }
}

/// Content width rows are laid out for: the real one while a scope is active, the narrowest
/// supported one otherwise.
fn layout_width() -> i32 {
    MEASURE.with(|cell| cell.get().map_or(MIN_W, |m| m.lw))
}

/// Height in logical pixels of `text` word-wrapped to `width` logical pixels, measured with the
/// same font and flags it is painted with, so a row is exactly as tall as its text whatever the
/// window width, text size or language. Never less than `line`. With no measuring context (no
/// window yet) it falls back to a conservative character-count estimate.
fn wrapped_height(text: &str, face: Face, width: i32, line: i32) -> i32 {
    if text.is_empty() {
        return 0;
    }
    MEASURE.with(|cell| match cell.get() {
        Some(m) if !m.dc.is_null() => unsafe {
            let font = if face == Face::Body { m.body } else { m.small };
            let old = SelectObject(m.dc, font);
            let wide = to_wide(text);
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: ((width.max(1)) as f32 * m.k).round() as i32,
                bottom: 0,
            };
            DrawTextW(
                m.dc,
                wide.as_ptr(),
                -1,
                &mut rect,
                DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX | DT_LEFT,
            );
            SelectObject(m.dc, old);
            ((rect.bottom as f32 / m.k).ceil() as i32).max(line)
        },
        _ => {
            let per_line = ((width as f32 / 6.5) as usize).max(1);
            line * ((text.chars().count() + per_line - 1) / per_line).max(1) as i32
        }
    })
}

// Text columns. Painting and measuring use the same functions so they cannot disagree.

/// Page title description, notes: the content column.
fn column_w(lw: i32) -> i32 {
    lw - 2 * MARGIN
}

/// Inside a card.
fn card_text_w(lw: i32) -> i32 {
    lw - 2 * (MARGIN + PAD)
}

/// Beside a switch and its On/Off word.
fn toggle_text_w(lw: i32) -> i32 {
    card_text_w(lw) - 120
}

/// Beside the action button of an action card.
fn action_text_w(id: Button, lw: i32) -> i32 {
    let (l, ..) = action_pill(id, lw, 0);
    l - 16 - (MARGIN + PAD)
}

/// What the Quick looks card says before a look is chosen.
const PRESET_HINT: &str = "Pick a starting look. Every option below stays editable afterwards.";

/// Space under the title line of a card before the description, and below it.
const DESC_TOP: i32 = 30;
const DESC_BOTTOM: i32 = 12;
/// Gap between the lines of a status card.
const PARAGRAPH: i32 = 4;

/// Heights of the lines of a status card: the first in body size, the rest small.
fn about_line_heights(lines: &[String], lw: i32) -> Vec<i32> {
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let (face, nominal) = if index == 0 {
                (Face::Body, 20)
            } else {
                (Face::Small, 16)
            };
            wrapped_height(line, face, card_text_w(lw), nominal).max(nominal) + PARAGRAPH
        })
        .collect()
}

/// Width in logical pixels of one line of `text`, measured with the painting font. Falls back
/// to an estimate when there is no measuring context.
fn text_width(text: &str, face: Face) -> i32 {
    MEASURE.with(|cell| match cell.get() {
        Some(m) if !m.dc.is_null() => unsafe {
            let font = if face == Face::Body { m.body } else { m.small };
            let old = SelectObject(m.dc, font);
            let wide: Vec<u16> = text.encode_utf16().collect();
            let mut size = windows_sys::Win32::Foundation::SIZE { cx: 0, cy: 0 };
            windows_sys::Win32::Graphics::Gdi::GetTextExtentPoint32W(
                m.dc,
                wide.as_ptr(),
                wide.len() as i32,
                &mut size,
            );
            SelectObject(m.dc, old);
            (size.cx as f32 / m.k).ceil() as i32
        },
        _ => text.chars().count() as i32 * 8,
    })
}

/// Where the credit's words and its link sit (content coordinates, relative to the row's top)
/// and how tall the row is. On a narrow page the link drops to a second line instead of being
/// clipped or overlapping anything.
struct CreditLayout {
    prefix: (i32, i32, i32, i32),
    link: (i32, i32, i32, i32),
    height: i32,
}

fn credit_layout(lw: i32) -> CreditLayout {
    let x0 = MARGIN + 2;
    let prefix_w = text_width(crate::CREDIT_PREFIX, Face::Body);
    let link_w = text_width(crate::CREATOR, Face::Body);
    if x0 + prefix_w + link_w <= lw - MARGIN {
        CreditLayout {
            prefix: (x0, 8, x0 + prefix_w, 28),
            link: (x0 + prefix_w, 8, x0 + prefix_w + link_w, 28),
            height: 36 + GAP,
        }
    } else {
        CreditLayout {
            prefix: (x0, 8, x0 + prefix_w, 28),
            link: (x0, 30, x0 + link_w, 50),
            height: 58 + GAP,
        }
    }
}

/// Height of the brand header: the mark is 56 logical pixels, the text wraps beside it.
fn brand_text_w(lw: i32) -> i32 {
    lw - 2 * MARGIN - 56 - 16
}

/// Rows include the gap under their card.
fn row_height(row: &Row) -> i32 {
    let lw = layout_width();
    match row {
        Row::Header(_) => 44,
        Row::PageTitle(_, description) => {
            8 + 36 + 4 + wrapped_height(description, Face::Body, column_w(lw), 20) + 12
        }
        Row::Tip(parts) => {
            let body = parts.get(1).map_or(0, |text| {
                wrapped_height(text, Face::Body, card_text_w(lw), 20)
            });
            32 + body + 12 + GAP
        }
        Row::SearchField => 52 + GAP,
        Row::Note(text) => 2 + wrapped_height(text, Face::Small, column_w(lw) - 2, 16) + 8,
        Row::Toggle(id) => {
            let (_, desc) = toggle_meta(*id);
            (DESC_TOP + wrapped_height(desc, Face::Small, toggle_text_w(lw), 16) + DESC_BOTTOM)
                .max(56)
                + GAP
        }
        Row::Choice(_) => (40 + 16 * 2).max(56) + GAP,
        Row::Slider(_) => 92 + GAP,
        Row::Color(_) => 52 + GAP,
        Row::Presets => {
            // The description under the buttons changes with the selected look. The card is
            // sized for the longest one so choosing a look never moves the page.
            let longest = PRESETS
                .iter()
                .map(|preset| preset.blurb)
                .chain([PRESET_HINT])
                .map(|text| wrapped_height(text, Face::Small, card_text_w(lw), 16))
                .max()
                .unwrap_or(16);
            58 + longest + 12 + GAP
        }
        Row::RuleList | Row::ExcludeList => 0, // sized by the caller from the list length
        Row::Action(id) => {
            let (_, desc, _) = button_meta(*id);
            if desc.is_empty() {
                60 + GAP
            } else {
                (DESC_TOP
                    + wrapped_height(desc, Face::Small, action_text_w(*id, lw), 16)
                    + DESC_BOTTOM)
                    .max(60)
                    + GAP
            }
        }
        Row::AddField(_) => 52 + GAP,
        Row::Lines(items) => 20 + lines_block_height(items, lw) + GAP,
        Row::Prose(items) => 24 + prose_heights(items, lw).iter().sum::<i32>() + GAP,
        Row::Brand => {
            let text =
                36 + 4 + wrapped_height(crate::BRAND_TAGLINE, Face::Body, brand_text_w(lw), 20);
            8 + text.max(56) + 16
        }
        Row::Credit => credit_layout(lw).height,
        Row::Buttons(_) => 52 + GAP,
        Row::WindowList | Row::RecipeList | Row::LookList => 0, // sized by the caller
        Row::Filter => 52 + GAP,
        Row::About(lines) => 24 + about_line_heights(lines, lw).iter().sum::<i32>() + GAP,
    }
}

/// Heights of the paragraphs of a `Prose` card, each followed by a gap.
fn prose_heights(items: &[String], lw: i32) -> Vec<i32> {
    items
        .iter()
        .map(|line| wrapped_height(line, Face::Body, card_text_w(lw), 20).max(20) + 8)
        .collect()
}

/// Height of the paragraphs of a `Lines` card.
fn lines_block_height(items: &[String], lw: i32) -> i32 {
    items.iter().map(|line| line_height(line, lw)).sum()
}

fn line_height(line: &str, lw: i32) -> i32 {
    wrapped_height(line, Face::Small, card_text_w(lw), 16).max(16)
}

// ---------------------------------------------------------------------------
// Window plumbing
// ---------------------------------------------------------------------------

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
        let (x, y, w, h) = place(560, 860);
        let hwnd = CreateWindowExW(
            0,
            pcw(&class),
            pcw(&title),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE | WS_CLIPCHILDREN,
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
        info.hIcon = super::icon::window_icon(instance, true);
        info.hIconSm = super::icon::window_icon(instance, false);
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
        if let Some(ui) = get_ui(hwnd).as_mut() {
            ui.refresh_theme(hwnd);
        }
        // QA aid: walk the UI Automation tree through its COM vtables and write the result.
        if std::env::var_os("ALLBLACK11_UIA_SELFTEST").is_some() {
            let handle = hwnd as isize;
            std::thread::spawn(move || {
                // Let the first paint and layout finish.
                std::thread::sleep(std::time::Duration::from_millis(1500));
                let report = uia::self_test(handle as HWND);
                let text = report.join("\r\n");
                let _ = write_atomic(&config_dir().join("uia-selftest.txt"), &text);
            });
        }
        return 0;
    }
    if msg == WM_GETMINMAXINFO {
        let info = &mut *(lparam as *mut MINMAXINFO);
        let k = crate::layout_scale(dpi_of(hwnd), uistyle::text_scale_percent());
        let nav_min = if uistyle::icon_font_available() {
            crate::ui_metrics::NAV_COMPACT
        } else {
            crate::ui_metrics::NAV_TEXT_ONLY
        };
        info.ptMinTrackSize.x = ((MIN_W + nav_min + 16) as f32 * k) as i32;
        info.ptMinTrackSize.y = ((MIN_H + 47) as f32 * k) as i32;
        return 0;
    }
    let ui = get_ui(hwnd);
    if ui.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let ui = &mut *ui;
    match msg {
        WM_ERASEBKGND => 1,
        // WM_GETOBJECT: a screen reader or other UI Automation client asks about this window.
        0x003D => match uia::handle_get_object(hwnd, wparam, lparam) {
            Some(result) => result,
            None => DefWindowProcW(hwnd, msg, wparam, lparam),
        },
        // Questions from the UI Automation provider objects, always answered on this thread.
        uia::WM_UIA_QUERY => {
            if let Some(query) = (lparam as *mut uia::Query).as_mut() {
                ui.uia_answer(hwnd, query);
            }
            0
        }
        WM_PAINT => {
            paint(hwnd, ui);
            0
        }
        WM_SIZE => {
            ui.relayout(hwnd);
            0
        }
        WM_DPICHANGED => {
            let dpi = (wparam & 0xFFFF) as u32;
            ui.set_dpi(dpi);
            let suggested = &*(lparam as *const RECT);
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                suggested.left,
                suggested.top,
                suggested.right - suggested.left,
                suggested.bottom - suggested.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            ui.relayout(hwnd);
            0
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam >> 16) as u16 as i16) as i32;
            ui.scroll -= delta * 64 / 120;
            ui.relayout(hwnd);
            0
        }
        WM_LBUTTONDOWN => {
            ui.pointer.press(true);
            InvalidateRect(hwnd, std::ptr::null(), 0);
            if let Some((lx, cy)) = ui.to_content(lparam) {
                if !ui.trial_active() && ui.pick_hit(ui.lw(hwnd), lx, cy) {
                    ui.begin_pick(hwnd);
                    return 0;
                }
                if let Some(slider) = ui.hit_slider(cy).filter(|_| !ui.trial_active()) {
                    ui.drag = Some(slider);
                    SetCapture(hwnd);
                    ui.set_slider(hwnd, slider, lx);
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                }
            }
            0
        }
        WM_MOUSELEAVE => {
            ui.pointer.set(None);
            let hover_changed = ui.pointer.update_hover();
            if ui.ext.nav_hover.take().is_some() || hover_changed {
                InvalidateRect(hwnd, std::ptr::null(), 0);
            }
            0
        }
        WM_MOUSEMOVE => {
            // Hover state for the navigation pane.
            let client_x = (lparam_x(lparam) as f32 / ui.k) as i32;
            let client_y = (lparam_y(lparam) as f32 / ui.k) as i32;
            let hover = pages::nav_hit(ui, client_x, client_y)
                .and_then(|page| Page::ALL.iter().position(|p| *p == page));
            if hover != ui.ext.nav_hover {
                ui.ext.nav_hover = hover;
                InvalidateRect(hwnd, std::ptr::null(), 0);
            }
            // Hover for content controls: pointer in content coordinates (the same ones the
            // rows are painted in), repainting only when the control under it changes.
            ui.pointer.set(Some((client_x - ui.content_x, client_y)));
            if ui.pointer.update_hover() {
                InvalidateRect(hwnd, std::ptr::null(), 0);
            }
            {
                let mut track = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                TrackMouseEvent(&mut track);
            }
            if ui.ext.picking {
                windows_sys::Win32::UI::WindowsAndMessaging::SetCursor(LoadCursorW(
                    std::ptr::null_mut(),
                    windows_sys::Win32::UI::WindowsAndMessaging::IDC_CROSS,
                ));
            }
            if let Some(slider) = ui.drag {
                let lx = (lparam_x(lparam) as f32 / ui.k) as i32 - ui.content_x;
                ui.set_slider(hwnd, slider, lx);
                InvalidateRect(hwnd, std::ptr::null(), 0);
            }
            0
        }
        WM_LBUTTONUP => {
            ui.pointer.press(false);
            if ui.ext.picking {
                ui.end_pick(hwnd);
                ui.relayout(hwnd);
                return 0;
            }
            if ui.chrome_click(hwnd, lparam) {
                ui.relayout(hwnd);
                return 0;
            }
            if ui.drag.take().is_some() {
                ReleaseCapture();
                ui.commit();
            } else if let Some((lx, cy)) = ui.to_content(lparam) {
                ui.click(hwnd, lx, cy);
            }
            ui.relayout(hwnd);
            0
        }
        WM_COMMAND => {
            let id = (wparam & 0xFFFF) as u16;
            let code = ((wparam >> 16) & 0xFFFF) as u16;
            if code == EN_CHANGE && id == ID_HELP {
                ui.ext.help_query = get_text(ui.edits.help);
                ui.relayout(hwnd);
            } else if code == EN_CHANGE && id == ID_FILTER {
                ui.ext.filter = get_text(ui.edits.filter);
                ui.relayout(hwnd);
            } else if code == EN_CHANGE {
                if ui.edit_live(id) {
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                }
            } else if code == EN_KILLFOCUS {
                ui.edit_committed(id);
                InvalidateRect(hwnd, std::ptr::null(), 0);
            }
            0
        }
        WM_CTLCOLOREDIT => {
            let dc = wparam as HDC;
            SetTextColor(dc, th().text);
            SetBkColor(dc, th().edit_bg);
            ui.edit_brush as LRESULT
        }
        WM_KEYDOWN => {
            let ctrl = windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyState(0x11) < 0;
            let shift = windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyState(0x10) < 0;
            match wparam {
                0x1B => {
                    if ui.trial_active() {
                        ui.trial_revert();
                    } else {
                        DestroyWindow(hwnd);
                    }
                    return 0;
                }
                // F6 moves between the navigation pane and the content; Shift+F6 goes back.
                0x75 if ui.nav_w > 0 => {
                    ui.ext.nav_focus = !ui.ext.nav_focus;
                    if ui.ext.nav_focus {
                        ui.ext.focus = None;
                    } else {
                        ui.move_focus(hwnd, false);
                    }
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                    return 0;
                }
                // Inside the navigation pane the arrows choose the destination.
                0x26 | 0x28 | 0x24 | 0x23 if ui.ext.nav_focus => {
                    let count = Page::ALL.len();
                    let current = Page::ALL.iter().position(|p| *p == ui.page).unwrap_or(0);
                    let next = match wparam {
                        0x26 => (current + count - 1) % count,
                        0x28 => (current + 1) % count,
                        0x24 => 0,
                        _ => count - 1,
                    };
                    ui.set_page(hwnd, Page::ALL[next]);
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                    return 0;
                }
                0x4B if ctrl => {
                    // Ctrl+K: the command palette.
                    palette_ui::open(ui.owner);
                    return 0;
                }
                0x09 if ctrl => {
                    // Ctrl+Tab and Ctrl+Shift+Tab switch pages.
                    let count = Page::ALL.len();
                    let current = Page::ALL.iter().position(|p| *p == ui.page).unwrap_or(0);
                    let next = if shift {
                        (current + count - 1) % count
                    } else {
                        (current + 1) % count
                    };
                    ui.set_page(hwnd, Page::ALL[next]);
                    return 0;
                }
                0x09 => {
                    ui.move_focus(hwnd, shift);
                    ui.relayout(hwnd);
                    return 0;
                }
                0x20 | 0x0D => {
                    ui.activate_focus(hwnd);
                    return 0;
                }
                0x25 if ui.ext.focus.is_some() => {
                    ui.arrow_key(hwnd, -1);
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                    return 0;
                }
                0x27 if ui.ext.focus.is_some() => {
                    ui.arrow_key(hwnd, 1);
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                    return 0;
                }
                0x21 => ui.scroll -= ui.view_height(hwnd) - 40, // Page Up
                0x22 => ui.scroll += ui.view_height(hwnd) - 40, // Page Down
                0x26 => ui.scroll -= 48,                        // Up
                0x28 => ui.scroll += 48,                        // Down
                0x24 => ui.scroll = 0,                          // Home
                0x23 => ui.scroll = i32::MAX / 2,               // End
                _ => {}
            }
            ui.relayout(hwnd);
            0
        }
        WM_TIMER => {
            if wparam == TIMER_UI {
                ui.on_timer(hwnd);
            }
            0
        }
        WM_GOTO_PAGE => {
            let index = PENDING_PAGE.swap(usize::MAX, Ordering::AcqRel);
            if let Some(page) = Page::ALL.get(index) {
                ui.set_page(hwnd, *page);
            }
            0
        }
        // Windows changed light/dark, accent, high contrast or text size: follow it.
        0x001A | 0x0015 | 0x031A => {
            ui.refresh_theme(hwnd);
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_REFRESH => {
            ui.reload_from_disk();
            ui.relayout(hwnd);
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            uia::disconnect(hwnd);
            SETTINGS.store(0, Ordering::Release);
            let ui = Box::from_raw(take_ui(hwnd));
            ui.fonts.delete();
            if !ui.bg_brush.is_null() {
                DeleteObject(ui.bg_brush);
            }
            if !ui.edit_brush.is_null() {
                DeleteObject(ui.edit_brush);
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn style_window(hwnd: HWND, mode: crate::UiMode) {
    unsafe {
        // The title bar follows AllBlack11's own theme, not the look it applies to other apps.
        let dark: i32 = i32::from(mode == crate::UiMode::Dark);
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

impl Fonts {
    /// `effective_dpi` is 96 times the layout scale (display scale times text size).
    fn make(effective_dpi: u32) -> Self {
        Self {
            body: uistyle::style_font(crate::BODY, effective_dpi),
            small: uistyle::style_font(crate::CAPTION, effective_dpi),
            head: uistyle::style_font(crate::BODY_STRONG, effective_dpi),
            subtitle: uistyle::style_font(crate::SUBTITLE, effective_dpi),
            title: uistyle::style_font(crate::TITLE, effective_dpi),
            icon: uistyle::icon_font(effective_dpi, 16),
        }
    }

    fn delete(&self) {
        for font in [
            self.body,
            self.small,
            self.head,
            self.subtitle,
            self.title,
            self.icon,
        ] {
            if !font.is_null() {
                unsafe { DeleteObject(font) };
            }
        }
    }
}

// ---------------------------------------------------------------------------
// State and behavior
// ---------------------------------------------------------------------------

impl Ui {
    fn new(hwnd: HWND, owner: HWND) -> Self {
        let dpi = dpi_of(hwnd);
        let config = cfgio::load();
        uistyle::refresh(config.ui.appearance);
        let k = crate::layout_scale(dpi, uistyle::text_scale_percent());
        let fonts = Fonts::make((96.0 * k).round() as u32);
        let edits = Edits {
            filter: make_edit(hwnd, ID_FILTER, fonts.body),
            caption: make_edit(hwnd, ID_CAPTION, fonts.body),
            border: make_edit(hwnd, ID_BORDER, fonts.body),
            text: make_edit(hwnd, ID_TEXT, fonts.body),
            tint: make_edit(hwnd, ID_TINT, fonts.body),
            rule: make_edit(hwnd, ID_RULE, fonts.body),
            exclude: make_edit(hwnd, ID_EXCLUDE, fonts.body),
            look: make_edit(hwnd, ID_LOOK, fonts.body),
            help: make_edit(hwnd, ID_HELP, fonts.body),
        };
        let mut ui = Self {
            hwnd,
            page: Page::Home,
            ext: pages::Ext::default(),
            owner,
            autostart: registry::autostart_enabled(),
            rule: None,
            exclude_sel: None,
            preset_sel: None,
            scroll: 0,
            drag: None,
            pointer: controls::Pointer::default(),
            nav_tips: std::ptr::null_mut(),
            nav_tip_text: Vec::new(),
            nav_w: crate::ui_metrics::NAV_EXPANDED,
            content_x: crate::ui_metrics::NAV_EXPANDED,
            client_lh: 0,
            status: String::new(),
            config,
            edits,
            fonts,
            bg_brush: unsafe { CreateSolidBrush(th().bg) },
            edit_brush: unsafe { CreateSolidBrush(th().edit_bg) },
            k,
            accent: theme_accent().0,
            build: super::dwm::windows_build(),
            arch_line: sysinfo::arch_line(),
            arch: sysinfo::arch_status(),
        };
        ui.push_colors_to_edits();
        ui.refresh_packs();
        ui.init_onboarding();
        ui
    }

    fn preview_h(&self) -> i32 {
        if matches!(self.page, Page::Appearance) {
            PREVIEW_H
        } else {
            0
        }
    }

    /// Where the trial bar sits while a trial runs: the top of the content, above the rows.
    fn status_top(&self) -> i32 {
        self.preview_h()
    }

    /// Top of the scrolling rows: the preview (Appearance only), then the Keep/Revert bar
    /// while a trial runs.
    fn top(&self) -> i32 {
        self.status_top() + if self.trial_active() { STATUS_H } else { 0 }
    }

    /// Height reserved at the bottom for the status message when no trial is running.
    fn bottom_h(&self) -> i32 {
        if self.trial_active() {
            0
        } else {
            STATUS_H
        }
    }

    fn px(&self, logical: i32) -> i32 {
        (logical as f32 * self.k).round() as i32
    }

    /// Display scale or text size changed: rebuild the scale and every font.
    fn set_dpi(&mut self, dpi: u32) {
        self.k = crate::layout_scale(dpi, uistyle::text_scale_percent());
        self.fonts.delete();
        self.fonts = Fonts::make((96.0 * self.k).round() as u32);
        for edit in self.all_edits() {
            unsafe { SendMessageW(edit, WM_SETFONT, self.fonts.body as WPARAM, 1) };
        }
    }

    /// Windows changed its light/dark mode, accent color, high contrast or text size, or the
    /// user chose an appearance: re-read the theme and repaint everything with it.
    fn refresh_theme(&mut self, hwnd: HWND) {
        let mode = uistyle::refresh(self.config.ui.appearance);
        self.accent = theme_accent().0;
        unsafe {
            DeleteObject(self.bg_brush);
            DeleteObject(self.edit_brush);
            self.bg_brush = CreateSolidBrush(th().bg);
            self.edit_brush = CreateSolidBrush(th().edit_bg);
        }
        self.set_dpi(dpi_of(hwnd));
        style_window(hwnd, mode);
        let theme = to_wide(if mode == crate::UiMode::Dark {
            "DarkMode_CFD"
        } else {
            "CFD"
        });
        for edit in self.all_edits() {
            unsafe { SetWindowTheme(edit, pcw(&theme), std::ptr::null()) };
        }
        self.relayout(hwnd);
    }

    fn all_edits(&self) -> [HWND; 9] {
        [
            self.edits.filter,
            self.edits.caption,
            self.edits.border,
            self.edits.text,
            self.edits.tint,
            self.edits.rule,
            self.edits.exclude,
            self.edits.look,
            self.edits.help,
        ]
    }

    /// Client-area logical width.
    /// Client width in logical px, navigation pane included.
    fn client_lw(&self, hwnd: HWND) -> i32 {
        (client_size(hwnd).0 as f32 / self.k) as i32
    }

    /// Width of the content area: everything right of the navigation pane.
    fn lw(&self, hwnd: HWND) -> i32 {
        (self.client_lw(hwnd) - self.nav_w)
            .min(MAX_CONTENT)
            .max(MIN_W)
    }

    /// Navigation pane width for the current window width: icons and labels when there is
    /// room, an icon rail when there is not (Microsoft: a navigation pane collapses to its
    /// compact form as the window narrows).
    fn pick_nav_w(&self, hwnd: HWND) -> i32 {
        if self.ext.setup_active() {
            0
        } else if !uistyle::icon_font_available() {
            // No icons to collapse to: keep the labels at every width.
            crate::ui_metrics::NAV_TEXT_ONLY
        } else if self.client_lw(hwnd) >= crate::ui_metrics::NAV_BREAKPOINT {
            crate::ui_metrics::NAV_EXPANDED
        } else {
            crate::ui_metrics::NAV_COMPACT
        }
    }

    /// Height of the scrolling region, in logical pixels.
    fn view_height(&self, hwnd: HWND) -> i32 {
        ((client_size(hwnd).1 as f32 / self.k) as i32 - self.top() - self.bottom_h()).max(1)
    }

    fn relayout(&mut self, hwnd: HWND) {
        self.nav_w = self.pick_nav_w(hwnd);
        // Long lines are hard to read, so content stops growing at MAX_CONTENT (a product choice;
        // the Microsoft pages read for this pass give no line length). Pages stay left-aligned beside the pane; the setup flow, which has no
        // pane, is centered.
        let spare = (self.client_lw(hwnd) - self.nav_w - self.lw(hwnd)).max(0);
        self.content_x = self.nav_w + if self.nav_w == 0 { spare / 2 } else { 0 };
        self.client_lh = (client_size(hwnd).1 as f32 / self.k) as i32;
        let view = self.view_height(hwnd);
        let max_scroll = (self.content_height() - view).max(0);
        self.scroll = self.scroll.clamp(0, max_scroll);
        self.position_edits(hwnd);
        self.update_nav_tooltips(hwnd);
        unsafe { InvalidateRect(hwnd, std::ptr::null(), 0) };
    }

    /// In the icon-only rail a pointer needs the names: one system tooltip per destination.
    /// (Screen readers get the same names from UI Automation.)
    fn update_nav_tooltips(&mut self, hwnd: HWND) {
        use windows_sys::Win32::UI::Controls::{
            InitCommonControlsEx, ICC_WIN95_CLASSES, INITCOMMONCONTROLSEX, TTF_SUBCLASS,
            TTM_ADDTOOLW, TTM_DELTOOLW, TTS_ALWAYSTIP, TTS_NOPREFIX, TTTOOLINFOW,
        };
        let compact = self.nav_w == crate::ui_metrics::NAV_COMPACT;
        if self.nav_tips.is_null() {
            if !compact {
                return;
            }
            unsafe {
                let init = INITCOMMONCONTROLSEX {
                    dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
                    dwICC: ICC_WIN95_CLASSES,
                };
                InitCommonControlsEx(&init);
                let class = to_wide("tooltips_class32");
                self.nav_tips = CreateWindowExW(
                    windows_sys::Win32::UI::WindowsAndMessaging::WS_EX_TOPMOST,
                    pcw(&class),
                    std::ptr::null(),
                    windows_sys::Win32::UI::WindowsAndMessaging::WS_POPUP
                        | TTS_ALWAYSTIP
                        | TTS_NOPREFIX,
                    0,
                    0,
                    0,
                    0,
                    hwnd,
                    std::ptr::null_mut(),
                    GetModuleHandleW(std::ptr::null()),
                    std::ptr::null(),
                );
            }
            if self.nav_tips.is_null() {
                return;
            }
        }
        // Remove the tools added last time, then add one per destination if the rail is showing.
        self.nav_tip_text = Page::ALL
            .iter()
            .map(|page| to_wide(page.nav_label()))
            .collect();
        for index in 0..Page::ALL.len() {
            let mut tool: TTTOOLINFOW = unsafe { std::mem::zeroed() };
            tool.cbSize = std::mem::size_of::<TTTOOLINFOW>() as u32;
            tool.hwnd = hwnd;
            tool.uId = index;
            unsafe { SendMessageW(self.nav_tips, TTM_DELTOOLW, 0, &tool as *const _ as LPARAM) };
        }
        if !compact {
            return;
        }
        for (index, text) in self.nav_tip_text.iter().enumerate() {
            let (l, t, r, b) = pages::nav_item_rect(self, index);
            let mut tool: TTTOOLINFOW = unsafe { std::mem::zeroed() };
            tool.cbSize = std::mem::size_of::<TTTOOLINFOW>() as u32;
            tool.uFlags = TTF_SUBCLASS;
            tool.hwnd = hwnd;
            tool.uId = index;
            tool.rect = RECT {
                left: self.px(l),
                top: self.px(t),
                right: self.px(r),
                bottom: self.px(b),
            };
            tool.lpszText = text.as_ptr() as *mut u16;
            unsafe { SendMessageW(self.nav_tips, TTM_ADDTOOLW, 0, &tool as *const _ as LPARAM) };
        }
    }

    /// Mouse position to (logical x, content y). None above the scrolling region.
    fn to_content(&self, lparam: LPARAM) -> Option<(i32, i32)> {
        let lx = (lparam_x(lparam) as f32 / self.k) as i32 - self.content_x;
        let ly = (lparam_y(lparam) as f32 / self.k) as i32;
        if ly < self.top() || lx < 0 {
            return None;
        }
        Some((lx, ly - self.top() + self.scroll))
    }

    fn reload_from_disk(&mut self) {
        let focus = unsafe { GetFocus() };
        let editing = self.all_edits().contains(&focus);
        self.config = cfgio::load();
        self.autostart = registry::autostart_enabled();
        self.accent = theme_accent().0;
        if self
            .rule
            .is_some_and(|index| index >= self.config.rules.len())
        {
            self.rule = None;
        }
        if self
            .exclude_sel
            .is_some_and(|index| index >= self.config.exclude.len())
        {
            self.exclude_sel = None;
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
        if self.trial_active() {
            // A trial is running in memory. Saving now would persist it without a Keep.
            self.status = "Keep or revert the trial look first.".into();
            return;
        }
        if let Err(err) = cfgio::store(&self.config) {
            self.status = format!("Could not save: {err}");
            return;
        }
        unsafe { PostMessageW(self.owner, WM_SETTINGS_APPLIED, 0, 0) };
    }

    // --- layout -----------------------------------------------------------

    fn rows(&self) -> Vec<Laid> {
        self.rows_for_page()
    }

    fn content_height(&self) -> i32 {
        self.rows()
            .last()
            .map(|row| row.y + row.h + 24)
            .unwrap_or(0)
    }

    // --- input ------------------------------------------------------------

    fn click(&mut self, hwnd: HWND, lx: i32, cy: i32) {
        let lw = self.lw(hwnd);
        let _measure = MeasureScope::new(self, None, lw);
        if self.trial_active() {
            self.status = "Keep or revert the trial look first (the buttons above).".into();
            return;
        }
        for row in self.rows() {
            if cy < row.y || cy >= row.y + row.h {
                continue;
            }
            let local = cy - row.y;
            match &row.row {
                Row::Toggle(id) => self.toggle(*id),
                Row::Choice(id) => self.cycle(*id),
                Row::Presets => {
                    if let Some(index) = preset_at(lw, lx, local) {
                        self.apply_preset(index);
                    }
                }
                Row::Action(id) => {
                    let (l, t, r, b) = action_pill(*id, lw, row.h);
                    if lx >= l && lx < r && local >= t && local < b {
                        self.button(*id);
                    }
                }
                Row::AddField(id) => {
                    let (l, t, r, b) = add_pill(lw);
                    if lx >= l && lx < r && local >= t && local < b {
                        self.button(*id);
                    }
                }
                Row::Buttons(list) => self.click_buttons(lw, lx, local, list.clone(), row.h),
                Row::Credit => {
                    let (l, t, r, b) = credit_layout(lw).link;
                    if lx >= l - 2 && lx < r + 2 && local >= t - 2 && local < b + 2 {
                        self.open_credit();
                    }
                }
                Row::WindowList => self.click_window_list((local - 8) / LIST_LINE),
                Row::RecipeList => self.click_recipe_list((local - 8) / LIST_LINE),
                Row::LookList => self.click_look_list((local - 8) / LIST_LINE),
                Row::RuleList => self.pick_rule((local - 8) / LIST_LINE),
                Row::ExcludeList => self.pick_exclude((local - 8) / LIST_LINE),
                _ => {}
            }
            return;
        }
    }

    fn apply_preset(&mut self, index: usize) {
        let mut next = self.config.clone();
        if !crate::apply_preset(&mut next, index) {
            return;
        }
        self.rule = None;
        self.preset_sel = Some(index);
        self.push_colors_to_edits();
        self.begin_trial(PRESETS[index].name, next);
    }

    fn toggle(&mut self, id: Toggle) {
        match id {
            Toggle::Theme => self.config.enabled = !self.config.enabled,
            Toggle::DefaultOn => {
                let on = !self.config.defaults.enabled.unwrap_or(true);
                self.config.defaults.enabled = Some(on);
            }
            Toggle::RuleOn => {
                if let Some(rule) = self.rule.and_then(|index| self.config.rules.get_mut(index)) {
                    let on = !rule.style.enabled.unwrap_or(true);
                    rule.style.enabled = Some(on);
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
            Toggle::Oled => {
                self.config.shell.oled_taskbar = !self.config.shell.oled_taskbar;
                self.status =
                    "Taskbar changes can need an Explorer restart. See Restart Explorer below."
                        .into();
            }
            Toggle::Black => {
                self.config.shell.black_shell = !self.config.shell.black_shell;
                self.status = "Start and notification center update as Windows redraws them. Restart Explorer if they do not.".into();
            }
            Toggle::Autostart => {
                let next = !self.autostart;
                if let Err(err) = registry::set_autostart(next) {
                    self.status = err;
                    return;
                }
                self.autostart = next;
                self.status = if next {
                    "AllBlack11 will start when you sign in.".into()
                } else {
                    "AllBlack11 will not start automatically.".into()
                };
                return;
            }
            Toggle::Reapply => {
                self.config.reapply_on_foreground = !self.config.reapply_on_foreground
            }
            Toggle::Logging => self.config.logging = !self.config.logging,
            Toggle::FocusLook
            | Toggle::ReduceBattery
            | Toggle::ReduceRemote
            | Toggle::ReduceTransparency
            | Toggle::EmulatedX64
            | Toggle::IdentityAccents
            | Toggle::DayNight
            | Toggle::PaletteHotkey
            | Toggle::AutoUpdateCheck
            | Toggle::ShowTitles => {
                self.toggle_ext(id);
                return;
            }
            Toggle::Experimental => {
                self.config.experimental_dark_legacy = !self.config.experimental_dark_legacy;
                self.status = if self.config.experimental_dark_legacy {
                    "Dark legacy windows are on. Classic dialogs may be recolored in-process."
                        .into()
                } else {
                    "Dark legacy windows are off. The hook is unloaded.".into()
                };
            }
        }
        self.commit();
    }

    fn cycle(&mut self, id: Choice) {
        if matches!(
            id,
            Choice::Palette
                | Choice::RuleScope
                | Choice::DayScene
                | Choice::NightScene
                | Choice::DayStarts
                | Choice::NightStarts
                | Choice::AppAppearance
                | Choice::UpdateChannel
                | Choice::Pack
        ) {
            self.cycle_ext(id);
            return;
        }
        let mut style = self.current_style();
        match id {
            Choice::Backdrop => style.backdrop = next_backdrop(style.backdrop),
            Choice::Corners => style.corners = next_corners(style.corners),
            _ => {}
        }
        self.write_style(style);
        self.commit();
    }

    fn button(&mut self, id: Button) {
        if self.run_button_ext(id) {
            return;
        }
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
                if let Some(index) = self.config.rules.iter().position(|rule| {
                    rule.process.eq_ignore_ascii_case(&name)
                        && rule.class.is_none()
                        && rule.path.is_none()
                        && rule.package.is_none()
                        && rule.title.is_none()
                }) {
                    self.rule = Some(index);
                } else {
                    let mut rule = Rule::for_process(&name);
                    rule.style.enabled = Some(true);
                    self.config.rules.push(rule);
                    self.rule = Some(self.config.rules.len() - 1);
                }
                set_text(self.edits.rule, "");
                self.push_colors_to_edits();
                self.status = "Rule added. Edit its look under Look for one app.".into();
                self.commit();
            }
            Button::RemoveRule => {
                let Some(index) = self.rule else {
                    self.status = "Pick a rule in the list first.".into();
                    return;
                };
                if index < self.config.rules.len() {
                    self.config.rules.remove(index);
                }
                self.rule = None;
                self.push_colors_to_edits();
                self.commit();
            }
            Button::AddExclude => {
                let name = file_name(&get_text(self.edits.exclude)).trim().to_string();
                if name.is_empty() {
                    self.status = "Type an exe name to exclude.".into();
                    return;
                }
                if !self
                    .config
                    .exclude
                    .iter()
                    .any(|item| item.eq_ignore_ascii_case(&name))
                {
                    self.config.exclude.push(name);
                }
                set_text(self.edits.exclude, "");
                self.commit();
            }
            Button::RemoveExclude => {
                let Some(index) = self.exclude_sel else {
                    self.status = "Pick an app in the list first.".into();
                    return;
                };
                if index < self.config.exclude.len() {
                    self.config.exclude.remove(index);
                }
                self.exclude_sel = None;
                self.commit();
            }
            Button::RestartExplorer => {
                // The tray host asks for confirmation and owns the restart.
                self.status = "Waiting for your answer…".into();
                unsafe { PostMessageW(self.owner, WM_RESTART_EXPLORER, 0, 0) };
            }
            Button::OpenFolder => {
                self.status = match registry::open_file(&config_dir()) {
                    Ok(()) => "Opened the data folder.".into(),
                    Err(err) => err,
                };
            }
            Button::OpenLog => {
                let path = log_path();
                self.status = if path.exists() {
                    match registry::open_file(&path) {
                        Ok(()) => "Opened the log.".into(),
                        Err(err) => err,
                    }
                } else {
                    "There is no log yet. Turn on Write a log first.".into()
                };
            }
            Button::Diagnostics => {
                unsafe { PostMessageW(self.owner, WM_RUN_COMMAND, CMD_DIAGNOSTICS as usize, 0) };
                self.status = "Writing the diagnostics report…".into();
            }
            _ => {}
        }
    }

    /// Validate while typing, without ever showing an error for a half-typed value.
    fn edit_live(&mut self, id: u16) -> bool {
        let Some(hwnd) = self.color_edit(id) else {
            return false;
        };
        let Ok(color) = parse_color(get_text(hwnd).trim()) else {
            return false;
        };
        let mut style = self.current_style();
        let slot = match id {
            ID_CAPTION => &mut style.caption,
            ID_BORDER => &mut style.border,
            ID_TEXT => &mut style.text,
            _ => &mut style.acrylic_tint,
        };
        if *slot == color {
            return false;
        }
        *slot = color;
        self.write_style(style);
        self.status.clear();
        self.commit();
        true
    }

    fn edit_committed(&mut self, id: u16) {
        let Some(hwnd) = self.color_edit(id) else {
            return;
        };
        let text = get_text(hwnd);
        if let Err(err) = parse_color(text.trim()) {
            self.status = err;
            self.push_colors_to_edits();
        }
    }

    fn color_edit(&self, id: u16) -> Option<HWND> {
        match id {
            ID_CAPTION => Some(self.edits.caption),
            ID_BORDER => Some(self.edits.border),
            ID_TEXT => Some(self.edits.text),
            ID_TINT => Some(self.edits.tint),
            _ => None,
        }
    }

    fn set_slider(&mut self, hwnd: HWND, slider: Slider, lx: i32) {
        let (x0, x1) = slider_track(self.lw(hwnd));
        let fraction = ((lx - x0) as f32 / (x1 - x0).max(1) as f32).clamp(0.0, 1.0);
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

    fn hit_slider(&self, cy: i32) -> Option<Slider> {
        for row in self.rows() {
            if cy >= row.y && cy < row.y + row.h {
                if let Row::Slider(id) = row.row {
                    if cy >= row.y + 56 {
                        return Some(id);
                    }
                }
            }
        }
        None
    }

    fn pick_rule(&mut self, index: i32) {
        if index >= 0 && (index as usize) < self.config.rules.len() {
            self.rule = Some(index as usize);
            self.push_colors_to_edits();
        }
    }

    fn pick_exclude(&mut self, index: i32) {
        if index >= 0 && (index as usize) < self.config.exclude.len() {
            self.exclude_sel = Some(index as usize);
        }
    }

    /// Place or hide each EDIT. An EDIT is a real child window, so it must be hidden
    /// while its row is scrolled out of view or under the pinned preview.
    fn position_edits(&self, hwnd: HWND) {
        let lw = self.lw(hwnd);
        let (_, client_h) = client_size(hwnd);
        let mut targets: Vec<(HWND, Option<(i32, i32, i32, i32)>)> =
            self.all_edits().iter().map(|edit| (*edit, None)).collect();
        for row in self.rows() {
            let top = row.y - self.scroll + self.top();
            let spot = |l: i32, t: i32, w: i32, h: i32| {
                let rect = (
                    self.px(l + self.content_x),
                    self.px(top + t),
                    self.px(w),
                    self.px(h),
                );
                let visible = rect.1 >= self.px(self.top()) && rect.1 + rect.3 <= client_h;
                visible.then_some(rect)
            };
            let wanted = match row.row {
                Row::Color(id) => self
                    .color_edit(id)
                    .map(|edit| (edit, spot(lw - MARGIN - PAD - 120, 14, 120, 24))),
                Row::SearchField => Some((
                    self.edits.help,
                    spot(MARGIN + PAD + 100, 14, lw - 2 * MARGIN - 2 * PAD - 100, 24),
                )),
                Row::Filter => Some((
                    self.edits.filter,
                    spot(MARGIN + PAD + 70, 14, lw - 2 * MARGIN - 2 * PAD - 70, 24),
                )),
                Row::AddField(Button::AddRule) => Some((
                    self.edits.rule,
                    spot(MARGIN + PAD, 14, lw - 2 * MARGIN - 2 * PAD - 96, 24),
                )),
                Row::AddField(Button::AddExclude) => Some((
                    self.edits.exclude,
                    spot(MARGIN + PAD, 14, lw - 2 * MARGIN - 2 * PAD - 96, 24),
                )),
                Row::AddField(Button::SaveLook) => Some((
                    self.edits.look,
                    spot(MARGIN + PAD, 14, lw - 2 * MARGIN - 2 * PAD - 96, 24),
                )),
                _ => None,
            };
            if let Some((edit, rect)) = wanted {
                if let Some(slot) = targets.iter_mut().find(|slot| slot.0 == edit) {
                    slot.1 = rect;
                }
            }
        }
        for (edit, rect) in targets {
            match rect {
                Some((x, y, w, h)) => place_edit(edit, x, y, w, h, true),
                None => place_edit(edit, 0, 0, 0, 0, false),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Shared geometry (logical), used by painting and by hit-testing.
// ---------------------------------------------------------------------------

fn slider_track(lw: i32) -> (i32, i32) {
    (MARGIN + 6, lw - MARGIN - 6)
}

/// Right-aligned button inside a card. Rectangles are relative to the row's top.
fn action_pill(id: Button, lw: i32, row_h: i32) -> (i32, i32, i32, i32) {
    let (_, _, label) = button_meta(id);
    let width = (label.chars().count() as i32 * 8 + 32).clamp(92, 220);
    let card_h = row_h - GAP;
    let right = lw - MARGIN - PAD;
    (
        right - width,
        (card_h - 32) / 2,
        right,
        (card_h - 32) / 2 + 32,
    )
}

fn add_pill(lw: i32) -> (i32, i32, i32, i32) {
    (lw - MARGIN - PAD - 84, 10, lw - MARGIN - PAD, 42)
}

fn preset_rect(lw: i32, index: usize) -> (i32, i32, i32, i32) {
    let gap = 8;
    let inner = lw - 2 * MARGIN - 2 * PAD;
    let width = (inner - 3 * gap) / 4;
    let left = MARGIN + PAD + index as i32 * (width + gap);
    (left, 14, left + width, 48)
}

fn preset_at(lw: i32, lx: i32, local_y: i32) -> Option<usize> {
    (0..PRESETS.len()).find(|&index| {
        let (l, t, r, b) = preset_rect(lw, index);
        lx >= l && lx < r && local_y >= t && local_y < b
    })
}

// ---------------------------------------------------------------------------
// Painting
// ---------------------------------------------------------------------------

struct Canvas {
    dc: HDC,
    k: f32,
}

const DT_LEFT: u32 = 0x0000;
const DT_CENTER: u32 = 0x0001;
const DT_RIGHT: u32 = 0x0002;
const DT_VCENTER: u32 = 0x0004;
const DT_SINGLELINE: u32 = 0x0020;
const DT_WORDBREAK: u32 = 0x0010;
const DT_CALCRECT: u32 = 0x0400;
const DT_NOCLIP: u32 = 0x0100;
const DT_NOPREFIX: u32 = 0x0800;
const DT_END_ELLIPSIS: u32 = 0x8000;

impl Canvas {
    fn px(&self, v: i32) -> i32 {
        (v as f32 * self.k).round() as i32
    }

    /// Draw an icon, `size` logical pixels square, at a logical position.
    fn icon(
        &self,
        icon: windows_sys::Win32::UI::WindowsAndMessaging::HICON,
        x: i32,
        y: i32,
        size: i32,
    ) {
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::DrawIconEx(
                self.dc,
                self.px(x),
                self.px(y),
                icon,
                self.px(size),
                self.px(size),
                0,
                std::ptr::null_mut(),
                windows_sys::Win32::UI::WindowsAndMessaging::DI_NORMAL,
            );
        }
    }

    fn rect(&self, l: i32, t: i32, r: i32, b: i32) -> RECT {
        RECT {
            left: self.px(l),
            top: self.px(t),
            right: self.px(r),
            bottom: self.px(b),
        }
    }

    fn fill(&self, l: i32, t: i32, r: i32, b: i32, color: u32) {
        unsafe {
            let brush = CreateSolidBrush(color);
            FillRect(self.dc, &self.rect(l, t, r, b), brush);
            DeleteObject(brush);
        }
    }

    /// Rounded rectangle. `radius` is the corner radius in logical pixels.
    #[allow(clippy::too_many_arguments)]
    fn round(
        &self,
        l: i32,
        t: i32,
        r: i32,
        b: i32,
        radius: i32,
        fill: Option<u32>,
        stroke: Option<u32>,
    ) {
        unsafe {
            let brush = match fill {
                Some(color) => CreateSolidBrush(color),
                None => GetStockObject(NULL_BRUSH) as HBRUSH,
            };
            let pen = match stroke {
                Some(color) => CreatePen(PS_SOLID, self.px(1).max(1), color),
                None => GetStockObject(NULL_PEN),
            };
            let old_brush = SelectObject(self.dc, brush);
            let old_pen = SelectObject(self.dc, pen);
            let rect = self.rect(l, t, r, b);
            let d = self.px(radius * 2);
            RoundRect(self.dc, rect.left, rect.top, rect.right, rect.bottom, d, d);
            SelectObject(self.dc, old_pen);
            SelectObject(self.dc, old_brush);
            if fill.is_some() {
                DeleteObject(brush);
            }
            if stroke.is_some() {
                DeleteObject(pen);
            }
        }
    }

    fn line(&self, x1: i32, y1: i32, x2: i32, y2: i32, color: u32, width: i32) {
        unsafe {
            let pen = CreatePen(PS_SOLID, self.px(width).max(1), color);
            let old = SelectObject(self.dc, pen);
            MoveToEx(self.dc, self.px(x1), self.px(y1), std::ptr::null_mut());
            LineTo(self.dc, self.px(x2), self.px(y2));
            SelectObject(self.dc, old);
            DeleteObject(pen);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn text(
        &self,
        l: i32,
        t: i32,
        r: i32,
        b: i32,
        text: &str,
        color: u32,
        font: HFONT,
        flags: u32,
    ) {
        unsafe {
            let old = SelectObject(self.dc, font);
            SetTextColor(self.dc, color);
            let mut rect = self.rect(l, t, r, b);
            let wide = to_wide(text);
            // Wrapped text is measured to fit, so it needs no clip rectangle; and where a renderer
            // breaks a line a few pixels later than it measured, an unclipped word is whole
            // instead of sliced at the edge. The window and scroll clips still apply.
            let clip = if flags & DT_WORDBREAK != 0 {
                DT_NOCLIP
            } else {
                0
            };
            DrawTextW(
                self.dc,
                wide.as_ptr(),
                -1,
                &mut rect,
                flags | DT_NOPREFIX | clip,
            );
            SelectObject(self.dc, old);
        }
    }
}

fn paint(hwnd: HWND, ui: &mut Ui) {
    unsafe {
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
        FillRect(dc, &full, ui.bg_brush);

        let canvas = Canvas { dc, k: ui.k };
        let lw = ui.lw(hwnd);
        // Rows are measured with this DC and these fonts, exactly as they are painted.
        let _measure = MeasureScope::new(ui, Some(dc), lw);
        ui.pointer.begin_frame();

        // Everything right of the navigation pane is drawn in content coordinates: the
        // origin moves, and a clip keeps it out of the pane.
        let nav_px = canvas.px(ui.content_x);
        SetViewportOrgEx(dc, nav_px, 0, std::ptr::null_mut());
        IntersectClipRect(dc, 0, 0, w - nav_px, h);

        // Scrolling rows, clipped under the pinned preview and status strip.
        let pinned = ui.top();
        IntersectClipRect(
            dc,
            0,
            canvas.px(pinned),
            w,
            canvas.px(ui.client_lh - ui.bottom_h()),
        );
        let rows = ui.rows();
        let focus_row = ui
            .ext
            .focus
            .and_then(|ordinal| ui.interactive_rows().get(ordinal).map(|(index, _)| *index));
        for (index, row) in rows.iter().enumerate() {
            let top = row.y - ui.scroll + pinned;
            if canvas.px(top) > h || canvas.px(top + row.h) < canvas.px(pinned) {
                continue;
            }
            paint_row(&canvas, ui, row, top, lw);
            pages::paint_tutorial_highlight(&canvas, ui, row, top, lw);
            if focus_row == Some(index) {
                pages::paint_focus_ring(&canvas, ui, row, top, lw);
            }
        }
        paint_scrollbar(&canvas, ui, hwnd, lw);
        SelectClipRgn(dc, std::ptr::null_mut());

        if ui.preview_h() > 0 {
            paint_preview(&canvas, ui, lw);
            // The preview sets its own origin; put the content origin back.
            SetViewportOrgEx(dc, nav_px, 0, std::ptr::null_mut());
            SelectClipRgn(dc, std::ptr::null_mut());
            IntersectClipRect(dc, 0, 0, w - nav_px, h);
        }
        paint_status(&canvas, ui, lw, ui.client_lh);
        SelectClipRgn(dc, std::ptr::null_mut());
        SetViewportOrgEx(dc, 0, 0, std::ptr::null_mut());
        if ui.nav_w > 0 {
            pages::paint_nav(&canvas, ui);
        }

        BitBlt(screen, 0, 0, w, h, dc, 0, 0, SRCCOPY);
        SelectObject(dc, old_bitmap);
        DeleteObject(bitmap);
        DeleteDC(dc);
        windows_sys::Win32::Graphics::Gdi::EndPaint(hwnd, &ps);
    }
}

fn paint_scrollbar(c: &Canvas, ui: &Ui, hwnd: HWND, lw: i32) {
    let view = ui.view_height(hwnd);
    let content = ui.content_height();
    if content <= view {
        return;
    }
    let track = view - 8;
    let thumb = (track as f32 * view as f32 / content as f32).max(28.0) as i32;
    let max_scroll = (content - view).max(1);
    let offset = ((track - thumb) as f32 * ui.scroll as f32 / max_scroll as f32) as i32;
    let top = ui.top() + 4 + offset;
    c.round(lw - 7, top, lw - 3, top + thumb, 2, Some(th().track), None);
}

/// The Keep/Revert bar while a trial runs (top), otherwise the status message (bottom). A
/// message always carries words; it is never just a color.
fn paint_status(c: &Canvas, ui: &Ui, lw: i32, lh: i32) {
    if ui.trial_active() {
        let top = ui.status_top();
        c.fill(0, top, lw, top + STATUS_H, th().bg);
        pages::paint_trial_bar(c, ui, lw);
        return;
    }
    let top = lh - STATUS_H;
    c.fill(0, top, lw, lh, th().bg);
    c.fill(MARGIN, top, lw - MARGIN, top + 1, th().edge);
    let (text, color) = if ui.status.is_empty() {
        ("Changes apply as you make them.", th().muted)
    } else {
        (ui.status.as_str(), th().text)
    };
    c.text(
        MARGIN,
        top + 1,
        lw - MARGIN,
        lh,
        text,
        color,
        ui.fonts.small,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
    );
}

fn card(c: &Canvas, top: i32, h: i32, lw: i32) {
    c.round(
        MARGIN,
        top,
        lw - MARGIN,
        top + h - GAP,
        crate::ui_metrics::CONTROL_RADIUS,
        Some(th().card),
        Some(th().edge),
    );
}

fn paint_row(c: &Canvas, ui: &Ui, row: &Laid, top: i32, lw: i32) {
    let fonts = &ui.fonts;
    let left = MARGIN + PAD;
    let right = lw - MARGIN - PAD;
    let card_h = row.h - GAP;
    match &row.row {
        Row::Header(text) => {
            c.text(
                MARGIN,
                top + 14,
                lw - MARGIN,
                top + 40,
                text,
                th().text,
                fonts.head,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
        }
        Row::Tip(parts) => pages::paint_tip(c, ui, parts, top, row.h, lw),
        Row::PageTitle(title, description) => {
            c.text(
                MARGIN,
                top + 8,
                lw - MARGIN,
                top + 44,
                title,
                th().text,
                fonts.title,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
            );
            c.text(
                MARGIN,
                top + 48,
                lw - MARGIN,
                top + row.h,
                description,
                th().muted,
                fonts.body,
                DT_LEFT | DT_WORDBREAK,
            );
        }
        Row::Note(text) => {
            c.text(
                MARGIN + 2,
                top + 2,
                lw - MARGIN,
                top + row.h,
                text,
                th().muted,
                fonts.small,
                DT_LEFT | DT_WORDBREAK,
            );
        }
        Row::Toggle(id) => {
            card(c, top, row.h, lw);
            let (label, desc) = toggle_meta(*id);
            let on = toggle_on(ui, *id);
            c.text(
                left,
                top + 10,
                right - 120,
                top + 30,
                label,
                th().text,
                fonts.body,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
            );
            c.text(
                left,
                top + 30,
                right - 120,
                top + card_h - 4,
                desc,
                th().muted,
                fonts.small,
                DT_LEFT | DT_WORDBREAK,
            );
            // "On" text, then the switch, both right-aligned and vertically centered.
            let mid = top + card_h / 2;
            c.text(
                right - 100,
                mid - 12,
                right - 52,
                mid + 12,
                if on { "On" } else { "Off" },
                th().text,
                fonts.body,
                DT_RIGHT | DT_VCENTER | DT_SINGLELINE,
            );
            draw_toggle(c, right - 40, mid - 10, on, ui.accent);
        }
        Row::Choice(id) => {
            card(c, top, row.h, lw);
            c.text(
                left,
                top + 10,
                right - 190,
                top + 30,
                choice_title(*id),
                th().text,
                fonts.body,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
            c.text(
                left,
                top + 30,
                right - 190,
                top + card_h - 4,
                choice_desc(ui, *id),
                th().muted,
                fonts.small,
                DT_LEFT | DT_WORDBREAK,
            );
            let mid = top + card_h / 2;
            c.round(
                right - 176,
                mid - 16,
                right,
                mid + 16,
                5,
                Some(th().card_hi),
                Some(th().edge),
            );
            c.text(
                right - 164,
                mid - 16,
                right - 30,
                mid + 16,
                &choice_text(ui, *id),
                th().text,
                fonts.body,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
            // A drawn chevron, not a glyph: it never depends on a font having the character.
            let (cx, cy) = (right - 19, mid);
            c.line(cx - 4, cy - 2, cx, cy + 2, th().muted, 2);
            c.line(cx, cy + 2, cx + 4, cy - 2, th().muted, 2);
        }
        Row::Slider(id) => {
            card(c, top, row.h, lw);
            let (label, desc) = slider_meta(*id);
            let (fraction, value) = slider_state(ui, *id);
            c.text(
                left,
                top + 10,
                right - 90,
                top + 30,
                label,
                th().text,
                fonts.body,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
            c.text(
                right - 90,
                top + 10,
                right,
                top + 30,
                &value,
                th().text,
                fonts.body,
                DT_RIGHT | DT_VCENTER | DT_SINGLELINE,
            );
            c.text(
                left,
                top + 30,
                right,
                top + 54,
                desc,
                th().muted,
                fonts.small,
                DT_LEFT | DT_WORDBREAK,
            );
            let (x0, x1) = slider_track(lw);
            draw_slider(c, x0, x1, top + 72, fraction, ui.accent);
        }
        Row::Color(id) => {
            card(c, top, row.h, lw);
            let style = ui.current_style();
            let color = match *id {
                ID_CAPTION => style.caption,
                ID_BORDER => style.border,
                ID_TEXT => style.text,
                _ => style.acrylic_tint,
            };
            // A box that does not hold a usable color says so, in words and with a mark, not
            // only in color. The look keeps the last valid value meanwhile.
            let invalid = ui
                .color_edit(*id)
                .map(get_text)
                .is_some_and(|text| parse_color(text.trim()).is_err());
            if invalid {
                c.text(
                    left,
                    top + 6,
                    right - 180,
                    top + 28,
                    color_label(*id),
                    th().text,
                    fonts.body,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE,
                );
                c.text(
                    left,
                    top + 28,
                    right - 150,
                    top + card_h - 2,
                    "⚠ Not a color yet. Use #RRGGBB, accent, none or default.",
                    th().bad,
                    fonts.small,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
            } else {
                c.text(
                    left,
                    top,
                    right - 180,
                    top + card_h,
                    color_label(*id),
                    th().text,
                    fonts.body,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE,
                );
            }
            draw_swatch(c, right - 120 - 34, top + card_h / 2 - 9, color, ui.accent);
        }
        Row::Presets => {
            card(c, top, row.h, lw);
            for (index, preset) in PRESETS.iter().enumerate() {
                let (l, t, r, b) = preset_rect(lw, index);
                let selected = ui.preset_sel == Some(index);
                c.round(
                    l,
                    top + t,
                    r,
                    top + b,
                    5,
                    Some(th().card_hi),
                    Some(if selected { ui.accent } else { th().edge }),
                );
                c.text(
                    l,
                    top + t,
                    r,
                    top + b,
                    preset.name,
                    th().text,
                    fonts.body,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
            }
            let blurb = ui
                .preset_sel
                .map(|index| PRESETS[index].blurb)
                .unwrap_or(PRESET_HINT);
            c.text(
                left,
                top + 58,
                right,
                top + card_h - 6,
                blurb,
                th().muted,
                fonts.small,
                DT_LEFT | DT_WORDBREAK,
            );
        }
        Row::RuleList => draw_list(
            c,
            ui,
            top,
            row.h,
            lw,
            ui.config.rules.iter().map(|rule| rule.label()),
            ui.rule,
            "No rules yet. Add one below or from Fix this app.",
        ),
        Row::ExcludeList => draw_list(
            c,
            ui,
            top,
            row.h,
            lw,
            ui.config.exclude.iter().cloned(),
            ui.exclude_sel,
            "Nothing excluded.",
        ),
        Row::Action(id) => {
            card(c, top, row.h, lw);
            let (title, desc, button) = button_meta(*id);
            let (l, t, r, b) = action_pill(*id, lw, row.h);
            let text_right = l - 16;
            c.text(
                left,
                top + 10,
                text_right,
                top + 30,
                title,
                th().text,
                fonts.body,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
            );
            c.text(
                left,
                top + 30,
                text_right,
                top + card_h - 4,
                desc,
                th().muted,
                fonts.small,
                DT_LEFT | DT_WORDBREAK,
            );
            controls::draw_button(
                c,
                ui,
                (l, top + t, r, top + b),
                button,
                controls::emphasis_of(*id),
                controls::State::default(),
            );
        }
        Row::AddField(id) => {
            card(c, top, row.h, lw);
            let (_, _, label) = button_meta(*id);
            let (l, t, r, b) = add_pill(lw);
            controls::draw_button(
                c,
                ui,
                (l, top + t, r, top + b),
                label,
                controls::Emphasis::Standard,
                controls::State::default(),
            );
        }
        Row::Lines(_)
        | Row::Buttons(_)
        | Row::WindowList
        | Row::RecipeList
        | Row::LookList
        | Row::SearchField
        | Row::Filter => {
            pages::paint_extra(c, ui, row, top, lw);
        }
        Row::Brand => {
            // The mark is decoration: under a contrast theme it is left out and the words move
            // to the margin, so nothing depends on it.
            let mark = uistyle::brand_icon(c.px(56));
            let text_left = if mark.is_some() {
                MARGIN + 56 + 16
            } else {
                MARGIN
            };
            if let Some(icon) = mark {
                c.icon(icon, MARGIN, top + 8, 56);
            }
            c.text(
                text_left,
                top + 8,
                lw - MARGIN,
                top + 44,
                crate::BRAND_NAME,
                th().text,
                fonts.title,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
            );
            c.text(
                text_left,
                top + 48,
                lw - MARGIN,
                top + row.h,
                crate::BRAND_TAGLINE,
                th().muted,
                fonts.body,
                DT_LEFT | DT_WORDBREAK,
            );
        }
        Row::Credit => {
            let layout = credit_layout(lw);
            let (pl, pt, pr, pb) = layout.prefix;
            c.text(
                pl,
                top + pt,
                pr + 4,
                top + pb,
                crate::CREDIT_PREFIX.trim_end(),
                th().muted,
                fonts.body,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
            let (ll, lt, lr, lb) = layout.link;
            let rect = (ll, top + lt, lr, top + lb);
            let hot = controls::hot(ui, rect);
            let color = if hot {
                crate::blend_colors(th().brand, th().text, 0.35)
            } else {
                th().brand
            };
            c.text(
                ll,
                top + lt,
                lr + 4,
                top + lb,
                crate::CREATOR,
                color,
                fonts.body,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
            // An underline marks it as a link without relying on color.
            c.fill(ll, top + lb - 3, lr, top + lb - 2, color);
        }
        Row::Prose(lines) => {
            card(c, top, row.h, lw);
            let mut y = top + 12;
            for (line, height) in lines.iter().zip(prose_heights(lines, lw)) {
                c.text(
                    left,
                    y,
                    right,
                    y + height,
                    line,
                    th().text,
                    fonts.body,
                    DT_LEFT | DT_WORDBREAK,
                );
                y += height;
            }
        }
        Row::About(lines) => {
            card(c, top, row.h, lw);
            let mut y = top + 12;
            for (index, (line, height)) in
                lines.iter().zip(about_line_heights(lines, lw)).enumerate()
            {
                let (font, color) = if index == 0 {
                    (fonts.body, th().text)
                } else {
                    (fonts.small, th().muted)
                };
                c.text(
                    left,
                    y,
                    right,
                    y + height,
                    line,
                    color,
                    font,
                    DT_LEFT | DT_WORDBREAK,
                );
                y += height;
            }
        }
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
        Toggle::FocusLook => ui.config.focus.enabled,
        Toggle::ReduceBattery => ui.config.reduce_effects.on_battery_saver,
        Toggle::ReduceRemote => ui.config.reduce_effects.on_remote_session,
        Toggle::ReduceTransparency => ui.config.reduce_effects.on_transparency_off,
        Toggle::EmulatedX64 => ui.config.legacy.emulated_x64,
        Toggle::ShowTitles => ui.ext.show_titles,
        Toggle::IdentityAccents => ui.config.features.identity_accents,
        Toggle::DayNight => ui.config.features.day_night.enabled,
        Toggle::PaletteHotkey => ui.config.features.palette_hotkey,
        Toggle::AutoUpdateCheck => ui.config.updates.auto_check,
    }
}

fn choice_text(ui: &Ui, id: Choice) -> String {
    let style = ui.current_style();
    let day_night = &ui.config.features.day_night;
    match id {
        Choice::Backdrop => backdrop_label(style.backdrop).to_string(),
        Choice::Corners => corners_label(style.corners).to_string(),
        Choice::Palette => if ui.config.legacy.palette == "black" {
            "True black"
        } else {
            "Graphite"
        }
        .to_string(),
        Choice::RuleScope => match ui.ext.rule_scope {
            0 => "Executable",
            1 => "Executable + class",
            2 => "Package",
            _ => "Window class",
        }
        .to_string(),
        Choice::AppAppearance => ui.config.ui.appearance.label().to_string(),
        Choice::UpdateChannel => ui.config.updates.channel.label().to_string(),
        Choice::Pack => ui
            .ext
            .packs
            .get(ui.ext.pack_sel)
            .map(|(pack, imported)| {
                if *imported {
                    format!("{} (imported)", pack.name)
                } else {
                    pack.name.clone()
                }
            })
            .unwrap_or_else(|| "No packs".to_string()),
        Choice::DayScene => ui.scene_name_for(&day_night.day_scene),
        Choice::NightScene => ui.scene_name_for(&day_night.night_scene),
        Choice::DayStarts => day_night.day_starts.clone(),
        Choice::NightStarts => day_night.night_starts.clone(),
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

fn draw_toggle(c: &Canvas, x: i32, y: i32, on: bool, accent: u32) {
    c.round(
        x,
        y,
        x + 40,
        y + 20,
        10,
        Some(if on { accent } else { th().track }),
        None,
    );
    let cx = if on { x + 22 } else { x + 4 };
    draw_knob(c, cx + 7, y + 10, 7, 0x00FF_FFFF);
}

fn draw_slider(c: &Canvas, x0: i32, x1: i32, y: i32, fraction: f32, accent: u32) {
    c.round(x0, y - 2, x1, y + 2, 2, Some(th().track), None);
    let thumb = x0 + (fraction.clamp(0.0, 1.0) * (x1 - x0) as f32) as i32;
    c.round(x0, y - 2, thumb.max(x0 + 3), y + 2, 2, Some(accent), None);
    draw_knob(c, thumb, y, 8, 0x00FF_FFFF);
}

fn draw_knob(c: &Canvas, cx: i32, cy: i32, radius: i32, color: u32) {
    unsafe {
        let brush = CreateSolidBrush(color);
        let old = SelectObject(c.dc, brush);
        let null_pen = GetStockObject(NULL_PEN);
        let old_pen = SelectObject(c.dc, null_pen);
        Ellipse(
            c.dc,
            c.px(cx - radius),
            c.px(cy - radius),
            c.px(cx + radius),
            c.px(cy + radius),
        );
        SelectObject(c.dc, old_pen);
        SelectObject(c.dc, old);
        DeleteObject(brush);
    }
}

fn draw_swatch(c: &Canvas, x: i32, y: i32, color: Color, accent: u32) {
    let (fill, hatched) = match color {
        Color::Rgb { r, g, b } => (rgb(r, g, b), false),
        Color::Accent => (accent, false),
        Color::None => (0x0044_4444, true),
        Color::Default => (0x0088_8888, true),
    };
    c.round(x, y, x + 22, y + 18, 4, Some(fill), Some(0x0070_7070));
    if hatched {
        c.line(x + 4, y + 14, x + 18, y + 4, 0x00D0_D0D0, 1);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_list(
    c: &Canvas,
    ui: &Ui,
    top: i32,
    height: i32,
    lw: i32,
    items: impl Iterator<Item = String>,
    selected: Option<usize>,
    empty: &str,
) {
    card(c, top, height, lw);
    let mut any = false;
    for (index, item) in items.enumerate() {
        any = true;
        let line_top = top + 8 + index as i32 * LIST_LINE;
        if selected == Some(index) {
            c.round(
                MARGIN + 6,
                line_top,
                lw - MARGIN - 6,
                line_top + LIST_LINE,
                4,
                Some(th().card_hi),
                Some(ui.accent),
            );
        }
        c.text(
            MARGIN + PAD,
            line_top,
            lw - MARGIN - PAD,
            line_top + LIST_LINE,
            &item,
            th().text,
            ui.fonts.body,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
    }
    if !any {
        c.text(
            MARGIN + PAD,
            top + 8,
            lw - MARGIN - PAD,
            top + 8 + LIST_LINE,
            empty,
            th().muted,
            ui.fonts.small,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
    }
}

// --- live preview -----------------------------------------------------------

fn rgb(r: u8, g: u8, b: u8) -> u32 {
    u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16)
}

fn blend(a: u32, b: u32, t: f32) -> u32 {
    let mix = |shift: u32| -> u32 {
        let x = ((a >> shift) & 0xFF) as f32;
        let y = ((b >> shift) & 0xFF) as f32;
        (x * (1.0 - t) + y * t).round().clamp(0.0, 255.0) as u32
    };
    mix(0) | (mix(8) << 8) | (mix(16) << 16)
}

/// Keep an accent that is nearly black readable on a black window.
fn resolve(color: Color, accent: u32, fallback: u32) -> u32 {
    match color {
        Color::Rgb { r, g, b } => rgb(r, g, b),
        Color::Accent => accent,
        Color::Default | Color::None => fallback,
    }
}

/// Deep blue, teal, plum, as COLORREF.
const WALLPAPER: [u32; 3] = [0x005C_300F, 0x0064_5C0F, 0x0070_2C46];

fn paint_preview(c: &Canvas, ui: &Ui, lw_full: i32) {
    let style = ui.current_style();
    let black_shell = ui.config.shell.black_shell;
    let oled = ui.config.shell.oled_taskbar || black_shell;

    // The preview is an inset rounded card. Everything inside is drawn relative to
    // its top-left corner, so `lw` below is the card's width.
    let lw = lw_full - 2 * MARGIN;
    let ph = PREVIEW_H - 16;
    let (ox, oy) = (c.px(MARGIN + ui.content_x), c.px(8));
    unsafe {
        SetViewportOrgEx(c.dc, ox, oy, std::ptr::null_mut());
        let outer = CreateRoundRectRgn(
            ox,
            oy,
            ox + c.px(lw) + 1,
            oy + c.px(ph) + 1,
            c.px(16),
            c.px(16),
        );
        SelectClipRgn(c.dc, outer);

        // Wallpaper.
        let third = lw / 3;
        c.fill(0, 0, third, ph, WALLPAPER[0]);
        c.fill(third, 0, third * 2, ph, WALLPAPER[1]);
        c.fill(third * 2, 0, lw, ph, WALLPAPER[2]);
        c.text(
            12,
            4,
            200,
            20,
            "Live preview",
            0x00E6_DCD2,
            ui.fonts.small,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );

        // Start menu mock.
        let (sl, st, sr, sb) = (lw - 20 - 180, 22, lw - 20, 114);
        let start_fill = if black_shell {
            rgb(0, 0, 0)
        } else {
            rgb(44, 44, 50)
        };
        c.round(
            sl,
            st,
            sr,
            sb,
            8,
            Some(start_fill),
            Some(if black_shell {
                rgb(34, 34, 34)
            } else {
                rgb(70, 70, 78)
            }),
        );
        let search = if black_shell {
            rgb(24, 24, 24)
        } else {
            rgb(62, 62, 70)
        };
        c.round(sl + 12, st + 10, sr - 12, st + 26, 8, Some(search), None);
        for row in 0..2 {
            for col in 0..4 {
                let l = sl + 14 + col * 38;
                let t = st + 38 + row * 28;
                let tile = if row == 0 && col == 0 {
                    ui.accent
                } else if black_shell {
                    rgb(26, 26, 26)
                } else {
                    rgb(68, 68, 76)
                };
                c.round(l, t, l + 28, t + 20, 5, Some(tile), None);
            }
        }

        // Mock window.
        let (wl, wt, wr, wb) = (20, 22, lw - 20 - 180 - 16, 114);
        let radius = match style.corners {
            Corners::Default | Corners::Round => 9,
            Corners::Small => 4,
            Corners::Square => 0,
        };
        let caption = resolve(style.caption, ui.accent, rgb(32, 32, 32));
        let text_color = resolve(style.text, ui.accent, rgb(255, 255, 255));
        // Clip regions are in device coordinates, so add the viewport origin by hand.
        let inner = if radius == 0 {
            CreateRectRgn(ox + c.px(wl), oy + c.px(wt), ox + c.px(wr), oy + c.px(wb))
        } else {
            CreateRoundRectRgn(
                ox + c.px(wl),
                oy + c.px(wt),
                ox + c.px(wr) + 1,
                oy + c.px(wb) + 1,
                c.px(radius * 2),
                c.px(radius * 2),
            )
        };
        SelectClipRgn(c.dc, inner);
        c.fill(wl, wt, wr, wt + 28, caption);
        fill_backdrop(c, &style, ui.accent, wl, wt + 28, wr, wb, lw);
        for (index, width) in [96, 140, 70].iter().enumerate() {
            let y = wt + 42 + index as i32 * 14;
            c.round(
                wl + 14,
                y,
                (wl + 14 + width).min(wr - 14),
                y + 6,
                3,
                Some(rgb(86, 86, 94)),
                None,
            );
        }
        SelectClipRgn(c.dc, outer);
        DeleteObject(inner);
        c.text(
            wl + 12,
            wt,
            wl + 120,
            wt + 28,
            "Notepad",
            text_color,
            ui.fonts.small,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
        let by = wt + 14;
        c.line(wr - 62, by, wr - 52, by, text_color, 1);
        c.round(wr - 40, by - 5, wr - 30, by + 5, 0, None, Some(text_color));
        c.line(wr - 20, by - 5, wr - 10, by + 5, text_color, 1);
        c.line(wr - 20, by + 5, wr - 10, by - 5, text_color, 1);
        if style.border != Color::None {
            let border = resolve(style.border, ui.accent, rgb(90, 90, 96));
            c.round(wl, wt, wr, wb, radius, None, Some(border));
        }

        // Taskbar mock.
        let tb_top = ph - 34;
        let bar = if oled { rgb(0, 0, 0) } else { rgb(38, 38, 44) };
        c.fill(0, tb_top, lw, ph, bar);
        if !oled {
            c.fill(0, tb_top, lw, tb_top + 1, rgb(62, 62, 70));
        }
        let icons = 6;
        let start_x = lw / 2 - icons * 16;
        for index in 0..icons {
            let l = start_x + index * 32;
            let tint = if index == 0 {
                ui.accent
            } else if oled {
                rgb(70, 70, 76)
            } else {
                rgb(120, 120, 130)
            };
            c.round(l, tb_top + 8, l + 18, tb_top + 26, 4, Some(tint), None);
        }
        c.round(
            lw - 78,
            tb_top + 10,
            lw - 14,
            tb_top + 24,
            4,
            Some(if oled {
                rgb(26, 26, 26)
            } else {
                rgb(58, 58, 66)
            }),
            None,
        );

        SelectClipRgn(c.dc, std::ptr::null_mut());
        DeleteObject(outer);
        SetViewportOrgEx(c.dc, 0, 0, std::ptr::null_mut());
    }
    // Hairline border around the card.
    c.round(
        MARGIN + ui.content_x,
        8,
        MARGIN + ui.content_x + lw,
        8 + ph,
        crate::ui_metrics::OVERLAY_RADIUS,
        None,
        Some(th().edge),
    );
}

#[allow(clippy::too_many_arguments)]
fn fill_backdrop(c: &Canvas, style: &Style, accent: u32, l: i32, t: i32, r: i32, b: i32, lw: i32) {
    let solid = |color: u32| c.fill(l, t, r, b, color);
    let through = |overlay: u32, amount: f32| {
        let third = lw / 3;
        let bands = [(0, third), (third, third * 2), (third * 2, lw)];
        for (index, (x0, x1)) in bands.iter().enumerate() {
            let (from, to) = ((*x0).max(l), (*x1).min(r));
            if from < to {
                c.fill(from, t, to, b, blend(WALLPAPER[index], overlay, amount));
            }
        }
    };
    match style.backdrop {
        Backdrop::None => solid(rgb(32, 32, 32)),
        Backdrop::Auto | Backdrop::Mica => solid(rgb(30, 36, 42)),
        Backdrop::MicaAlt => solid(rgb(22, 26, 30)),
        Backdrop::Acrylic => through(rgb(40, 40, 44), 0.6),
        Backdrop::TintedAcrylic => {
            let tint = resolve(style.acrylic_tint, accent, rgb(0, 0, 0));
            through(tint, f32::from(style.acrylic_opacity) / 255.0);
        }
    }
}

// ---------------------------------------------------------------------------
// Labels, config helpers, and Win32 helpers
// ---------------------------------------------------------------------------

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

fn make_edit(parent: HWND, id: u16, font: HFONT) -> HWND {
    unsafe {
        let class = to_wide("EDIT");
        let hwnd = CreateWindowExW(
            0,
            pcw(&class),
            std::ptr::null(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | 0x0080, // ES_AUTOHSCROLL
            0,
            -100,
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
            SendMessageW(hwnd, WM_SETFONT, font as WPARAM, 1);
        }
        hwnd
    }
}

fn place_edit(hwnd: HWND, x: i32, y: i32, w: i32, h: i32, show: bool) {
    unsafe {
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            x,
            y,
            w.max(1),
            h.max(1),
            SWP_NOZORDER | SWP_NOACTIVATE | if show { SWP_SHOWWINDOW } else { SWP_HIDEWINDOW },
        );
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

fn client_size(hwnd: HWND) -> (i32, i32) {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    unsafe { GetClientRect(hwnd, &mut rect) };
    (
        (rect.right - rect.left).max(1),
        (rect.bottom - rect.top).max(1),
    )
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

/// Center a window of logical size `width` x `height` on the monitor under the cursor,
/// scaled to that monitor and shrunk to fit its work area.
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
        let (mut dpi_x, mut dpi_y) = (0u32, 0u32);
        let dpi = if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) == 0
            && dpi_x != 0
        {
            dpi_x
        } else {
            96
        };
        let work = info.rcWork;
        let k = dpi as f32 / 96.0;
        let avail_w = work.right - work.left;
        let avail_h = work.bottom - work.top;
        let w = ((width as f32 * k) as i32).min(avail_w - 24);
        let h = ((height as f32 * k) as i32).min(avail_h - 24);
        let x = work.left + (avail_w - w) / 2;
        let y = work.top + (avail_h - h) / 2;
        (x, y, w, h)
    }
}
