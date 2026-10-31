//! Commands from the Settings window to the tray host. Settings runs on the same thread, but
//! it never reaches into the host's state: it queues a request, posts one message, and the
//! host executes it between its own events. That keeps every mutation in one place.

use std::sync::Mutex;

use windows_sys::Win32::UI::WindowsAndMessaging::WM_APP;

use crate::config::Style;
use crate::{Config, Fingerprint};

pub const WM_APP_COMMAND: u32 = WM_APP + 9;

pub enum AppCommand {
    /// Apply `style` to one window for a short time, without touching the config.
    TryOnWindow {
        key: i64,
        fingerprint: Fingerprint,
        style: Style,
    },
    /// Put one window back as it was.
    RestoreWindow {
        key: i64,
        fingerprint: Fingerprint,
    },
    /// Run this whole config in memory for a few seconds. Nothing is saved until `TrialKeep`.
    TrialStart(Box<Config>),
    TrialKeep,
    TrialRevert,
    /// Replace `config.json` with the last known good copy.
    RestoreLastKnownGood,
}

static QUEUE: Mutex<Vec<AppCommand>> = Mutex::new(Vec::new());

pub fn push(command: AppCommand) {
    QUEUE
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .push(command);
}

pub fn drain() -> Vec<AppCommand> {
    std::mem::take(&mut *QUEUE.lock().unwrap_or_else(|err| err.into_inner()))
}
