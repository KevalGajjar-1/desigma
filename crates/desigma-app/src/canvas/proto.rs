//! Prototype mode on the canvas: hotspots, noodles (connectors), flow flags
//! and drag-to-connect. Everything is painted as egui overlays on top of the
//! tiles; the tile renderer is untouched.

use super::draw::label_rects;
use super::interact;
use crate::app::{App, RightTab};
use crate::proto::{self, geom};
use crate::theme;
use crate::util::{v2, world_aabb};
use crate::widgets;
use desigma_doc::{NodeId, NodeKind};
use egui::{Align2, Color32, Pos2, Rect as ERect, Response, Shape, Stroke, StrokeKind, Ui};

/// Prototype editing is on while the Prototype tab is showing.
pub fn active(app: &App) -> bool {
    app.ui.right_tab == Some(RightTab::Prototype) && !app.ui.dev_mode
}

/// Screen rect of a layer's bounding box.
pub fn screen_rect(app: &App, rect: ERect, id: NodeId) -> ERect {
    let bb = world_aabb(&app.doc, id);
    ERect::from_min_max(app.view.to_screen(rect.min, v2(bb.x, bb.y)), app.view.to_screen(rect.min, v2(bb.x + bb.w, bb.y + bb.h)))
}

fn can_connect(app: &App, id: NodeId) -> bool {
    app.doc.get(id).is_some_and(|n| n.visible && !matches!(n.kind, NodeKind::Page { .. }))
}

/// Layers that show a hotspot: the one under the pointer and the selection.
pub fn hotspot_layers(app: &App) -> Vec<NodeId> {
    let mut v: Vec<NodeId> = Vec::new();
    for id in app.proto.hover.iter().chain(app.selection.iter()) {
        if can_connect(app, *id) && !v.contains(id) {
            v.push(*id);
        }
    }
    v
}

/// Hotspot center (screen) of a layer.
pub fn hotspot_pos(app: &App, rect: ERect, id: NodeId) -> Pos2 {
    geom::hotspot_center(screen_rect(app, rect, id))
}

/// The hotspot under `pos`, if any (selected layers win over hovered).
pub fn hotspot_at(app: &App, rect: ERect, pos: Pos2) -> Option<NodeId> {
    hotspot_layers(app).into_iter().rev().find(|id| (hotspot_pos(app, rect, *id) - pos).length() <= theme::PROTO_HOTSPOT_HIT)
}

/// Top-level frame under a page point that may be a connection target.
pub fn frame_at(app: &App, page: desigma_doc::Vec2) -> Option<NodeId> {
    let hit = interact::hit_under(app, page, true)?;
    let top = proto::top_frame(&app.doc, hit)?;
    proto::destination_frames(&app.doc, hit).contains(&top).then_some(top)
}

/// Pointer handling; returns true when the canvas interaction was consumed.
pub fn process(app: &mut App, ui: &mut Ui, rect: ERect, resp: &Response) -> bool {
    if !active(app) {
        app.proto.connect = None;
        app.proto.hover = None;
        app.proto.hot = None;
        return false;
    }
    let (pos, pressed, down, esc) = ui.input(|i| (i.pointer.latest_pos(), i.pointer.primary_pressed(), i.pointer.primary_down(), i.key_pressed(egui::Key::Escape)));
    let inside = pos.filter(|p| rect.contains(*p) && resp.hovered());
    let idle = matches!(app.canvas.inter, super::Interaction::None) && app.canvas.text.is_none() && app.canvas.vedit.is_none();

    if let Some((src, _)) = app.proto.connect {
        if esc {
            app.proto.connect = None;
            return true;
        }
        if let Some(p) = pos {
            app.proto.connect = Some((src, p));
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        if !down {
            app.proto.connect = None;
            if let Some(p) = pos.filter(|p| rect.contains(*p)) {
                let page = app.view.to_page(rect.min, p);
                if let Some(dest) = frame_at(app, page) {
                    if proto::top_frame(&app.doc, src) != Some(dest) {
                        app.proto_connect(src, dest);
                        app.select_only(src);
                    }
                }
            }
        }
        return true;
    }

    app.proto.hover = if idle {
        inside.and_then(|p| interact::hit_under(app, app.view.to_page(rect.min, p), true)).filter(|id| can_connect(app, *id))
    } else {
        None
    };
    app.proto.hot = if idle { inside.and_then(|p| hotspot_at(app, rect, p)) } else { None };
    if let Some(h) = app.proto.hot {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        if pressed {
            app.proto.connect = Some((h, pos.unwrap_or(rect.center())));
            return true;
        }
    }
    false
}

fn arrow_head(p: &egui::Painter, n: &geom::Noodle, col: Color32) {
    p.add(Shape::convex_polygon(n.arrow.to_vec(), col, Stroke::NONE));
}

fn draw_noodle(p: &egui::Painter, n: &geom::Noodle, col: Color32) {
    let t = n.trimmed();
    p.add(egui::epaint::CubicBezierShape::from_points_stroke(t, false, Color32::TRANSPARENT, Stroke::new(theme::PROTO_NOODLE_WIDTH, col)));
    arrow_head(p, n, col);
}

fn hotspot(p: &egui::Painter, c: Pos2, big: bool) {
    let r = theme::PROTO_HOTSPOT_RADIUS * if big { 1.25 } else { 1.0 };
    p.circle_filled(c, r + theme::PROTO_HOTSPOT_RING, Color32::WHITE);
    p.circle_filled(c, r, theme::PROTO_BLUE);
}

/// Layers whose interactions draw noodles: the selection and everything below it.
fn noodle_sources(app: &App) -> Vec<NodeId> {
    let mut out: Vec<NodeId> = Vec::new();
    for s in &app.selection {
        app.doc.walk(*s, |n, _| {
            if out.len() < 400 && !n.prototype.reactions.is_empty() && !out.contains(&n.id) {
                out.push(n.id);
            }
        });
    }
    if let Some(h) = app.proto.hover {
        if proto::has_interactions(&app.doc, h) && !out.contains(&h) {
            out.push(h);
        }
    }
    out
}

pub fn overlays(app: &App, ui: &Ui, rect: ERect) {
    if !active(app) {
        return;
    }
    let p = ui.painter().with_clip_rect(rect);
    // Flow flags above the labels of starting frames.
    let labels = label_rects(app, rect);
    for f in app.doc.flows.iter().filter(|f| f.page == app.page) {
        let Some((_, lr)) = labels.iter().find(|(id, _)| *id == f.start) else { continue };
        let font = theme::font_small_strong();
        let w = widgets::text_width(&p, &f.name, &font) + 30.0;
        let r = ERect::from_min_size(Pos2::new(lr.left(), lr.top() - theme::PROTO_FLAG_GAP - theme::PROTO_FLAG_HEIGHT), egui::vec2(w, theme::PROTO_FLAG_HEIGHT));
        p.rect_filled(r, theme::cr(theme::RADIUS_MEDIUM), theme::PROTO_BLUE);
        crate::icons::paint(&p, crate::icons::Icon::Flag, Pos2::new(r.left() + 11.0, r.center().y), 12.0, Color32::WHITE);
        p.text(Pos2::new(r.left() + 21.0, r.center().y), Align2::LEFT_CENTER, &f.name, font, Color32::WHITE);
    }

    // Noodles of the selected layers.
    for src in noodle_sources(app) {
        let s = screen_rect(app, rect, src);
        for dest in proto::connections(&app.doc, src) {
            let d = screen_rect(app, rect, dest);
            draw_noodle(&p, &geom::noodle(s, d), theme::PROTO_BLUE);
        }
    }

    // Hotspots.
    for id in hotspot_layers(app) {
        if app.proto.connect.is_some_and(|(s, _)| s == id) {
            continue;
        }
        hotspot(&p, hotspot_pos(app, rect, id), app.proto.hot == Some(id));
    }

    // Connection being dragged.
    if let Some((src, ptr)) = app.proto.connect {
        let s = screen_rect(app, rect, src);
        let page = app.view.to_page(rect.min, ptr);
        let target = frame_at(app, page).filter(|t| proto::top_frame(&app.doc, src) != Some(*t));
        let dst = match target {
            Some(t) => {
                let tr = screen_rect(app, rect, t);
                p.rect_stroke(tr, 0.0, Stroke::new(2.0, theme::PROTO_BLUE), StrokeKind::Outside);
                tr
            }
            None => ERect::from_center_size(ptr, egui::vec2(0.0, 0.0)),
        };
        draw_noodle(&p, &geom::noodle(s, dst), theme::PROTO_BLUE);
        hotspot(&p, geom::hotspot_center(s), true);
    }
}
