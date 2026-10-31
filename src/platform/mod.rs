//! Win32 tray host. This module is the only place that calls Windows APIs.
//!
//! The usual path does not inject. `SetWinEventHook` uses `WINEVENT_OUTOFCONTEXT`
//! and a null module, so that callback runs here. Cross-process calls on that path
//! are public DWM attributes, `SetWindowTheme`, and limited process queries.
//!
//! `experimentalDarkLegacy` is off unless the user opts in. Only then does
//! `legacy` load `AllBlack11Hook.dll` into same-architecture processes that are
//! not elevated, protected, or excluded.

mod app;
mod cfgio;
mod composition;
mod dwm;
mod icon;
mod legacy;
mod menu;
mod metrics;
mod registry;
mod settings;
mod shellwin;
mod sysinfo;
mod uxtheme;
mod wide;

pub fn start() -> i32 {
    app::start()
}
