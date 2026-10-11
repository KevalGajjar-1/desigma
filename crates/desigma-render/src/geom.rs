//! Node geometry: converts node kinds into tiny-skia paths in node-local space.

use crate::boolean;
use desigma_doc as d;
use desigma_doc::{Document, Node, NodeKind};
use std::f32::consts::{PI, TAU};
use tiny_skia as sk;

const KAPPA: f32 = 0.552_284_8;

/// Fill/stroke geometry of a node in local space.
#[derive(Clone, Debug)]
pub struct Shape {
    pub paths: Vec<(sk::Path, sk::FillRule)>,
    /// All sub-paths are closed (inside/outside stroke alignment is meaningful).
    pub closed: bool,
    /// Whether fills apply (lines have no fill).
    pub fillable: bool,
    /// Plain rectangle (used by per-side stroke weights).
    pub is_rect: bool,
    /// Precomputed stroke outline (`Node::stroke_geometry`): filled with the
    /// stroke paints instead of stroking `paths`.
    pub stroke_paths: Option<Vec<(sk::Path, sk::FillRule)>>,
}

impl Shape {
    fn single(path: sk::Path, rule: sk::FillRule, closed: bool) -> Self {
        Self { paths: vec![(path, rule)], closed, fillable: true, is_rect: false, stroke_paths: None }
    }
}

pub fn to_sk_transform(a: &d::Affine) -> sk::Transform {
    sk::Transform::from_row(a.a, a.b, a.c, a.d, a.tx, a.ty)
}

pub fn fill_rule(r: d::FillRule) -> sk::FillRule {
    match r {
        d::FillRule::NonZero => sk::FillRule::Winding,
        d::FillRule::EvenOdd => sk::FillRule::EvenOdd,
    }
}

/// Appends an elliptical arc as cubic segments; assumes the current point is
/// already at the arc start. Angles are in radians (y-down, clockwise positive).
fn arc_to(pb: &mut sk::PathBuilder, cx: f32, cy: f32, rx: f32, ry: f32, a0: f32, sweep: f32) {
    let n = ((sweep.abs() / (PI / 2.0)).ceil() as usize).max(1);
    let step = sweep / n as f32;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let mut a = a0;
    for _ in 0..n {
        let b = a + step;
        let (sa, ca) = a.sin_cos();
        let (sb, cb) = b.sin_cos();
        pb.cubic_to(
            cx + rx * (ca - k * sa),
            cy + ry * (sa + k * ca),
            cx + rx * (cb + k * sb),
            cy + ry * (sb - k * cb),
            cx + rx * cb,
            cy + ry * sb,
        );
        a = b;
    }
}

fn oval(pb: &mut sk::PathBuilder, cx: f32, cy: f32, rx: f32, ry: f32, reverse: bool) {
    pb.move_to(cx + rx, cy);
    arc_to(pb, cx, cy, rx, ry, 0.0, if reverse { -TAU } else { TAU });
    pb.close();
}

/// Rounded rectangle with independent corner radii (tl, tr, br, bl), each
/// clamped to half of the smaller side.
pub fn rounded_rect(w: f32, h: f32, radii: [f32; 4]) -> Option<sk::Path> {
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let lim = w.min(h) / 2.0;
    let r = radii.map(|v| v.max(0.0).min(lim));
    let mut pb = sk::PathBuilder::new();
    if r.iter().all(|v| *v <= 0.0) {
        pb.move_to(0.0, 0.0);
        pb.line_to(w, 0.0);
        pb.line_to(w, h);
        pb.line_to(0.0, h);
        pb.close();
        return pb.finish();
    }
    let [tl, tr, br, bl] = r;
    let k = KAPPA;
    pb.move_to(tl, 0.0);
    pb.line_to(w - tr, 0.0);
    pb.cubic_to(w - tr + tr * k, 0.0, w, tr - tr * k, w, tr);
    pb.line_to(w, h - br);
    pb.cubic_to(w, h - br + br * k, w - br + br * k, h, w - br, h);
    pb.line_to(bl, h);
    pb.cubic_to(bl - bl * k, h, 0.0, h - bl + bl * k, 0.0, h - bl);
    pb.line_to(0.0, tl);
    pb.cubic_to(0.0, tl - tl * k, tl - tl * k, 0.0, tl, 0.0);
    pb.close();
    pb.finish()
}

fn ellipse_path(w: f32, h: f32, arc: &d::ArcData) -> Option<sk::Path> {
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let (cx, cy, rx, ry) = (w / 2.0, h / 2.0, w / 2.0, h / 2.0);
    let ir = arc.inner_radius.clamp(0.0, 0.999);
    let raw = arc.end - arc.start;
    let full = raw.abs() >= TAU - 1e-4;
    let mut pb = sk::PathBuilder::new();
    if full {
        oval(&mut pb, cx, cy, rx, ry, false);
        if ir > 0.0 {
            oval(&mut pb, cx, cy, rx * ir, ry * ir, true);
        }
        return pb.finish();
    }
    let mut sweep = raw;
    if sweep < 0.0 {
        sweep += TAU;
    }
    if sweep <= 1e-6 {
        return None;
    }
    let a0 = arc.start;
    let a1 = a0 + sweep;
    let pt = |a: f32, r: f32| (cx + rx * r * a.cos(), cy + ry * r * a.sin());
    let (sx, sy) = pt(a0, 1.0);
    if ir > 0.0 {
        pb.move_to(sx, sy);
        arc_to(&mut pb, cx, cy, rx, ry, a0, sweep);
        let (ex, ey) = pt(a1, ir);
        pb.line_to(ex, ey);
        arc_to(&mut pb, cx, cy, rx * ir, ry * ir, a1, -sweep);
        pb.close();
    } else {
        pb.move_to(cx, cy);
        pb.line_to(sx, sy);
        arc_to(&mut pb, cx, cy, rx, ry, a0, sweep);
        pb.close();
    }
    pb.finish()
}

/// Polygon/star outline normalised to fill the node bounds.
fn poly_path(w: f32, h: f32, pts: &[(f32, f32)]) -> Option<sk::Path> {
    if pts.len() < 3 || w <= 0.0 || h <= 0.0 {
        return None;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (x, y) in pts {
        x0 = x0.min(*x);
        y0 = y0.min(*y);
        x1 = x1.max(*x);
        y1 = y1.max(*y);
    }
    let (sw, sh) = ((x1 - x0).max(1e-6), (y1 - y0).max(1e-6));
    let mut pb = sk::PathBuilder::new();
    for (i, (x, y)) in pts.iter().enumerate() {
        let (px, py) = ((x - x0) / sw * w, (y - y0) / sh * h);
        if i == 0 {
            pb.move_to(px, py);
        } else {
            pb.line_to(px, py);
        }
    }
    pb.close();
    pb.finish()
}

fn polygon_points(count: u32) -> Vec<(f32, f32)> {
    let n = count.max(3);
    (0..n)
        .map(|i| {
            let a = -PI / 2.0 + TAU * i as f32 / n as f32;
            (a.cos(), a.sin())
        })
        .collect()
}

fn star_points(count: u32, inner: f32) -> Vec<(f32, f32)> {
    let n = count.max(3);
    (0..n * 2)
        .map(|i| {
            let a = -PI / 2.0 + PI * i as f32 / n as f32;
            let r = if i % 2 == 0 { 1.0 } else { inner.clamp(0.0, 1.0) };
            (a.cos() * r, a.sin() * r)
        })
        .collect()
}

/// Converts document vector paths to tiny-skia paths.
pub fn vector_paths(paths: &[d::Path]) -> Shape {
    let mut out = Vec::new();
    let mut closed = true;
    for p in paths {
        let mut pb = sk::PathBuilder::new();
        let mut open = false;
        let mut started = false;
        for c in &p.cmds {
            match *c {
                d::PathCmd::MoveTo(v) => {
                    if open {
                        closed = false;
                    }
                    pb.move_to(v.x, v.y);
                    started = true;
                    open = true;
                }
                d::PathCmd::LineTo(v) => {
                    if !started {
                        pb.move_to(v.x, v.y);
                        started = true;
                    } else {
                        pb.line_to(v.x, v.y);
                    }
                    open = true;
                }
                d::PathCmd::QuadTo(c1, v) => {
                    if started {
                        pb.quad_to(c1.x, c1.y, v.x, v.y);
                    }
                    open = true;
                }
                d::PathCmd::CubicTo(c1, c2, v) => {
                    if started {
                        pb.cubic_to(c1.x, c1.y, c2.x, c2.y, v.x, v.y);
                    }
                    open = true;
                }
                d::PathCmd::Close => {
                    pb.close();
                    open = false;
                }
            }
        }
        if open {
            closed = false;
        }
        if let Some(path) = pb.finish() {
            out.push((path, fill_rule(p.fill_rule)));
        }
    }
    Shape { paths: out, closed, fillable: true, is_rect: false, stroke_paths: None }
}

/// Geometry for a node, or `None` when it has no own geometry (groups, pages,
/// text, unknown). `tol` is the flattening tolerance in local units used by
/// boolean operations.
///
/// Precomputed `fill_geometry` / `stroke_geometry` (imported from Figma) take
/// precedence over geometry derived from the node kind.
pub fn node_shape(doc: &Document, n: &Node, tol: f32) -> Option<Shape> {
    let given = n.fill_geometry.as_deref().map(vector_paths).filter(|s| !s.paths.is_empty());
    let mut shape = match given {
        Some(s) => Some(s),
        None => derived_shape(doc, n, tol),
    }?;
    if let Some(sg) = n.stroke_geometry.as_deref().map(vector_paths).filter(|s| !s.paths.is_empty()) {
        shape.stroke_paths = Some(sg.paths);
    }
    Some(shape)
}

fn derived_shape(doc: &Document, n: &Node, tol: f32) -> Option<Shape> {
    let (w, h) = (n.size.x, n.size.y);
    match &n.kind {
        NodeKind::Frame(_)
        | NodeKind::Component(_)
        | NodeKind::ComponentSet(_)
        | NodeKind::Instance { .. }
        | NodeKind::Section
        | NodeKind::Rectangle => {
            let radii = if matches!(n.kind, NodeKind::Section) { [0.0; 4] } else { n.corner_radii };
            let mut s = Shape::single(rounded_rect(w, h, radii)?, sk::FillRule::Winding, true);
            s.is_rect = true;
            Some(s)
        }
        NodeKind::Ellipse(arc) => Some(Shape::single(ellipse_path(w, h, arc)?, sk::FillRule::Winding, true)),
        NodeKind::Line => {
            let mut pb = sk::PathBuilder::new();
            pb.move_to(0.0, 0.0);
            pb.line_to(w, 0.0);
            Some(Shape { paths: vec![(pb.finish()?, sk::FillRule::Winding)], closed: false, fillable: false, is_rect: false, stroke_paths: None })
        }
        NodeKind::Polygon { count } => Some(Shape::single(poly_path(w, h, &polygon_points(*count))?, sk::FillRule::Winding, true)),
        NodeKind::Star { count, inner_ratio } => {
            Some(Shape::single(poly_path(w, h, &star_points(*count, *inner_ratio))?, sk::FillRule::Winding, true))
        }
        NodeKind::Vector { paths } => {
            let s = vector_paths(paths);
            if s.paths.is_empty() {
                None
            } else {
                Some(s)
            }
        }
        NodeKind::BooleanOp(op) => boolean::boolean_shape(doc, n, *op, tol),
        NodeKind::Page { .. } | NodeKind::Group | NodeKind::Text(_) | NodeKind::Unknown { .. } => None,
    }
}

/// Axis-aligned bounds of a rect transformed by `ts`: (x0, y0, x1, y1).
pub fn transformed_bounds(ts: &sk::Transform, x: f32, y: f32, w: f32, h: f32) -> (f32, f32, f32, f32) {
    let pts = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (px, py) in pts {
        let tx = ts.sx * px + ts.kx * py + ts.tx;
        let ty = ts.ky * px + ts.sy * py + ts.ty;
        x0 = x0.min(tx);
        y0 = y0.min(ty);
        x1 = x1.max(tx);
        y1 = y1.max(ty);
    }
    (x0, y0, x1, y1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_rect_bounds() {
        let p = rounded_rect(100.0, 50.0, [30.0, 0.0, 0.0, 0.0]).unwrap();
        let b = p.bounds();
        assert_eq!((b.left(), b.top(), b.right(), b.bottom()), (0.0, 0.0, 100.0, 50.0));
    }
}
