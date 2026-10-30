use std::ptr::null_mut;

use windows_sys::core::PCWSTR;
use windows_sys::Win32::Foundation::HINSTANCE;
use windows_sys::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS,
};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, GetSystemMetrics, LoadImageW, HICON, ICONINFO, IMAGE_ICON, SM_CXSMICON,
};

use super::wide::{pcw, to_wide};

type LoadIconWithScaleDown =
    unsafe extern "system" fn(HINSTANCE, PCWSTR, i32, i32, *mut HICON) -> i32;

pub fn load_tray_icon(instance: HINSTANCE) -> HICON {
    let size = unsafe { GetSystemMetrics(SM_CXSMICON) }.clamp(16, 256);
    if let Some(icon) = load_scaled(instance, size) {
        return icon;
    }
    let loaded = unsafe { LoadImageW(instance, 1 as PCWSTR, IMAGE_ICON, size, size, 0) };
    if !loaded.is_null() {
        return loaded as HICON;
    }
    tray_icon(true)
}

fn load_scaled(instance: HINSTANCE, size: i32) -> Option<HICON> {
    let module_name = to_wide("comctl32.dll");
    let module = unsafe { LoadLibraryW(pcw(&module_name)) };
    if module.is_null() {
        return None;
    }
    let proc_addr = unsafe { GetProcAddress(module, b"LoadIconWithScaleDown\0".as_ptr())? };
    let load: LoadIconWithScaleDown = unsafe { std::mem::transmute(proc_addr as usize) };
    let mut icon: HICON = std::ptr::null_mut();
    let hr = unsafe { load(instance, 1 as PCWSTR, size, size, &mut icon) };
    if hr >= 0 && !icon.is_null() {
        Some(icon)
    } else {
        None
    }
}

pub fn tray_icon(enabled: bool) -> HICON {
    let metric = unsafe { GetSystemMetrics(SM_CXSMICON) };
    let size = metric.clamp(16, 64);
    let pixels = render(size as u32, enabled);
    unsafe { icon_from_bgra(size, &pixels) }
}

fn render(size: u32, enabled: bool) -> Vec<u8> {
    let border = (size as f32 * 0.12).max(1.0);
    let radius = size as f32 * 0.30;
    let (fill_r, fill_g, fill_b, edge_r, edge_g, edge_b) = if enabled {
        (8, 8, 8, 0x01, 0x85, 0x74)
    } else {
        (48, 48, 48, 120, 120, 120)
    };
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let outer = coverage(x, y, size, radius, 0.0);
            let inner = coverage(x, y, size, (radius - border).max(0.0), border);
            let edge = (outer - inner).max(0.0);
            let mut red = edge_r as f32 * edge + fill_r as f32 * inner;
            let mut green = edge_g as f32 * edge + fill_g as f32 * inner;
            let mut blue = edge_b as f32 * edge + fill_b as f32 * inner;
            let mut alpha = outer;
            if enabled && dash_contains(x, y, size) && inner > 0.6 {
                let mix = 0.95;
                red = red * (1.0 - mix) + 0x01 as f32 * mix;
                green = green * (1.0 - mix) + 0x85 as f32 * mix;
                blue = blue * (1.0 - mix) + 0x74 as f32 * mix;
                alpha = alpha.max(inner);
            }
            let offset = ((y * size + x) * 4) as usize;
            pixels[offset] = (blue * alpha) as u8;
            pixels[offset + 1] = (green * alpha) as u8;
            pixels[offset + 2] = (red * alpha) as u8;
            pixels[offset + 3] = (alpha * 255.0) as u8;
        }
    }
    pixels
}

fn dash_contains(x: u32, y: u32, size: u32) -> bool {
    let left = (size as f32 * 0.28) as u32;
    let right = (size as f32 * 0.72) as u32;
    let top = (size as f32 * 0.30) as u32;
    let thickness = (size / 8).max(1);
    x >= left && x < right && y >= top && y < top + thickness
}

fn coverage(x: u32, y: u32, size: u32, radius: f32, inset: f32) -> f32 {
    let mut hits = 0.0;
    for sy in 0..2 {
        for sx in 0..2 {
            let px = x as f32 + (sx as f32 + 0.5) / 2.0;
            let py = y as f32 + (sy as f32 + 0.5) / 2.0;
            if inside(px, py, size as f32, radius, inset) {
                hits += 1.0;
            }
        }
    }
    hits / 4.0
}

fn inside(px: f32, py: f32, size: f32, radius: f32, inset: f32) -> bool {
    let left = inset;
    let top = inset;
    let right = size - inset;
    let bottom = size - inset;
    if px < left || py < top || px >= right || py >= bottom {
        return false;
    }
    let radius = radius.min((right - left) * 0.5).max(0.0);
    let cx = if px < left + radius {
        left + radius
    } else if px > right - radius {
        right - radius
    } else {
        px
    };
    let cy = if py < top + radius {
        top + radius
    } else if py > bottom - radius {
        bottom - radius
    } else {
        py
    };
    let dx = px - cx;
    let dy = py - cy;
    dx * dx + dy * dy <= radius * radius
}

unsafe fn icon_from_bgra(size: i32, pixels: &[u8]) -> HICON {
    let mut info: BITMAPINFO = std::mem::zeroed();
    info.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: size,
        biHeight: -size,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        biSizeImage: (size * size * 4) as u32,
        biXPelsPerMeter: 0,
        biYPelsPerMeter: 0,
        biClrUsed: 0,
        biClrImportant: 0,
    };
    let mut bits = null_mut();
    let color = CreateDIBSection(null_mut(), &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    if color.is_null() || bits.is_null() {
        return null_mut();
    }
    std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u8, pixels.len());
    let mask = CreateBitmap(size, size, 1, 1, null_mut());
    let icon_info = ICONINFO {
        fIcon: 1,
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask,
        hbmColor: color,
    };
    let icon = CreateIconIndirect(&icon_info);
    if !color.is_null() {
        DeleteObject(color);
    }
    if !mask.is_null() {
        DeleteObject(mask);
    }
    icon
}
