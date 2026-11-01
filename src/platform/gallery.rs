//! The wallpaper gallery's Windows side: preview pictures, the on-demand download, and applying a
//! wallpaper through the Windows desktop wallpaper service.
//!
//! Network, disk and shell calls run on one short-lived worker thread at a time; the UI only reads
//! the job state. A failure changes the message and nothing else: it cannot affect styling,
//! recovery or uninstall. Nothing here may panic (the release build aborts on panic).
//!
//! The `IDesktopWallpaper` vtable below was checked against `shobjidl_core.idl` in the Windows SDK.
//! None of it has run on Windows hardware yet.

use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::HBITMAP;
use windows_sys::Win32::Graphics::GdiPlus::{
    GdipCreateBitmapFromFile, GdipCreateHBITMAPFromBitmap, GdipDisposeImage, GdiplusShutdown,
    GdiplusStartup, GdiplusStartupInput, GpBitmap, GpImage,
};
use windows_sys::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_LOCAL_SERVER,
    COINIT_MULTITHREADED,
};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::download::{fetch_to_file, hash_file, FetchResult};
use crate::update::Integrity;
use crate::wallpaper::{
    asset_url, release_tag, thumb_bytes, Applied, Entry, Fit, Previous, State, MAX_IMAGE_BYTES,
};

use super::wide::{config_dir, log_line, pcw, to_wide, write_atomic};

const CLSID_DESKTOP_WALLPAPER: GUID = GUID::from_u128(0xC2CF3110_460E_4fc1_B9D0_8A1C0C9CC4BD);
const IID_IDESKTOP_WALLPAPER: GUID = GUID::from_u128(0xB92B56A9_8B55_4E14_9A89_0199BBB6F93B);

/// `IDesktopWallpaper` from `shobjidl_core.idl`: IUnknown, then these in this order. Slots after
/// GetPosition (slideshow methods) are not used and not declared.
#[repr(C)]
struct Vtbl {
    query_interface: usize,
    add_ref: usize,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    set_wallpaper: unsafe extern "system" fn(*mut c_void, *const u16, *const u16) -> i32,
    get_wallpaper: unsafe extern "system" fn(*mut c_void, *const u16, *mut *mut u16) -> i32,
    get_monitor_device_path_at: unsafe extern "system" fn(*mut c_void, u32, *mut *mut u16) -> i32,
    get_monitor_device_path_count: unsafe extern "system" fn(*mut c_void, *mut u32) -> i32,
    get_monitor_rect: unsafe extern "system" fn(*mut c_void, *const u16, *mut RECT) -> i32,
    set_background_color: unsafe extern "system" fn(*mut c_void, u32) -> i32,
    get_background_color: unsafe extern "system" fn(*mut c_void, *mut u32) -> i32,
    set_position: unsafe extern "system" fn(*mut c_void, u32) -> i32,
    get_position: unsafe extern "system" fn(*mut c_void, *mut u32) -> i32,
}

#[repr(C)]
struct Object {
    vtbl: *const Vtbl,
}

/// One live `IDesktopWallpaper` on the current thread. Released on drop.
struct Wallpaper {
    object: *mut c_void,
}

impl Wallpaper {
    unsafe fn vtbl(&self) -> &Vtbl {
        &*(*(self.object as *mut Object)).vtbl
    }

    fn create() -> Result<Wallpaper, String> {
        let mut object: *mut c_void = std::ptr::null_mut();
        let hr = unsafe {
            CoCreateInstance(
                &CLSID_DESKTOP_WALLPAPER,
                std::ptr::null_mut(),
                CLSCTX_LOCAL_SERVER,
                &IID_IDESKTOP_WALLPAPER,
                &mut object,
            )
        };
        if hr < 0 || object.is_null() {
            return Err(format!(
                "Windows would not open its wallpaper service (code {hr:#010x})."
            ));
        }
        Ok(Wallpaper { object })
    }

    /// A string Windows allocated, copied out and freed.
    unsafe fn take_string(ptr: *mut u16) -> String {
        if ptr.is_null() {
            return String::new();
        }
        let mut len = 0usize;
        while len < 4096 && *ptr.add(len) != 0 {
            len += 1;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
        CoTaskMemFree(ptr as *const c_void);
        text
    }

    fn monitor_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        unsafe {
            let mut count = 0u32;
            if (self.vtbl().get_monitor_device_path_count)(self.object, &mut count) < 0 {
                return ids;
            }
            for index in 0..count.min(16) {
                let mut out: *mut u16 = std::ptr::null_mut();
                if (self.vtbl().get_monitor_device_path_at)(self.object, index, &mut out) >= 0 {
                    let id = Self::take_string(out);
                    if !id.is_empty() {
                        ids.push(id);
                    }
                }
            }
        }
        ids
    }

    /// The display's rectangle, or None for a display that is not attached (S_FALSE).
    fn monitor_rect(&self, id: &str) -> Option<RECT> {
        let wide = to_wide(id);
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let hr = unsafe { (self.vtbl().get_monitor_rect)(self.object, pcw(&wide), &mut rect) };
        (hr == 0).then_some(rect)
    }

    fn wallpaper_of(&self, monitor: Option<&str>) -> String {
        let wide = monitor.map(to_wide);
        let id = wide.as_ref().map_or(std::ptr::null(), |w| w.as_ptr());
        let mut out: *mut u16 = std::ptr::null_mut();
        unsafe {
            if (self.vtbl().get_wallpaper)(self.object, id, &mut out) < 0 {
                return String::new();
            }
            Self::take_string(out)
        }
    }

    fn set_wallpaper(&self, monitor: Option<&str>, path: &str) -> Result<(), String> {
        let wide_monitor = monitor.map(to_wide);
        let id = wide_monitor
            .as_ref()
            .map_or(std::ptr::null(), |w| w.as_ptr());
        let wide_path = to_wide(path);
        let hr = unsafe { (self.vtbl().set_wallpaper)(self.object, id, pcw(&wide_path)) };
        if hr < 0 {
            Err(format!("Windows refused the wallpaper (code {hr:#010x})."))
        } else {
            Ok(())
        }
    }

    fn position(&self) -> Option<u32> {
        let mut value = 0u32;
        (unsafe { (self.vtbl().get_position)(self.object, &mut value) } >= 0).then_some(value)
    }

    fn set_position(&self, value: u32) -> Result<(), String> {
        let hr = unsafe { (self.vtbl().set_position)(self.object, value) };
        if hr < 0 {
            Err(format!("Windows refused the fit (code {hr:#010x})."))
        } else {
            Ok(())
        }
    }

    fn background(&self) -> Option<u32> {
        let mut value = 0u32;
        (unsafe { (self.vtbl().get_background_color)(self.object, &mut value) } >= 0)
            .then_some(value)
    }

    fn set_background(&self, color: u32) {
        unsafe { (self.vtbl().set_background_color)(self.object, color) };
    }
}

impl Drop for Wallpaper {
    fn drop(&mut self) {
        unsafe { (self.vtbl().release)(self.object) };
    }
}

// --- job state ---------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Job {
    Idle,
    Downloading { id: String, done: u64, total: u64 },
    Applying { id: String },
    PuttingBack,
    ListingDisplays,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Display {
    pub id: String,
    pub label: String,
}

static JOB: Mutex<Job> = Mutex::new(Job::Idle);
static MESSAGE: Mutex<Option<String>> = Mutex::new(None);
static DISPLAYS: Mutex<Vec<Display>> = Mutex::new(Vec::new());
static STAMP: AtomicU64 = AtomicU64::new(0);
static CANCEL: AtomicBool = AtomicBool::new(false);
static BUSY: AtomicBool = AtomicBool::new(false);

pub fn job() -> Job {
    JOB.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn stamp() -> u64 {
    STAMP.load(Ordering::Relaxed)
}

pub fn busy() -> bool {
    BUSY.load(Ordering::Relaxed)
}

/// What the last job finished with, once.
pub fn take_message() -> Option<String> {
    MESSAGE.lock().unwrap_or_else(|e| e.into_inner()).take()
}

pub fn displays() -> Vec<Display> {
    DISPLAYS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn cancel() {
    CANCEL.store(true, Ordering::Relaxed);
}

fn set_job(next: Job) {
    *JOB.lock().unwrap_or_else(|e| e.into_inner()) = next;
    STAMP.fetch_add(1, Ordering::Relaxed);
}

fn finish(message: String) {
    *MESSAGE.lock().unwrap_or_else(|e| e.into_inner()) = Some(message);
    set_job(Job::Idle);
    BUSY.store(false, Ordering::Relaxed);
}

fn begin() -> bool {
    BUSY.compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
        .is_ok()
}

fn spawn(name: &str, work: impl FnOnce() + Send + 'static) -> bool {
    std::thread::Builder::new()
        .name(name.to_string())
        .spawn(work)
        .is_ok()
}

// --- files -------------------------------------------------------------------------------------

fn local_root() -> PathBuf {
    match std::env::var_os("LOCALAPPDATA") {
        Some(base) if !base.is_empty() => PathBuf::from(base).join("AllBlack11"),
        _ => std::env::temp_dir().join("AllBlack11"),
    }
}

pub fn images_dir() -> PathBuf {
    local_root().join("wallpapers")
}

/// The preview picture's file, for the full-screen preview of a wallpaper not yet downloaded.
pub fn preview_file(entry: &Entry) -> PathBuf {
    thumbs_dir().join(format!("{}.jpg", entry.id))
}

fn thumbs_dir() -> PathBuf {
    local_root().join("wallpaper-previews")
}

fn state_path() -> PathBuf {
    config_dir().join("wallpaper-state.json")
}

pub fn image_path(entry: &Entry) -> PathBuf {
    images_dir().join(&entry.file)
}

/// Whether the full image is on this PC with the size the manifest lists. The hash is checked
/// again right before it is applied.
pub fn downloaded(entry: &Entry) -> bool {
    std::fs::metadata(image_path(entry)).is_ok_and(|m| m.is_file() && m.len() == entry.bytes)
}

pub fn load_state() -> State {
    std::fs::read_to_string(state_path())
        .map(|text| State::parse(&text))
        .unwrap_or_default()
}

pub fn save_state(state: &State) {
    if let Err(err) = write_atomic(&state_path(), &state.to_json()) {
        log_line(
            "warn",
            &format!("could not save wallpaper-state.json: {err}"),
        );
    }
}

// --- previews ----------------------------------------------------------------------------------

/// The preview pictures as bitmaps, decoded once from the JPEGs that ship inside the exe. Windows'
/// own picture support (GDI+) reads them from a small cache folder; it is started and stopped
/// inside this call, so nothing stays loaded.
pub fn load_previews(entries: &[Entry]) -> Vec<(String, HBITMAP)> {
    let dir = thumbs_dir();
    let _ = std::fs::create_dir_all(&dir);
    let mut out = Vec::new();
    unsafe {
        let mut token = 0usize;
        let input = GdiplusStartupInput {
            GdiplusVersion: 1,
            DebugEventCallback: 0,
            SuppressBackgroundThread: 0,
            SuppressExternalCodecs: 0,
        };
        if GdiplusStartup(&mut token, &input, std::ptr::null_mut()) != 0 {
            return out;
        }
        for entry in entries {
            let Some(bytes) = thumb_bytes(&entry.id) else {
                continue;
            };
            let path = dir.join(format!("{}.jpg", entry.id));
            let current = std::fs::metadata(&path).is_ok_and(|m| m.len() == bytes.len() as u64);
            if !current && std::fs::write(&path, bytes).is_err() {
                continue;
            }
            if let Some(bitmap) = hbitmap_from_file(&path.to_string_lossy()) {
                out.push((entry.id.clone(), bitmap));
            }
        }
        GdiplusShutdown(token);
    }
    out
}

/// A GDI bitmap from a picture file. GDI+ must already be started. Black behind transparency.
pub unsafe fn hbitmap_from_file(path: &str) -> Option<HBITMAP> {
    let wide = to_wide(path);
    let mut bitmap: *mut GpBitmap = std::ptr::null_mut();
    if GdipCreateBitmapFromFile(pcw(&wide), &mut bitmap) != 0 || bitmap.is_null() {
        return None;
    }
    let mut handle: HBITMAP = std::ptr::null_mut();
    let status = GdipCreateHBITMAPFromBitmap(bitmap, &mut handle, 0xFF00_0000);
    GdipDisposeImage(bitmap as *mut GpImage);
    (status == 0 && !handle.is_null()).then_some(handle)
}

// --- download ----------------------------------------------------------------------------------

/// Download one full-size image, with the user's consent (the button names its size). It goes to a
/// `.part` file, is checked against the SHA-256 in this exe's manifest, then renamed.
pub fn download(entry: &Entry) {
    let Some(url) = asset_url(&release_tag(), entry) else {
        finish("That download address is not one AllBlack11 trusts.".into());
        return;
    };
    if !begin() {
        return;
    }
    CANCEL.store(false, Ordering::Relaxed);
    set_job(Job::Downloading {
        id: entry.id.clone(),
        done: 0,
        total: entry.bytes,
    });
    let entry = entry.clone();
    let started = spawn("allblack11-wallpaper-download", move || {
        let result = (|| -> Result<FetchResult, String> {
            let mut response = super::http::get(&url, "application/octet-stream")?;
            if response.status == 404 {
                return Err(format!(
                    "The wallpaper files are not attached to release {} yet, so there is nothing to download. They are published with the release.",
                    release_tag()
                ));
            }
            if response.status != 200 {
                return Err(crate::update::describe_http_status(response.status));
            }
            let id = entry.id.clone();
            let total = entry.bytes;
            Ok(fetch_to_file(
                &mut response,
                &images_dir(),
                &entry.file,
                MAX_IMAGE_BYTES,
                Some(entry.bytes),
                &Integrity::Published(entry.sha256.clone()),
                &CANCEL,
                &mut |done| {
                    *JOB.lock().unwrap_or_else(|e| e.into_inner()) = Job::Downloading {
                        id: id.clone(),
                        done,
                        total,
                    };
                },
            ))
        })();
        let message = match result {
            Ok(FetchResult::Verified { .. }) => format!(
                "Downloaded {} and checked it against the published checksum.",
                entry.name
            ),
            Ok(FetchResult::Unpublished { .. }) => {
                // Cannot happen with a published digest; treated as a failure rather than trusted.
                let _ = std::fs::remove_file(image_path(&entry));
                "The download could not be checked, so it was deleted.".to_string()
            }
            Ok(FetchResult::Cancelled) => "Download cancelled.".to_string(),
            Ok(FetchResult::Failed(message)) | Err(message) => {
                log_line("warn", &format!("wallpaper download failed: {message}"));
                message
            }
        };
        finish(message);
    });
    if !started {
        finish("Could not start the download.".into());
    }
}

// --- displays, apply, put back -----------------------------------------------------------------

struct ComScope(bool);

impl ComScope {
    fn enter() -> ComScope {
        // S_OK and S_FALSE both mean "initialized on this thread, balance with CoUninitialize".
        ComScope(unsafe { CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED as u32) } >= 0)
    }
}

impl Drop for ComScope {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

/// Read the attached displays through Windows. The list shows up in `displays()` when done.
pub fn refresh_displays() {
    if !begin() {
        return;
    }
    set_job(Job::ListingDisplays);
    let started = spawn("allblack11-wallpaper-displays", || {
        let _com = ComScope::enter();
        let mut list = Vec::new();
        let message = match Wallpaper::create() {
            Ok(service) => {
                for (index, id) in service.monitor_ids().into_iter().enumerate() {
                    if let Some(rect) = service.monitor_rect(&id) {
                        list.push(Display {
                            id,
                            label: format!(
                                "Display {} ({}×{})",
                                index + 1,
                                rect.right - rect.left,
                                rect.bottom - rect.top
                            ),
                        });
                    }
                }
                String::new()
            }
            Err(message) => message,
        };
        *DISPLAYS.lock().unwrap_or_else(|e| e.into_inner()) = list;
        if message.is_empty() {
            set_job(Job::Idle);
            BUSY.store(false, Ordering::Relaxed);
        } else {
            finish(message);
        }
    });
    if !started {
        finish("Could not read the displays.".into());
    }
}

/// Apply a downloaded wallpaper to one display (`monitor` is its Windows identifier) or all.
/// What the displays showed before is saved first so it can be put back.
pub fn apply(entry: &Entry, monitor: Option<String>, fit: Fit) {
    if !downloaded(entry) {
        finish("Download this wallpaper first.".into());
        return;
    }
    if !begin() {
        return;
    }
    set_job(Job::Applying {
        id: entry.id.clone(),
    });
    let entry = entry.clone();
    let started = spawn("allblack11-wallpaper-apply", move || {
        let path = image_path(&entry);
        // The file is checked again here: whatever is applied is exactly what was published.
        if hash_file(&path).map_or(true, |actual| actual != entry.sha256) {
            let _ = std::fs::remove_file(&path);
            finish(format!(
                "The saved {} image did not match its checksum, so it was deleted. Download it again.",
                entry.name
            ));
            return;
        }
        let _com = ComScope::enter();
        let result = (|| -> Result<(), String> {
            let service = Wallpaper::create()?;
            let targets: Vec<String> = match &monitor {
                Some(id) => vec![id.clone()],
                None => service.monitor_ids(),
            };
            // What is there now, per display, before anything changes.
            let before_position = service.position().unwrap_or(Fit::Fill.position());
            let before: Vec<Previous> = targets
                .iter()
                .map(|id| Previous {
                    monitor: id.clone(),
                    path: service.wallpaper_of(Some(id)),
                    position: before_position,
                })
                .collect();
            let before_background = service.background();
            service.set_position(fit.position())?;
            let mut black = false;
            if fit != Fit::Fill {
                // Bars around the picture are black, so a black wallpaper stays one piece.
                service.set_background(0);
                black = true;
            }
            match &monitor {
                Some(id) => service.set_wallpaper(Some(id), &path.to_string_lossy())?,
                None => service.set_wallpaper(None, &path.to_string_lossy())?,
            }
            let mut state = load_state();
            state.set_fit(&entry.id, fit);
            state.remember_apply(
                Applied {
                    id: entry.id.clone(),
                    monitor: monitor.clone().unwrap_or_default(),
                    fit: Some(fit),
                },
                before,
                if black { before_background } else { None },
            );
            save_state(&state);
            Ok(())
        })();
        finish(match result {
            Ok(()) => format!(
                "{} is now your wallpaper. Windows uses one fit for all displays.",
                entry.name
            ),
            Err(message) => {
                log_line("warn", &format!("wallpaper apply failed: {message}"));
                message
            }
        });
    });
    if !started {
        finish("Could not start applying the wallpaper.".into());
    }
}

/// Put back what the displays showed before the last apply (only pictures can be put back).
pub fn put_back() {
    let state = load_state();
    if !state.can_put_back() {
        finish("There is no earlier picture to put back. It may have been a solid color or a slideshow.".into());
        return;
    }
    if !begin() {
        return;
    }
    set_job(Job::PuttingBack);
    let started = spawn("allblack11-wallpaper-restore", move || {
        let _com = ComScope::enter();
        let result = (|| -> Result<usize, String> {
            let service = Wallpaper::create()?;
            let mut restored = 0;
            if let Some(first) = state.previous.iter().find(|p| !p.path.is_empty()) {
                if let Some(position) = Fit::from_position(first.position) {
                    service.set_position(position.position())?;
                }
            }
            for previous in state.previous.iter().filter(|p| !p.path.is_empty()) {
                if !std::path::Path::new(&previous.path).is_file() {
                    continue;
                }
                let monitor = (!previous.monitor.is_empty()).then_some(previous.monitor.as_str());
                service.set_wallpaper(monitor, &previous.path)?;
                restored += 1;
            }
            if let Some(color) = state.previous_background {
                service.set_background(color);
            }
            Ok(restored)
        })();
        finish(match result {
            Ok(0) => "The earlier picture is no longer on this PC, so nothing was changed.".into(),
            Ok(n) => format!("Put back the earlier wallpaper on {n} display(s)."),
            Err(message) => message,
        });
        let mut cleared = load_state();
        cleared.previous.clear();
        cleared.previous_background = None;
        cleared.applied = None;
        save_state(&cleared);
    });
    if !started {
        finish("Could not start putting the wallpaper back.".into());
    }
}

/// Show the file in Explorer, or the folder when it has not been downloaded.
pub fn open_location(entry: &Entry) -> Result<(), String> {
    let file = image_path(entry);
    let (target, args) = if file.is_file() {
        (
            "explorer.exe".to_string(),
            format!("/select,\"{}\"", file.to_string_lossy()),
        )
    } else {
        std::fs::create_dir_all(images_dir()).map_err(|e| e.to_string())?;
        (
            "explorer.exe".to_string(),
            format!("\"{}\"", images_dir().to_string_lossy()),
        )
    };
    let verb = to_wide("open");
    let file = to_wide(&target);
    let params = to_wide(&args);
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut() as HWND,
            pcw(&verb),
            pcw(&file),
            pcw(&params),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    } as isize;
    if result > 32 {
        Ok(())
    } else {
        Err("Windows could not open the folder.".into())
    }
}
