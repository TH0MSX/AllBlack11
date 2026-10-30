//! Who the experimental legacy-window hook is allowed to enter.
//! The decision is pure so it can be tested without Windows.

use crate::policy::process_always_skipped;

#[derive(Debug, Clone, Copy)]
pub struct InjectTarget<'a> {
    pub exe: &'a str,
    pub class_name: &'a str,
    pub excluded: bool,
    pub elevated: bool,
    /// `OpenProcess` failed. Treat that as protected or privileged and stay out.
    pub protected: bool,
    pub same_arch: bool,
    /// Explorer window whose title is the Control Panel folder.
    pub control_panel: bool,
}

pub fn inject_allowed(target: &InjectTarget<'_>) -> Result<(), &'static str> {
    if target.protected {
        return Err("protected");
    }
    if target.elevated {
        return Err("elevated");
    }
    if !target.same_arch {
        return Err("architecture");
    }
    if target.exe.is_empty() {
        return Err("unknown-process");
    }
    if process_always_skipped(target.exe) || target.excluded {
        return Err("excluded");
    }
    if is_dialog(target.class_name) || is_mmc(target.exe) || target.control_panel {
        Ok(())
    } else {
        Err("not-legacy")
    }
}

pub fn is_dialog(class_name: &str) -> bool {
    class_name == "#32770"
}

pub fn is_mmc(exe: &str) -> bool {
    exe.eq_ignore_ascii_case("mmc.exe")
}

pub fn is_control_panel_title(title: &str) -> bool {
    let title = title.trim();
    title.eq_ignore_ascii_case("Control Panel")
        || title.to_ascii_lowercase().starts_with("control panel\\")
        || title.to_ascii_lowercase().starts_with("control panel ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target<'a>(exe: &'a str, class_name: &'a str) -> InjectTarget<'a> {
        InjectTarget {
            exe,
            class_name,
            excluded: false,
            elevated: false,
            protected: false,
            same_arch: true,
            control_panel: false,
        }
    }

    #[test]
    fn allows_a_same_arch_dialog_and_mmc_and_control_panel() {
        assert!(inject_allowed(&target("notepad.exe", "#32770")).is_ok());
        assert!(inject_allowed(&target("mmc.exe", "MMCMainFrame")).is_ok());
        let mut panel = target("explorer.exe", "CabinetWClass");
        panel.control_panel = true;
        assert!(inject_allowed(&panel).is_ok());
    }

    #[test]
    fn refuses_elevated_protected_foreign_excluded_and_ordinary_windows() {
        let mut elevated = target("notepad.exe", "#32770");
        elevated.elevated = true;
        assert_eq!(inject_allowed(&elevated), Err("elevated"));

        let mut protected = target("csrss.exe", "#32770");
        protected.protected = true;
        assert_eq!(inject_allowed(&protected), Err("protected"));

        let mut foreign = target("notepad.exe", "#32770");
        foreign.same_arch = false;
        assert_eq!(inject_allowed(&foreign), Err("architecture"));

        let mut game = target("steam.exe", "#32770");
        game.excluded = true;
        assert_eq!(inject_allowed(&game), Err("excluded"));

        assert_eq!(
            inject_allowed(&target("dwm.exe", "#32770")),
            Err("excluded")
        );
        assert_eq!(
            inject_allowed(&target("notepad.exe", "Notepad")),
            Err("not-legacy")
        );
        assert_eq!(
            inject_allowed(&target("explorer.exe", "CabinetWClass")),
            Err("not-legacy")
        );
        assert!(is_control_panel_title("Control Panel"));
        assert!(is_control_panel_title("Control Panel\\All Control Panel Items"));
        assert!(!is_control_panel_title("Documents"));
    }
}
