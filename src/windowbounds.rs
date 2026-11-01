//! Where the Settings window opens: a landscape default, and the last position and size put back
//! safely when monitors, scaling or the screen layout have changed.
//!
//! Pure. The window code gives it the saved record and the monitors' work areas and applies the
//! answer; every rule that could strand a window off-screen is tested here.

use serde::{Deserialize, Serialize};

/// The landscape size to start with, in logical pixels (96 dpi, text scale 100%). A starting
/// point, clamped to the screen, never a fixed requirement.
pub const DEFAULT_SIZE: (i32, i32) = (1120, 760);
/// The smallest the window may be resized to, in logical pixels.
pub const MIN_SIZE: (i32, i32) = (720, 520);

/// What is saved. Position is in screen pixels; size in logical pixels so a different scaling on
/// the next start gives the same apparent size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    #[serde(default)]
    pub maximized: bool,
}

impl Bounds {
    /// A damaged or hand-edited file is "nothing saved", never an error.
    pub fn parse(text: &str) -> Option<Bounds> {
        let bounds: Bounds = serde_json::from_str(text).ok()?;
        // Absurd values mean a corrupt record.
        let sane = (-100_000..100_000).contains(&bounds.x)
            && (-100_000..100_000).contains(&bounds.y)
            && (200..=20_000).contains(&bounds.w)
            && (200..=20_000).contains(&bounds.h);
        sane.then_some(bounds)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default() + "\n"
    }
}

/// One monitor's usable area (screen pixels) and its scale (96 dpi = 1.0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorkArea {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub scale: f32,
}

impl WorkArea {
    fn width(&self) -> i32 {
        self.right - self.left
    }
    fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

/// The window rectangle in screen pixels, and whether to maximize it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub maximized: bool,
}

fn overlap(a: (i32, i32, i32, i32), area: &WorkArea) -> i64 {
    let w = (a.2.min(area.right) - a.0.max(area.left)).max(0) as i64;
    let h = (a.3.min(area.bottom) - a.1.max(area.top)).max(0) as i64;
    w * h
}

/// Size for an area: the logical size scaled to that monitor, kept inside it (a margin is left so
/// the frame is never flush with the edges) and never below the minimum when the screen allows.
fn size_in(area: &WorkArea, logical: (i32, i32)) -> (i32, i32) {
    let scale = area.scale.max(0.5);
    let min_w = (MIN_SIZE.0 as f32 * scale) as i32;
    let min_h = (MIN_SIZE.1 as f32 * scale) as i32;
    let max_w = (area.width() - 24).max(1);
    let max_h = (area.height() - 24).max(1);
    let w = ((logical.0 as f32 * scale) as i32).max(min_w).min(max_w);
    let h = ((logical.1 as f32 * scale) as i32).max(min_h).min(max_h);
    (w, h)
}

fn centered(area: &WorkArea, size: (i32, i32)) -> (i32, i32) {
    (
        area.left + (area.width() - size.0) / 2,
        area.top + (area.height() - size.1) / 2,
    )
}

/// Decide where the window goes. `areas` are the monitors now attached, the first being the one
/// to use when nothing better is known (the one with the pointer, or the primary).
pub fn restore(saved: Option<Bounds>, areas: &[WorkArea]) -> Option<Placement> {
    let first = areas.first()?;
    let fallback = || {
        let (w, h) = size_in(first, DEFAULT_SIZE);
        let (x, y) = centered(first, (w, h));
        Placement {
            x,
            y,
            w,
            h,
            maximized: false,
        }
    };
    let Some(saved) = saved else {
        return Some(fallback());
    };
    // The monitor that holds most of the saved window; the saved size is in logical pixels, so use
    // the scale of the monitor it was on to turn the position's extent back into pixels.
    let guess = (saved.x, saved.y, saved.x + saved.w, saved.y + saved.h);
    let best = areas
        .iter()
        .max_by_key(|area| overlap(guess, area))
        .filter(|area| overlap(guess, area) >= 120 * 60);
    let Some(area) = best else {
        // The monitor it was on is gone (or the window was far away): start in the default place.
        return Some(fallback());
    };
    let (w, h) = size_in(area, (saved.w, saved.h));
    // Keep the whole window on the work area, nudging rather than recentering: the person put it
    // there on purpose.
    let x = saved.x.clamp(area.left, (area.right - w).max(area.left));
    let y = saved.y.clamp(area.top, (area.bottom - h).max(area.top));
    Some(Placement {
        x,
        y,
        w,
        h,
        maximized: saved.maximized,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(left: i32, top: i32, w: i32, h: i32, scale: f32) -> WorkArea {
        WorkArea {
            left,
            top,
            right: left + w,
            bottom: top + h,
            scale,
        }
    }

    #[test]
    fn the_default_is_landscape_and_fits_a_normal_display() {
        let p = restore(None, &[area(0, 0, 1920, 1040, 1.0)]).unwrap();
        assert_eq!((p.w, p.h), (1120, 760));
        assert!(p.w > p.h, "landscape");
        assert_eq!(
            (p.x, p.y),
            ((1920 - 1120) / 2, (1040 - 760) / 2),
            "centered"
        );
        assert!(!p.maximized);
    }

    #[test]
    fn the_default_is_clamped_to_a_small_work_area() {
        let p = restore(None, &[area(0, 0, 1024, 728, 1.0)]).unwrap();
        assert!(p.w <= 1000 && p.h <= 704, "{p:?}");
        assert!(p.x >= 0 && p.y >= 0 && p.x + p.w <= 1024 && p.y + p.h <= 728);
        // Even a tiny screen gets a window that fits it.
        let tiny = restore(None, &[area(0, 0, 800, 560, 1.0)]).unwrap();
        assert!(tiny.w <= 776 && tiny.h <= 536);
    }

    #[test]
    fn the_default_scales_with_the_monitor() {
        let p = restore(None, &[area(0, 0, 3840, 2080, 2.0)]).unwrap();
        assert_eq!((p.w, p.h), (2240, 1520), "1120x760 logical at 200%");
        let p150 = restore(None, &[area(0, 0, 2560, 1400, 1.5)]).unwrap();
        assert_eq!((p150.w, p150.h), (1680, 1140));
    }

    #[test]
    fn a_saved_position_comes_back_exactly_when_it_still_fits() {
        let saved = Bounds {
            x: 200,
            y: 100,
            w: 1000,
            h: 700,
            maximized: false,
        };
        let p = restore(Some(saved), &[area(0, 0, 1920, 1040, 1.0)]).unwrap();
        assert_eq!((p.x, p.y, p.w, p.h), (200, 100, 1000, 700));
        let maximized = Bounds {
            maximized: true,
            ..saved
        };
        assert!(
            restore(Some(maximized), &[area(0, 0, 1920, 1040, 1.0)])
                .unwrap()
                .maximized
        );
    }

    #[test]
    fn a_disconnected_monitor_does_not_strand_the_window() {
        // Saved on a second monitor at x = 3000; only the first is attached now.
        let saved = Bounds {
            x: 3100,
            y: 80,
            w: 1100,
            h: 740,
            maximized: false,
        };
        let p = restore(Some(saved), &[area(0, 0, 1920, 1040, 1.0)]).unwrap();
        assert!(
            p.x >= 0 && p.x + p.w <= 1920 && p.y >= 0 && p.y + p.h <= 1040,
            "{p:?}"
        );
        // Saved at a negative position (a monitor to the left) with that monitor gone.
        let left = Bounds {
            x: -1800,
            y: 50,
            w: 1100,
            h: 740,
            maximized: false,
        };
        let q = restore(Some(left), &[area(0, 0, 1920, 1040, 1.0)]).unwrap();
        assert!(q.x >= 0 && q.x + q.w <= 1920);
    }

    #[test]
    fn a_window_partly_off_the_edge_is_pulled_back_in() {
        let saved = Bounds {
            x: 1500,
            y: 600,
            w: 1100,
            h: 740,
            maximized: false,
        };
        let p = restore(Some(saved), &[area(0, 0, 1920, 1040, 1.0)]).unwrap();
        assert!(p.x + p.w <= 1920 && p.y + p.h <= 1040, "{p:?}");
        assert!(p.x > 700, "nudged, not re-centered: {p:?}");
    }

    #[test]
    fn a_second_monitor_is_used_when_it_is_still_there() {
        let areas = [area(0, 0, 1920, 1040, 1.0), area(1920, 0, 2560, 1400, 1.5)];
        let saved = Bounds {
            x: 2200,
            y: 100,
            w: 1000,
            h: 700,
            maximized: false,
        };
        let p = restore(Some(saved), &areas).unwrap();
        assert!(p.x >= 1920, "stays on the second monitor: {p:?}");
        // Logical size is applied at that monitor's scale.
        assert_eq!((p.w, p.h), (1500, 1050));
    }

    #[test]
    fn a_saved_size_below_the_minimum_or_above_the_screen_is_corrected() {
        let small = Bounds {
            x: 10,
            y: 10,
            w: 300,
            h: 250,
            maximized: false,
        };
        let p = restore(Some(small), &[area(0, 0, 1920, 1040, 1.0)]).unwrap();
        assert!(p.w >= MIN_SIZE.0 && p.h >= MIN_SIZE.1, "{p:?}");
        let huge = Bounds {
            x: 0,
            y: 0,
            w: 5000,
            h: 4000,
            maximized: false,
        };
        let q = restore(Some(huge), &[area(0, 0, 1920, 1040, 1.0)]).unwrap();
        assert!(q.w <= 1896 && q.h <= 1016, "{q:?}");
    }

    #[test]
    fn a_damaged_record_is_nothing_saved() {
        assert_eq!(Bounds::parse("not json"), None);
        assert_eq!(Bounds::parse("{}"), None);
        assert_eq!(Bounds::parse(r#"{"x":0,"y":0,"w":10,"h":10}"#), None);
        assert_eq!(
            Bounds::parse(r#"{"x":999999999,"y":0,"w":1000,"h":700}"#),
            None
        );
        let ok = Bounds {
            x: -5,
            y: 7,
            w: 1100,
            h: 700,
            maximized: true,
        };
        assert_eq!(Bounds::parse(&ok.to_json()), Some(ok));
        assert!(restore(None, &[]).is_none(), "no monitors is not a crash");
    }
}
