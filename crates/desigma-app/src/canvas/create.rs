//! Creating nodes from tools: shapes, text, pen paths.

use super::tools::Tool;
use crate::app::App;
use crate::edit::Tx;
use crate::layout;
use crate::theme;
use crate::util::*;
use desigma_doc::*;

/// Where a drag-drawn shape ends up and how it is shaped, in page space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawGeom {
    pub rect: Rect,
    /// For lines: start point, length and angle (radians).
    pub line: Option<(Vec2, f32, f32)>,
}

/// Computes the drawn geometry for a drag from `start` to `cur`.
/// Shift constrains (square / 15 degree steps), alt draws from the center.
pub fn draw_geom(tool: Tool, start: Vec2, cur: Vec2, shift: bool, alt: bool) -> DrawGeom {
    let mut dx = cur.x - start.x;
    let mut dy = cur.y - start.y;
    if matches!(tool, Tool::Line | Tool::Arrow) {
        let mut len = (dx * dx + dy * dy).sqrt();
        let mut ang = dy.atan2(dx);
        if shift {
            ang = crate::canvas::handles::snap_angle(ang, 15.0);
        }
        let origin = if alt { sub(start, v2(ang.cos() * len, ang.sin() * len)) } else { start };
        if alt {
            len *= 2.0;
        }
        dx = ang.cos() * len;
        dy = ang.sin() * len;
        let end = add(origin, v2(dx, dy));
        return DrawGeom { rect: rect_from_points(origin, end), line: Some((origin, len, ang)) };
    }
    if shift {
        let s = dx.abs().max(dy.abs());
        dx = s * if dx < 0.0 { -1.0 } else { 1.0 };
        dy = s * if dy < 0.0 { -1.0 } else { 1.0 };
    }
    let rect =
        if alt { Rect::new(start.x - dx.abs(), start.y - dy.abs(), dx.abs() * 2.0, dy.abs() * 2.0) } else { rect_from_points(start, add(start, v2(dx, dy))) };
    DrawGeom { rect, line: None }
}

/// Deepest frame-like container containing `p` (page space), skipping `exclude`.
pub fn container_at(doc: &Document, page: NodeId, p: Vec2, exclude: &[NodeId]) -> NodeId {
    fn rec(doc: &Document, parent: NodeId, parent_world: Affine, p: Vec2, exclude: &[NodeId]) -> Option<NodeId> {
        let pn = doc.get(parent)?;
        for &c in pn.children.iter().rev() {
            if exclude.contains(&c) {
                continue;
            }
            let Some(n) = doc.get(c) else { continue };
            if !n.visible || n.locked {
                continue;
            }
            let world = parent_world * n.transform;
            let Some(inv) = world.invert() else { continue };
            let l = inv.apply(p);
            let inside = l.x >= 0.0 && l.y >= 0.0 && l.x <= n.size.x && l.y <= n.size.y;
            let frame_like = matches!(n.kind, NodeKind::Frame(_) | NodeKind::Section | NodeKind::Component(_) | NodeKind::ComponentSet(_));
            if frame_like && inside {
                return rec(doc, c, world, p, exclude).or(Some(c));
            }
            if n.kind.is_container() && !frame_like {
                if let Some(h) = rec(doc, c, world, p, exclude) {
                    return Some(h);
                }
            }
        }
        None
    }
    rec(doc, page, Affine::IDENTITY, p, exclude).unwrap_or(page)
}

fn grey() -> Paint {
    Paint::solid(Color::from_hex(&format!("{:06X}", theme::DEFAULT_SHAPE_FILL)).unwrap_or(Color::WHITE))
}

/// A fresh node for a drawing tool (size/transform filled in by the caller).
pub fn new_shape(app: &mut App, tool: Tool) -> Node {
    let id = app.doc.alloc_id();
    let r = Rect::new(0.0, 0.0, 0.0, 0.0);
    let mut n = match tool {
        Tool::Frame => {
            let mut f = Node::frame(id, r);
            f.fills = vec![Paint::solid(Color::from_hex(&format!("{:06X}", theme::DEFAULT_FRAME_FILL)).unwrap_or(Color::WHITE))];
            f.name = app.unique_name("Frame");
            f
        }
        Tool::Section => {
            let mut s = Node::new(id, NodeKind::Section);
            s.fills = vec![Paint::solid(Color::rgb(0xF5, 0xF5, 0xF5))];
            s.name = app.unique_name("Section");
            s
        }
        Tool::Slice => {
            let mut s = Node::rectangle(id, r);
            s.fills.clear();
            s.name = app.unique_name("Slice");
            s
        }
        Tool::Ellipse => {
            let mut e = Node::ellipse(id, r);
            e.name = app.unique_name("Ellipse");
            e
        }
        Tool::Polygon => {
            let mut s = Node::new(id, NodeKind::Polygon { count: 3 });
            s.fills = vec![grey()];
            s.name = app.unique_name("Polygon");
            s
        }
        Tool::Star => {
            let mut s = Node::new(id, NodeKind::Star { count: 5, inner_ratio: 0.382 });
            s.fills = vec![grey()];
            s.name = app.unique_name("Star");
            s
        }
        Tool::Line | Tool::Arrow => {
            let mut s = Node::new(id, NodeKind::Line);
            s.stroke.paints = vec![Paint::solid(Color::from_hex(&format!("{:06X}", theme::DEFAULT_STROKE)).unwrap_or(Color::BLACK))];
            s.stroke.weight = 1.0;
            s.stroke.align = StrokeAlign::Center;
            if tool == Tool::Arrow {
                s.stroke.cap = StrokeCap::Round;
            }
            s.name = app.unique_name(if tool == Tool::Arrow { "Arrow" } else { "Line" });
            s
        }
        _ => {
            let mut s = Node::rectangle(id, r);
            s.name = app.unique_name("Rectangle");
            s
        }
    };
    n.size = Vec2::ZERO;
    n
}

/// Applies draw geometry to `node` (id already inserted in `parent`).
pub fn apply_geom(doc: &mut Document, tx: &mut Tx, id: NodeId, parent: NodeId, g: &DrawGeom) {
    let Some(inv) = doc.world_transform(parent).invert() else { return };
    let Some(n) = tx.ed.node(doc, id) else { return };
    match g.line {
        Some((origin, len, ang)) => {
            let o = inv.apply(origin);
            n.transform = Affine::translate(o.x, o.y) * Affine::rotate(ang);
            n.size = v2(len.max(0.0), 0.0);
        }
        None => {
            let o = inv.apply(v2(g.rect.x, g.rect.y));
            n.transform = Affine::translate(o.x, o.y);
            n.size = v2(g.rect.w.max(1.0), g.rect.h.max(1.0));
        }
    }
    layout::relayout_up(doc, &mut tx.ed, id);
}

/// A new empty text node placed at `at` (page space) inside `parent`.
pub fn new_text(app: &mut App, parent: NodeId, at: Vec2) -> Option<(Node, Vec2)> {
    let inv = app.doc.world_transform(parent).invert()?;
    let o = inv.apply(at);
    let id = app.doc.alloc_id();
    let style = TextStyle { font_size: theme::DEFAULT_TEXT_SIZE, fills: vec![Paint::solid(Color::BLACK)], ..Default::default() };
    let mut n = Node::text(id, Rect::new(o.x, o.y, 10.0, 10.0), "", style);
    if let NodeKind::Text(t) = &n.kind {
        n.size = layout::estimate_text_size(t);
    }
    n.name = "Text".into();
    Some((n, o))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PenPoint {
    pub pos: Vec2,
    pub hin: Option<Vec2>,
    pub hout: Option<Vec2>,
}

impl PenPoint {
    pub fn new(pos: Vec2) -> Self {
        Self { pos, hin: None, hout: None }
    }
}

/// Builds path commands for `pts` (page space) shifted by `-origin`.
pub fn pen_path(pts: &[PenPoint], closed: bool, origin: Vec2) -> Path {
    let o = |p: Vec2| sub(p, origin);
    let mut cmds = Vec::new();
    let Some(first) = pts.first() else { return Path::default() };
    cmds.push(PathCmd::MoveTo(o(first.pos)));
    let seg = |cmds: &mut Vec<PathCmd>, a: &PenPoint, b: &PenPoint| {
        if a.hout.is_some() || b.hin.is_some() {
            cmds.push(PathCmd::CubicTo(o(a.hout.unwrap_or(a.pos)), o(b.hin.unwrap_or(b.pos)), o(b.pos)));
        } else {
            cmds.push(PathCmd::LineTo(o(b.pos)));
        }
    };
    for w in pts.windows(2) {
        seg(&mut cmds, &w[0], &w[1]);
    }
    if closed && pts.len() > 2 {
        seg(&mut cmds, pts.last().unwrap(), first);
        cmds.push(PathCmd::Close);
    }
    Path { cmds, fill_rule: FillRule::NonZero }
}

/// Bounding box of the pen path including sampled curves.
pub fn pen_bounds(pts: &[PenPoint], closed: bool) -> Rect {
    let mut samples = Vec::new();
    let n = pts.len();
    let segs = if closed && n > 2 { n } else { n.saturating_sub(1) };
    for i in 0..segs {
        let a = &pts[i];
        let b = &pts[(i + 1) % n];
        let (c1, c2) = (a.hout.unwrap_or(a.pos), b.hin.unwrap_or(b.pos));
        for k in 0..=16 {
            let t = k as f32 / 16.0;
            let u = 1.0 - t;
            let x = u * u * u * a.pos.x + 3.0 * u * u * t * c1.x + 3.0 * u * t * t * c2.x + t * t * t * b.pos.x;
            let y = u * u * u * a.pos.y + 3.0 * u * u * t * c1.y + 3.0 * u * t * t * c2.y + t * t * t * b.pos.y;
            samples.push(v2(x, y));
        }
    }
    if samples.is_empty() {
        samples.extend(pts.iter().map(|p| p.pos));
    }
    aabb_of(&samples)
}

/// Creates a Vector node from pen points in `tx`; returns its id.
pub fn commit_pen(app: &mut App, tx: &mut Tx, pts: &[PenPoint], closed: bool) -> Option<NodeId> {
    if pts.len() < 2 {
        return None;
    }
    let parent = container_at(&app.doc, app.page, pts[0].pos, &[]);
    let inv = app.doc.world_transform(parent).invert()?;
    let local_pts: Vec<PenPoint> =
        pts.iter().map(|p| PenPoint { pos: inv.apply(p.pos), hin: p.hin.map(|h| inv.apply(h)), hout: p.hout.map(|h| inv.apply(h)) }).collect();
    let bb = pen_bounds(&local_pts, closed);
    let origin = v2(bb.x, bb.y);
    let id = app.doc.alloc_id();
    let mut n = Node::new(id, NodeKind::Vector { paths: vec![pen_path(&local_pts, closed, origin)] });
    n.name = app.unique_name("Vector");
    n.transform = Affine::translate(bb.x, bb.y);
    n.size = v2(bb.w.max(1.0), bb.h.max(1.0));
    n.stroke.paints = vec![Paint::solid(Color::BLACK)];
    n.stroke.weight = 1.0;
    n.stroke.align = StrokeAlign::Center;
    let index = app.doc.get(parent).map(|p| p.children.len()).unwrap_or(0);
    tx.cmd(&mut app.doc, Command::Insert { parent, index, nodes: vec![n] }).ok()?;
    Some(id)
}

/// Freehand polyline from raw points (pencil tool), lightly decimated.
pub fn decimate(points: &[Vec2], min_dist: f32) -> Vec<PenPoint> {
    let mut out: Vec<PenPoint> = Vec::new();
    for p in points {
        if out.last().map(|l| dist(l.pos, *p) >= min_dist).unwrap_or(true) {
            out.push(PenPoint::new(*p));
        }
    }
    if let (Some(last), Some(raw)) = (out.last().copied(), points.last()) {
        if last.pos != *raw {
            out.push(PenPoint::new(*raw));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_drag_and_modifiers() {
        let g = draw_geom(Tool::Rectangle, v2(10.0, 10.0), v2(50.0, 30.0), false, false);
        assert_eq!(g.rect, Rect::new(10.0, 10.0, 40.0, 20.0));
        let g = draw_geom(Tool::Rectangle, v2(10.0, 10.0), v2(50.0, 30.0), true, false);
        assert_eq!((g.rect.w, g.rect.h), (40.0, 40.0));
        let g = draw_geom(Tool::Rectangle, v2(10.0, 10.0), v2(0.0, 0.0), false, false);
        assert_eq!(g.rect, Rect::new(0.0, 0.0, 10.0, 10.0));
        let g = draw_geom(Tool::Ellipse, v2(100.0, 100.0), v2(120.0, 110.0), false, true);
        assert_eq!(g.rect, Rect::new(80.0, 90.0, 40.0, 20.0));
    }

    #[test]
    fn line_drag_snaps_with_shift() {
        let g = draw_geom(Tool::Line, v2(0.0, 0.0), v2(100.0, 10.0), true, false);
        let (_, len, ang) = g.line.unwrap();
        assert!(ang.abs() < 1e-5, "snaps to 0 degrees");
        assert!((len - (100.0f32 * 100.0 + 100.0).sqrt()).abs() < 1e-3);
        let g = draw_geom(Tool::Line, v2(0.0, 0.0), v2(30.0, 40.0), false, false);
        assert!((g.line.unwrap().1 - 50.0).abs() < 1e-4);
    }

    #[test]
    fn container_lookup_prefers_deepest_frame() {
        let mut d = Document::new("t");
        let page = d.add_page("p");
        let a = d.alloc_id();
        let outer = d.insert(page, Node::frame(a, Rect::new(0.0, 0.0, 200.0, 200.0))).unwrap();
        let b = d.alloc_id();
        let inner = d.insert(outer, Node::frame(b, Rect::new(50.0, 50.0, 100.0, 100.0))).unwrap();
        assert_eq!(container_at(&d, page, v2(60.0, 60.0), &[]), inner);
        assert_eq!(container_at(&d, page, v2(10.0, 10.0), &[]), outer);
        assert_eq!(container_at(&d, page, v2(500.0, 10.0), &[]), page);
        assert_eq!(container_at(&d, page, v2(60.0, 60.0), &[inner]), outer);
    }

    #[test]
    fn pen_path_lines_and_curves() {
        let pts = [PenPoint::new(v2(10.0, 10.0)), PenPoint { pos: v2(50.0, 10.0), hin: Some(v2(40.0, 0.0)), hout: None }, PenPoint::new(v2(50.0, 50.0))];
        let p = pen_path(&pts, false, v2(10.0, 10.0));
        assert_eq!(p.cmds.len(), 3);
        assert!(matches!(p.cmds[0], PathCmd::MoveTo(v) if v == v2(0.0, 0.0)));
        assert!(matches!(p.cmds[1], PathCmd::CubicTo(..)));
        assert!(matches!(p.cmds[2], PathCmd::LineTo(_)));
        let closed = pen_path(&pts, true, v2(0.0, 0.0));
        assert!(matches!(closed.cmds.last(), Some(PathCmd::Close)));
        let bb = pen_bounds(&pts, false);
        assert!(bb.y < 10.0, "curve bulges above the straight segment");
    }

    #[test]
    fn decimate_drops_close_points_keeps_last() {
        let raw: Vec<Vec2> = (0..20).map(|i| v2(i as f32 * 0.5, 0.0)).collect();
        let d = decimate(&raw, 3.0);
        assert!(d.len() < raw.len());
        assert_eq!(d.last().unwrap().pos, *raw.last().unwrap());
    }
}
