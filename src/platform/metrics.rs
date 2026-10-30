//! Per-user `CaptionHeight`, backed up before the first change and restored on demand.
//! Writing the registry value does not resize open title bars; Windows applies it at sign-in.

use serde::{Deserialize, Serialize};
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::Graphics::Gdi::{GetDC, GetDeviceCaps, ReleaseDC, LOGPIXELSY};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ, RRF_RT_REG_SZ,
};

use crate::caption::{caption_twips_to_pixels, pixels_to_caption_twips};
use crate::config::normalize_caption_height;

use super::wide::{config_dir, pcw, to_wide};

const METRICS_KEY: &str = "Control Panel\\Desktop\\WindowMetrics";
const CAPTION: &str = "CaptionHeight";

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MetricsBackup {
    present: bool,
    #[serde(default)]
    caption_height: String,
}

pub fn apply_caption_height(pixels: Option<u32>) -> Result<(), String> {
    match normalize_caption_height(pixels) {
        None => restore_caption_height(),
        Some(pixels) => {
            ensure_backup()?;
            let twips = pixels_to_caption_twips(pixels, system_dpi());
            write_caption(&twips.to_string())
        }
    }
}

pub fn restore_caption_height() -> Result<(), String> {
    let Some(backup) = read_backup()? else {
        return Ok(());
    };
    if backup.present {
        write_caption(&backup.caption_height)
    } else {
        delete_caption()
    }
}

#[allow(dead_code)]
pub fn current_caption_pixels() -> Option<u32> {
    let text = read_caption().ok()??;
    let twips: i32 = text.trim().parse().ok()?;
    let pixels = caption_twips_to_pixels(twips, system_dpi());
    if pixels == 0 {
        None
    } else {
        Some(pixels)
    }
}

fn ensure_backup() -> Result<(), String> {
    if backup_path().exists() {
        return Ok(());
    }
    let backup = match read_caption()? {
        Some(value) => MetricsBackup {
            present: true,
            caption_height: value,
        },
        None => MetricsBackup {
            present: false,
            caption_height: String::new(),
        },
    };
    write_backup(&backup)
}

fn read_backup() -> Result<Option<MetricsBackup>, String> {
    let path = backup_path();
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path).map_err(|err| err.to_string())?;
    serde_json::from_str(&text).map(Some).map_err(|err| err.to_string())
}

fn write_backup(backup: &MetricsBackup) -> Result<(), String> {
    let path = backup_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let text = serde_json::to_string_pretty(backup).map_err(|err| err.to_string())?;
    std::fs::write(path, format!("{text}\n")).map_err(|err| err.to_string())
}

fn backup_path() -> std::path::PathBuf {
    config_dir().join("metrics-backup.json")
}

fn read_caption() -> Result<Option<String>, String> {
    let subkey = to_wide(METRICS_KEY);
    let value = to_wide(CAPTION);
    let mut buf = [0u16; 32];
    let mut size = (buf.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            pcw(&subkey),
            pcw(&value),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr() as *mut _,
            &mut size,
        )
    };
    if status == 2 {
        return Ok(None);
    }
    if status != ERROR_SUCCESS {
        return Err(format!("could not read CaptionHeight ({status})"));
    }
    let chars = (size as usize / 2).saturating_sub(1);
    Ok(Some(String::from_utf16_lossy(&buf[..chars.min(buf.len())]).trim_end_matches('\0').to_string()))
}

fn write_caption(text: &str) -> Result<(), String> {
    with_metrics_key(|key| {
        let name = to_wide(CAPTION);
        let wide = to_wide(text);
        let bytes = unsafe { std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2) };
        let status = unsafe {
            RegSetValueExW(key, pcw(&name), 0, REG_SZ, bytes.as_ptr(), bytes.len() as u32)
        };
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(format!("could not write CaptionHeight ({status})"))
        }
    })
}

fn delete_caption() -> Result<(), String> {
    with_metrics_key(|key| {
        let name = to_wide(CAPTION);
        let status = unsafe { RegDeleteValueW(key, pcw(&name)) };
        if status == ERROR_SUCCESS || status == 2 {
            Ok(())
        } else {
            Err(format!("could not remove CaptionHeight ({status})"))
        }
    })
}

fn with_metrics_key(body: impl FnOnce(HKEY) -> Result<(), String>) -> Result<(), String> {
    let subkey = to_wide(METRICS_KEY);
    let mut key = std::ptr::null_mut();
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            pcw(&subkey),
            0,
            KEY_QUERY_VALUE | KEY_SET_VALUE,
            &mut key,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(format!("could not open WindowMetrics ({status})"));
    }
    let result = body(key);
    unsafe { RegCloseKey(key) };
    result
}

fn system_dpi() -> u32 {
    unsafe {
        let dc = GetDC(std::ptr::null_mut());
        if dc.is_null() {
            return 96;
        }
        let dpi = GetDeviceCaps(dc, LOGPIXELSY as i32);
        ReleaseDC(std::ptr::null_mut(), dc);
        if dpi <= 0 {
            96
        } else {
            dpi as u32
        }
    }
}
