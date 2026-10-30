//! Win32 tray host. This module is the only place that calls Windows APIs.
//!
//! Nothing here injects code. `SetWinEventHook` is called with `WINEVENT_OUTOFCONTEXT`
//! and a null module handle, so the callback runs in this process. The only cross-process
//! calls are public DWM and Win32 queries (`PROCESS_QUERY_LIMITED_INFORMATION`).

mod app;
mod composition;
mod dwm;
mod icon;
mod registry;
mod shellwin;
mod wide;

pub fn start() -> i32 {
    app::start()
}
