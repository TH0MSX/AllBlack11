//! Who the experimental legacy-window layer is allowed to enter.
//! Pure, so it can be tested without Windows. Window and process lists live in
//! `legacy_policy`; architecture routing lives in `target`.

use crate::legacy_policy::{host_kind, is_dialog_class, HostKind};
use crate::target::{route, Route, TargetArch};

#[derive(Debug, Clone, Copy)]
pub struct InjectTarget<'a> {
    pub exe: &'a str,
    pub class_name: &'a str,
    pub excluded: bool,
    pub elevated: bool,
    /// `OpenProcess` failed. Treat that as protected or privileged and stay out.
    pub protected: bool,
    /// Machine type of the controller that would do the work (this exe).
    pub controller_machine: u16,
    pub arch: TargetArch,
    /// The packaged x64 helper is installed and the user enabled emulated-x64 coverage.
    pub helper_available: bool,
}

/// `Ok(route)` says which binary may treat the window. `Err` is a stable reason.
pub fn inject_allowed(target: &InjectTarget<'_>) -> Result<Route, &'static str> {
    if target.protected {
        return Err("protected");
    }
    if target.elevated {
        return Err("elevated");
    }
    if target.exe.is_empty() {
        return Err("unknown-process");
    }
    if target.excluded {
        return Err("excluded");
    }
    if host_kind(target.exe) == HostKind::Denied {
        return Err("excluded");
    }
    // Only classic dialogs are ever candidates, in every host. This is deliberate: it keeps
    // Explorer-hosted Control Panel, Explorer folder windows and MMC main frames out.
    if !is_dialog_class(target.class_name) {
        return Err("not-legacy");
    }
    match route(target.controller_machine, target.arch) {
        Route::Unsupported(reason) => Err(reason),
        Route::X64Helper if !target.helper_available => Err("x64-helper-off"),
        other => Ok(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arch::{MACHINE_AMD64, MACHINE_ARM64};

    fn target<'a>(exe: &'a str, class_name: &'a str) -> InjectTarget<'a> {
        InjectTarget {
            exe,
            class_name,
            excluded: false,
            elevated: false,
            protected: false,
            controller_machine: MACHINE_ARM64,
            arch: TargetArch::Arm64Native,
            helper_available: false,
        }
    }

    #[test]
    fn allows_a_native_dialog() {
        assert_eq!(inject_allowed(&target("notepad.exe", "#32770")), Ok(Route::Native));
        // MMC dialogs are dialogs; the MMC main frame is not.
        assert_eq!(inject_allowed(&target("mmc.exe", "#32770")), Ok(Route::Native));
        assert_eq!(inject_allowed(&target("mmc.exe", "MMCMainFrame")), Err("not-legacy"));
    }

    #[test]
    fn explorer_control_panel_and_folder_windows_are_refused_whatever_the_title() {
        // There is no title parameter: titles are localized and editable, so they are never
        // consulted. Class alone keeps Explorer's windows out.
        for class in ["CabinetWClass", "ExploreWClass", "ShellTabWindowClass", "WorkerW", "Progman"] {
            assert_eq!(inject_allowed(&target("explorer.exe", class)), Err("not-legacy"), "{class}");
        }
    }

    #[test]
    fn refuses_elevated_protected_excluded_denied_and_ordinary_windows() {
        let mut elevated = target("notepad.exe", "#32770");
        elevated.elevated = true;
        assert_eq!(inject_allowed(&elevated), Err("elevated"));

        let mut protected = target("csrss.exe", "#32770");
        protected.protected = true;
        assert_eq!(inject_allowed(&protected), Err("protected"));

        let mut game = target("steam.exe", "#32770");
        game.excluded = true;
        assert_eq!(inject_allowed(&game), Err("excluded"));

        assert_eq!(inject_allowed(&target("dwm.exe", "#32770")), Err("excluded"));
        assert_eq!(inject_allowed(&target("EasyAntiCheat.exe", "#32770")), Err("excluded"));
        assert_eq!(inject_allowed(&target("notepad.exe", "Notepad")), Err("not-legacy"));
        assert_eq!(inject_allowed(&target("", "#32770")), Err("unknown-process"));
    }

    #[test]
    fn arm64_controller_routes_by_target_architecture() {
        let mut emulated = target("legacyapp.exe", "#32770");
        emulated.arch = TargetArch::X64Emulated;
        assert_eq!(inject_allowed(&emulated), Err("x64-helper-off"));
        emulated.helper_available = true;
        assert_eq!(inject_allowed(&emulated), Ok(Route::X64Helper));

        let mut ec = target("ecapp.exe", "#32770");
        ec.arch = TargetArch::Arm64Ec;
        ec.helper_available = true;
        assert_eq!(inject_allowed(&ec), Err("arm64ec"));

        let mut x86 = target("old.exe", "#32770");
        x86.arch = TargetArch::X86Emulated;
        assert_eq!(inject_allowed(&x86), Err("x86-process"));
    }

    #[test]
    fn x64_controller_cannot_reach_arm64_targets() {
        let mut t = target("native.exe", "#32770");
        t.controller_machine = MACHINE_AMD64;
        t.arch = TargetArch::Arm64Native;
        assert_eq!(inject_allowed(&t), Err("needs-arm64-build"));
        t.arch = TargetArch::X64Emulated;
        assert_eq!(inject_allowed(&t), Ok(Route::Native));
    }
}
