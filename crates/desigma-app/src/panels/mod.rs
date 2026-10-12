pub mod assets;
pub mod components;
pub mod context;
pub mod effects;
pub mod export;
pub mod inspect;
pub mod layers;
pub mod layoutguide;
pub mod left;
pub mod menus;
pub mod properties;
pub mod proto_actions;
pub mod prototype;
pub mod rail;
pub mod right;
pub mod stroke;
pub mod toolbar;
pub mod typography;
pub mod var_bind;
pub mod vertex;

use crate::theme;
use egui::{Rect, Ui};

/// Runs `f` in a child ui clipped to `rect` (so scrolled content never bleeds).
pub fn clipped<R>(ui: &mut Ui, rect: Rect, f: impl FnOnce(&mut Ui) -> R) -> R {
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
        ui.set_clip_rect(ui.clip_rect().intersect(rect));
        f(ui)
    })
    .inner
}

/// Draws a thin scrollbar thumb on the right edge of `rect`.
pub fn scrollbar(ui: &Ui, rect: Rect, scroll: f32, content_h: f32) {
    if content_h <= rect.height() + 0.5 {
        return;
    }
    let track = rect.height();
    let thumb_h = (track * rect.height() / content_h).max(24.0);
    let max_scroll = content_h - rect.height();
    let t = if max_scroll > 0.0 { scroll / max_scroll } else { 0.0 };
    let y = rect.top() + (track - thumb_h) * t;
    let r = Rect::from_min_size(egui::pos2(rect.right() - 6.0, y), egui::vec2(4.0, thumb_h));
    ui.painter().rect_filled(r, theme::cr(2), theme::TEXT_TERTIARY);
}

/// Applies wheel scrolling when the pointer is over `rect`.
pub fn wheel_scroll(ui: &Ui, rect: Rect, scroll: &mut f32, content_h: f32) {
    let max_scroll = (content_h - rect.height()).max(0.0);
    if ui.rect_contains_pointer(rect) {
        let dy = ui.input(|i| i.smooth_scroll_delta.y);
        *scroll -= dy;
    }
    *scroll = scroll.clamp(0.0, max_scroll);
}
