//! Custom-painted widgets that replace stock egui looks.

/// Anything usable as an egui id salt.
pub trait Key: std::hash::Hash + std::fmt::Debug {}
impl<T: std::hash::Hash + std::fmt::Debug> Key for T {}

pub mod field;
pub mod menu;

use crate::icons::{self, Icon};
use crate::theme;
use egui::{Align2, Color32, CornerRadius, FontId, Id, Painter, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

/// Width of a string in the given font.
pub fn text_width(p: &Painter, s: &str, font: &FontId) -> f32 {
    p.layout_no_wrap(s.to_string(), font.clone(), Color32::WHITE).size().x
}

/// Shortens `s` with an ellipsis so that it fits `max_w`.
pub fn elide(p: &Painter, s: &str, font: &FontId, max_w: f32) -> String {
    if text_width(p, s, font) <= max_w {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let cand: String = chars[..mid].iter().collect::<String>() + "…";
        if text_width(p, &cand, font) <= max_w {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    chars[..lo].iter().collect::<String>() + "…"
}

pub fn hline(p: &Painter, x0: f32, x1: f32, y: f32, color: Color32) {
    p.rect_filled(Rect::from_min_max(Pos2::new(x0, y), Pos2::new(x1, y + theme::BORDER_WIDTH)), 0.0, color);
}

pub fn vline(p: &Painter, x: f32, y0: f32, y1: f32, color: Color32) {
    p.rect_filled(Rect::from_min_max(Pos2::new(x, y0), Pos2::new(x + theme::BORDER_WIDTH, y1)), 0.0, color);
}

/// Layered soft shadow (approximates a CSS box-shadow with blur).
pub fn shadow(p: &Painter, rect: Rect, radius: u8, offset: Vec2, blur: f32, color: Color32) {
    let steps = (blur / 1.5).ceil().max(1.0) as i32;
    for i in (0..steps).rev() {
        let t = i as f32 / steps as f32;
        let grow = blur * t;
        let a = (color.a() as f32 * (1.0 - t) * (1.0 - t) / steps as f32 * 2.0).clamp(0.0, 255.0) as u8;
        let c = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a);
        p.rect_filled(rect.translate(offset).expand(grow), CornerRadius::same(radius.saturating_add(grow as u8)), c);
    }
}

/// Elevation 200 style shadow stack used by menus/popovers.
pub fn elevation_shadow(p: &Painter, rect: Rect, radius: u8) {
    shadow(p, rect, radius, Vec2::new(0.0, 3.0), 8.0, theme::SHADOW_COLOR);
    shadow(p, rect, radius, Vec2::new(0.0, 1.0), 3.0, theme::SHADOW_COLOR_STRONG);
}

/// Background for icon-like buttons: hover / pressed / selected states.
pub fn button_bg(p: &Painter, rect: Rect, r: &Response, selected: bool, radius: CornerRadius) {
    let fill = if selected {
        theme::BG_SELECTED
    } else if r.is_pointer_button_down_on() {
        theme::BG_PRESSED
    } else if r.hovered() {
        theme::BG_HOVER
    } else {
        Color32::TRANSPARENT
    };
    if fill != Color32::TRANSPARENT {
        p.rect_filled(rect, radius, fill);
    }
}

/// Square icon button used throughout the panels (24px by default).
pub fn icon_button(ui: &mut Ui, rect: Rect, id: impl crate::widgets::Key, icon: Icon, selected: bool, tip: &str) -> Response {
    let r = ui.interact(rect, ui.id().with(id), Sense::click());
    let p = ui.painter();
    button_bg(p, rect, &r, selected, theme::cr(theme::RADIUS_MEDIUM));
    let col = if selected { theme::ICON_SELECTED } else if r.hovered() { theme::ICON } else { theme::ICON_SECONDARY };
    icons::paint(p, icon, rect.center(), 16.0, col);
    if !tip.is_empty() {
        return r.on_hover_text(tip);
    }
    r
}

/// Pill-shaped small tag (e.g. the "Free" plan badge).
pub fn badge(p: &Painter, pos: Pos2, s: &str) -> Rect {
    let font = theme::font_small_strong();
    let w = text_width(p, s, &font) + theme::BADGE_PAD_X * 2.0;
    let r = Rect::from_min_size(pos, Vec2::new(w, theme::BADGE_HEIGHT));
    p.rect_filled(r, theme::cr(theme::RADIUS_SMALL), theme::BADGE_BG);
    p.text(r.center(), Align2::CENTER_CENTER, s, font, theme::TEXT_SECONDARY);
    r
}

/// Checkerboard-backed color swatch.
pub fn swatch(p: &Painter, rect: Rect, color: Color32) {
    let r = theme::cr(theme::SWATCH_RADIUS);
    p.rect_filled(rect, r, theme::CHECKER_A);
    if color.a() < 255 {
        let h = rect.height() / 2.0;
        p.rect_filled(Rect::from_min_size(rect.min + Vec2::new(0.0, 0.0), Vec2::new(h, h)), 0.0, theme::CHECKER_B);
        p.rect_filled(Rect::from_min_size(rect.min + Vec2::new(h, h), Vec2::new(h, h)), 0.0, theme::CHECKER_B);
    }
    p.rect_filled(rect, r, color);
    p.rect_stroke(rect, r, Stroke::new(1.0, theme::BORDER_TRANSLUCENT), StrokeKind::Inside);
}

pub fn fmt_num(v: f32) -> String {
    let s = format!("{:.2}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    if s == "-0" || s.is_empty() {
        "0".into()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_formatting() {
        assert_eq!(fmt_num(10.0), "10");
        assert_eq!(fmt_num(10.5), "10.5");
        assert_eq!(fmt_num(-0.001), "0");
        assert_eq!(fmt_num(1.239), "1.24");
        assert_eq!(fmt_num(0.0), "0");
    }
}
