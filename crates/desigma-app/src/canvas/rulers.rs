//! Rulers (Shift+R) and user guides.
//!
//! Two 20px strips on the top and left of the canvas show tick labels in page
//! units, relative to the origin of the selected frame (or 0). The extent of the
//! selection is highlighted. Dragging from a ruler creates a guide, a red line
//! that other layers snap to; dragging it back onto its ruler deletes it.
//!
//! Guides are kept per page in [`App::page_guides`]; the document model has no
//! field for them, so they are not saved with the file.

use super::snap::{self, GuideAxis};
use super::*;
use crate::theme;
use crate::util::*;
use egui::{Align2, Color32, Painter, Pos2, Stroke, Vec2 as EV};

/// A guide line in page coordinates: a vertical line at x or a horizontal one at y.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RulerGuide {
    pub axis: GuideAxis,
    pub pos: f32,
}

/// A guide being created or moved.
#[derive(Clone, Copy, Debug)]
pub struct GuideDrag {
    pub axis: GuideAxis,
    /// Index in the page's guides when an existing guide is dragged.
    pub index: Option<usize>,
    pub pos: f32,
    /// Whether the pointer currently sits on the ruler it came from (releasing there deletes).
    pub on_ruler: bool,
}

/// The ruler strips: top (without the corner) and left.
pub fn strips(rect: ERect) -> (ERect, ERect) {
    let s = theme::RULER_SIZE;
    (
        ERect::from_min_max(Pos2::new(rect.left() + s, rect.top()), Pos2::new(rect.right(), rect.top() + s)),
        ERect::from_min_max(Pos2::new(rect.left(), rect.top() + s), Pos2::new(rect.left() + s, rect.bottom())),
    )
}

/// Page position that reads as zero on the rulers: the origin of the top-level
/// frame holding the selection (when it is a single frame), otherwise (0, 0).
pub fn origin(app: &App) -> DV {
    let mut frame = None;
    for id in &app.selection {
        let mut cur = *id;
        let mut top = cur;
        while let Some(p) = app.doc.get(cur).and_then(|n| n.parent) {
            if p == app.page {
                break;
            }
            top = p;
            cur = p;
        }
        let top_level = app.doc.get(top).is_some_and(|n| n.parent == Some(app.page) && n.kind.frame_props().is_some());
        if !top_level {
            return DV::ZERO;
        }
        match frame {
            None => frame = Some(top),
            Some(f) if f == top => {}
            _ => return DV::ZERO,
        }
    }
    frame.map(|f| app.doc.world_transform(f).apply(DV::ZERO)).unwrap_or(DV::ZERO)
}

/// Major tick step in page units: 1, 2, 5, 10, 20, 50, ... such that ticks are
/// at least ~50 screen px apart.
pub fn tick_step(zoom: f32) -> f32 {
    let mut mag = 1.0f32;
    loop {
        for m in [1.0, 2.0, 5.0] {
            let s = mag * m;
            if s * zoom >= 50.0 {
                return s;
            }
        }
        mag *= 10.0;
        if mag > 1.0e9 {
            return mag;
        }
    }
}

/// Tick positions (value relative to the origin, level 0 = labelled major, 1 = half, 2 = minor)
/// covering `lo..=hi` (relative values).
pub fn ticks(lo: f32, hi: f32, zoom: f32) -> Vec<(f32, u8)> {
    let step = tick_step(zoom);
    let minor = if step * zoom / 10.0 >= 6.0 {
        step / 10.0
    } else if step * zoom / 5.0 >= 6.0 {
        step / 5.0
    } else {
        step
    };
    let n_lo = (lo / minor).floor() as i64 - 1;
    let n_hi = (hi / minor).ceil() as i64 + 1;
    let per = (step / minor).round() as i64;
    let mut out = Vec::new();
    for n in n_lo..=n_hi {
        let v = n as f32 * minor;
        let level = if n.rem_euclid(per.max(1)) == 0 {
            0
        } else if per % 2 == 0 && n.rem_euclid(per) == per / 2 {
            1
        } else {
            2
        };
        out.push((v, level));
    }
    out
}

fn fmt_value(v: f32) -> String {
    if (v - v.round()).abs() < 0.005 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.1}")
    }
}

fn crisp(v: f32) -> f32 {
    v.round() + 0.5
}

fn rotated_text(p: &Painter, pos: Pos2, text: &str, color: Color32) {
    let galley = p.layout_no_wrap(text.to_string(), theme::font_ruler(), color);
    p.add(egui::epaint::TextShape::new(pos, galley, color).with_angle(-std::f32::consts::FRAC_PI_2));
}

fn guides_of(app: &App) -> &[RulerGuide] {
    app.page_guides.get(&app.page).map(|v| v.as_slice()).unwrap_or(&[])
}

/// Guides as snap targets (see [`snap::guide_rect`]).
pub fn guide_rects(app: &App) -> Vec<Rect> {
    if !app.ui.rulers {
        return vec![];
    }
    guides_of(app).iter().map(|g| snap::guide_rect(g.axis, g.pos)).collect()
}

fn page_pos(app: &App, rect: ERect, pos: Pos2, axis: GuideAxis) -> f32 {
    let p = app.view.to_page(rect.min, pos);
    let v = match axis {
        GuideAxis::X => p.x,
        GuideAxis::Y => p.y,
    };
    if app.ui.no_pixel_snap {
        v
    } else {
        v.round()
    }
}

/// Starts a guide drag when the press hits a ruler (new guide) or, with the move
/// tool, an existing guide. Returns true when the press was consumed.
pub fn press(app: &mut App, rect: ERect, pos: Pos2) -> bool {
    if !app.ui.rulers || app.canvas.text.is_some() {
        return false;
    }
    let (top, left) = strips(rect);
    let new = if top.contains(pos) {
        Some(GuideAxis::Y)
    } else if left.contains(pos) {
        Some(GuideAxis::X)
    } else {
        None
    };
    if let Some(axis) = new {
        let v = page_pos(app, rect, pos, axis);
        app.canvas.inter = Interaction::Guide(GuideDrag { axis, index: None, pos: v, on_ruler: true });
        return true;
    }
    if !matches!(app.tool, tools::Tool::Move | tools::Tool::Scale) {
        return false;
    }
    if let Some((i, g)) = hit_guide(app, rect, pos) {
        app.canvas.inter = Interaction::Guide(GuideDrag { axis: g.axis, index: Some(i), pos: g.pos, on_ruler: false });
        return true;
    }
    false
}

/// The guide under a screen point (within 4px), if any.
pub fn hit_guide(app: &App, rect: ERect, pos: Pos2) -> Option<(usize, RulerGuide)> {
    if !app.ui.rulers {
        return None;
    }
    let (top, left) = strips(rect);
    if top.contains(pos) || left.contains(pos) {
        return None;
    }
    guides_of(app).iter().enumerate().find_map(|(i, g)| {
        let s = match g.axis {
            GuideAxis::X => app.view.to_screen(rect.min, v2(g.pos, 0.0)).x - pos.x,
            GuideAxis::Y => app.view.to_screen(rect.min, v2(0.0, g.pos)).y - pos.y,
        };
        (s.abs() <= 4.0).then_some((i, *g))
    })
}

pub fn drag(app: &App, g: &mut GuideDrag, rect: ERect, pos: Pos2) {
    g.pos = page_pos(app, rect, pos, g.axis);
    let (top, left) = strips(rect);
    // Back on a ruler (or off the canvas): releasing now removes / cancels the guide.
    let over = match g.axis {
        GuideAxis::Y => pos.y < top.bottom(),
        GuideAxis::X => pos.x < left.right(),
    };
    g.on_ruler = over || !rect.contains(pos);
}

pub fn release(app: &mut App, g: GuideDrag) {
    let list = app.page_guides.entry(app.page).or_default();
    match (g.index, g.on_ruler) {
        (Some(i), true) => {
            if i < list.len() {
                list.remove(i);
            }
        }
        (Some(i), false) => {
            if let Some(slot) = list.get_mut(i) {
                slot.pos = g.pos;
            }
        }
        (None, false) => list.push(RulerGuide { axis: g.axis, pos: g.pos }),
        (None, true) => {}
    }
    if list.is_empty() {
        app.page_guides.remove(&app.page);
    }
}

/// Draws the guides (under the strips) and the rulers.
pub fn overlay(app: &App, ui: &Ui, rect: ERect) {
    if !app.ui.rulers {
        return;
    }
    let p = ui.painter().with_clip_rect(rect);
    let (top, left) = strips(rect);
    let dragging = match &app.canvas.inter {
        Interaction::Guide(g) => Some(*g),
        _ => None,
    };
    let to_s = |v: DV| app.view.to_screen(rect.min, v);
    let line = |axis: GuideAxis, pos: f32| match axis {
        GuideAxis::X => {
            let x = crisp(to_s(v2(pos, 0.0)).x);
            [Pos2::new(x, top.bottom()), Pos2::new(x, rect.bottom())]
        }
        GuideAxis::Y => {
            let y = crisp(to_s(v2(0.0, pos)).y);
            [Pos2::new(left.right(), y), Pos2::new(rect.right(), y)]
        }
    };
    for (i, g) in guides_of(app).iter().enumerate() {
        if dragging.is_some_and(|d| d.index == Some(i)) {
            continue;
        }
        p.line_segment(line(g.axis, g.pos), Stroke::new(1.0, theme::RULER_GUIDE));
    }
    if let Some(d) = dragging.filter(|d| !d.on_ruler) {
        p.line_segment(line(d.axis, d.pos), Stroke::new(1.0, theme::RULER_GUIDE));
        // value badge next to the pointer
        if let Some(ptr) = ui.input(|i| i.pointer.latest_pos()) {
            let o = origin(app);
            let v = d.pos - if d.axis == GuideAxis::X { o.x } else { o.y };
            let text = fmt_value(v);
            let font = theme::font_small_strong();
            let w = crate::widgets::text_width(&p, &text, &font) + theme::CANVAS_BADGE_PAD_X * 2.0;
            let r = ERect::from_min_size(ptr + EV::new(12.0, 12.0), EV::new(w, theme::CANVAS_BADGE_HEIGHT));
            p.rect_filled(r, theme::cr(theme::CANVAS_BADGE_RADIUS), theme::RULER_GUIDE);
            p.text(r.center(), Align2::CENTER_CENTER, text, font, Color32::WHITE);
        }
    }

    // ---- strips
    let bg = theme::BG;
    p.rect_filled(top, 0.0, bg);
    p.rect_filled(left, 0.0, bg);
    p.rect_filled(ERect::from_min_size(rect.min, EV::splat(theme::RULER_SIZE)), 0.0, bg);
    p.line_segment([Pos2::new(rect.left(), top.bottom() - 0.5), Pos2::new(rect.right(), top.bottom() - 0.5)], Stroke::new(1.0, theme::BORDER));
    p.line_segment([Pos2::new(left.right() - 0.5, rect.top()), Pos2::new(left.right() - 0.5, rect.bottom())], Stroke::new(1.0, theme::BORDER));

    let o = origin(app);
    let z = app.view.zoom;
    let tick = Stroke::new(1.0, theme::RULER_TICK);
    let font = theme::font_ruler();
    // horizontal ruler
    {
        let lo = app.view.to_page(rect.min, Pos2::new(top.left(), 0.0)).x - o.x;
        let hi = app.view.to_page(rect.min, Pos2::new(top.right(), 0.0)).x - o.x;
        let tp = p.with_clip_rect(top);
        for (v, level) in ticks(lo, hi, z) {
            let x = crisp(to_s(v2(v + o.x, 0.0)).x);
            let len = [theme::RULER_SIZE - 6.0, 6.0, 3.0][level as usize];
            tp.line_segment([Pos2::new(x, top.bottom() - len), Pos2::new(x, top.bottom())], tick);
            if level == 0 {
                tp.text(Pos2::new(x + 3.0, top.top() + 2.0), Align2::LEFT_TOP, fmt_value(v), font.clone(), theme::RULER_TEXT);
            }
        }
    }
    // vertical ruler
    {
        let lo = app.view.to_page(rect.min, Pos2::new(0.0, left.top())).y - o.y;
        let hi = app.view.to_page(rect.min, Pos2::new(0.0, left.bottom())).y - o.y;
        let lp = p.with_clip_rect(left);
        for (v, level) in ticks(lo, hi, z) {
            let y = crisp(to_s(v2(0.0, v + o.y)).y);
            let len = [theme::RULER_SIZE - 6.0, 6.0, 3.0][level as usize];
            lp.line_segment([Pos2::new(left.right() - len, y), Pos2::new(left.right(), y)], tick);
            if level == 0 {
                rotated_text(&lp, Pos2::new(left.left() + 2.0, y - 3.0), &fmt_value(v), theme::RULER_TEXT);
            }
        }
    }

    // ---- selection extent
    let sel_box = union_all(app.selection.iter().map(|i| world_aabb(&app.doc, *i)));
    if let Some(bb) = sel_box {
        let (x0, x1) = (to_s(v2(bb.x, 0.0)).x, to_s(v2(bb.x + bb.w, 0.0)).x);
        let (y0, y1) = (to_s(v2(0.0, bb.y)).y, to_s(v2(0.0, bb.y + bb.h)).y);
        let tp = p.with_clip_rect(top);
        let band = ERect::from_min_max(Pos2::new(x0.max(top.left()), top.top()), Pos2::new(x1.max(top.left()), top.bottom()));
        tp.rect_filled(band, 0.0, theme::RULER_SELECTION_FILL);
        let lp = p.with_clip_rect(left);
        let band = ERect::from_min_max(Pos2::new(left.left(), y0.max(left.top())), Pos2::new(left.right(), y1.max(left.top())));
        lp.rect_filled(band, 0.0, theme::RULER_SELECTION_FILL);
        for (x, v) in [(x0, bb.x - o.x), (x1, bb.x + bb.w - o.x)] {
            let text = fmt_value(v);
            let w = crate::widgets::text_width(&p, &text, &font) + 4.0;
            let r = ERect::from_center_size(Pos2::new(x, top.center().y), EV::new(w, 12.0));
            tp.rect_filled(r, 2.0, bg);
            tp.text(r.center(), Align2::CENTER_CENTER, text, font.clone(), theme::CANVAS_SELECTION);
        }
        for (y, v) in [(y0, bb.y - o.y), (y1, bb.y + bb.h - o.y)] {
            let text = fmt_value(v);
            let w = crate::widgets::text_width(&p, &text, &font) + 4.0;
            let r = ERect::from_center_size(Pos2::new(left.center().x, y), EV::new(12.0, w));
            lp.rect_filled(r, 2.0, bg);
            let galley = lp.layout_no_wrap(text, font.clone(), theme::CANVAS_SELECTION);
            let (gw, gh) = (galley.size().x, galley.size().y);
            lp.add(egui::epaint::TextShape::new(Pos2::new(r.center().x - gh / 2.0, r.center().y + gw / 2.0), galley, theme::CANVAS_SELECTION).with_angle(-std::f32::consts::FRAC_PI_2));
        }
    }
}

/// Cursor for the point under the pointer: resize arrows over rulers and guides.
pub fn cursor_at(app: &App, rect: ERect, pos: Pos2) -> Option<egui::CursorIcon> {
    if !app.ui.rulers || app.canvas.text.is_some() {
        return None;
    }
    let (top, left) = strips(rect);
    if top.contains(pos) {
        return Some(egui::CursorIcon::ResizeVertical);
    }
    if left.contains(pos) {
        return Some(egui::CursorIcon::ResizeHorizontal);
    }
    hit_guide(app, rect, pos).map(|(_, g)| if g.axis == GuideAxis::X { egui::CursorIcon::ResizeHorizontal } else { egui::CursorIcon::ResizeVertical })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_are_nice_and_wide_enough() {
        assert_eq!(tick_step(1.0), 50.0);
        assert_eq!(tick_step(0.1), 500.0);
        assert_eq!(tick_step(8.0), 10.0);
        assert_eq!(tick_step(100.0), 1.0);
        assert_eq!(tick_step(0.02), 5000.0);
        for z in [0.013f32, 0.3, 0.77, 1.0, 2.5, 13.0, 200.0] {
            let s = tick_step(z);
            assert!(s * z >= 50.0 && (s / 1.0).fract() == 0.0, "zoom {z} step {s}");
        }
    }

    #[test]
    fn ticks_cover_range_with_majors_on_steps() {
        let t = ticks(-30.0, 260.0, 2.0);
        let majors: Vec<f32> = t.iter().filter(|(_, l)| *l == 0).map(|(v, _)| *v).collect();
        assert!(majors.contains(&0.0) && majors.contains(&50.0) && majors.contains(&250.0));
        assert!(majors.iter().all(|v| (v / 50.0).fract() == 0.0));
        assert!(t.iter().any(|(_, l)| *l == 1));
        assert!(t.first().unwrap().0 <= -30.0 && t.last().unwrap().0 >= 260.0);
        // ticks are ordered and unique
        assert!(t.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn values_format_without_noise() {
        assert_eq!(fmt_value(100.0), "100");
        assert_eq!(fmt_value(-0.0), "0");
        assert_eq!(fmt_value(2.5), "2.5");
    }
}
