//! Snapping of a moving rectangle (or of dragged edges) to other nodes'
//! edges/centers, plus the guide lines and distance measurements Figma shows.

use desigma_doc::Rect;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GuideAxis {
    /// Vertical line at x.
    X,
    /// Horizontal line at y.
    Y,
}

/// An alignment guide: a line at `pos` spanning `from..to` on the other axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Guide {
    pub axis: GuideAxis,
    pub pos: f32,
    pub from: f32,
    pub to: f32,
}

/// A distance measurement between two rects, drawn as a line with a label.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measure {
    pub a: (f32, f32),
    pub b: (f32, f32),
    pub value: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SnapResult {
    pub dx: f32,
    pub dy: f32,
    pub guides: Vec<Guide>,
    pub measures: Vec<Measure>,
}

fn xs(r: &Rect) -> [f32; 3] {
    [r.x, r.x + r.w * 0.5, r.x + r.w]
}
fn ys(r: &Rect) -> [f32; 3] {
    [r.y, r.y + r.h * 0.5, r.y + r.h]
}

/// Best (smallest |delta|) shift taking any of `mine` onto any of `targets`
/// within `threshold`. Returns the shift and the target coordinate hit.
pub fn snap_values(mine: &[f32], targets: &[f32], threshold: f32) -> Option<(f32, f32)> {
    let mut best: Option<(f32, f32)> = None;
    for m in mine {
        for t in targets {
            let d = t - m;
            if d.abs() <= threshold && best.map(|(bd, _)| d.abs() < bd.abs()).unwrap_or(true) {
                best = Some((d, *t));
            }
        }
    }
    best
}

/// Snaps `moving` (all of left/center/right and top/middle/bottom) to `others`.
/// `threshold` is in page units (callers divide the pixel threshold by zoom).
pub fn snap_rect(moving: &Rect, others: &[Rect], threshold: f32) -> SnapResult {
    let ox: Vec<f32> = others.iter().flat_map(xs).collect();
    let oy: Vec<f32> = others.iter().flat_map(ys).collect();
    let sx = snap_values(&xs(moving), &ox, threshold);
    let sy = snap_values(&ys(moving), &oy, threshold);
    finish(moving, others, sx, sy)
}

/// Snaps only specific edge coordinates (used while resizing: only the dragged
/// edges move). `x_edges`/`y_edges` are the coordinates currently dragged.
pub fn snap_edges(moving: &Rect, x_edges: &[f32], y_edges: &[f32], others: &[Rect], threshold: f32) -> SnapResult {
    let ox: Vec<f32> = others.iter().flat_map(xs).collect();
    let oy: Vec<f32> = others.iter().flat_map(ys).collect();
    let sx = if x_edges.is_empty() { None } else { snap_values(x_edges, &ox, threshold) };
    let sy = if y_edges.is_empty() { None } else { snap_values(y_edges, &oy, threshold) };
    finish(moving, others, sx, sy)
}

fn finish(moving: &Rect, others: &[Rect], sx: Option<(f32, f32)>, sy: Option<(f32, f32)>) -> SnapResult {
    let dx = sx.map(|s| s.0).unwrap_or(0.0);
    let dy = sy.map(|s| s.0).unwrap_or(0.0);
    let snapped = Rect::new(moving.x + dx, moving.y + dy, moving.w, moving.h);
    let mut guides = Vec::new();
    if let Some((_, line)) = sx {
        let (mut from, mut to) = (snapped.y, snapped.y + snapped.h);
        for o in others {
            if xs(o).iter().any(|v| (v - line).abs() < 1e-3) {
                from = from.min(o.y);
                to = to.max(o.y + o.h);
            }
        }
        guides.push(Guide { axis: GuideAxis::X, pos: line, from, to });
    }
    if let Some((_, line)) = sy {
        let (mut from, mut to) = (snapped.x, snapped.x + snapped.w);
        for o in others {
            if ys(o).iter().any(|v| (v - line).abs() < 1e-3) {
                from = from.min(o.x);
                to = to.max(o.x + o.w);
            }
        }
        guides.push(Guide { axis: GuideAxis::Y, pos: line, from, to });
    }
    let measures = if guides.is_empty() { vec![] } else { measurements(&snapped, others) };
    SnapResult { dx, dy, guides, measures }
}

/// Distances from `r` to the nearest neighbour on each side that overlaps it
/// on the perpendicular axis.
pub fn measurements(r: &Rect, others: &[Rect]) -> Vec<Measure> {
    let mut out = Vec::new();
    let cy = r.y + r.h * 0.5;
    let cx = r.x + r.w * 0.5;
    let mut left: Option<f32> = None; // right edge of the nearest rect on the left
    let mut right: Option<f32> = None;
    let mut up: Option<f32> = None;
    let mut down: Option<f32> = None;
    for o in others {
        let y_overlap = o.y < r.y + r.h && o.y + o.h > r.y;
        let x_overlap = o.x < r.x + r.w && o.x + o.w > r.x;
        if y_overlap {
            if o.x + o.w <= r.x {
                left = Some(left.map_or(o.x + o.w, |l: f32| l.max(o.x + o.w)));
            } else if o.x >= r.x + r.w {
                right = Some(right.map_or(o.x, |l: f32| l.min(o.x)));
            }
        }
        if x_overlap {
            if o.y + o.h <= r.y {
                up = Some(up.map_or(o.y + o.h, |l: f32| l.max(o.y + o.h)));
            } else if o.y >= r.y + r.h {
                down = Some(down.map_or(o.y, |l: f32| l.min(o.y)));
            }
        }
    }
    if let Some(l) = left {
        out.push(Measure { a: (l, cy), b: (r.x, cy), value: r.x - l });
    }
    if let Some(l) = right {
        out.push(Measure { a: (r.x + r.w, cy), b: (l, cy), value: l - (r.x + r.w) });
    }
    if let Some(l) = up {
        out.push(Measure { a: (cx, l), b: (cx, r.y), value: r.y - l });
    }
    if let Some(l) = down {
        out.push(Measure { a: (cx, r.y + r.h), b: (cx, l), value: l - (r.y + r.h) });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snaps_left_edge_to_other_right_edge() {
        let other = Rect::new(0.0, 0.0, 100.0, 100.0);
        let moving = Rect::new(102.0, 300.0, 50.0, 50.0);
        let r = snap_rect(&moving, &[other], 4.0);
        assert_eq!(r.dx, -2.0);
        assert_eq!(r.dy, 0.0);
        assert_eq!(r.guides.len(), 1);
        assert_eq!(r.guides[0].axis, GuideAxis::X);
        assert_eq!(r.guides[0].pos, 100.0);
        assert_eq!(r.guides[0].from, 0.0);
        assert_eq!(r.guides[0].to, 350.0);
    }

    #[test]
    fn snaps_centers() {
        let other = Rect::new(0.0, 0.0, 100.0, 100.0); // cx = 50
        let moving = Rect::new(23.0, 200.0, 50.0, 50.0); // cx = 48 -> snap +2
        let r = snap_rect(&moving, &[other], 4.0);
        assert_eq!(r.dx, 2.0);
    }

    #[test]
    fn no_snap_outside_threshold() {
        let other = Rect::new(0.0, 0.0, 100.0, 100.0);
        let moving = Rect::new(300.0, 300.0, 50.0, 50.0);
        let r = snap_rect(&moving, &[other], 4.0);
        assert_eq!((r.dx, r.dy), (0.0, 0.0));
        assert!(r.guides.is_empty() && r.measures.is_empty());
    }

    #[test]
    fn picks_closest_candidate() {
        assert_eq!(snap_values(&[10.0], &[7.0, 11.0, 13.0], 5.0), Some((1.0, 11.0)));
        assert_eq!(snap_values(&[10.0], &[], 5.0), None);
    }

    #[test]
    fn measures_gap_to_neighbour() {
        let a = Rect::new(0.0, 0.0, 100.0, 100.0);
        let r = Rect::new(150.0, 0.0, 50.0, 100.0);
        let m = measurements(&r, &[a]);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].value, 50.0);
    }

    #[test]
    fn edge_snap_only_moves_dragged_edge() {
        let other = Rect::new(0.0, 0.0, 100.0, 100.0);
        let moving = Rect::new(200.0, 3.0, 50.0, 50.0);
        // dragging only the top edge (y=3) should snap to y=0 and ignore x
        let r = snap_edges(&moving, &[], &[moving.y], &[other], 4.0);
        assert_eq!((r.dx, r.dy), (0.0, -3.0));
    }
}
