//! AllBlack11 core: config, window policy, and the snapshot file used to undo changes.
//! The Win32 tray loop lives in `platform` and is compiled only for Windows.

mod arch;
mod caption;
mod compat;
mod config;
mod effects;
mod explain;
mod inject;
mod legacy_policy;
mod lkg;
mod menu_model;
mod policy;
mod presets;
mod recipes;
mod report;
mod rules;
mod shell;
mod snapshot;
mod target;
mod timefmt;
mod version;

pub use arch::{classify, machine_name, ArchStatus, BUILD_MACHINE};
pub use compat::{Entry as CompatEntry, Notebook, Status as CompatStatus};
pub use config::{
    base_style, bundled_default_json, parse_config, set_enabled_flag, Backdrop, Color, Config,
    Corners, FocusSettings, LegacySettings, PartialStyle, ReduceEffects, Rule, Style,
    FALLBACK_ACCENT_BGR,
};
pub use effects::{focus_style, reduce_reason, reduced, Explicit, SystemState};
pub use explain::{
    attribute_support, explain, framework_hint, reason_text, AttributeSupport, Confirm,
    Explanation, Framework, FrameworkHint, Support, TitleBar,
};
pub use inject::{inject_allowed, InjectTarget};
pub use legacy_policy::{
    capability, contrast_ratio, host_kind, palette, window_verdict, Capability, HostKind, Level,
    Palette, SharedState, Tech, Verdict, PALETTES,
};
pub use lkg::{is_pre_schema_2, pick as pick_config, Picked, Source as ConfigSource};
pub use menu_model::{
    flatten as flatten_menu, Drawn as MenuDrawn, Item as MenuItem, ItemKind as MenuItemKind,
};
pub use policy::{
    decide, is_borderless_fullscreen, rect_covers_monitor, Decision, WindowFacts, WS_CHILD,
    WS_DLGFRAME, WS_EX_LAYERED, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
    WS_POPUP,
};
pub use presets::{apply as apply_preset, Preset, PRESETS};
pub use recipes::{
    apply as apply_recipe, bundled as bundled_recipes, export_current as export_recipe,
    parse_recipe, preview as preview_recipe, Change, Recipe, Trust,
};
pub use report::{redact, render as render_report, source_text, Attempt, LegacyView, Report};
pub use rules::{
    describe_conflict, find_conflicts, glob_match, rank, resolve, why_it_won, Conflict,
    ConflictKind, Criterion, Resolved, RuleMatch, Source, WindowIdentity,
};
pub use shell::{blacken_shell_slots, ShellBackup, SHELL_SLOT_BYTES};
pub use snapshot::{format_snapshots, parse_snapshots, same_process, SnapshotFile, StoredSnapshot};
pub use target::{classify_target, pe_machine, route, Route, TargetArch};
pub use timefmt::format_unix_utc;
pub use version::display_version;

#[cfg(windows)]
mod platform;

#[cfg(windows)]
pub use platform::start;
