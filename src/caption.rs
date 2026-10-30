//! `HKCU\Control Panel\Desktop\WindowMetrics\CaptionHeight` is a negative twip count.
//! Pixels and twips meet at 1440 twips per inch.

/// Negative twips Windows stores for a caption that is `pixels` tall at `dpi`.
pub fn pixels_to_caption_twips(pixels: u32, dpi: u32) -> i32 {
    let dpi = dpi.max(1);
    let twips = (i64::from(pixels) * 1440) / i64::from(dpi);
    -(twips.min(i64::from(i32::MAX)) as i32)
}

/// Pixel height of a stored CaptionHeight value. Positive or zero twips are not a height.
#[cfg_attr(not(test), allow(dead_code))]
pub fn caption_twips_to_pixels(twips: i32, dpi: u32) -> u32 {
    if twips >= 0 {
        return 0;
    }
    let dpi = dpi.max(1);
    let pixels = (i64::from(-twips) * i64::from(dpi)) / 1440;
    pixels.max(0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_96dpi_caption_is_minus_330_twips() {
        assert_eq!(pixels_to_caption_twips(22, 96), -330);
        assert_eq!(caption_twips_to_pixels(-330, 96), 22);
    }

    #[test]
    fn round_trip_at_a_higher_dpi() {
        let twips = pixels_to_caption_twips(33, 144);
        assert_eq!(twips, -330);
        assert_eq!(caption_twips_to_pixels(twips, 144), 33);
    }

    #[test]
    fn non_negative_twips_are_not_a_height() {
        assert_eq!(caption_twips_to_pixels(0, 96), 0);
        assert_eq!(caption_twips_to_pixels(15, 96), 0);
    }
}
