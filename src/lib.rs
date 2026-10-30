//! AllBlack11 core: config, window policy, and the snapshot file used to undo changes.
//! The Win32 tray loop lives in `platform` and is compiled only for Windows.

mod caption;
mod config;
mod inject;
mod policy;
mod shell;
mod snapshot;
mod timefmt;

pub use config::{
    base_style, bundled_default_json, parse_config, set_enabled_flag, Backdrop, Color, Config,
    Corners, PartialStyle, Rule, Style, FALLBACK_ACCENT_BGR,
};
pub use policy::{
    decide, is_borderless_fullscreen, rect_covers_monitor, Decision, WindowFacts, WS_CHILD,
    WS_DLGFRAME, WS_EX_LAYERED, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
    WS_POPUP,
};
pub use shell::{blacken_shell_slots, ShellBackup, SHELL_SLOT_BYTES};
pub use snapshot::{format_snapshots, parse_snapshots, same_process, SnapshotFile, StoredSnapshot};
pub use timefmt::format_unix_utc;

#[cfg(windows)]
mod platform;

#[cfg(windows)]
pub use platform::start;
