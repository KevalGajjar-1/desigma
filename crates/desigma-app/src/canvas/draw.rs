//! Canvas overlays: frame labels, hover/selection outlines, handles, size
//! badge, guides, marquee, pen preview and the text caret.

use super::handles::Handle;
use super::interact::{selection_box, text_wrap_width};
use super::snap::GuideAxis;
use super::*;
use crate::theme;
use crate::util::*;
use desigma_doc::NodeKind;
use egui::{Align2, Color32, Painter, Pos2, Stroke, StrokeKind, Vec2 as EV};
use std::time::Duration;

fn sel_color(app: &App, id: NodeId) -> Color32 {
    match app.doc.get(id).map(|n| &n.kind) {
        Some(NodeKind::Component(_) | NodeKind::ComponentSet(_) | NodeKind::Instance { .. }) => theme::CANVAS_COMPONENT,
        _ => theme::CANVAS_SELECTION,
    }
}

fn page_is_light(app: &App) -> bool {
    match app.doc.get(app.page).map(|n| &n.kind) {
        Some(NodeKind::Page { background }) => {
            let f = |v: f32| (v * 255.0) as u8;
            theme::is_light(Color32::from_rgb(f(background.r), f(background.g), f(background.b)))
        }
        _ => true,
    }
}

/// Screen rects of the name labels above top-level frames.
pub fn label_rects(app: &App, rect: ERect) -> Vec<(NodeId, ERect)> {
    let mut out = Vec::new();
    if app.view.zoom < theme::CANVAS_LABEL_MAX_ZOOM_HIDE {
        return out;
    }
    let Some(page) = app.doc.get(app.page) else { return out };
    for id in &page.children {
        let Some(n) = app.doc.get(*id) else { continue };
        if !n.visible || !matches!(n.kind, NodeKind::Frame(_) | NodeKind::Section | NodeKind::Component(_) | NodeKind::ComponentSet(_)) {
            continue;
        }
        let bb = world_aabb(&app.doc, *id);
        let tl = app.view.to_screen(rect.min, v2(bb.x, bb.y));
        let w_px = bb.w * app.view.zoom;
        let text_w = n.name.chars().count() as f32 * theme::FONT_SIZE_LABEL_CANVAS * 0.66;
        let w = text_w.min(w_px.max(24.0));
        let h = theme::FONT_SIZE_LABEL_CANVAS + 3.0;
        let r = ERect::from_min_size(Pos2::new(tl.x, tl.y - theme::CANVAS_LABEL_GAP - h), EV::new(w, h));
        if r.intersects(rect) {
            out.push((*id, r));
        }
    }
    out
}

fn outline(app: &App, p: &Painter, rect: ERect, id: NodeId, col: Color32, width: f32) {
    let corners = world_corners(&app.doc, id).map(|c| app.view.to_screen(rect.min, c));
    let is_line = matches!(app.doc.get(id).map(|n| &n.kind), Some(NodeKind::Line));
    if is_line {
        p.line_segment([corners[0], corners[1]], Stroke::new(width.max(1.0), col));
    } else {
        p.add(egui::Shape::closed_line(corners.to_vec(), Stroke::new(width, col)));
    }
}

fn badge(p: &Painter, center_top: Pos2, text: &str, col: Color32) {
    let font = theme::font_small_strong();
    let w = crate::widgets::text_width(p, text, &font) + theme::CANVAS_BADGE_PAD_X * 2.0;
    let r = ERect::from_min_size(Pos2::new(center_top.x - w / 2.0, center_top.y), EV::new(w, theme::CANVAS_BADGE_HEIGHT));
    p.rect_filled(r, theme::cr(theme::CANVAS_BADGE_RADIUS), col);
    p.text(r.center(), Align2::CENTER_CENTER, text, font, theme::CANVAS_SIZE_BADGE_TEXT);
}

pub fn overlays(app: &App, ui: &Ui, rect: ERect) {
    let p = ui.painter().with_clip_rect(rect);
    let to_s = |v: DV| app.view.to_screen(rect.min, v);
    let light = page_is_light(app);

    // Frame labels.
    let lab_col = if light { theme::CANVAS_LABEL } else { theme::CANVAS_LABEL_DARK };
    for (id, r) in label_rects(app, rect) {
        let Some(n) = app.doc.get(id) else { continue };
        let selected = app.is_selected(id);
        let hovered = app.hover == Some(id) || app.hover_layer == Some(id);
        let col = if selected || hovered { sel_color(app, id) } else { lab_col };
        let name = crate::widgets::elide(&p, &n.name, &theme::font_body(), r.width());
        p.text(Pos2::new(r.left(), r.center().y), Align2::LEFT_CENTER, name, theme::font_body(), col);
    }

    // Hover outline.
    for h in [app.hover, app.hover_layer].into_iter().flatten() {
        if !app.is_selected(h) && app.doc.get(h).is_some() {
            outline(app, &p, rect, h, sel_color(app, h), theme::CANVAS_HOVER_WIDTH);
        }
    }

    // Selection.
    if !app.selection.is_empty() {
        let multi = app.selection.len() > 1;
        for id in &app.selection {
            if app.doc.get(*id).is_some() {
                outline(app, &p, rect, *id, sel_color(app, *id), theme::CANVAS_OUTLINE_WIDTH);
            }
        }
        if let Some((corners, is_line)) = selection_box(app) {
            let col = sel_color(app, app.selection[0]);
            let s = corners.map(|c| to_s(c));
            if multi {
                p.add(egui::Shape::closed_line(s.to_vec(), Stroke::new(theme::CANVAS_OUTLINE_WIDTH, col)));
            }
            let w = (s[1] - s[0]).length();
            let h = (s[3] - s[0]).length();
            let small = w.min(h) <= theme::CANVAS_HANDLE_SIZE * 3.0;
            let only_corners = small && !is_line;
            let dvs = corners.map(|c| dv(to_s(c)));
            if app.canvas.text.is_none() && app.canvas.vedit.is_none() && matches!(app.tool, tools::Tool::Move | tools::Tool::Scale) {
                for hnd in Handle::ALL {
                    if is_line && !matches!(hnd, Handle::L | Handle::R) {
                        continue;
                    }
                    if only_corners && !hnd.is_corner() {
                        continue;
                    }
                    let c = handles::handle_pos(&dvs, hnd);
                    let r = ERect::from_center_size(Pos2::new(c.x, c.y), EV::splat(theme::CANVAS_HANDLE_SIZE));
                    p.rect_filled(r, 0.0, theme::CANVAS_HANDLE_FILL);
                    p.rect_stroke(r, 0.0, Stroke::new(theme::CANVAS_HANDLE_BORDER, col), StrokeKind::Inside);
                }
            }
            // Size badge under the lowest corner.
            let label = if let [one] = app.selection.as_slice() {
                app.doc.get(*one).map(|n| format!("{} × {}", n.size.x.round() as i64, n.size.y.round() as i64))
            } else {
                union_all(app.selection.iter().map(|i| world_aabb(&app.doc, *i))).map(|b| format!("{} × {}", b.w.round() as i64, b.h.round() as i64))
            };
            if let Some(label) = label {
                let bottom = s.iter().map(|c| c.y).fold(f32::MIN, f32::max);
                let cx = s.iter().map(|c| c.x).sum::<f32>() / 4.0;
                badge(&p, Pos2::new(cx, bottom + theme::CANVAS_BADGE_OFFSET + 3.0), &label, col);
            }
        }
    }

    vector::overlay(app, &p, rect);

    // Snap guides & measurements.
    let gcol = theme::GUIDE_COLOR;
    for g in &app.canvas.guides {
        let (a, b) = match g.axis {
            GuideAxis::X => (to_s(v2(g.pos, g.from)), to_s(v2(g.pos, g.to))),
            GuideAxis::Y => (to_s(v2(g.from, g.pos)), to_s(v2(g.to, g.pos))),
        };
        p.line_segment([a, b], Stroke::new(theme::GUIDE_WIDTH, gcol));
        for e in [a, b] {
            let k = theme::GUIDE_X_SIZE;
            p.line_segment([e + EV::new(-k, -k), e + EV::new(k, k)], Stroke::new(theme::GUIDE_WIDTH, gcol));
            p.line_segment([e + EV::new(-k, k), e + EV::new(k, -k)], Stroke::new(theme::GUIDE_WIDTH, gcol));
        }
    }
    for m in &app.canvas.measures {
        let a = to_s(v2(m.a.0, m.a.1));
        let b = to_s(v2(m.b.0, m.b.1));
        p.line_segment([a, b], Stroke::new(theme::GUIDE_WIDTH, gcol));
        let horiz = (a.y - b.y).abs() < 0.5;
        let tick = if horiz { EV::new(0.0, 4.0) } else { EV::new(4.0, 0.0) };
        for e in [a, b] {
            p.line_segment([e - tick, e + tick], Stroke::new(theme::GUIDE_WIDTH, gcol));
        }
        let mid = Pos2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
        let text = format!("{}", m.value.round() as i64);
        let font = theme::font_small_strong();
        let w = crate::widgets::text_width(&p, &text, &font) + theme::GUIDE_TEXT_PAD * 2.0;
        let r = ERect::from_center_size(mid + EV::new(0.0, if horiz { -10.0 } else { 0.0 }), EV::new(w, theme::GUIDE_TEXT_HEIGHT));
        p.rect_filled(r, theme::cr(theme::CANVAS_BADGE_RADIUS), gcol);
        p.text(r.center(), Align2::CENTER_CENTER, text, font, theme::GUIDE_TEXT);
    }

    // Auto-layout insertion indicator.
    if let Interaction::Move(m) = &app.canvas.inter {
        if let Some(r) = &m.reorder {
            p.line_segment([to_s(r.line[0]), to_s(r.line[1])], Stroke::new(theme::AL_INDICATOR_WIDTH, theme::DROP_INDICATOR));
        }
    }

    // Marquee.
    if let Interaction::Marquee { start, cur, .. } = &app.canvas.inter {
        let (a, b) = (to_s(*start), to_s(*cur));
        let r = ERect::from_two_pos(a, b);
        p.rect_filled(r, 0.0, theme::CANVAS_MARQUEE_FILL);
        p.rect_stroke(r, 0.0, Stroke::new(1.0, theme::CANVAS_SELECTION), StrokeKind::Inside);
    }

    // Pen / pencil previews.
    if let Some(pen) = &app.canvas.pen {
        draw_pen(app, &p, rect, &pen.pts, ui.input(|i| i.pointer.latest_pos()));
    }
    if let Interaction::Pencil(pts) = &app.canvas.inter {
        let line: Vec<Pos2> = pts.iter().map(|q| to_s(*q)).collect();
        p.add(egui::Shape::line(line, Stroke::new(theme::PEN_LINE_WIDTH, theme::CANVAS_SELECTION)));
    }

    // Text caret.
    if let Some(sess) = &app.canvas.text {
        if let Some(n) = app.doc.get(sess.id) {
            if let NodeKind::Text(t) = &n.kind {
                let w = app.doc.world_transform(sess.id);
                let (x, y, lh) = crate::textmeasure::caret_rect(t, text_wrap_width(t, n.size), sess.caret);
                let a = to_s(w.apply(v2(x, y)));
                let b = to_s(w.apply(v2(x, y + lh)));
                let blink = (ui.input(|i| i.time) * 2.0) as i64 % 2 == 0;
                if blink {
                    p.line_segment([a, b], Stroke::new(theme::TEXT_CARET_WIDTH, if light { Color32::BLACK } else { Color32::WHITE }));
                }
                ui.ctx().request_repaint_after(Duration::from_millis(250));
            }
        }
    }
}

fn dv(p: Pos2) -> DV {
    DV::new(p.x, p.y)
}

fn draw_pen(app: &App, p: &Painter, rect: ERect, pts: &[create::PenPoint], pointer: Option<Pos2>) {
    let to_s = |v: DV| app.view.to_screen(rect.min, v);
    let col = theme::CANVAS_SELECTION;
    let stroke = Stroke::new(theme::PEN_LINE_WIDTH, col);
    for w in pts.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        if a.hout.is_some() || b.hin.is_some() {
            let shape = egui::epaint::CubicBezierShape::from_points_stroke(
                [to_s(a.pos), to_s(a.hout.unwrap_or(a.pos)), to_s(b.hin.unwrap_or(b.pos)), to_s(b.pos)],
                false,
                Color32::TRANSPARENT,
                stroke,
            );
            p.add(shape);
        } else {
            p.line_segment([to_s(a.pos), to_s(b.pos)], stroke);
        }
    }
    if let (Some(last), Some(ptr)) = (pts.last(), pointer) {
        if rect.contains(ptr) {
            p.line_segment([to_s(last.pos), ptr], Stroke::new(1.0, col.gamma_multiply(0.6)));
        }
    }
    for (i, pt) in pts.iter().enumerate() {
        let c = to_s(pt.pos);
        for h in [pt.hin, pt.hout].into_iter().flatten() {
            p.line_segment([c, to_s(h)], Stroke::new(1.0, col));
            p.circle_filled(to_s(h), 2.5, col);
        }
        let near_first = i == 0 && pts.len() >= 3 && pointer.map(|q| (q - c).length() < 8.0).unwrap_or(false);
        let r = ERect::from_center_size(c, EV::splat(theme::PEN_POINT_SIZE * 2.0 * if near_first { 1.5 } else { 1.0 }));
        p.rect_filled(r, 0.0, theme::PEN_POINT);
        p.rect_stroke(r, 0.0, Stroke::new(1.0, col), StrokeKind::Inside);
    }
}
