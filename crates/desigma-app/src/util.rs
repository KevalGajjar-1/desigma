//! Small geometry helpers on top of the shared document model.

use desigma_doc::{Affine, Document, NodeId, Rect, Vec2};

pub fn v2(x: f32, y: f32) -> Vec2 {
    Vec2::new(x, y)
}
pub fn add(a: Vec2, b: Vec2) -> Vec2 {
    v2(a.x + b.x, a.y + b.y)
}
pub fn sub(a: Vec2, b: Vec2) -> Vec2 {
    v2(a.x - b.x, a.y - b.y)
}
pub fn scale(a: Vec2, s: f32) -> Vec2 {
    v2(a.x * s, a.y * s)
}
pub fn dist(a: Vec2, b: Vec2) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}
/// Applies only the linear part of `t` (no translation).
pub fn apply_linear(t: &Affine, p: Vec2) -> Vec2 {
    v2(t.a * p.x + t.c * p.y, t.b * p.x + t.d * p.y)
}

pub fn rect_center(r: &Rect) -> Vec2 {
    v2(r.x + r.w * 0.5, r.y + r.h * 0.5)
}

pub fn rect_from_points(a: Vec2, b: Vec2) -> Rect {
    Rect::new(a.x.min(b.x), a.y.min(b.y), (a.x - b.x).abs(), (a.y - b.y).abs())
}

pub fn aabb_of(points: &[Vec2]) -> Rect {
    if points.is_empty() {
        return Rect::default();
    }
    let mut x0 = f32::MAX;
    let mut y0 = f32::MAX;
    let mut x1 = f32::MIN;
    let mut y1 = f32::MIN;
    for p in points {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

pub fn rect_corners(r: &Rect) -> [Vec2; 4] {
    [v2(r.x, r.y), v2(r.x + r.w, r.y), v2(r.x + r.w, r.y + r.h), v2(r.x, r.y + r.h)]
}

/// Page-space corners of a node (tl, tr, br, bl).
pub fn world_corners(doc: &Document, id: NodeId) -> [Vec2; 4] {
    let Some(n) = doc.get(id) else { return [Vec2::ZERO; 4] };
    let w = doc.world_transform(id);
    let r = Rect::new(0.0, 0.0, n.size.x, n.size.y);
    rect_corners(&r).map(|p| w.apply(p))
}

pub fn world_aabb(doc: &Document, id: NodeId) -> Rect {
    aabb_of(&world_corners(doc, id))
}

pub fn union_all(rects: impl IntoIterator<Item = Rect>) -> Option<Rect> {
    rects.into_iter().reduce(|a, b| a.union(&b))
}

pub fn parent_world(doc: &Document, id: NodeId) -> Affine {
    match doc.get(id).and_then(|n| n.parent) {
        Some(p) => doc.world_transform(p),
        None => Affine::IDENTITY,
    }
}

/// Converts a page-space delta to the parent space of `id`.
pub fn delta_to_parent(doc: &Document, id: NodeId, d: Vec2) -> Vec2 {
    match parent_world(doc, id).invert() {
        Some(inv) => apply_linear(&inv, d),
        None => d,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use desigma_doc::Node;

    #[test]
    fn world_aabb_of_nested() {
        let mut d = Document::new("t");
        let page = d.add_page("p");
        let fid = d.alloc_id();
        let f = d.insert(page, Node::frame(fid, Rect::new(100.0, 50.0, 200.0, 200.0))).unwrap();
        let rid = d.alloc_id();
        let r = d.insert(f, Node::rectangle(rid, Rect::new(10.0, 20.0, 30.0, 40.0))).unwrap();
        let bb = world_aabb(&d, r);
        assert_eq!((bb.x, bb.y, bb.w, bb.h), (110.0, 70.0, 30.0, 40.0));
        let dp = delta_to_parent(&d, r, v2(5.0, 5.0));
        assert_eq!((dp.x, dp.y), (5.0, 5.0));
    }

    #[test]
    fn aabb_points() {
        let r = aabb_of(&[v2(1.0, 5.0), v2(-3.0, 2.0)]);
        assert_eq!((r.x, r.y, r.w, r.h), (-3.0, 2.0, 4.0, 3.0));
    }
}
