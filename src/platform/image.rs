//! Reading a few pixels from a picture, for "Palette from a picture".
//!
//! Local and bounded: the file size and the image dimensions are checked before any pixel is
//! read, and at most 64 x 64 points are sampled with GDI+ (which ships with Windows). Nothing
//! is stored, uploaded or kept after the call. GDI+ is started and stopped inside this
//! function, so nothing stays loaded.
//!
//! Evidence: cross-compiled only. Never run on Windows hardware.

use windows_sys::Win32::Graphics::GdiPlus::{
    GdipBitmapGetPixel, GdipCreateBitmapFromFile, GdipDisposeImage, GdipGetImageHeight,
    GdipGetImageWidth, GdiplusShutdown, GdiplusStartup, GdiplusStartupInput, GpBitmap, GpImage,
};

use super::wide::{pcw, to_wide};

/// Sample colors (COLORREF) from the picture at `path`. Pixels that are mostly transparent
/// are skipped.
pub fn sample(path: &str) -> Result<Vec<u32>, String> {
    let meta =
        std::fs::metadata(path).map_err(|err| format!("Could not open the picture: {err}"))?;
    if !meta.is_file() {
        return Err("That is not a file.".into());
    }
    if meta.len() > crate::IMAGE_MAX_BYTES {
        // Rejected before any decoder sees it.
        crate::check_image(1, 1, meta.len())?;
    }
    unsafe {
        let mut token = 0usize;
        let input = GdiplusStartupInput {
            GdiplusVersion: 1,
            DebugEventCallback: 0,
            SuppressBackgroundThread: 0,
            SuppressExternalCodecs: 0,
        };
        if GdiplusStartup(&mut token, &input, std::ptr::null_mut()) != 0 {
            return Err("Windows picture support could not start.".into());
        }
        let result = read_pixels(path, meta.len());
        GdiplusShutdown(token);
        result
    }
}

unsafe fn read_pixels(path: &str, file_bytes: u64) -> Result<Vec<u32>, String> {
    let wide = to_wide(path);
    let mut bitmap: *mut GpBitmap = std::ptr::null_mut();
    if GdipCreateBitmapFromFile(pcw(&wide), &mut bitmap) != 0 || bitmap.is_null() {
        return Err("Windows could not read that picture. Try a PNG or JPEG.".into());
    }
    let image = bitmap as *mut GpImage;
    let (mut width, mut height) = (0u32, 0u32);
    GdipGetImageWidth(image, &mut width);
    GdipGetImageHeight(image, &mut height);
    if let Err(err) = crate::check_image(width, height, file_bytes) {
        GdipDisposeImage(image);
        return Err(err);
    }
    let mut samples = Vec::new();
    for (x, y) in crate::sample_points(width, height) {
        let mut argb = 0u32;
        if GdipBitmapGetPixel(bitmap, x as i32, y as i32, &mut argb) != 0 {
            continue;
        }
        if (argb >> 24) < 128 {
            continue;
        }
        let (r, g, b) = ((argb >> 16) & 0xFF, (argb >> 8) & 0xFF, argb & 0xFF);
        samples.push(r | (g << 8) | (b << 16));
    }
    GdipDisposeImage(image);
    Ok(samples)
}
