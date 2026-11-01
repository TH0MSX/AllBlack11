//! The in-app updater: finds a newer release on GitHub, downloads it with consent, verifies it,
//! and hands it to the installer. It never updates by itself and never touches the running exe.
//!
//! All network and disk work runs on one short-lived worker thread at a time; the UI only reads
//! the phase. A failure here changes the phase and nothing else: styling, recovery and uninstall
//! of the installed version do not depend on this module. Nothing here may panic (the release
//! build aborts on panic), so there is no `unwrap`, no unchecked index, no unchecked slice.
//!
//! Policy and every decision that can be tested without Windows live in `crate::update` and
//! `crate::download`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
use std::sync::Mutex;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, SW_SHOWNORMAL, WM_APP};

use crate::download::{fetch_to_file, hash_file, remove_files, FetchResult};
use crate::update::{
    check_due, releases_api_url, select, stale_files, Arch, Candidate, Channel, DownloadRecord,
    Outcome, Phase, State, Version, CHECK_INTERVAL_SECS, MAX_INSTALLER_BYTES,
};

use super::wide::{config_dir, log_line, pcw, to_wide, write_atomic};

/// Posted to the host window when a check finds a release the user has not been told about.
pub const WM_UPDATE_FOUND: u32 = WM_APP + 20;

static PHASE: Mutex<Phase> = Mutex::new(Phase::Idle);
static STAMP: AtomicU64 = AtomicU64::new(0);
static CANCEL: AtomicBool = AtomicBool::new(false);
static BUSY: AtomicBool = AtomicBool::new(false);
static HOST: AtomicIsize = AtomicIsize::new(0);

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn phase() -> Phase {
    PHASE.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Changes every time the phase changes, so the UI knows when to lay the page out again.
pub fn stamp() -> u64 {
    STAMP.load(Ordering::Relaxed)
}

/// A worker is running (checking or downloading): the page keeps repainting.
pub fn busy() -> bool {
    BUSY.load(Ordering::Relaxed)
}

fn set_phase(next: Phase) {
    *PHASE.lock().unwrap_or_else(|e| e.into_inner()) = next;
    STAMP.fetch_add(1, Ordering::Relaxed);
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// --- files -------------------------------------------------------------------------------------

fn state_path() -> PathBuf {
    config_dir().join("update-state.json")
}

/// Downloads live under LocalAppData (not the roaming profile).
fn updates_dir() -> PathBuf {
    match std::env::var_os("LOCALAPPDATA") {
        Some(base) if !base.is_empty() => PathBuf::from(base).join("AllBlack11").join("updates"),
        _ => std::env::temp_dir().join("AllBlack11-updates"),
    }
}

fn load_state() -> State {
    std::fs::read_to_string(state_path())
        .map(|text| State::parse(&text))
        .unwrap_or_default()
}

fn save_state(state: &State) {
    if let Err(err) = write_atomic(&state_path(), &state.to_json()) {
        log_line("warn", &format!("could not save update-state.json: {err}"));
    }
}

fn change_state(change: impl FnOnce(&mut State)) {
    let mut state = load_state();
    change(&mut state);
    save_state(&state);
}

// --- start-up ------------------------------------------------------------------------------------

/// Called once at start. Removes stale downloads, and brings back a download that was finished
/// and verified in an earlier session (so "Install" is still there after a restart).
pub fn init(host: HWND) {
    HOST.store(host as isize, Ordering::Relaxed);
    let dir = updates_dir();
    let state = load_state();
    let mut listing: Vec<(String, u64)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            let age = entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            listing.push((name, age));
        }
    }
    let keep = state.download.as_ref().map(|d| d.file.as_str());
    let stale = stale_files(&listing, keep, 14);
    if !stale.is_empty() {
        let removed = remove_files(&dir, &stale);
        log_line("info", &format!("removed {removed} stale update file(s)"));
    }
    // A recorded download is only trusted if the file is still there and still hashes the same.
    if let Some(record) = state.download.clone() {
        let path = dir.join(&record.file);
        let still_good = hash_file(&path).is_ok_and(|actual| actual == record.sha256);
        if still_good {
            if let Some(version) = Version::parse(&record.tag) {
                let current = Version::parse(current_version());
                if current.is_none_or(|current| version > current) {
                    set_phase(Phase::Ready {
                        candidate: ready_candidate(&record, &version),
                        path,
                        sha256: record.sha256,
                        verified: true,
                    });
                    return;
                }
            }
        }
        change_state(|state| state.download = None);
        let _ = std::fs::remove_file(dir.join(&record.file));
    }
}

fn ready_candidate(record: &DownloadRecord, version: &Version) -> Candidate {
    Candidate {
        tag: record.tag.clone(),
        version: version.clone(),
        preview: version.is_preview(),
        notes: String::new(),
        page_url: format!("{}/tag/{}", crate::update::releases_page_url(), record.tag),
        asset_name: record.file.clone(),
        asset_url: String::new(),
        size: 0,
        integrity: crate::update::Integrity::Published(record.sha256.clone()),
    }
}

/// How long until the next automatic check should run, in milliseconds. At least a minute, so a
/// start-up never hits the network in the first seconds; at most a day.
pub fn next_check_delay_ms() -> u32 {
    let state = load_state();
    if check_due(state.last_check, now_secs()) {
        return 45_000;
    }
    let left = CHECK_INTERVAL_SECS.saturating_sub(now_secs().saturating_sub(state.last_check));
    (left.clamp(60, CHECK_INTERVAL_SECS) * 1000) as u32
}

// --- worker plumbing -------------------------------------------------------------------------------

fn begin() -> bool {
    BUSY.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
}

fn end() {
    BUSY.store(false, Ordering::Release);
}

fn spawn(name: &str, work: impl FnOnce() + Send + 'static) -> bool {
    std::thread::Builder::new()
        .name(name.into())
        .spawn(work)
        .is_ok()
}

fn tell_host(candidate: &Candidate) {
    let host = HOST.load(Ordering::Relaxed);
    if host == 0 {
        return;
    }
    let state = load_state();
    if state.notified.as_deref() == Some(candidate.tag.as_str()) {
        return; // one release is announced once
    }
    unsafe { PostMessageW(host as HWND, WM_UPDATE_FOUND, 0, 0) };
}

/// The tag the host balloon should name, and a record that it has been announced.
pub fn take_announcement() -> Option<(String, bool)> {
    let candidate = match phase() {
        Phase::Available(c) => c,
        _ => return None,
    };
    let state = load_state();
    if state.notified.as_deref() == Some(candidate.tag.as_str()) {
        return None;
    }
    change_state(|state| state.notified = Some(candidate.tag.clone()));
    Some((candidate.tag, candidate.preview))
}

// --- actions -------------------------------------------------------------------------------------

/// Look for a newer release. `manual` is a button press: failures are shown. An automatic check
/// that fails stays quiet (it is only logged) and leaves the page as it was.
pub fn check(manual: bool, channel: Channel) {
    if !begin() {
        return;
    }
    let before = phase();
    set_phase(Phase::Checking);
    let started = spawn("allblack11-update-check", move || {
        let result = (|| -> Result<Outcome, String> {
            let current = Version::parse(current_version())
                .ok_or_else(|| "this build has no usable version number.".to_string())?;
            let text = super::http::get_text(&releases_api_url(), 512 * 1024)?;
            let releases = crate::update::parse_releases(&text)?;
            let state = load_state();
            Ok(select(
                &releases,
                &current,
                channel.includes_previews(&current),
                state.skipped.as_deref(),
                Arch::of_this_build(),
            ))
        })();
        match result {
            Ok(outcome) => {
                change_state(|state| state.last_check = now_secs());
                match outcome {
                    Outcome::UpToDate => set_phase(Phase::UpToDate),
                    Outcome::Available(candidate) => {
                        tell_host(&candidate);
                        set_phase(Phase::Available(candidate));
                    }
                    Outcome::Skipped(candidate) => set_phase(Phase::Skipped(candidate)),
                    Outcome::NoInstaller { reason, .. } => set_phase(Phase::NoInstaller(reason)),
                }
            }
            Err(message) => {
                log_line("warn", &format!("update check failed: {message}"));
                if manual {
                    set_phase(Phase::Failed(message));
                } else {
                    set_phase(before);
                }
            }
        }
        end();
    });
    if !started {
        set_phase(Phase::Failed("could not start the check.".into()));
        end();
    }
}

/// Download the offered installer. The user pressed Download; nothing is installed by this.
pub fn download() {
    let Phase::Available(candidate) = phase() else {
        return;
    };
    if !crate::update::is_release_asset_url(&candidate.asset_url, &candidate.tag) {
        set_phase(Phase::Failed(
            "the release lists a download address this updater does not trust.".into(),
        ));
        return;
    }
    if !begin() {
        return;
    }
    CANCEL.store(false, Ordering::Relaxed);
    set_phase(Phase::Downloading {
        candidate: candidate.clone(),
        done: 0,
        total: candidate.size,
    });
    let started = spawn("allblack11-update-download", move || {
        let result = (|| -> Result<FetchResult, String> {
            let mut response = super::http::get(&candidate.asset_url, "application/octet-stream")?;
            if response.status != 200 {
                return Err(crate::update::describe_http_status(response.status));
            }
            let total = if candidate.size > 0 {
                candidate.size
            } else {
                response.length.unwrap_or(0)
            };
            let expected = (candidate.size > 0).then_some(candidate.size);
            let progress_for = candidate.clone();
            Ok(fetch_to_file(
                &mut response,
                &updates_dir(),
                &candidate.asset_name,
                MAX_INSTALLER_BYTES,
                expected,
                &candidate.integrity,
                &CANCEL,
                &mut |done| {
                    // Only the numbers change while downloading; the page is not laid out again.
                    *PHASE.lock().unwrap_or_else(|e| e.into_inner()) = Phase::Downloading {
                        candidate: progress_for.clone(),
                        done,
                        total,
                    };
                },
            ))
        })();
        match result {
            Ok(FetchResult::Verified { path, sha256 }) => ready(&candidate, path, sha256, true),
            Ok(FetchResult::Unpublished { path, sha256 }) => ready(&candidate, path, sha256, false),
            Ok(FetchResult::Cancelled) => set_phase(Phase::Available(candidate)),
            Ok(FetchResult::Failed(message)) => {
                log_line("warn", &format!("update download failed: {message}"));
                set_phase(Phase::Failed(message));
            }
            Err(message) => {
                log_line("warn", &format!("update download failed: {message}"));
                set_phase(Phase::Failed(message));
            }
        }
        end();
    });
    if !started {
        set_phase(Phase::Failed("could not start the download.".into()));
        end();
    }
}

fn ready(candidate: &Candidate, path: PathBuf, sha256: String, verified: bool) {
    change_state(|state| {
        state.download = Some(DownloadRecord {
            tag: candidate.tag.clone(),
            file: candidate.asset_name.clone(),
            sha256: sha256.clone(),
        })
    });
    set_phase(Phase::Ready {
        candidate: candidate.clone(),
        path,
        sha256,
        verified,
    });
}

pub fn cancel() {
    CANCEL.store(true, Ordering::Relaxed);
}

pub fn skip() {
    if let Phase::Available(candidate) = phase() {
        change_state(|state| state.skipped = Some(candidate.tag.clone()));
        set_phase(Phase::Skipped(candidate));
    }
}

pub fn unskip() {
    if let Phase::Skipped(candidate) = phase() {
        change_state(|state| state.skipped = None);
        set_phase(Phase::Available(candidate));
    }
}

pub fn discard() {
    if let Phase::Ready { path, .. } = phase() {
        let _ = std::fs::remove_file(&path);
        change_state(|state| state.download = None);
        set_phase(Phase::Idle);
    }
}

/// Start the downloaded installer (the user pressed Install). The file is hashed again first, so
/// one that changed since it was checked is deleted instead of run. The caller then exits the
/// app; Setup also stops a running AllBlack11 (`--exit`, which puts window frames back).
pub fn install() -> Result<(), String> {
    let Phase::Ready { path, sha256, .. } = phase() else {
        return Err("There is no downloaded update to install.".into());
    };
    let actual =
        hash_file(&path).map_err(|err| format!("The downloaded file could not be read: {err}"))?;
    if actual != sha256 {
        let _ = std::fs::remove_file(&path);
        change_state(|state| state.download = None);
        set_phase(Phase::Idle);
        return Err("The downloaded file changed after it was checked, so it was deleted. Download it again.".into());
    }
    let verb = to_wide("open");
    let target = to_wide(&path.display().to_string());
    let code = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            pcw(&verb),
            pcw(&target),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if (code as isize) > 32 {
        Ok(())
    } else {
        Err(format!(
            "Windows could not start the installer (code {}).",
            code as isize
        ))
    }
}
