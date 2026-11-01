//! Shared control rendering for AllBlack11's own windows: one button style with every state,
//! one pointer tracker, one focus ring. Pages call these instead of drawing rounded
//! rectangles themselves, so hover, pressed, focus and disabled look the same everywhere.
//!
//! Child module of `settings` (it needs `Canvas` and the fonts). Evidence: cross-compiled and
//! rendered under Wine for layout. Hover/pressed feel and focus visibility on Windows are
//! unverified.

use std::cell::{Cell, RefCell};

use super::*;

pub type Rect = (i32, i32, i32, i32);

/// Where the pointer is, in the same content coordinates the rows are painted in, and which
/// interactive rectangles were drawn last frame. Interior mutability because painting takes
/// `&Ui`.
#[derive(Default)]
pub struct Pointer {
    pos: Cell<Option<(i32, i32)>>,
    down: Cell<bool>,
    /// Rectangles of interactive controls from the last paint.
    rects: RefCell<Vec<Rect>>,
    /// The one under the pointer when it last moved.
    hover: Cell<Option<Rect>>,
}

impl Pointer {
    pub fn set(&self, pos: Option<(i32, i32)>) {
        self.pos.set(pos);
    }

    pub fn press(&self, down: bool) {
        self.down.set(down);
    }

    /// Call at the start of a paint.
    pub fn begin_frame(&self) {
        self.rects.borrow_mut().clear();
    }

    /// Recompute which control is under the pointer. True when that changed (repaint needed).
    pub fn update_hover(&self) -> bool {
        let now = self.pos.get().and_then(|(x, y)| {
            self.rects
                .borrow()
                .iter()
                .copied()
                .find(|(l, t, r, b)| x >= *l && x < *r && y >= *t && y < *b)
        });
        let changed = now != self.hover.get();
        self.hover.set(now);
        changed
    }

    fn register(&self, rect: Rect) {
        self.rects.borrow_mut().push(rect);
    }

    pub fn is_hot(&self, rect: Rect) -> bool {
        self.hover.get() == Some(rect)
    }

    pub fn is_pressed(&self, rect: Rect) -> bool {
        self.down.get() && self.is_hot(rect)
    }
}

/// Register an interactive rectangle that is not a button (a text link) and say whether the
/// pointer is over it, so hover feels the same everywhere.
pub fn hot(ui: &Ui, rect: Rect) -> bool {
    ui.pointer.register(rect);
    ui.pointer.is_hot(rect)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Emphasis {
    /// A normal button.
    Standard,
    /// The one main action of a group: filled with the accent color.
    Primary,
}

/// Which buttons are the main action where they appear. At most one per group in practice.
pub fn emphasis_of(id: Button) -> Emphasis {
    match id {
        Button::GoFix | Button::PickWindow | Button::TryScene | Button::TryLook => {
            Emphasis::Primary
        }
        _ => Emphasis::Standard,
    }
}

#[derive(Clone, Copy, Default)]
pub struct State {
    pub focused: bool,
    pub disabled: bool,
    /// Something is happening because of this control (the picker is waiting for a drop).
    pub active: bool,
}

/// A button with a text label: standard or primary, and every state. `rect` is in content
/// coordinates. The label keeps the same words in every state; state is shown by fill, edge
/// and text together, so no state depends on color alone.
pub fn draw_button(c: &Canvas, ui: &Ui, rect: Rect, label: &str, emphasis: Emphasis, state: State) {
    let theme = th();
    let (l, t, r, b) = rect;
    if !state.disabled {
        ui.pointer.register(rect);
    }
    let hot = !state.disabled && ui.pointer.is_hot(rect);
    let pressed = !state.disabled && ui.pointer.is_pressed(rect);
    let (accent, on_accent) = theme_accent();
    let high_contrast = uistyle::mode() == crate::UiMode::HighContrast;
    let (fill, edge, text) = match (emphasis, state.disabled) {
        (_, true) => (theme.card, theme.edge, theme.disabled),
        (Emphasis::Primary, false) => {
            let fill = if pressed {
                crate::blend_colors(accent, theme.bg, 0.25)
            } else if hot {
                crate::blend_colors(accent, theme.text, 0.12)
            } else {
                accent
            };
            (fill, fill, on_accent)
        }
        (Emphasis::Standard, false) => {
            let fill = if pressed {
                crate::blend_colors(theme.card_hi, theme.bg, 0.3)
            } else if hot {
                crate::blend_colors(theme.card_hi, theme.text, 0.08)
            } else {
                theme.card_hi
            };
            let edge = if state.active || hot {
                theme.control_edge
            } else {
                theme.edge
            };
            (fill, edge, theme.text)
        }
    };
    // In high contrast the system decides: a highlighted fill with its own text color.
    let (fill, text) = if high_contrast && (hot || pressed) && !state.disabled {
        (theme.card_hi, theme.text)
    } else {
        (fill, text)
    };
    c.round(
        l,
        t,
        r,
        b,
        crate::ui_metrics::CONTROL_RADIUS,
        Some(fill),
        Some(edge),
    );
    c.text(
        l + 4,
        t,
        r - 4,
        b,
        label,
        text,
        ui.fonts.body,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
    );
    if state.focused {
        focus_ring(c, rect);
    }
}

/// The keyboard focus ring: two pixels, outside the control, in the theme's focus color,
/// which is 3:1 against every surface (tested in `uitheme`).
pub fn focus_ring(c: &Canvas, rect: Rect) {
    let (l, t, r, b) = rect;
    let color = th().focus;
    let radius = crate::ui_metrics::CONTROL_RADIUS + 1;
    c.round(l - 2, t - 2, r + 2, b + 2, radius, None, Some(color));
    c.round(l - 1, t - 1, r + 1, b + 1, radius, None, Some(color));
}
