//! Legacy-engine policy that must be identical in every binary that makes a decision:
//! AllBlack11.exe, the x64 helper, and the hook DLL (which compiles this very file through
//! `#[path]`). Everything here is pure, has no dependencies, and is unit-tested.
//!
//! Three questions are answered here and nowhere else:
//!  1. Which processes and window classes may the legacy layer ever touch?
//!  2. What can be done for each control family, how, and how well? (the capability table)
//!  3. What does the shared-memory block between exe and hook look like?

#![allow(dead_code)]

// ---------------------------------------------------------------------------
// Shared state between AllBlack11.exe and the hook DLLs
// ---------------------------------------------------------------------------

/// Bumped whenever [`SharedState`] changes, so an exe and a hook from different builds
/// never interpret each other's memory. A mismatch makes the hook inert.
pub const MAGIC: u32 = 0xAB11_0035;

pub const FLAG_CLASSIC_BUTTONS: u32 = 1 << 0;
pub const FLAG_THEME_PUSH_BUTTONS: u32 = 1 << 1;

/// Colors are COLORREF (`0x00BBGGRR`). The hook only reads this block.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SharedState {
    pub magic: u32,
    pub enabled: u32,
    pub bg: u32,
    pub text: u32,
    pub link: u32,
    pub heading: u32,
    pub edit_bg: u32,
    pub disabled_text: u32,
    pub readonly_bg: u32,
    pub flags: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub name: &'static str,
    pub bg: u32,
    pub text: u32,
    pub link: u32,
    pub heading: u32,
    pub edit_bg: u32,
    pub disabled_text: u32,
    pub readonly_bg: u32,
}

/// Two supported palettes. "Graphite" is easier on the eyes in a bright room; "Black" is
/// true black for OLED. Both keep body text above 4.5:1 and disabled text above 3:1.
pub const PALETTES: [Palette; 2] = [
    Palette {
        name: "graphite",
        bg: 0x001C_1C1C,
        text: 0x00F0_F0F0,
        link: 0x00FF_B56E,
        heading: 0x00FF_FFFF,
        edit_bg: 0x002B_2B2B,
        disabled_text: 0x008C_8C8C,
        readonly_bg: 0x0024_2424,
    },
    Palette {
        name: "black",
        bg: 0x0000_0000,
        text: 0x00EE_EEEE,
        link: 0x00FF_B56E,
        heading: 0x00FF_FFFF,
        edit_bg: 0x0018_1818,
        disabled_text: 0x008A_8A8A,
        readonly_bg: 0x0010_1010,
    },
];

pub fn palette(name: &str) -> Palette {
    PALETTES
        .iter()
        .find(|palette| palette.name.eq_ignore_ascii_case(name))
        .copied()
        .unwrap_or(PALETTES[0])
}

impl SharedState {
    pub fn from_palette(palette: Palette, flags: u32) -> Self {
        Self {
            magic: MAGIC,
            enabled: 1,
            bg: palette.bg,
            text: palette.text,
            link: palette.link,
            heading: palette.heading,
            edit_bg: palette.edit_bg,
            disabled_text: palette.disabled_text,
            readonly_bg: palette.readonly_bg,
            flags,
        }
    }
}

/// WCAG relative luminance of a COLORREF.
pub fn luminance(color: u32) -> f64 {
    let channel = |shift: u32| -> f64 {
        let value = f64::from((color >> shift) & 0xFF) / 255.0;
        if value <= 0.03928 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(0) + 0.7152 * channel(8) + 0.0722 * channel(16)
}

pub fn contrast_ratio(a: u32, b: u32) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

// ---------------------------------------------------------------------------
// Processes and hosts
// ---------------------------------------------------------------------------

/// Processes that host the shell, the lock screen, or services. Not removable from config.
pub const ALWAYS_SKIP_PROCESSES: &[&str] = &[
    "allblack11.exe",
    "allblack11helper.x64.exe",
    "pepatheme.exe",
    "dwm.exe",
    "csrss.exe",
    "winlogon.exe",
    "wininit.exe",
    "lsass.exe",
    "lsaiso.exe",
    "services.exe",
    "smss.exe",
    "fontdrvhost.exe",
    "sihost.exe",
    "ctfmon.exe",
    "lockapp.exe",
    "logonui.exe",
    "userinit.exe",
    "startmenuexperiencehost.exe",
    "searchhost.exe",
    "searchapp.exe",
    "shellexperiencehost.exe",
    "shellhost.exe",
    "textinputhost.exe",
    "widgetboard.exe",
    "widgets.exe",
    "widgetservice.exe",
    "gamebar.exe",
    "gamebarftserver.exe",
    "gamebarpresencewriter.exe",
    "runtimebroker.exe",
    "taskhostw.exe",
    "systemsettingsbroker.exe",
    "securityhealthsystray.exe",
    "securityhealthservice.exe",
    "mousocoreworker.exe",
    "compattelrunner.exe",
];

/// Anti-cheat, game-streaming and security-sensitive processes. The legacy layer never
/// enters these, even when the user has not excluded them. Matching is by lowercase name.
pub const SENSITIVE_PROCESSES: &[&str] = &[
    "easyanticheat.exe",
    "easyanticheat_eos.exe",
    "beservice.exe",
    "beclient.exe",
    "vgc.exe",
    "vgtray.exe",
    "faceitclient.exe",
    "faceit.exe",
    "esportal.exe",
    "gameguard.des",
    "xigncode.exe",
    "geforcenow.exe",
    "geforcenowstreamer.exe",
    "geforcenowcontainer.exe",
    "parsecd.exe",
    "parsec.exe",
    "steamservice.exe",
    "msmpeng.exe",
    "mssense.exe",
    "nissrv.exe",
    "securityhealthui.exe",
    "consent.exe",
    "credentialuibroker.exe",
    "logonui.exe",
];

pub fn process_always_skipped(exe: &str) -> bool {
    let lower = exe.to_ascii_lowercase();
    ALWAYS_SKIP_PROCESSES.iter().any(|name| *name == lower)
}

pub fn process_sensitive(exe: &str) -> bool {
    let lower = exe.to_ascii_lowercase();
    SENSITIVE_PROCESSES.iter().any(|name| *name == lower)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostKind {
    Other,
    Explorer,
    Mmc,
    Denied,
}

pub fn host_kind(file_name: &str) -> HostKind {
    if process_always_skipped(file_name) || process_sensitive(file_name) {
        HostKind::Denied
    } else if file_name.eq_ignore_ascii_case("explorer.exe") {
        HostKind::Explorer
    } else if file_name.eq_ignore_ascii_case("mmc.exe") {
        HostKind::Mmc
    } else {
        HostKind::Other
    }
}

// ---------------------------------------------------------------------------
// Window classes and dialog verdicts
// ---------------------------------------------------------------------------

pub fn is_dialog_class(class: &str) -> bool {
    class == "#32770"
}

/// Classes that are Explorer or shell UI, or that host a foreign rendering engine. A dialog
/// that contains one of these has a client area the legacy layer cannot darken coherently,
/// so the whole dialog is left alone rather than half-darkened.
const SHELL_SURFACE_CLASSES: &[&str] = &[
    "DirectUIHWND",
    "SHELLDLL_DefView",
    "ShellTabWindowClass",
    "CabinetWClass",
    "ExploreWClass",
    "NamespaceTreeControl",
    "WorkerW",
    "ReBarWindow32",
    "ToolbarWindow32",
    "Internet Explorer_Server",
    "Shell Embedding",
    "Shell DocObject View",
    "CtrlNotifySink",
    "Chrome_RenderWidgetHostHWND",
    "Chrome_WidgetWin_0",
    "Chrome_WidgetWin_1",
    "CefBrowserWindow",
    "WebView2",
    "Windows.UI.Core.CoreWindow",
    "Microsoft.UI.Content.DesktopChildSiteBridge",
    "XAMLWindow",
    "WinUIDesktopWin32WindowClass",
    "ApplicationFrameWindow",
    "DesktopWindowXamlSource",
];

pub fn is_shell_surface_class(class: &str) -> bool {
    SHELL_SURFACE_CLASSES
        .iter()
        .any(|name| name.eq_ignore_ascii_case(class))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Treat the dialog and its supported controls.
    Full,
    /// Treat it, but some controls stay as Windows draws them. The list names the classes.
    Partial(Vec<String>),
    /// Leave the whole window alone. The reason is stable text for diagnostics.
    Skip(&'static str),
}

/// Decide what to do with a window.
/// * `class` is the window's own class.
/// * `children` are the classes of its descendants, in any order.
///
/// Only classic dialogs (`#32770`) are ever candidates. Explorer-hosted Control Panel,
/// Explorer folder windows, MMC main frames, and anything else are never candidates, in any
/// host, whatever their title says. Titles are not consulted at all: they are localized and
/// user-editable.
pub fn window_verdict(host: HostKind, class: &str, children: &[&str]) -> Verdict {
    if host == HostKind::Denied {
        return Verdict::Skip("denied-process");
    }
    if is_shell_surface_class(class) {
        return Verdict::Skip("shell-surface");
    }
    if !is_dialog_class(class) {
        return Verdict::Skip("not-a-dialog");
    }
    if children.iter().any(|child| is_shell_surface_class(child)) {
        return Verdict::Skip("contains-shell-surface");
    }
    let mut partial: Vec<String> = Vec::new();
    for child in children {
        let capability = capability(child, 0);
        if matches!(capability.level, Level::Partial | Level::Unsupported)
            && !partial.iter().any(|seen| seen.eq_ignore_ascii_case(child))
        {
            partial.push((*child).to_string());
        }
    }
    if partial.is_empty() {
        Verdict::Full
    } else {
        Verdict::Partial(partial)
    }
}

// ---------------------------------------------------------------------------
// Capability table
// ---------------------------------------------------------------------------

/// How a control family is treated. The hook implements each technique once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tech {
    /// Leave the control exactly as it is.
    Leave,
    /// Parent-supplied colors through `WM_CTLCOLOR*`.
    CtlColor,
    /// `SetWindowTheme(DarkMode_CFD)` plus `WM_CTLCOLOR*`. Edits, combos, list boxes.
    ThemeCfd,
    /// `SetWindowTheme(DarkMode_Explorer)`. Push buttons, scrollbars, links.
    ThemeExplorer,
    /// Remove the visual style (`SetWindowTheme("", "")`) so the control paints classically,
    /// then supply text and background colors through `WM_CTLCOLORSTATIC`. Check boxes,
    /// radio buttons, group boxes.
    Classic,
    /// List-view colors through `LVM_SET*COLOR` (originals saved and restored) + theme.
    ListView,
    /// Tree-view colors through `TVM_SET*COLOR` (originals saved and restored) + theme.
    TreeView,
    /// `DarkMode_ItemsView` on a list-view header.
    Header,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// Expected to look right on every supported Windows 11 build.
    Supported,
    /// Uses a technique that works on current Windows 11 builds but is not guaranteed by
    /// documentation. Verify with the harness.
    BestEffort,
    /// Part of the control stays light. The result is readable, but mixed.
    Partial,
    /// Not touched because the app draws it, or touching it is known to break it.
    Unsupported,
    /// Readable as Windows draws it, so there is nothing to do.
    Untouched,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capability {
    pub tech: Tech,
    pub level: Level,
    pub note: &'static str,
}

const BS_TYPE_MASK: u32 = 0x0F;
const BS_PUSHBUTTON: u32 = 0x0;
const BS_DEFPUSHBUTTON: u32 = 0x1;
const BS_CHECKBOX: u32 = 0x2;
const BS_AUTOCHECKBOX: u32 = 0x3;
const BS_RADIOBUTTON: u32 = 0x4;
const BS_3STATE: u32 = 0x5;
const BS_AUTO3STATE: u32 = 0x6;
const BS_GROUPBOX: u32 = 0x7;
const BS_AUTORADIOBUTTON: u32 = 0x9;
const BS_OWNERDRAW: u32 = 0xB;

/// What can be done for a control of this class. `style` is the window style, used only to
/// tell button kinds apart.
pub fn capability(class: &str, style: u32) -> Capability {
    let lower = class.to_ascii_lowercase();
    match lower.as_str() {
        "static" => Capability {
            tech: Tech::CtlColor,
            level: Level::Supported,
            note: "Text and background through WM_CTLCOLORSTATIC. Disabled text uses the system gray.",
        },
        "edit" => Capability {
            tech: Tech::ThemeCfd,
            level: Level::Supported,
            note: "Dark field and scrollbars. Read-only and disabled edits arrive as static and are colored separately.",
        },
        "button" => match style & BS_TYPE_MASK {
            BS_PUSHBUTTON | BS_DEFPUSHBUTTON => Capability {
                tech: Tech::ThemeExplorer,
                level: Level::BestEffort,
                note: "Dark push-button visuals come from the DarkMode_Explorer visual style.",
            },
            BS_CHECKBOX | BS_AUTOCHECKBOX | BS_RADIOBUTTON | BS_AUTORADIOBUTTON | BS_3STATE
            | BS_AUTO3STATE | BS_GROUPBOX => Capability {
                tech: Tech::Classic,
                level: Level::Supported,
                note: "Painted classically (no visual style) so text color can be supplied. Glyphs look classic, not Fluent.",
            },
            BS_OWNERDRAW => Capability {
                tech: Tech::Leave,
                level: Level::Unsupported,
                note: "Owner-drawn by the app. Forcing colors onto it would fight the app's own painting.",
            },
            _ => Capability {
                tech: Tech::Leave,
                level: Level::Unsupported,
                note: "Split buttons and command links are not recolored.",
            },
        },
        "combobox" => Capability {
            tech: Tech::ThemeCfd,
            level: Level::Supported,
            note: "Dark field, arrow and drop-down list.",
        },
        "combolbox" | "listbox" => Capability {
            tech: Tech::ThemeCfd,
            level: Level::Supported,
            note: "Colors through WM_CTLCOLORLISTBOX; scrollbars dark.",
        },
        "syslistview32" => Capability {
            tech: Tech::ListView,
            level: Level::BestEffort,
            note: "Background, text and scrollbars. Original colors are saved and restored.",
        },
        "sysheader32" => Capability {
            tech: Tech::Header,
            level: Level::BestEffort,
            note: "Header uses the DarkMode_ItemsView style.",
        },
        "systreeview32" => Capability {
            tech: Tech::TreeView,
            level: Level::BestEffort,
            note: "Background, text and scrollbars. Original colors are saved and restored.",
        },
        "scrollbar" => Capability {
            tech: Tech::ThemeExplorer,
            level: Level::BestEffort,
            note: "Standalone scrollbar uses the dark Explorer style.",
        },
        "syslink" => Capability {
            tech: Tech::ThemeExplorer,
            level: Level::BestEffort,
            note: "Link color comes from the visual style; the harness shows whether it is readable on your build.",
        },
        "systabcontrol32" => Capability {
            tech: Tech::Leave,
            level: Level::Partial,
            note: "The tab strip stays light. Pages inside it are darkened.",
        },
        "richedit20w" | "richedit20a" | "richedit50w" | "riched20" | "richedit" => Capability {
            tech: Tech::Leave,
            level: Level::Partial,
            note: "Left as is: recoloring would rewrite the document's own formatting. Scrollbars may stay light.",
        },
        "sysdatetimepick32" | "sysmonthcal32" => Capability {
            tech: Tech::Leave,
            level: Level::Partial,
            note: "Calendar and date picker keep their light look.",
        },
        "msctls_progress32" | "msctls_trackbar32" | "msctls_updown32" | "msctls_statusbar32"
        | "sysanimate32" | "sysipaddress32" => Capability {
            tech: Tech::Leave,
            level: Level::Untouched,
            note: "Readable as Windows draws it.",
        },
        "tooltips_class32" => Capability {
            tech: Tech::Leave,
            level: Level::Unsupported,
            note: "Tooltips are not recolored.",
        },
        _ if is_shell_surface_class(class) => Capability {
            tech: Tech::Leave,
            level: Level::Unsupported,
            note: "Explorer or foreign-engine surface. Never touched.",
        },
        _ => Capability {
            tech: Tech::Leave,
            level: Level::Untouched,
            note: "Unknown control class. Left as is.",
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explorer_control_panel_is_never_a_candidate() {
        // The window title is irrelevant: only classes are consulted.
        assert_eq!(
            window_verdict(HostKind::Explorer, "CabinetWClass", &[]),
            Verdict::Skip("shell-surface")
        );
        assert_eq!(
            window_verdict(
                HostKind::Explorer,
                "CabinetWClass",
                &["DirectUIHWND", "ReBarWindow32"]
            ),
            Verdict::Skip("shell-surface")
        );
        assert_eq!(
            window_verdict(HostKind::Explorer, "ShellTabWindowClass", &[]),
            Verdict::Skip("shell-surface")
        );
    }

    #[test]
    fn mmc_main_frame_is_not_a_candidate_but_its_dialogs_are() {
        assert_eq!(
            window_verdict(HostKind::Mmc, "MMCMainFrame", &["ToolbarWindow32"]),
            Verdict::Skip("not-a-dialog")
        );
        assert_eq!(
            window_verdict(HostKind::Mmc, "#32770", &["Static", "Button"]),
            Verdict::Full
        );
    }

    #[test]
    fn dialogs_with_shell_surfaces_are_skipped_whole() {
        // File Open/Save dialogs embed an Explorer view.
        assert_eq!(
            window_verdict(
                HostKind::Other,
                "#32770",
                &["Static", "DirectUIHWND", "Edit", "ComboBox"]
            ),
            Verdict::Skip("contains-shell-surface")
        );
        assert_eq!(
            window_verdict(HostKind::Other, "#32770", &["Internet Explorer_Server"]),
            Verdict::Skip("contains-shell-surface")
        );
    }

    #[test]
    fn partial_dialogs_report_which_controls_stay_light() {
        match window_verdict(
            HostKind::Other,
            "#32770",
            &["Static", "SysTabControl32", "RichEdit20W"],
        ) {
            Verdict::Partial(list) => {
                assert!(list.iter().any(|item| item == "SysTabControl32"));
                assert!(list.iter().any(|item| item == "RichEdit20W"));
                assert!(!list.iter().any(|item| item == "Static"));
            }
            other => panic!("expected partial, got {other:?}"),
        }
    }

    #[test]
    fn denied_and_sensitive_processes_are_refused_everywhere() {
        for exe in [
            "dwm.exe",
            "ShellExperienceHost.exe",
            "EasyAntiCheat.exe",
            "GeForceNOW.exe",
            "consent.exe",
        ] {
            assert_eq!(host_kind(exe), HostKind::Denied, "{exe}");
            assert_eq!(
                window_verdict(host_kind(exe), "#32770", &[]),
                Verdict::Skip("denied-process")
            );
        }
        assert_eq!(host_kind("explorer.exe"), HostKind::Explorer);
        assert_eq!(host_kind("MMC.EXE"), HostKind::Mmc);
        assert_eq!(host_kind("notepad.exe"), HostKind::Other);
    }

    #[test]
    fn button_kinds_get_different_techniques() {
        assert_eq!(capability("Button", 0).tech, Tech::ThemeExplorer);
        assert_eq!(capability("Button", 1).tech, Tech::ThemeExplorer);
        for style in [2, 3, 4, 5, 6, 7, 9] {
            assert_eq!(
                capability("Button", style).tech,
                Tech::Classic,
                "style {style}"
            );
        }
        assert_eq!(capability("button", 0xB).level, Level::Unsupported);
        assert_eq!(capability("Button", 0xE).level, Level::Unsupported);
    }

    #[test]
    fn every_family_in_the_table_has_a_note_and_nothing_claims_universal_support() {
        for class in [
            "Static",
            "Edit",
            "Button",
            "ComboBox",
            "ListBox",
            "SysListView32",
            "SysTreeView32",
            "SysTabControl32",
            "RichEdit20W",
            "SysLink",
            "DirectUIHWND",
            "SomethingNew",
        ] {
            let capability = capability(class, 0);
            assert!(!capability.note.is_empty(), "{class}");
        }
        assert_eq!(capability("DirectUIHWND", 0).level, Level::Unsupported);
        assert_eq!(capability("SomethingNew", 0).level, Level::Untouched);
    }

    #[test]
    fn palettes_keep_text_readable() {
        for palette in PALETTES {
            assert!(
                contrast_ratio(palette.text, palette.bg) >= 7.0,
                "{} body",
                palette.name
            );
            assert!(
                contrast_ratio(palette.text, palette.edit_bg) >= 7.0,
                "{} edit",
                palette.name
            );
            assert!(
                contrast_ratio(palette.text, palette.readonly_bg) >= 7.0,
                "{} readonly",
                palette.name
            );
            assert!(
                contrast_ratio(palette.disabled_text, palette.bg) >= 3.0,
                "{} disabled",
                palette.name
            );
            assert!(
                contrast_ratio(palette.disabled_text, palette.edit_bg) >= 3.0,
                "{} disabled edit",
                palette.name
            );
            assert!(
                contrast_ratio(palette.link, palette.bg) >= 4.5,
                "{} link",
                palette.name
            );
            assert!(
                contrast_ratio(palette.heading, palette.bg) >= 7.0,
                "{} heading",
                palette.name
            );
        }
        assert_eq!(palette("BLACK").name, "black");
        assert_eq!(palette("nonsense").name, "graphite");
    }

    #[test]
    fn shared_state_layout_is_stable() {
        // Exe and hook must agree. A size change must come with a MAGIC bump.
        assert_eq!(std::mem::size_of::<SharedState>(), 40);
        let state = SharedState::from_palette(PALETTES[0], FLAG_CLASSIC_BUTTONS);
        assert_eq!(state.magic, MAGIC);
        assert_eq!(state.enabled, 1);
    }
}
