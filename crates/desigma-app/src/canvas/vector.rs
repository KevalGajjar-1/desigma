//! Vector edit mode: vertices and bezier handles of a Vector node can be
//! dragged, segments split by clicking them, vertices deleted. Every
//! drag/insert/delete is one undo step through [`Tx`].

use super::create::PenPoint;
use super::*;
use crate::edit::Tx;
use crate::theme;
use crate::util::*;
use desigma_doc::{Document, FillRule, NodeKind, Path, PathCmd};
use egui::{Modifiers, Painter, Pos2, Stroke, StrokeKind};

/// One sub-path of the edited vector as anchor points with handles
/// (absolute positions in node-local space).
#[derive(Clone, Debug, PartialEq)]
pub struct VPath {
    pub pts: Vec<PenPoint>,
    pub closed: bool,
    /// Index of the document `Path` this sub-path belongs to.
    pub path: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    In,
    Out,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Anchor(usize, usize),
    Handle(usize, usize, Side),
    /// Segment starting at point `usize` of sub-path `usize`, at parameter `t`.
    Segment(usize, usize, f32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragKind {
    Anchor,
    Handle(Side),
}

pub struct VDrag {
    pub tx: Tx,
    pub kind: DragKind,
    pub at: (usize, usize),
    /// World transform and own transform of the node when the drag began;
    /// all math happens in that frame.
    pub world0: Affine,
    pub t0: Affine,
    pub start: Vec<VPath>,
    pub changed: bool,
}

pub struct VectorEdit {
    pub id: NodeId,
    pub paths: Vec<VPath>,
    pub rules: Vec<FillRule>,
    pub sel: Option<(usize, usize)>,
    pub drag: Option<VDrag>,
}

// ------------------------------------------------------------ path <-> model
fn near(a: DV, b: DV) -> bool {
    dist(a, b) < 1e-4
}

fn handle(anchor: DV, h: DV) -> Option<DV> {
    (!near(anchor, h)).then_some(h)
}

pub fn from_paths(paths: &[Path]) -> (Vec<VPath>, Vec<FillRule>) {
    let mut out: Vec<VPath> = Vec::new();
    for (pi, path) in paths.iter().enumerate() {
        let mut cur: Option<VPath> = None;
        let finish = |cur: &mut Option<VPath>, out: &mut Vec<VPath>| {
            if let Some(v) = cur.take() {
                if !v.pts.is_empty() {
                    out.push(v);
                }
            }
        };
        for cmd in &path.cmds {
            match *cmd {
                PathCmd::MoveTo(p) => {
                    finish(&mut cur, &mut out);
                    cur = Some(VPath { pts: vec![PenPoint::new(p)], closed: false, path: pi });
                }
                PathCmd::LineTo(p) => {
                    if let Some(v) = &mut cur {
                        v.pts.push(PenPoint::new(p));
                    }
                }
                PathCmd::QuadTo(c, p) => {
                    if let Some(v) = &mut cur {
                        let prev = v.pts.last().map(|q| q.pos).unwrap_or(p);
                        let c1 = add(prev, scale(sub(c, prev), 2.0 / 3.0));
                        let c2 = add(p, scale(sub(c, p), 2.0 / 3.0));
                        if let Some(l) = v.pts.last_mut() {
                            l.hout = handle(l.pos, c1);
                        }
                        v.pts.push(PenPoint { pos: p, hin: handle(p, c2), hout: None });
                    }
                }
                PathCmd::CubicTo(c1, c2, p) => {
                    if let Some(v) = &mut cur {
                        if let Some(l) = v.pts.last_mut() {
                            l.hout = handle(l.pos, c1);
                        }
                        v.pts.push(PenPoint { pos: p, hin: handle(p, c2), hout: None });
                    }
                }
                PathCmd::Close => {
                    if let Some(v) = &mut cur {
                        v.closed = true;
                        if v.pts.len() > 1 && near(v.pts[0].pos, v.pts[v.pts.len() - 1].pos) {
                            let last = v.pts.pop().unwrap();
                            v.pts[0].hin = last.hin;
                        }
                    }
                }
            }
        }
        finish(&mut cur, &mut out);
    }
    (out, paths.iter().map(|p| p.fill_rule).collect())
}

fn seg_cmd(a: &PenPoint, b: &PenPoint) -> PathCmd {
    if a.hout.is_some() || b.hin.is_some() {
        PathCmd::CubicTo(a.hout.unwrap_or(a.pos), b.hin.unwrap_or(b.pos), b.pos)
    } else {
        PathCmd::LineTo(b.pos)
    }
}

pub fn to_paths(model: &[VPath], rules: &[FillRule]) -> Vec<Path> {
    let n = rules.len().max(model.iter().map(|v| v.path + 1).max().unwrap_or(0));
    let mut paths: Vec<Path> = (0..n).map(|i| Path { cmds: vec![], fill_rule: rules.get(i).copied().unwrap_or_default() }).collect();
    for v in model {
        let Some(first) = v.pts.first() else { continue };
        let cmds = &mut paths[v.path].cmds;
        cmds.push(PathCmd::MoveTo(first.pos));
        for w in v.pts.windows(2) {
            cmds.push(seg_cmd(&w[0], &w[1]));
        }
        if v.closed {
            if let Some(last) = v.pts.last() {
                cmds.push(seg_cmd(last, first));
            }
            cmds.push(PathCmd::Close);
        }
    }
    paths.retain(|p| !p.cmds.is_empty());
    paths
}

fn segment_count(v: &VPath) -> usize {
    if v.closed {
        v.pts.len()
    } else {
        v.pts.len().saturating_sub(1)
    }
}

fn segment(v: &VPath, i: usize) -> (&PenPoint, &PenPoint) {
    (&v.pts[i], &v.pts[(i + 1) % v.pts.len()])
}

fn cubic_at(a: &PenPoint, b: &PenPoint, t: f32) -> DV {
    let (c1, c2) = (a.hout.unwrap_or(a.pos), b.hin.unwrap_or(b.pos));
    let u = 1.0 - t;
    let f = |p0: f32, p1: f32, p2: f32, p3: f32| u * u * u * p0 + 3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t * p3;
    v2(f(a.pos.x, c1.x, c2.x, b.pos.x), f(a.pos.y, c1.y, c2.y, b.pos.y))
}

/// Bounding box of the sampled curves of every sub-path.
pub fn bounds(model: &[VPath]) -> Rect {
    let mut pts = Vec::new();
    for v in model {
        for i in 0..segment_count(v) {
            let (a, b) = segment(v, i);
            for k in 0..=16 {
                pts.push(cubic_at(a, b, k as f32 / 16.0));
            }
        }
        if segment_count(v) == 0 {
            pts.extend(v.pts.iter().map(|p| p.pos));
        }
    }
    aabb_of(&pts)
}

/// Inserts a vertex on segment `i` at parameter `t`, keeping the curve's shape.
/// Returns the index of the new vertex.
pub fn split_segment(v: &mut VPath, i: usize, t: f32) -> usize {
    let n = v.pts.len();
    let j = (i + 1) % n;
    let (a, b) = (v.pts[i], v.pts[j]);
    let curved = a.hout.is_some() || b.hin.is_some();
    let mut np = PenPoint::new(cubic_at(&a, &b, t));
    if curved {
        let (c1, c2) = (a.hout.unwrap_or(a.pos), b.hin.unwrap_or(b.pos));
        let lerp = |p: DV, q: DV| add(p, scale(sub(q, p), t));
        let (p01, p12, p23) = (lerp(a.pos, c1), lerp(c1, c2), lerp(c2, b.pos));
        let (p012, p123) = (lerp(p01, p12), lerp(p12, p23));
        np.hin = Some(p012);
        np.hout = Some(p123);
        v.pts[i].hout = Some(p01);
        v.pts[j].hin = Some(p23);
    }
    if j == 0 {
        v.pts.push(np);
        v.pts.len() - 1
    } else {
        v.pts.insert(i + 1, np);
        i + 1
    }
}

fn sp(p: DV) -> Pos2 {
    Pos2::new(p.x, p.y)
}

/// Finds what is under screen point `p`. `to_screen` maps node-local points to screen.
pub fn hit_test(model: &[VPath], sel: Option<(usize, usize)>, to_screen: &dyn Fn(DV) -> DV, p: DV, radius: f32, seg_radius: f32) -> Option<Hit> {
    // Handles of the selected vertex and its neighbours first.
    if let Some((pi, i)) = sel {
        if let Some(v) = model.get(pi) {
            let n = v.pts.len();
            let mut near_pts = vec![i];
            if v.closed || i > 0 {
                near_pts.push((i + n - 1) % n);
            }
            if v.closed || i + 1 < n {
                near_pts.push((i + 1) % n);
            }
            for j in near_pts {
                if let Some(pt) = v.pts.get(j) {
                    for (side, h) in [(Side::In, pt.hin), (Side::Out, pt.hout)] {
                        if let Some(h) = h {
                            if dist(to_screen(h), p) <= radius {
                                return Some(Hit::Handle(pi, j, side));
                            }
                        }
                    }
                }
            }
        }
    }
    for (pi, v) in model.iter().enumerate() {
        for (i, pt) in v.pts.iter().enumerate() {
            if dist(to_screen(pt.pos), p) <= radius {
                return Some(Hit::Anchor(pi, i));
            }
        }
    }
    let mut best: Option<(f32, Hit)> = None;
    for (pi, v) in model.iter().enumerate() {
        for i in 0..segment_count(v) {
            let (a, b) = segment(v, i);
            let steps = 32;
            let mut prev = to_screen(a.pos);
            for k in 1..=steps {
                let t1 = k as f32 / steps as f32;
                let cur = to_screen(cubic_at(a, b, t1));
                let (d, f) = point_segment(p, prev, cur);
                if d <= seg_radius && best.map(|(bd, _)| d < bd).unwrap_or(true) {
                    let t = ((k - 1) as f32 + f) / steps as f32;
                    best = Some((d, Hit::Segment(pi, i, t)));
                }
                prev = cur;
            }
        }
    }
    best.map(|b| b.1)
}

/// Distance from `p` to segment `a`-`b` and the position along it (0..1).
fn point_segment(p: DV, a: DV, b: DV) -> (f32, f32) {
    let ab = sub(b, a);
    let len2 = ab.x * ab.x + ab.y * ab.y;
    let f = if len2 < 1e-9 { 0.0 } else { (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / len2).clamp(0.0, 1.0) };
    (dist(p, add(a, scale(ab, f))), f)
}

// ------------------------------------------------------------ session
/// Writes `model` into the node (kind, size, transform), re-origin-ing the
/// geometry so the node box stays tight. `t0` is the node transform the model's
/// coordinates are relative to.
fn write_model(doc: &mut Document, ed: &mut crate::edit::Edit, id: NodeId, model: &[VPath], rules: &[FillRule], t0: Affine) {
    let bb = bounds(model);
    let origin = v2(bb.x, bb.y);
    let shifted: Vec<VPath> = model
        .iter()
        .map(|v| VPath {
            pts: v.pts.iter().map(|p| PenPoint { pos: sub(p.pos, origin), hin: p.hin.map(|h| sub(h, origin)), hout: p.hout.map(|h| sub(h, origin)) }).collect(),
            ..v.clone()
        })
        .collect();
    if let Some(n) = ed.node(doc, id) {
        n.kind = NodeKind::Vector { paths: to_paths(&shifted, rules) };
        n.transform = t0 * Affine::translate(origin.x, origin.y);
        n.size = v2(bb.w.max(1.0), bb.h.max(1.0));
        n.fill_geometry = None;
        n.stroke_geometry = None;
    }
}

pub fn is_vector(app: &App, id: NodeId) -> bool {
    matches!(app.doc.get(id).map(|n| &n.kind), Some(NodeKind::Vector { .. }))
}

/// Enters point editing for a Vector node.
pub fn enter(app: &mut App, id: NodeId) -> bool {
    let Some(NodeKind::Vector { paths }) = app.doc.get(id).map(|n| n.kind.clone()) else { return false };
    let (model, rules) = from_paths(&paths);
    if model.is_empty() {
        return false;
    }
    app.selection = vec![id];
    app.canvas.vedit = Some(VectorEdit { id, paths: model, rules, sel: None, drag: None });
    true
}

pub fn exit(app: &mut App) {
    if let Some(mut v) = app.canvas.vedit.take() {
        if let Some(d) = v.drag.take() {
            d.tx.cancel(&mut app.doc);
        }
    }
    if matches!(app.canvas.inter, Interaction::VectorDrag) {
        app.canvas.inter = Interaction::None;
    }
    app.changed();
}

/// Re-reads the model from the document (after undo/redo or a committed edit).
pub fn reload(app: &mut App) {
    let Some(v) = app.canvas.vedit.as_mut() else { return };
    if v.drag.is_some() {
        return;
    }
    match app.doc.get(v.id).map(|n| &n.kind) {
        Some(NodeKind::Vector { paths }) => {
            let (m, r) = from_paths(paths);
            if m.is_empty() {
                app.canvas.vedit = None;
                return;
            }
            v.paths = m;
            v.rules = r;
            if let Some((pi, i)) = v.sel {
                if v.paths.get(pi).map(|p| i >= p.pts.len()).unwrap_or(true) {
                    v.sel = None;
                }
            }
        }
        _ => app.canvas.vedit = None,
    }
}

fn mapper<'a>(app: &'a App, rect: ERect, world: Affine) -> impl Fn(DV) -> DV + 'a {
    move |p: DV| {
        let s = app.view.to_screen(rect.min, world.apply(p));
        v2(s.x, s.y)
    }
}

/// Handles a primary press while editing. Returns false when the press
/// should leave edit mode and be processed normally.
pub fn press(app: &mut App, rect: ERect, pos: Pos2, mods: Modifiers) -> bool {
    let Some(v) = app.canvas.vedit.as_ref() else { return false };
    let id = v.id;
    let world = app.doc.world_transform(id);
    let Some(n) = app.doc.get(id) else { return false };
    let t0 = n.transform;
    let size = n.size;
    let hit = {
        let to_s = mapper(app, rect, world);
        hit_test(&v.paths, v.sel, &to_s, v2(pos.x, pos.y), theme::VEDIT_HIT, theme::VEDIT_SEGMENT_HIT)
    };
    match hit {
        Some(Hit::Anchor(pi, i)) => {
            if let Some(v) = app.canvas.vedit.as_mut() {
                v.sel = Some((pi, i));
            }
            begin_drag(app, DragKind::Anchor, (pi, i), world, t0);
            true
        }
        Some(Hit::Handle(pi, i, side)) => {
            if let Some(v) = app.canvas.vedit.as_mut() {
                v.sel = Some((pi, i));
            }
            begin_drag(app, DragKind::Handle(side), (pi, i), world, t0);
            true
        }
        Some(Hit::Segment(pi, i, t)) => {
            insert_point(app, pi, i, t, t0);
            true
        }
        None => {
            // Inside (or just around) the node: deselect the vertex and stay in edit mode.
            let inside = {
                let to_s = mapper(app, rect, world);
                let c = [v2(0.0, 0.0), v2(size.x, 0.0), v2(size.x, size.y), v2(0.0, size.y)].map(to_s);
                let (x0, x1) = (c.iter().map(|q| q.x).fold(f32::MAX, f32::min), c.iter().map(|q| q.x).fold(f32::MIN, f32::max));
                let (y0, y1) = (c.iter().map(|q| q.y).fold(f32::MAX, f32::min), c.iter().map(|q| q.y).fold(f32::MIN, f32::max));
                let m = theme::VEDIT_HIT;
                pos.x >= x0 - m && pos.x <= x1 + m && pos.y >= y0 - m && pos.y <= y1 + m
            };
            let _ = mods;
            if inside {
                if let Some(v) = app.canvas.vedit.as_mut() {
                    v.sel = None;
                }
                true
            } else {
                exit(app);
                false
            }
        }
    }
}

fn begin_drag(app: &mut App, kind: DragKind, at: (usize, usize), world0: Affine, t0: Affine) {
    let Some(v) = app.canvas.vedit.as_mut() else { return };
    v.drag = Some(VDrag { tx: Tx::new(), kind, at, world0, t0, start: v.paths.clone(), changed: false });
    app.canvas.inter = Interaction::VectorDrag;
}

/// Splits a segment by clicking it (one undo step) and selects the new vertex.
fn insert_point(app: &mut App, pi: usize, i: usize, t: f32, t0: Affine) {
    let Some(v) = app.canvas.vedit.as_mut() else { return };
    let (id, mut model, rules) = (v.id, v.paths.clone(), v.rules.clone());
    let Some(vp) = model.get_mut(pi) else { return };
    let ni = split_segment(vp, i, t);
    v.sel = Some((pi, ni));
    let mut tx = Tx::new();
    write_model(&mut app.doc, &mut tx.ed, id, &model, &rules, t0);
    app.run_tx(tx);
    reload(app);
}

/// Pointer moved while dragging a vertex or handle.
pub fn drag(app: &mut App, cur: DV, mods: Modifiers) {
    let App { doc, canvas, .. } = app;
    let Some(v) = canvas.vedit.as_mut() else { return };
    let id = v.id;
    let rules = v.rules.clone();
    let Some(d) = v.drag.as_mut() else { return };
    let Some(inv) = d.world0.invert() else { return };
    let local = inv.apply(cur);
    let mut model = d.start.clone();
    let (pi, i) = d.at;
    let Some(pt) = model.get_mut(pi).and_then(|p| p.pts.get_mut(i)) else { return };
    let orig = d.start[pi].pts[i];
    match d.kind {
        DragKind::Anchor => {
            let mut delta = sub(local, orig.pos);
            if mods.shift {
                if delta.x.abs() > delta.y.abs() {
                    delta.y = 0.0;
                } else {
                    delta.x = 0.0;
                }
            }
            pt.pos = add(orig.pos, delta);
            pt.hin = orig.hin.map(|h| add(h, delta));
            pt.hout = orig.hout.map(|h| add(h, delta));
        }
        DragKind::Handle(side) => {
            // The dragged handle follows the pointer; the opposite one mirrors it unless Alt is held.
            let mirrored = sub(scale(orig.pos, 2.0), local);
            match side {
                Side::In => {
                    pt.hin = Some(local);
                    if !mods.alt && orig.hout.is_some() {
                        pt.hout = Some(mirrored);
                    }
                }
                Side::Out => {
                    pt.hout = Some(local);
                    if !mods.alt && orig.hin.is_some() {
                        pt.hin = Some(mirrored);
                    }
                }
            }
        }
    }
    d.changed = true;
    write_model(doc, &mut d.tx.ed, id, &model, &rules, d.t0);
    v.paths = model;
    canvas.dirty = true;
}

/// Pointer released: commits the drag as one undo step.
pub fn release(app: &mut App) {
    let Some(v) = app.canvas.vedit.as_mut() else { return };
    let Some(d) = v.drag.take() else { return };
    if d.changed {
        app.run_tx(d.tx);
    }
    reload(app);
}

/// Removes the selected vertex (one undo step).
pub fn delete_selected(app: &mut App) {
    let Some(v) = app.canvas.vedit.as_mut() else { return };
    let Some((pi, i)) = v.sel else { return };
    let (id, mut model, rules) = (v.id, v.paths.clone(), v.rules.clone());
    let Some(vp) = model.get_mut(pi) else { return };
    if vp.pts.len() > 2 {
        vp.pts.remove(i);
    } else {
        if model.len() <= 1 {
            return;
        }
        model.remove(pi);
    }
    v.sel = None;
    let Some(t0) = app.doc.get(id).map(|n| n.transform) else { return };
    let mut tx = Tx::new();
    write_model(&mut app.doc, &mut tx.ed, id, &model, &rules, t0);
    app.run_tx(tx);
    reload(app);
}

// ------------------------------------------------------------ overlay
pub fn overlay(app: &App, p: &Painter, rect: ERect) {
    let Some(v) = &app.canvas.vedit else { return };
    let world = app.doc.world_transform(v.id);
    let to_s = mapper(app, rect, world);
    let col = theme::CANVAS_SELECTION;
    // Path preview.
    for vp in &v.paths {
        for i in 0..segment_count(vp) {
            let (a, b) = segment(vp, i);
            let curve = egui::epaint::CubicBezierShape::from_points_stroke(
                [sp(to_s(a.pos)), sp(to_s(a.hout.unwrap_or(a.pos))), sp(to_s(b.hin.unwrap_or(b.pos))), sp(to_s(b.pos))],
                false,
                egui::Color32::TRANSPARENT,
                Stroke::new(theme::CANVAS_OUTLINE_WIDTH, col),
            );
            p.add(curve);
        }
    }
    // Handles of the selected vertex and its neighbours.
    if let Some((pi, i)) = v.sel {
        if let Some(vp) = v.paths.get(pi) {
            let n = vp.pts.len();
            let mut idx = vec![i];
            if vp.closed || i > 0 {
                idx.push((i + n - 1) % n);
            }
            if vp.closed || i + 1 < n {
                idx.push((i + 1) % n);
            }
            for j in idx {
                let pt = vp.pts[j];
                for h in [pt.hin, pt.hout].into_iter().flatten() {
                    p.line_segment([sp(to_s(pt.pos)), sp(to_s(h))], Stroke::new(1.0, col));
                    p.circle_filled(sp(to_s(h)), theme::VEDIT_HANDLE_RADIUS, theme::CANVAS_HANDLE_FILL);
                    p.circle_stroke(sp(to_s(h)), theme::VEDIT_HANDLE_RADIUS, Stroke::new(1.0, col));
                }
            }
        }
    }
    // Vertices.
    for (pi, vp) in v.paths.iter().enumerate() {
        for (i, pt) in vp.pts.iter().enumerate() {
            let r = ERect::from_center_size(sp(to_s(pt.pos)), egui::Vec2::splat(theme::VEDIT_POINT_SIZE));
            let selected = v.sel == Some((pi, i));
            p.rect_filled(r, 0.0, if selected { col } else { theme::CANVAS_HANDLE_FILL });
            p.rect_stroke(r, 0.0, Stroke::new(1.0, col), StrokeKind::Inside);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_path() -> Vec<Path> {
        vec![Path {
            cmds: vec![PathCmd::MoveTo(v2(0.0, 0.0)), PathCmd::LineTo(v2(100.0, 0.0)), PathCmd::LineTo(v2(100.0, 100.0))],
            fill_rule: FillRule::NonZero,
        }]
    }

    fn id(p: &[VPath]) -> Vec<VPath> {
        p.to_vec()
    }

    #[test]
    fn path_roundtrip_open_closed_and_curves() {
        let (m, r) = from_paths(&line_path());
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].pts.len(), 3);
        assert!(!m[0].closed);
        assert_eq!(to_paths(&m, &r), line_path());

        let closed = vec![Path {
            cmds: vec![
                PathCmd::MoveTo(v2(0.0, 0.0)),
                PathCmd::CubicTo(v2(10.0, -20.0), v2(40.0, -20.0), v2(50.0, 0.0)),
                PathCmd::LineTo(v2(50.0, 50.0)),
                PathCmd::LineTo(v2(0.0, 0.0)),
                PathCmd::Close,
            ],
            fill_rule: FillRule::EvenOdd,
        }];
        let (m, r) = from_paths(&closed);
        assert!(m[0].closed);
        assert_eq!(m[0].pts.len(), 3, "the duplicate end vertex is merged into the start");
        assert_eq!(m[0].pts[0].hout, Some(v2(10.0, -20.0)));
        assert_eq!(m[0].pts[1].hin, Some(v2(40.0, -20.0)));
        assert_eq!(to_paths(&m, &r), closed, "round trip is lossless");

        // quads become cubics
        let q = vec![Path { cmds: vec![PathCmd::MoveTo(v2(0.0, 0.0)), PathCmd::QuadTo(v2(30.0, 30.0), v2(60.0, 0.0))], fill_rule: FillRule::NonZero }];
        let (m, _) = from_paths(&q);
        assert!(m[0].pts[0].hout.is_some() && m[0].pts[1].hin.is_some());
    }

    #[test]
    fn splitting_a_curve_keeps_its_shape() {
        let (mut m, _) = from_paths(&[Path {
            cmds: vec![PathCmd::MoveTo(v2(0.0, 0.0)), PathCmd::CubicTo(v2(0.0, 60.0), v2(100.0, 60.0), v2(100.0, 0.0))],
            fill_rule: FillRule::NonZero,
        }]);
        let (a, b) = (m[0].pts[0], m[0].pts[1]);
        let mid = cubic_at(&a, &b, 0.5);
        let ni = split_segment(&mut m[0], 0, 0.5);
        assert_eq!(ni, 1);
        assert_eq!(m[0].pts.len(), 3);
        assert!(dist(m[0].pts[1].pos, mid) < 1e-4);
        // the first half still passes through the quarter point of the original
        let q = cubic_at(&a, &b, 0.25);
        let half = cubic_at(&m[0].pts[0], &m[0].pts[1], 0.5);
        assert!(dist(q, half) < 1e-3, "{q:?} vs {half:?}");
    }

    #[test]
    fn splitting_the_closing_segment_appends() {
        let (mut m, _) = from_paths(&line_path());
        m[0].closed = true;
        let ni = split_segment(&mut m[0], 2, 0.5);
        assert_eq!(ni, 3);
        assert_eq!(m[0].pts[3].pos, v2(50.0, 50.0));
        assert!(m[0].pts[3].hin.is_none());
    }

    #[test]
    fn hit_priority_handle_then_anchor_then_segment() {
        let mut path = from_paths(&[Path {
            cmds: vec![PathCmd::MoveTo(v2(0.0, 0.0)), PathCmd::CubicTo(v2(30.0, 50.0), v2(70.0, 50.0), v2(100.0, 0.0))],
            fill_rule: FillRule::NonZero,
        }])
        .0;
        let ident = |p: DV| p;
        assert_eq!(hit_test(&path, None, &ident, v2(1.0, 1.0), 6.0, 5.0), Some(Hit::Anchor(0, 0)));
        // handles only respond for the selected vertex
        assert!(!matches!(hit_test(&path, None, &ident, v2(30.0, 50.0), 6.0, 5.0), Some(Hit::Handle(..))));
        assert_eq!(hit_test(&path, Some((0, 0)), &ident, v2(30.0, 50.0), 6.0, 5.0), Some(Hit::Handle(0, 0, Side::Out)));
        // a point on the curve's middle hits the segment
        let a = path[0].pts[0];
        let b = path[0].pts[1];
        let mid = cubic_at(&a, &b, 0.5);
        match hit_test(&path, None, &ident, mid, 6.0, 5.0) {
            Some(Hit::Segment(0, 0, t)) => assert!((t - 0.5).abs() < 0.05, "{t}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(hit_test(&path, None, &ident, v2(50.0, 300.0), 6.0, 5.0), None);
        path.clear();
    }

    #[test]
    fn bounds_cover_the_curve_not_just_the_anchors() {
        let (m, _) = from_paths(&[Path {
            cmds: vec![PathCmd::MoveTo(v2(0.0, 0.0)), PathCmd::CubicTo(v2(0.0, 60.0), v2(100.0, 60.0), v2(100.0, 0.0))],
            fill_rule: FillRule::NonZero,
        }]);
        let bb = bounds(&id(&m));
        assert!(bb.h > 40.0 && bb.w >= 99.9);
    }

    // ---- app level
    fn app_with_vector() -> (App, NodeId) {
        let mut d = Document::new("t");
        let page = d.add_page("p");
        let fid = d.alloc_id();
        let mut n = desigma_doc::Node::new(fid, NodeKind::Vector { paths: line_path() });
        n.transform = Affine::translate(200.0, 100.0);
        n.size = v2(100.0, 100.0);
        n.stroke.paints = vec![desigma_doc::Paint::solid(desigma_doc::Color::BLACK)];
        let id = d.insert(page, n).unwrap();
        let mut app = App::new(d);
        app.selection = vec![id];
        (app, id)
    }

    fn paths_of(app: &App, id: NodeId) -> Vec<Path> {
        match &app.doc.get(id).unwrap().kind {
            NodeKind::Vector { paths } => paths.clone(),
            _ => unreachable!(),
        }
    }

    #[test]
    fn only_vectors_can_be_edited() {
        let (mut app, id) = app_with_vector();
        let rid = app.doc.alloc_id();
        let r = app.doc.insert(app.page, desigma_doc::Node::rectangle(rid, Rect::new(0.0, 0.0, 10.0, 10.0))).unwrap();
        assert!(!enter(&mut app, r));
        assert!(enter(&mut app, id));
        assert!(app.canvas.vedit.is_some());
        exit(&mut app);
        assert!(app.canvas.vedit.is_none());
    }

    #[test]
    fn dragging_a_vertex_is_one_undo_step() {
        let (mut app, id) = app_with_vector();
        let before = paths_of(&app, id);
        enter(&mut app, id);
        let world = app.doc.world_transform(id);
        let t0 = app.doc.get(id).unwrap().transform;
        app.canvas.vedit.as_mut().unwrap().sel = Some((0, 1));
        begin_drag(&mut app, DragKind::Anchor, (0, 1), world, t0);
        // the vertex (100, 0) local = (300, 100) page; drag it by (+20, -30)
        for step in 1..=3 {
            drag(&mut app, v2(300.0 + 20.0 * step as f32 / 3.0, 100.0 - 10.0 * step as f32), Modifiers::NONE);
        }
        release(&mut app);
        assert!(app.canvas.vedit.as_ref().unwrap().drag.is_none());
        let n = app.doc.get(id).unwrap();
        // the geometry moved up by 30 and the box was re-fitted: world position of the vertex is right
        let p = match &n.kind {
            NodeKind::Vector { paths } => paths[0].cmds.iter().filter_map(|c| if let PathCmd::LineTo(p) = c { Some(*p) } else { None }).next().unwrap(),
            _ => unreachable!(),
        };
        let wp = app.doc.world_transform(id).apply(p);
        assert!((wp.x - 320.0).abs() < 1e-3 && (wp.y - 70.0).abs() < 1e-3, "{wp:?}");
        assert_eq!(n.size, v2(120.0, 130.0));
        app.undo();
        assert_eq!(paths_of(&app, id), before);
        assert_eq!(app.doc.get(id).unwrap().transform, Affine::translate(200.0, 100.0));
        assert!(!app.hist.can_undo(), "the whole drag was a single step");
        app.redo();
        let wp2 = app.doc.world_transform(id).apply(match &paths_of(&app, id)[0].cmds[1] {
            PathCmd::LineTo(p) => *p,
            _ => unreachable!(),
        });
        assert!((wp2.x - 320.0).abs() < 1e-3 && (wp2.y - 70.0).abs() < 1e-3);
    }

    #[test]
    fn dragging_a_handle_mirrors_the_opposite_one() {
        let mut d = Document::new("t");
        let page = d.add_page("p");
        let fid = d.alloc_id();
        let mut n = desigma_doc::Node::new(
            fid,
            NodeKind::Vector {
                paths: vec![Path {
                    cmds: vec![
                        PathCmd::MoveTo(v2(0.0, 50.0)),
                        PathCmd::CubicTo(v2(20.0, 50.0), v2(30.0, 0.0), v2(50.0, 0.0)),
                        PathCmd::CubicTo(v2(70.0, 0.0), v2(80.0, 50.0), v2(100.0, 50.0)),
                    ],
                    fill_rule: FillRule::NonZero,
                }],
            },
        );
        n.size = v2(100.0, 50.0);
        let id = d.insert(page, n).unwrap();
        let mut app = App::new(d);
        enter(&mut app, id);
        let world = app.doc.world_transform(id);
        let t0 = app.doc.get(id).unwrap().transform;
        app.canvas.vedit.as_mut().unwrap().sel = Some((0, 1));
        begin_drag(&mut app, DragKind::Handle(Side::Out), (0, 1), world, t0);
        drag(&mut app, v2(60.0, 10.0), Modifiers::NONE);
        let v = app.canvas.vedit.as_ref().unwrap();
        let pt = v.paths[0].pts[1];
        assert_eq!(pt.hout, Some(v2(60.0, 10.0)));
        assert_eq!(pt.hin, Some(v2(40.0, -10.0)), "mirrored through the vertex");
        release(&mut app);
        assert!(app.hist.can_undo());
        app.undo();
        assert!(!app.hist.can_undo());
    }

    #[test]
    fn adding_and_deleting_points_are_undoable() {
        let (mut app, id) = app_with_vector();
        let before = paths_of(&app, id);
        enter(&mut app, id);
        let t0 = app.doc.get(id).unwrap().transform;
        insert_point(&mut app, 0, 0, 0.5, t0);
        assert_eq!(app.canvas.vedit.as_ref().unwrap().paths[0].pts.len(), 4);
        assert_eq!(app.canvas.vedit.as_ref().unwrap().sel, Some((0, 1)));
        assert_eq!(paths_of(&app, id)[0].cmds.len(), 4);
        delete_selected(&mut app);
        assert_eq!(paths_of(&app, id), before);
        assert!(app.canvas.vedit.as_ref().unwrap().sel.is_none());
        app.undo();
        assert_eq!(paths_of(&app, id)[0].cmds.len(), 4, "undo restores the inserted point");
        app.undo();
        assert_eq!(paths_of(&app, id), before);
    }

    #[test]
    fn cannot_delete_the_last_path() {
        let (mut app, id) = app_with_vector();
        enter(&mut app, id);
        let v = app.canvas.vedit.as_mut().unwrap();
        v.paths[0].pts.truncate(2);
        v.sel = Some((0, 0));
        delete_selected(&mut app);
        assert!(app.doc.get(id).is_some());
        assert!(!app.hist.can_undo());
    }
}
