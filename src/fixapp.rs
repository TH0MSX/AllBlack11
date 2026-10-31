//! "Fix this app": the inspector's facts turned into a guided plan.
//!
//! Given what the inspector already knows about one window (`Report`) plus a few switches,
//! this answers, per surface, "should this work, might it not show, or will it not work, and
//! why", lists the safe actions, and lists the limits in plain words.
//!
//! Honesty rules (each has a test):
//!  * Choosing a process never implies every control inside it can be themed. The client
//!    surface and the legacy controls are reported on their own lines.
//!  * Framework names are a guess from the window class and are labeled that way.
//!  * "Seen on screen" is always "not verified": a successful API call is not a visible effect.
//!  * Nothing here says "confirmed", "guaranteed" or "all controls".

use serde::Serialize;

use crate::explain::{reason_text, Confirm, Framework, TitleBar};
use crate::report::Report;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Caption,
    Border,
    Backdrop,
    Client,
    LegacyControls,
}

impl Surface {
    pub fn label(self) -> &'static str {
        match self {
            Surface::Caption => "Title bar",
            Surface::Border => "Border",
            Surface::Backdrop => "Backdrop material",
            Surface::Client => "App content area",
            Surface::LegacyControls => "Classic dialog controls",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Expected to work on a normal window of this kind.
    Should,
    /// Windows may accept it, but it may not be visible.
    Maybe,
    /// AllBlack11 will not (or cannot) change this surface here.
    No,
    /// The question does not apply to this window.
    NotRelevant,
}

impl Level {
    pub fn label(self) -> &'static str {
        match self {
            Level::Should => "should work",
            Level::Maybe => "may not show",
            Level::No => "not changed",
            Level::NotRelevant => "not relevant",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SurfaceRow {
    pub surface: Surface,
    pub level: Level,
    pub why: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Apply the look being edited to this window only, for 20 seconds.
    Preview,
    /// Keep it: a per-app rule.
    SaveRule,
    /// Never theme this app.
    Exclude,
    /// Put this window back the way it was before AllBlack11.
    Restore,
}

/// Facts the report does not carry.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// Why effects are reduced right now, if they are.
    pub reduced: Option<&'static str>,
    pub legacy_enabled: bool,
    pub helper_enabled: bool,
    pub helper_installed: bool,
    /// AllBlack11 has touched this window already (a restore is meaningful).
    pub touched: bool,
}

#[derive(Debug, Clone)]
pub struct Plan {
    /// Process, architecture and route: what this is, before any claim about styling.
    pub identity: Vec<String>,
    pub surfaces: Vec<SurfaceRow>,
    pub actions: Vec<Action>,
    pub limits: Vec<String>,
}

fn route_line(arch: &str, ctx: &Context) -> String {
    match arch {
        "arm64" => "Native ARM64 process. Frame styling needs no helper; the legacy layer, if on, hooks it directly."
            .to_string(),
        "x64-emulated" => {
            if ctx.helper_enabled && ctx.helper_installed {
                "x64 process emulated on ARM64. Frame styling needs no helper. Classic dialogs are handled by the optional x64 helper (experimental)."
                    .to_string()
            } else {
                "x64 process emulated on ARM64. Frame styling needs no helper. Its classic dialogs are skipped unless the optional x64 helper is switched on."
                    .to_string()
            }
        }
        "x64" => "Native x64 process. Frame styling needs no helper.".to_string(),
        "arm64ec" => "ARM64EC process. Frame styling works; the legacy layer never touches these.".to_string(),
        "x86" => "32-bit process. Frame styling works; the legacy layer never touches these.".to_string(),
        _ => "Architecture could not be read. Frame styling still works; the legacy layer skips it.".to_string(),
    }
}

/// Turn a report into a plan. Pure: no Windows calls.
pub fn plan(report: &Report, ctx: &Context) -> Plan {
    let explanation = &report.explanation;
    let hint = &explanation.framework;
    let applied = explanation.applied;
    let skip_reason = explanation
        .reason
        .map(reason_text)
        .unwrap_or("AllBlack11 leaves this window alone.");
    let new_enough = |min: u32| report.build == 0 || report.build >= min;
    let mut surfaces = Vec::new();

    let blocked = |surface: Surface| SurfaceRow {
        surface,
        level: Level::No,
        why: skip_reason.to_string(),
    };

    // Title bar.
    surfaces.push(if !applied {
        blocked(Surface::Caption)
    } else if !new_enough(22000) {
        SurfaceRow {
            surface: Surface::Caption,
            level: Level::No,
            why: "Title bar colors need Windows 11 (build 22000 or newer).".into(),
        }
    } else {
        match hint.title_bar {
            TitleBar::System => SurfaceRow {
                surface: Surface::Caption,
                level: Level::Should,
                why: "A system title bar: caption and text colors can apply.".into(),
            },
            TitleBar::MayBeCustom => SurfaceRow {
                surface: Surface::Caption,
                level: Level::Maybe,
                why: format!(
                    "Looks like {} (a guess from the window class). Apps like this often draw over the title bar.",
                    hint.name
                ),
            },
            TitleBar::Custom => SurfaceRow {
                surface: Surface::Caption,
                level: Level::No,
                why: "No system title bar: the app draws its own, so caption colors will not show."
                    .into(),
            },
        }
    });

    // Border.
    surfaces.push(if !applied {
        blocked(Surface::Border)
    } else if !new_enough(22000) {
        SurfaceRow {
            surface: Surface::Border,
            level: Level::No,
            why: "Border colors need Windows 11 (build 22000 or newer).".into(),
        }
    } else if hint.title_bar == TitleBar::Custom {
        SurfaceRow {
            surface: Surface::Border,
            level: Level::Maybe,
            why: "Windows may accept a border color, but a window without a system frame may not show one."
                .into(),
        }
    } else {
        SurfaceRow {
            surface: Surface::Border,
            level: Level::Should,
            why: "The frame is drawn by Windows, so a border color can apply.".into(),
        }
    });

    // Backdrop material.
    surfaces.push(if !applied {
        blocked(Surface::Backdrop)
    } else if let Some(reason) = ctx.reduced {
        SurfaceRow {
            surface: Surface::Backdrop,
            level: Level::No,
            why: format!("Effects are reduced right now because of {reason}, so no material is applied."),
        }
    } else if !new_enough(22621) {
        SurfaceRow {
            surface: Surface::Backdrop,
            level: Level::No,
            why: "Mica, Mica Alt and Acrylic backdrops need Windows 11 22H2 (build 22621).".into(),
        }
    } else {
        SurfaceRow {
            surface: Surface::Backdrop,
            level: Level::Maybe,
            why: "Windows can accept a material, but it only shows where the app leaves its window transparent. An opaque content area hides it."
                .into(),
        }
    });

    // The inside of the window. AllBlack11 does not repaint it from outside.
    surfaces.push(SurfaceRow {
        surface: Surface::Client,
        level: match hint.framework {
            Framework::ClassicDialog => Level::Maybe,
            _ => Level::No,
        },
        why: match hint.framework {
            Framework::Chromium => "Web content is drawn by the app. Use the app's own dark theme.".into(),
            Framework::Terminal | Framework::Console => {
                "The terminal draws its own content. Use its own color settings.".into()
            }
            Framework::Office => "Office draws its own content. Use Office's own theme setting.".into(),
            Framework::ClassicDialog => {
                "Plain classic dialogs can be recolored by the optional legacy layer. See the next line.".into()
            }
            _ => "The app draws its own content. AllBlack11 does not repaint it from outside, so it keeps whatever the app shows.".into(),
        },
    });

    // Classic controls through the legacy layer.
    surfaces.push(match &report.legacy {
        None => SurfaceRow {
            surface: Surface::LegacyControls,
            level: Level::NotRelevant,
            why: if hint.framework == Framework::ClassicDialog {
                "The legacy layer could not assess this dialog.".into()
            } else {
                "Not a classic dialog, so the legacy layer does not apply.".into()
            },
        },
        Some(_) if !ctx.legacy_enabled => SurfaceRow {
            surface: Surface::LegacyControls,
            level: Level::NotRelevant,
            why: "Dark legacy windows is off (it is experimental and off by default). Nothing is loaded into any app.".into(),
        },
        Some(view) => {
            let gate_open = view.gate.starts_with("allowed");
            let full = view.verdict.contains("fully");
            let partial = view.verdict.contains("partly");
            if gate_open && full {
                SurfaceRow {
                    surface: Surface::LegacyControls,
                    level: Level::Should,
                    why: format!("Experimental. {}.", view.gate),
                }
            } else if gate_open && partial {
                SurfaceRow {
                    surface: Surface::LegacyControls,
                    level: Level::Maybe,
                    why: format!(
                        "Experimental, partial. Some controls stay light{}.",
                        if view.partial.is_empty() {
                            String::new()
                        } else {
                            format!(": {}", view.partial.join(", "))
                        }
                    ),
                }
            } else {
                SurfaceRow {
                    surface: Surface::LegacyControls,
                    level: Level::No,
                    why: if gate_open {
                        view.verdict.clone()
                    } else {
                        view.gate.clone()
                    },
                }
            }
        }
    });

    let mut identity = vec![
        format!("{} — window class {}", report.exe, report.class),
        route_line(&report.arch, ctx),
        format!(
            "Looks like: {} (a guess from the window class, not a certainty).",
            hint.name
        ),
    ];
    if let Some(reason) = explanation.reason {
        identity.push(format!("Left alone: {}", reason_text(reason)));
    }

    let mut actions = Vec::new();
    if applied {
        actions.push(Action::Preview);
        actions.push(Action::SaveRule);
    }
    if explanation.reason != Some("protected") {
        actions.push(Action::Exclude);
    }
    if ctx.touched {
        actions.push(Action::Restore);
    }

    let mut limits = vec![
        "Picking this process does not mean every control inside it can be themed. Each row above is about one surface.".to_string(),
        hint.client_note.to_string(),
    ];
    limits.extend(explanation.notes.iter().cloned());
    limits.dedup();

    Plan {
        identity,
        surfaces,
        actions,
        limits,
    }
}

/// One setting followed from what was asked to what is on screen. The three stay separate.
#[derive(Debug, Clone, Serialize)]
pub struct Triple {
    pub setting: &'static str,
    pub requested: String,
    pub windows_said: String,
    pub seen_on_screen: String,
}

const NOT_VERIFIED: &str = "not verified. AllBlack11 cannot see your screen; look at the window, or compare in the Materials demo.";

fn attribute_setting(attribute: u32) -> Option<&'static str> {
    Some(match attribute {
        20 => "Dark title bar",
        33 => "Corners",
        34 => "Border color",
        35 => "Title bar color",
        36 => "Title text color",
        38 => "Backdrop material",
        _ => return None,
    })
}

/// Requested value, what the API call returned, and what is seen. The last is always
/// "not verified": there is no screen read-back here, and a success code does not prove one.
pub fn triples(report: &Report) -> Vec<Triple> {
    let Some(resolved) = &report.explanation.resolved else {
        return Vec::new();
    };
    if !report.explanation.applied {
        return Vec::new();
    }
    let style = &resolved.style;
    let show = |value: &dyn erased::Show| value.show();
    let requested: [(&'static str, String); 6] = [
        ("Dark title bar", style.dark_mode.to_string()),
        ("Corners", show(&style.corners)),
        ("Border color", show(&style.border)),
        ("Title bar color", show(&style.caption)),
        ("Title text color", show(&style.text)),
        ("Backdrop material", show(&style.backdrop)),
    ];
    requested
        .into_iter()
        .map(|(setting, requested)| {
            let attempt = report
                .attempts
                .iter()
                .find(|attempt| attribute_setting(attempt.attribute) == Some(setting));
            let windows_said = match attempt {
                None => {
                    "no attempt recorded yet (the window has not been styled, or was left alone)"
                        .to_string()
                }
                Some(attempt) => match attempt.confirm {
                    Confirm::Rejected(_)
                    | Confirm::AcceptedUnverified
                    | Confirm::ReadbackMatches
                    | Confirm::NotApplied => attempt.confirm.text(),
                },
            };
            Triple {
                setting,
                requested,
                windows_said,
                seen_on_screen: NOT_VERIFIED.to_string(),
            }
        })
        .collect()
}

mod erased {
    use serde::Serialize;

    pub trait Show {
        fn show(&self) -> String;
    }

    impl<T: Serialize> Show for T {
        fn show(&self) -> String {
            match serde_json::to_value(self) {
                Ok(serde_json::Value::String(text)) => text,
                Ok(other) => other.to_string(),
                Err(_) => "?".into(),
            }
        }
    }
}

/// Lines for the "Style receipt": request, Windows' answer, and the unverified last column.
pub fn receipt_lines(report: &Report) -> Vec<String> {
    let rows = triples(report);
    if rows.is_empty() {
        return vec!["No style was requested for this window.".to_string()];
    }
    let mut lines = vec!["Requested  →  Windows said  →  Seen on screen".to_string()];
    for row in rows {
        lines.push(format!("{}", row.setting));
        lines.push(format!("  requested: {}", row.requested));
        lines.push(format!("  Windows said: {}", row.windows_said));
        lines.push(format!("  seen on screen: {}", row.seen_on_screen));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compat::Status;
    use crate::explain::{explain, Explanation};
    use crate::policy::normal_notepad;
    use crate::report::{Attempt, LegacyView};
    use crate::Config;

    fn report_for(class: &str, style: u32, arch: &str, build: u32) -> Report {
        let mut facts = normal_notepad();
        facts.class_name = class.to_string();
        facts.style = style;
        let explanation: Explanation = explain(&facts, &Config::default(), 0, build);
        Report {
            exe: facts.exe.clone(),
            class: class.to_string(),
            pid: 1,
            path: String::new(),
            package: String::new(),
            title: String::new(),
            arch: arch.to_string(),
            build,
            explanation,
            attempts: Vec::new(),
            legacy: None,
            compat: Status::Untested,
            compat_note: String::new(),
            rule_lines: vec![],
            support: vec![],
        }
    }

    fn row(plan: &Plan, surface: Surface) -> &SurfaceRow {
        plan.surfaces
            .iter()
            .find(|row| row.surface == surface)
            .unwrap()
    }

    const CAPTIONED: u32 = 0x00C0_0000;

    #[test]
    fn a_system_title_bar_should_take_caption_and_border_colors() {
        let report = report_for("Notepad", CAPTIONED, "arm64", 26100);
        let plan = plan(&report, &Context::default());
        assert_eq!(row(&plan, Surface::Caption).level, Level::Should);
        assert_eq!(row(&plan, Surface::Border).level, Level::Should);
        assert!(plan.actions.contains(&Action::Preview));
        assert!(plan.actions.contains(&Action::SaveRule));
    }

    #[test]
    fn an_app_that_draws_its_own_title_bar_never_promises_a_caption_color() {
        let report = report_for("Chrome_WidgetWin_1", 0, "x64-emulated", 26100);
        let plan = plan(&report, &Context::default());
        assert_eq!(row(&plan, Surface::Caption).level, Level::No);
        assert_ne!(row(&plan, Surface::Border).level, Level::Should);
        assert_eq!(row(&plan, Surface::Client).level, Level::No);
        assert!(row(&plan, Surface::Client).why.contains("own dark theme"));
    }

    #[test]
    fn a_backdrop_is_never_promised_because_content_is_usually_opaque() {
        let report = report_for("Notepad", CAPTIONED, "arm64", 26100);
        let plan = plan(&report, &Context::default());
        let backdrop = row(&plan, Surface::Backdrop);
        assert_eq!(backdrop.level, Level::Maybe);
        assert!(backdrop
            .why
            .contains("only shows where the app leaves its window transparent"));
    }

    #[test]
    fn reduced_effects_and_old_builds_turn_the_backdrop_into_no() {
        let report = report_for("Notepad", CAPTIONED, "arm64", 26100);
        let reduced = plan(
            &report,
            &Context {
                reduced: Some("battery saver"),
                ..Context::default()
            },
        );
        assert_eq!(row(&reduced, Surface::Backdrop).level, Level::No);
        assert!(row(&reduced, Surface::Backdrop)
            .why
            .contains("battery saver"));
        let old = report_for("Notepad", CAPTIONED, "arm64", 19045);
        let old = plan(&old, &Context::default());
        for surface in [Surface::Caption, Surface::Border, Surface::Backdrop] {
            assert_eq!(row(&old, surface).level, Level::No, "{surface:?}");
        }
    }

    #[test]
    fn legacy_controls_follow_the_switch_and_the_verdict() {
        let mut report = report_for("#32770", CAPTIONED, "arm64", 26100);
        report.legacy = Some(LegacyView {
            verdict: "this dialog can be fully treated".into(),
            gate: "allowed: native hook".into(),
            partial: vec![],
        });
        let off = plan(&report, &Context::default());
        assert_eq!(row(&off, Surface::LegacyControls).level, Level::NotRelevant);
        assert!(row(&off, Surface::LegacyControls).why.contains("off"));
        let on = plan(
            &report,
            &Context {
                legacy_enabled: true,
                ..Context::default()
            },
        );
        assert_eq!(row(&on, Surface::LegacyControls).level, Level::Should);
        report.legacy = Some(LegacyView {
            verdict: "this dialog can be partly treated".into(),
            gate: "allowed: native hook".into(),
            partial: vec!["Button".into()],
        });
        let partial = plan(
            &report,
            &Context {
                legacy_enabled: true,
                ..Context::default()
            },
        );
        assert_eq!(row(&partial, Surface::LegacyControls).level, Level::Maybe);
        assert!(row(&partial, Surface::LegacyControls)
            .why
            .contains("Button"));
    }

    #[test]
    fn a_window_that_is_left_alone_offers_no_preview_and_says_why() {
        let mut report = report_for("Notepad", CAPTIONED, "arm64", 26100);
        report.explanation.applied = false;
        report.explanation.reason = Some("protected");
        let plan = plan(&report, &Context::default());
        assert!(!plan.actions.contains(&Action::Preview));
        assert!(!plan.actions.contains(&Action::SaveRule));
        assert!(!plan.actions.contains(&Action::Exclude));
        assert_eq!(row(&plan, Surface::Caption).level, Level::No);
        assert!(plan
            .identity
            .iter()
            .any(|line| line.starts_with("Left alone")));
    }

    #[test]
    fn restore_is_offered_only_once_the_window_has_been_touched() {
        let report = report_for("Notepad", CAPTIONED, "arm64", 26100);
        assert!(!plan(&report, &Context::default())
            .actions
            .contains(&Action::Restore));
        let touched = Context {
            touched: true,
            ..Context::default()
        };
        assert!(plan(&report, &touched).actions.contains(&Action::Restore));
    }

    #[test]
    fn the_helper_route_is_described_honestly() {
        let report = report_for("Notepad", CAPTIONED, "x64-emulated", 26100);
        let off = plan(&report, &Context::default());
        assert!(off.identity[1].contains("skipped unless the optional x64 helper is switched on"));
        let on = plan(
            &report,
            &Context {
                helper_enabled: true,
                helper_installed: true,
                ..Context::default()
            },
        );
        assert!(on.identity[1].contains("experimental"));
        // Frame styling never needs the helper.
        assert!(on.identity[1].contains("Frame styling needs no helper"));
    }

    #[test]
    fn nothing_says_confirmed_guaranteed_or_all_controls() {
        for class in [
            "Notepad",
            "Chrome_WidgetWin_1",
            "#32770",
            "ApplicationFrameWindow",
        ] {
            let mut report = report_for(class, CAPTIONED, "arm64", 26100);
            report.attempts = vec![Attempt {
                name: "DWMWA_CAPTION_COLOR",
                attribute: 35,
                confirm: Confirm::ReadbackMatches,
            }];
            let plan = plan(&report, &Context::default());
            let mut text = plan.identity.join("\n");
            text.push_str(
                &plan
                    .surfaces
                    .iter()
                    .map(|r| r.why.clone())
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
            text.push_str(&plan.limits.join("\n"));
            text.push_str(&receipt_lines(&report).join("\n"));
            let lower = text.to_lowercase();
            for banned in [
                "confirmed",
                "guarantee",
                "all controls",
                "every control can",
            ] {
                assert!(
                    !lower.contains(banned),
                    "{class}: found {banned:?} in {text}"
                );
            }
        }
    }

    #[test]
    fn the_receipt_keeps_requested_api_and_seen_apart() {
        let mut report = report_for("Notepad", CAPTIONED, "arm64", 26100);
        report.attempts = vec![
            Attempt {
                name: "DWMWA_CAPTION_COLOR",
                attribute: 35,
                confirm: Confirm::ReadbackMatches,
            },
            Attempt {
                name: "DWMWA_BORDER_COLOR",
                attribute: 34,
                confirm: Confirm::Rejected(-2147024809),
            },
        ];
        let rows = triples(&report);
        let caption = rows
            .iter()
            .find(|r| r.setting == "Title bar color")
            .unwrap();
        assert!(caption.windows_said.contains("appearance not verified"));
        assert!(caption.seen_on_screen.starts_with("not verified"));
        let border = rows.iter().find(|r| r.setting == "Border color").unwrap();
        assert!(border.windows_said.contains("rejected"));
        // Every row says the same thing about the screen: AllBlack11 did not look.
        assert!(rows
            .iter()
            .all(|r| r.seen_on_screen.starts_with("not verified")));
        // A setting with no attempt on record says so rather than inventing a result.
        let text = rows.iter().find(|r| r.setting == "Corners").unwrap();
        assert!(text.windows_said.contains("no attempt recorded"));
    }
}
