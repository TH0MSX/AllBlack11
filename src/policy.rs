use crate::config::{exe_matches, resolve_style, Style};
use crate::Config;

pub const WS_POPUP: u32 = 0x8000_0000;
pub const WS_CHILD: u32 = 0x4000_0000;
pub const WS_DLGFRAME: u32 = 0x0040_0000;
pub const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
pub const WS_EX_TRANSPARENT: u32 = 0x0000_0020;
pub const WS_EX_LAYERED: u32 = 0x0008_0000;
pub const WS_EX_NOREDIRECTIONBITMAP: u32 = 0x0020_0000;

/// Processes that host the shell, the lock screen, or services. Not removable from config.
const ALWAYS_SKIP_PROCESSES: &[&str] = &[
    "allblack11.exe",
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

/// Window classes that belong to the shell, menus, or input UI.
const ALWAYS_SKIP_CLASSES: &[&str] = &[
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "Progman",
    "WorkerW",
    "WorkerA",
    "NotifyIconOverflowWindow",
    "TopLevelWindowForOverflowXamlIsland",
    "XamlExplorerHostIslandWindow",
    "WindowsDashboard",
    "MultitaskingViewFrame",
    "Shell_InputSwitchTopLevelWindow",
    "Shell_InputSwitchDismissOverlay",
    "Shell_LightDismissOverlay",
    "LockScreenBackstopFrame",
    "LockScreenControllerProxyWindow",
    "ForegroundStaging",
    "ApplicationManager_DesktopShellWindow",
    "DummyDWMListenerWindow",
    "EdgeUiInputTopWndClass",
    "EdgeUiInputWndClass",
    "NativeHWNDHost",
    "TaskListThumbnailWnd",
    "TaskListOverlayWnd",
    "TrayNotifyFlyout",
    "SystemTray_Main",
    "Windows.Internal.Shell.TabProxyWindow",
    "Windows.UI.Core.CoreWindow",
    "Microsoft.UI.Content.PopupWindowSiteBridge",
    "Microsoft.UI.Content.DesktopPopupSiteBridge",
    "ApplicationFrameInputSinkWindow",
    "ApplicationFrameTitleBarWindow",
    "ComboLBox",
    "DropDown",
    "#32768",
    "#32769",
    "SysShadow",
    "tooltips_class32",
    "IME",
    "MSCTFIME UI",
    "CiceroUIWndFrame",
    "Internet Explorer_Hidden",
    "OleMainThreadWndClass",
    "AllBlack11.Host",
    "PepaTheme.Host",
    "SoSplash",
    "MsoSplash",
    "SplashScreen",
    "Windows.UI.Input.InputSite.WindowClass",
    "Shell_Dialog",
    "DV2ControlHost",
    "ImmersiveLauncher",
    "Ghost",
];

const EXPLORER_ALLOWED_CLASSES: &[&str] = &["CabinetWClass", "ExploreWClass"];

#[derive(Debug, Clone)]
pub struct WindowFacts {
    pub exe: String,
    pub class_name: String,
    pub style: u32,
    pub exstyle: u32,
    pub width: i32,
    pub height: i32,
    pub top_level: bool,
    pub has_owner: bool,
    pub minimized: bool,
    pub covers_monitor: bool,
    pub pid_known: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Apply(Style),
    Skip(&'static str),
}

pub fn decide(facts: &WindowFacts, config: &Config, accent_bgr: u32) -> Decision {
    if !config.enabled {
        return Decision::Skip("disabled");
    }
    if !facts.top_level || facts.style & WS_CHILD != 0 {
        return Decision::Skip("child");
    }
    if facts.class_name.is_empty() {
        return Decision::Skip("no-class");
    }
    if class_listed(&facts.class_name, ALWAYS_SKIP_CLASSES)
        || class_listed(&facts.class_name, &config.exclude_classes)
    {
        return Decision::Skip("class");
    }
    if facts.exstyle & WS_EX_TOOLWINDOW != 0 {
        return Decision::Skip("toolwindow");
    }
    let dialog = facts.class_name == "#32770";
    if facts.style & WS_DLGFRAME == 0 && !dialog {
        return Decision::Skip("no-frame");
    }
    if facts.exstyle & WS_EX_LAYERED != 0 && facts.exstyle & WS_EX_TRANSPARENT != 0 {
        return Decision::Skip("click-through");
    }
    if !facts.minimized
        && facts.covers_monitor
        && (facts.exstyle & WS_EX_NOREDIRECTIONBITMAP != 0
            || is_borderless_fullscreen(facts.style, true))
    {
        return Decision::Skip("fullscreen");
    }
    if !facts.minimized && (facts.width < 80 || facts.height < 32) {
        return Decision::Skip("tiny");
    }
    if facts.has_owner && facts.style & WS_DLGFRAME == 0 && !dialog {
        return Decision::Skip("popup");
    }
    if !facts.pid_known || facts.exe.is_empty() {
        return Decision::Skip("unknown-process");
    }
    if exe_listed(&facts.exe, ALWAYS_SKIP_PROCESSES) || exe_listed(&facts.exe, &config.exclude) {
        return Decision::Skip("excluded");
    }
    if facts.exe.eq_ignore_ascii_case("explorer.exe")
        && !dialog
        && !class_listed(&facts.class_name, EXPLORER_ALLOWED_CLASSES)
    {
        return Decision::Skip("explorer-shell");
    }
    if facts.exstyle & WS_EX_NOREDIRECTIONBITMAP != 0 && facts.style & WS_DLGFRAME == 0 {
        return Decision::Skip("borderless-composition");
    }
    match resolve_style(config, &facts.exe, accent_bgr) {
        Some(style) => Decision::Apply(style),
        None => Decision::Skip("rule"),
    }
}

pub fn is_borderless_fullscreen(style: u32, covers_monitor: bool) -> bool {
    covers_monitor && style & WS_DLGFRAME == 0
}

pub fn rect_covers_monitor(window: (i32, i32, i32, i32), monitor: (i32, i32, i32, i32)) -> bool {
    const TOLERANCE: i32 = 4;
    window.0 <= monitor.0 + TOLERANCE
        && window.1 <= monitor.1 + TOLERANCE
        && window.2 >= monitor.2 - TOLERANCE
        && window.3 >= monitor.3 - TOLERANCE
}

fn class_listed(class_name: &str, list: &[impl AsRef<str>]) -> bool {
    list.iter()
        .any(|item| item.as_ref().eq_ignore_ascii_case(class_name))
}

fn exe_listed(exe: &str, list: &[impl AsRef<str>]) -> bool {
    list.iter().any(|item| exe_matches(item.as_ref(), exe))
}

/// Shell hosts, services, and our own process. The experimental hook uses the same list.
pub fn process_always_skipped(exe: &str) -> bool {
    exe_listed(exe, ALWAYS_SKIP_PROCESSES)
}

#[cfg(test)]
pub fn normal_notepad() -> WindowFacts {
    WindowFacts {
        exe: "notepad.exe".into(),
        class_name: "Notepad".into(),
        style: 0x00CF_0000, // WS_OVERLAPPEDWINDOW
        exstyle: 0,
        width: 900,
        height: 600,
        top_level: true,
        has_owner: false,
        minimized: false,
        covers_monitor: false,
        pid_known: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{bundled_default_json, parse_config, Backdrop, Color};
    use crate::FALLBACK_ACCENT_BGR;

    fn cfg() -> Config {
        parse_config(bundled_default_json()).unwrap()
    }

    fn skip(facts: &WindowFacts) -> &'static str {
        match decide(facts, &cfg(), FALLBACK_ACCENT_BGR) {
            Decision::Skip(reason) => reason,
            Decision::Apply(_) => panic!("expected skip"),
        }
    }

    #[test]
    fn themes_a_normal_framed_window() {
        match decide(&normal_notepad(), &cfg(), FALLBACK_ACCENT_BGR) {
            Decision::Apply(style) => {
                assert!(style.dark_mode);
                assert_eq!(style.backdrop, Backdrop::Mica);
                assert_eq!(style.border, Color::Accent);
            }
            Decision::Skip(reason) => panic!("skipped: {reason}"),
        }
    }

    #[test]
    fn skips_shell_taskbar_start_and_explorer_extras() {
        let mut taskbar = normal_notepad();
        taskbar.exe = "explorer.exe".into();
        taskbar.class_name = "Shell_TrayWnd".into();
        assert_eq!(skip(&taskbar), "class");

        let mut start = normal_notepad();
        start.exe = "StartMenuExperienceHost.exe".into();
        start.class_name = "XamlExplorerHostIslandWindow".into();
        assert_eq!(skip(&start), "class");

        let mut worker = normal_notepad();
        worker.exe = "explorer.exe".into();
        worker.class_name = "WorkerW".into();
        assert_eq!(skip(&worker), "class");

        let mut cabinet = normal_notepad();
        cabinet.exe = "explorer.exe".into();
        cabinet.class_name = "CabinetWClass".into();
        assert!(matches!(
            decide(&cabinet, &cfg(), FALLBACK_ACCENT_BGR),
            Decision::Apply(_)
        ));

        let mut copy_dialog = normal_notepad();
        copy_dialog.exe = "explorer.exe".into();
        copy_dialog.class_name = "#32770".into();
        assert!(matches!(
            decide(&copy_dialog, &cfg(), FALLBACK_ACCENT_BGR),
            Decision::Apply(_)
        ));

        let mut frameless_dialog = copy_dialog.clone();
        frameless_dialog.style = WS_POPUP | 0x00C0_0000;
        assert!(matches!(
            decide(&frameless_dialog, &cfg(), FALLBACK_ACCENT_BGR),
            Decision::Apply(_)
        ));
    }

    #[test]
    fn skips_games_launchers_and_fullscreen() {
        let mut gfn = normal_notepad();
        gfn.exe = "GeForceNOW.exe".into();
        assert_eq!(skip(&gfn), "excluded");

        let mut steam = normal_notepad();
        steam.exe = "steamwebhelper.exe".into();
        assert_eq!(skip(&steam), "excluded");

        let mut game = normal_notepad();
        game.style = WS_POPUP;
        game.covers_monitor = true;
        game.exstyle = WS_EX_NOREDIRECTIONBITMAP;
        assert_eq!(skip(&game), "no-frame");
        assert!(is_borderless_fullscreen(game.style, true));

        let mut maximized = normal_notepad();
        maximized.covers_monitor = true;
        assert!(matches!(
            decide(&maximized, &cfg(), FALLBACK_ACCENT_BGR),
            Decision::Apply(_)
        ));
        assert!(!is_borderless_fullscreen(maximized.style, true));

        let mut dxgi = normal_notepad();
        dxgi.covers_monitor = true;
        dxgi.exstyle = WS_EX_NOREDIRECTIONBITMAP;
        assert_eq!(skip(&dxgi), "fullscreen");
    }

    #[test]
    fn skips_tool_child_overlay_and_unknown() {
        let mut tool = normal_notepad();
        tool.exstyle = WS_EX_TOOLWINDOW;
        assert_eq!(skip(&tool), "toolwindow");

        let mut child = normal_notepad();
        child.top_level = false;
        assert_eq!(skip(&child), "child");

        let mut overlay = normal_notepad();
        overlay.exstyle = WS_EX_LAYERED | WS_EX_TRANSPARENT;
        assert_eq!(skip(&overlay), "click-through");

        let mut unknown = normal_notepad();
        unknown.exe.clear();
        unknown.pid_known = false;
        assert_eq!(skip(&unknown), "unknown-process");

        let mut dwm = normal_notepad();
        dwm.exe = "dwm.exe".into();
        assert_eq!(skip(&dwm), "excluded");

        let mut lock = normal_notepad();
        lock.exe = "LockApp.exe".into();
        assert_eq!(skip(&lock), "excluded");
    }

    #[test]
    fn respects_rule_and_global_disable() {
        let mut config = cfg();
        config.rules.push(crate::config::Rule {
            process: "notepad.exe".into(),
            style: crate::config::PartialStyle {
                enabled: Some(false),
                ..crate::config::PartialStyle::default()
            },
        });
        assert!(matches!(
            decide(&normal_notepad(), &config, 0),
            Decision::Skip("rule")
        ));
        config.enabled = false;
        assert!(matches!(
            decide(&normal_notepad(), &config, 0),
            Decision::Skip("disabled")
        ));
    }

    #[test]
    fn monitor_cover_uses_a_small_tolerance() {
        assert!(rect_covers_monitor((0, 0, 1920, 1080), (0, 0, 1920, 1080)));
        assert!(rect_covers_monitor(
            (-2, -2, 1922, 1082),
            (0, 0, 1920, 1080)
        ));
        assert!(!rect_covers_monitor((0, 0, 1920, 1040), (0, 0, 1920, 1080)));
    }

    #[test]
    fn extra_class_exclude_from_config() {
        let mut config = cfg();
        config.exclude_classes.push("Notepad".into());
        assert!(matches!(
            decide(&normal_notepad(), &config, 0),
            Decision::Skip("class")
        ));
    }
}
