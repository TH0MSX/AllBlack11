//! "Why didn't this work?" Pure text and tables that turn decisions into answers.
//!
//! Honesty rules this module enforces:
//!  * An attribute Windows *accepted* is not proof of a visible effect. Nothing here ever
//!    reports "confirmed" from an HRESULT. The strongest claim is [`Confirm::ReadbackMatches`],
//!    which says the value reads back, not that anyone saw it.
//!  * Framework detection is a heuristic from window classes. It is labeled as such.

use crate::policy::{decide, Decision, WindowFacts};
use crate::rules::{resolve, Resolved, WindowIdentity};
use crate::Config;

// ---------------------------------------------------------------------------
// What kind of app is this? (heuristic)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Framework {
    ClassicWin32,
    ClassicDialog,
    FileExplorer,
    Chromium,
    WinUi,
    UwpFrame,
    Wpf,
    Qt,
    Java,
    Terminal,
    Console,
    Office,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleBar {
    /// The window has a system caption, so DWM caption colors can apply.
    System,
    /// No system caption: the app draws its own, and DWM caption colors will not show.
    Custom,
    /// Has a caption, but this framework is known to paint over it or extend into it.
    MayBeCustom,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameworkHint {
    pub framework: Framework,
    pub name: &'static str,
    pub title_bar: TitleBar,
    pub client_note: &'static str,
}

const WS_CAPTION: u32 = 0x00C0_0000;

pub fn framework_hint(class: &str, exe: &str, style: u32) -> FrameworkHint {
    let has_caption = style & WS_CAPTION == WS_CAPTION;
    let lower = class.to_ascii_lowercase();
    let exe_lower = exe.to_ascii_lowercase();
    let (framework, name, client_note, may_be_custom) = if lower == "#32770" {
        (
            Framework::ClassicDialog,
            "Classic dialog",
            "Eligible for the legacy layer: its controls can be recolored from inside the process.",
            false,
        )
    } else if lower == "cabinetwclass" || lower == "explorewclass" {
        (
            Framework::FileExplorer,
            "File Explorer",
            "Explorer mixes light and dark surfaces of its own. Only the frame is changed; the legacy layer never touches it.",
            false,
        )
    } else if lower.starts_with("chrome_widgetwin") || lower.starts_with("cef") {
        (
            Framework::Chromium,
            "Chromium or Electron",
            "The web content is opaque and drawn by the app, so a backdrop material behind it will not show. Many of these apps draw their own title bar.",
            true,
        )
    } else if lower == "winuidesktopwin32windowclass" || lower == "microsoft.ui.content.desktopchildsitebridge" {
        (
            Framework::WinUi,
            "WinUI 3",
            "The app may set its own backdrop and extend into the title bar. The XAML content is opaque unless the app makes it transparent.",
            true,
        )
    } else if lower == "applicationframewindow" || lower == "windows.ui.core.corewindow" {
        (
            Framework::UwpFrame,
            "UWP or Store frame",
            "A frame hosted by ApplicationFrameHost. The content is XAML and opaque; caption colors may or may not apply.",
            true,
        )
    } else if lower.starts_with("hwndwrapper[") {
        (
            Framework::Wpf,
            "WPF",
            "The frame is a normal window, so DWM attributes apply. WPF content is opaque unless the app asks for transparency.",
            false,
        )
    } else if lower.starts_with("qt5") || lower.starts_with("qt6") || lower.contains("qwindow") {
        (
            Framework::Qt,
            "Qt",
            "The frame is a normal window. Qt paints its own client area.",
            false,
        )
    } else if lower.starts_with("sunawt") {
        (
            Framework::Java,
            "Java AWT or Swing",
            "The frame is a normal window. Swing paints its own client area.",
            false,
        )
    } else if lower == "cascadia_hosting_window_class" || exe_lower == "windowsterminal.exe" {
        (
            Framework::Terminal,
            "Windows Terminal",
            "Terminal draws its own tab row and has its own acrylic setting. Prefer its settings for the client area.",
            true,
        )
    } else if lower == "consolewindowclass" {
        (
            Framework::Console,
            "Console window",
            "The console host draws the client area.",
            false,
        )
    } else if lower == "opusapp" || lower == "xlmain" || lower == "pptframeclass" || lower == "olkexplorer" {
        (
            Framework::Office,
            "Microsoft Office",
            "Recent Office versions draw their own title bar and ignore caption colors.",
            true,
        )
    } else if lower.is_empty() {
        (Framework::Unknown, "Unknown", "No class information.", false)
    } else {
        (
            Framework::ClassicWin32,
            "Win32 or unknown toolkit",
            "Probably a normal window. If the client area is opaque, a backdrop material will not show through it.",
            false,
        )
    };
    let title_bar = if !has_caption {
        TitleBar::Custom
    } else if may_be_custom {
        TitleBar::MayBeCustom
    } else {
        TitleBar::System
    };
    FrameworkHint {
        framework,
        name,
        title_bar,
        client_note,
    }
}

impl TitleBar {
    pub fn text(self) -> &'static str {
        match self {
            TitleBar::System => "System title bar: caption, text and border colors can apply.",
            TitleBar::Custom => {
                "No system caption: this app draws its own title bar, so caption colors will not show."
            }
            TitleBar::MayBeCustom => {
                "Has a caption, but apps of this kind often paint over it. Caption colors may not show."
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Skip reasons in words
// ---------------------------------------------------------------------------

/// Text for every reason `decide()` and `inject_allowed()` can return.
pub fn reason_text(reason: &str) -> &'static str {
    match reason {
        "disabled" => "Theming is switched off in AllBlack11.",
        "child" => "This is a child window, not a top-level window.",
        "no-class" => "The window has no class name.",
        "class" => "Its window class is on the skip list (shell, menus, input UI, or your excludeClasses).",
        "toolwindow" => "It is a tool window (palette, floating toolbar). Those are left alone.",
        "no-frame" => "It has no standard frame for Windows to color.",
        "click-through" => "It is a click-through overlay.",
        "fullscreen" => "It covers the whole monitor, which is treated as a game or video.",
        "tiny" => "It is too small to be a normal window.",
        "popup" => "It is an owned popup without a frame.",
        "unknown-process" => "AllBlack11 could not identify the process (it may be protected).",
        "excluded" => "The executable is on the skip list (built-in shell and service list, or your exclusions).",
        "explorer-shell" => "It is part of the Explorer shell rather than a folder window.",
        "borderless-composition" => "It is a borderless composition window with no frame to color.",
        "rule" => "A rule (or the default look) switches theming off for this window.",
        "denied-process" => "This process is on the never-touch list (shell, security, anti-cheat, streaming).",
        "shell-surface" => "This window class is Explorer or shell UI and is never modified by the legacy layer.",
        "contains-shell-surface" => {
            "This dialog embeds an Explorer or web view. Darkening part of it would be unreadable, so all of it is left alone."
        }
        "not-a-dialog" => "The legacy layer only handles classic dialogs, and this is not one.",
        "not-legacy" => "The legacy layer only handles classic dialogs, and this is not one.",
        "protected" => "Windows would not let AllBlack11 inspect this process, so it is treated as protected.",
        "elevated" => "The process runs as administrator. AllBlack11 does not enter elevated processes.",
        "arm64ec" => "ARM64EC process. A plain ARM64 or x64 hook cannot load into it, so it is left alone.",
        "x86-process" => "32-bit x86 process. There is no x86 hook in this build.",
        "needs-arm64-build" => "This is a native ARM64 process and this is the x64 build. Use the ARM64 build.",
        "x64-helper-off" => {
            "Emulated x64 process. Turn on emulated-x64 coverage in the Legacy page to use the x64 helper."
        }
        "architecture" => "The process architecture does not match any available hook.",
        _ => "No further detail is available for this reason.",
    }
}

// ---------------------------------------------------------------------------
// What does this Windows build support?
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Support {
    Supported,
    BestEffort,
    Experimental,
    Unsupported,
    Untested,
}

impl Support {
    pub fn label(self) -> &'static str {
        match self {
            Support::Supported => "supported",
            Support::BestEffort => "best effort",
            Support::Experimental => "experimental",
            Support::Unsupported => "not on this Windows build",
            Support::Untested => "untested",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeSupport {
    pub name: &'static str,
    /// DWMWA_* value, or 0 for non-DWM mechanisms.
    pub attribute: u32,
    pub support: Support,
    pub note: &'static str,
}

/// What the effects pipeline can use on this Windows build. Build numbers come from
/// Microsoft's documentation of each `DWMWA_*` value.
pub fn attribute_support(build: u32) -> Vec<AttributeSupport> {
    let at = |min: u32| if build >= min { Support::Supported } else { Support::Unsupported };
    vec![
        AttributeSupport {
            name: "Dark title bar",
            attribute: 20,
            support: at(19041),
            note: "DWMWA_USE_IMMERSIVE_DARK_MODE",
        },
        AttributeSupport {
            name: "Window corners",
            attribute: 33,
            support: at(22000),
            note: "DWMWA_WINDOW_CORNER_PREFERENCE",
        },
        AttributeSupport {
            name: "Border color",
            attribute: 34,
            support: at(22000),
            note: "DWMWA_BORDER_COLOR",
        },
        AttributeSupport {
            name: "Title bar color",
            attribute: 35,
            support: at(22000),
            note: "DWMWA_CAPTION_COLOR",
        },
        AttributeSupport {
            name: "Title text color",
            attribute: 36,
            support: at(22000),
            note: "DWMWA_TEXT_COLOR",
        },
        AttributeSupport {
            name: "Mica, Mica Alt, Acrylic",
            attribute: 38,
            support: at(22621),
            note: "DWMWA_SYSTEMBACKDROP_TYPE",
        },
        AttributeSupport {
            name: "Tinted acrylic",
            attribute: 0,
            support: if build >= 22000 { Support::Experimental } else { Support::Unsupported },
            note: "SetWindowCompositionAttribute, undocumented. Falls back to Acrylic if refused.",
        },
    ]
}

// ---------------------------------------------------------------------------
// Did it work?
// ---------------------------------------------------------------------------

/// The strongest statement AllBlack11 can make about an attribute it set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirm {
    /// Nothing was attempted (the window was skipped).
    NotApplied,
    /// The API call failed. The HRESULT says why.
    Rejected(i32),
    /// The API returned success. That says nothing about what is on screen.
    AcceptedUnverified,
    /// The value reads back as written. Still not visual confirmation.
    ReadbackMatches,
}

impl Confirm {
    pub fn text(self) -> String {
        match self {
            Confirm::NotApplied => "not attempted".into(),
            Confirm::Rejected(hr) => format!("rejected (0x{:08X})", hr as u32),
            Confirm::AcceptedUnverified => "accepted by Windows; appearance not verified".into(),
            Confirm::ReadbackMatches => "accepted and reads back; appearance not verified".into(),
        }
    }
}

// ---------------------------------------------------------------------------
// The full answer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Explanation {
    pub applied: bool,
    pub headline: String,
    pub reason: Option<&'static str>,
    pub resolved: Option<Resolved>,
    pub framework: FrameworkHint,
    /// Extra things worth knowing, each one sentence.
    pub notes: Vec<String>,
}

pub fn explain(facts: &WindowFacts, config: &Config, accent_bgr: u32, build: u32) -> Explanation {
    let hint = framework_hint(&facts.class_name, &facts.exe, facts.style);
    let identity = WindowIdentity {
        exe: &facts.exe,
        class: &facts.class_name,
        path: &facts.path,
        package: &facts.package,
        title: facts.title.as_deref(),
    };
    let resolved = resolve(config, &identity, accent_bgr);
    let decision = decide(facts, config, accent_bgr);
    let mut notes = Vec::new();
    let (applied, headline, reason) = match decision {
        Decision::Apply(style) => {
            if hint.title_bar != TitleBar::System {
                notes.push(hint.title_bar.text().to_string());
            }
            if style.backdrop.dwm_value() > 1 || style.backdrop == crate::Backdrop::TintedAcrylic {
                notes.push(
                    "A backdrop material only shows where the app leaves its client area transparent. Opaque content hides it."
                        .to_string(),
                );
            }
            (true, "AllBlack11 styles this window.".to_string(), None)
        }
        Decision::Skip(reason) => (
            false,
            format!("AllBlack11 leaves this window alone. {}", reason_text(reason)),
            Some(reason),
        ),
    };
    if build != 0 && build < 22000 {
        notes.push("This Windows build is older than Windows 11, so most effects are unavailable.".to_string());
    }
    Explanation {
        applied,
        headline,
        reason,
        resolved: Some(resolved),
        framework: hint,
        notes,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::normal_notepad;

    #[test]
    fn framework_hints_cover_the_families_and_say_why() {
        let chromium = framework_hint("Chrome_WidgetWin_1", "msedge.exe", WS_CAPTION);
        assert_eq!(chromium.framework, Framework::Chromium);
        assert_eq!(chromium.title_bar, TitleBar::MayBeCustom);

        // No caption bits at all: definitely custom.
        let frameless = framework_hint("Chrome_WidgetWin_1", "app.exe", 0);
        assert_eq!(frameless.title_bar, TitleBar::Custom);

        assert_eq!(framework_hint("#32770", "a.exe", WS_CAPTION).framework, Framework::ClassicDialog);
        assert_eq!(framework_hint("HwndWrapper[App.exe;;1234]", "a.exe", WS_CAPTION).framework, Framework::Wpf);
        assert_eq!(framework_hint("CabinetWClass", "explorer.exe", WS_CAPTION).framework, Framework::FileExplorer);
        assert_eq!(framework_hint("CASCADIA_HOSTING_WINDOW_CLASS", "WindowsTerminal.exe", WS_CAPTION).framework, Framework::Terminal);
        assert_eq!(framework_hint("Notepad", "notepad.exe", WS_CAPTION).title_bar, TitleBar::System);
        assert_eq!(framework_hint("", "a.exe", WS_CAPTION).framework, Framework::Unknown);
    }

    #[test]
    fn every_reason_the_policy_can_return_has_a_real_sentence() {
        for reason in [
            "disabled", "child", "no-class", "class", "toolwindow", "no-frame", "click-through",
            "fullscreen", "tiny", "popup", "unknown-process", "excluded", "explorer-shell",
            "borderless-composition", "rule", "denied-process", "shell-surface",
            "contains-shell-surface", "not-a-dialog", "not-legacy", "protected", "elevated",
            "arm64ec", "x86-process", "needs-arm64-build", "x64-helper-off", "architecture",
        ] {
            let text = reason_text(reason);
            assert!(text.len() > 20, "{reason}");
            assert_ne!(text, reason_text("definitely-not-a-reason"), "{reason} has no text");
        }
    }

    #[test]
    fn attribute_support_follows_documented_build_numbers() {
        let old = attribute_support(19045);
        assert_eq!(old[0].support, Support::Supported); // dark title bar
        assert_eq!(old[1].support, Support::Unsupported); // corners need 22000
        let w11 = attribute_support(22000);
        assert_eq!(w11[2].support, Support::Supported);
        assert_eq!(w11[5].support, Support::Unsupported); // backdrop needs 22621
        let new = attribute_support(26100);
        assert!(new.iter().all(|a| a.support != Support::Unsupported));
        assert_eq!(new.last().unwrap().support, Support::Experimental);
    }

    #[test]
    fn confirm_never_claims_visual_confirmation() {
        for confirm in [Confirm::NotApplied, Confirm::Rejected(-2147024809), Confirm::AcceptedUnverified, Confirm::ReadbackMatches] {
            let text = confirm.text();
            assert!(!text.to_lowercase().contains("confirmed"), "{text}");
        }
        assert!(Confirm::Rejected(-2147024809).text().contains("0x80070057"));
    }

    #[test]
    fn explain_gives_an_answer_for_applied_and_skipped_windows() {
        let config = Config::default();
        let applied = explain(&normal_notepad(), &config, 0, 26100);
        assert!(applied.applied);
        assert!(applied.reason.is_none());

        let mut excluded = normal_notepad();
        excluded.exe = "steam.exe".into();
        let config = crate::parse_config(r#"{"exclude":["steam.exe"]}"#).unwrap();
        let skipped = explain(&excluded, &config, 0, 26100);
        assert!(!skipped.applied);
        assert_eq!(skipped.reason, Some("excluded"));
        assert!(skipped.headline.contains("skip list"));
    }

    #[test]
    fn a_custom_title_bar_is_called_out_when_a_window_is_styled() {
        let mut facts = normal_notepad();
        facts.class_name = "Chrome_WidgetWin_1".into();
        facts.exe = "app.exe".into();
        let config = Config::default();
        let answer = explain(&facts, &config, 0, 26100);
        assert!(answer.applied);
        assert!(answer.notes.iter().any(|note| note.contains("paint over it") || note.contains("own title bar")));
    }
}
