//! AllBlack11 core: config, window policy, and the snapshot file used to undo changes.
//! The Win32 tray loop lives in `platform` and is compiled only for Windows.

mod arch;
mod caption;
mod config;
mod inject;
mod legacy_policy;
mod policy;
mod presets;
mod rules;
mod shell;
mod snapshot;
mod target;
mod timefmt;
mod version;

pub use arch::{classify, machine_name, ArchStatus, BUILD_MACHINE};
pub use config::{
    base_style, bundled_default_json, parse_config, set_enabled_flag, Backdrop, Color, Config,
    Corners, LegacySettings, PartialStyle, Rule, Style, FALLBACK_ACCENT_BGR,
};
pub use inject::{inject_allowed, InjectTarget};
pub use legacy_policy::{
    capability, contrast_ratio, host_kind, palette, window_verdict, Capability, HostKind, Level, Palette,
    SharedState, Tech, Verdict, PALETTES,
};
pub use target::{classify_target, pe_machine, route, Route, TargetArch};
pub use policy::{
    decide, is_borderless_fullscreen, rect_covers_monitor, Decision, WindowFacts, WS_CHILD,
    WS_DLGFRAME, WS_EX_LAYERED, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
    WS_POPUP,
};
pub use rules::{
    describe_conflict, find_conflicts, glob_match, rank, resolve, why_it_won, Conflict, ConflictKind,
    Criterion, Resolved, RuleMatch, Source, WindowIdentity,
};
pub use presets::{apply as apply_preset, Preset, PRESETS};
pub use shell::{blacken_shell_slots, ShellBackup, SHELL_SLOT_BYTES};
pub use snapshot::{format_snapshots, parse_snapshots, same_process, SnapshotFile, StoredSnapshot};
pub use timefmt::format_unix_utc;
pub use version::display_version;

#[cfg(windows)]
mod platform;

#[cfg(windows)]
pub use platform::start;
