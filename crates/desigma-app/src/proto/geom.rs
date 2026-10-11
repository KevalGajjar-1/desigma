//! Pure geometry for the prototype UI: connector ("noodle") curves, overlay
//! placement and the screen-space offsets of the stock transitions.

use desigma_doc::{Direction, OverlayPosition, Rect, TransitionKind, Vec2};
use egui::Pos2;

// ------------------------------------------------------------------ noodles

/// A connector from a hotspot to a destination frame: cubic bezier leaving
/// horizontally to the right, entering the destination's left edge, plus the
/// arrow head triangle at the destination.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Noodle {
    pub p0: Pos2,
    pub c0: Pos2,
    pub c1: Pos2,
    pub p1: Pos2,
    /// Arrow head: tip first.
    pub arrow: [Pos2; 3],
}

pub const ARROW_LEN: f32 = 9.0;

/// Where a layer's hotspot sits: the middle of its right edge.
pub fn hotspot_center(src: egui::Rect) -> Pos2 {
    Pos2::new(src.right(), src.center().y)
}

/// Connector from `src` (screen rect of the interactive layer) to `dst`
/// (screen rect of the destination frame).
pub fn noodle(src: egui::Rect, dst: egui::Rect) -> Noodle {
    let p0 = hotspot_center(src);
    // Land where the line from the source meets the frame's left edge, kept
    // clear of the corners so the arrow is never clipped.
    let margin = (dst.height() * 0.5).min(16.0);
    let y1 = p0.y.clamp(dst.top() + margin, dst.bottom() - margin);
    let p1 = Pos2::new(dst.left(), y1);
    let reach = ((p1.x - p0.x).abs() * 0.5).clamp(40.0, 260.0);
    let c0 = Pos2::new(p0.x + reach, p0.y);
    let c1 = Pos2::new(p1.x - reach, p1.y);
    let l = ARROW_LEN;
    Noodle { p0, c0, c1, p1, arrow: [p1, Pos2::new(p1.x - l, p1.y - l * 0.6), Pos2::new(p1.x - l, p1.y + l * 0.6)] }
}

impl Noodle {
    #[cfg(test)]
    pub fn point(&self, t: f32) -> Pos2 {
        let u = 1.0 - t;
        let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
        let pts = [self.p0, self.c0, self.c1, self.p1];
        let mut out = Pos2::ZERO;
        for (wi, p) in w.iter().zip(pts) {
            out.x += wi * p.x;
            out.y += wi * p.y;
        }
        out
    }

    /// Where the stroke should end so it does not poke through the arrow head.
    pub fn stroke_end(&self) -> Pos2 {
        Pos2::new(self.p1.x - ARROW_LEN * 0.8, self.p1.y)
    }

    /// The curve cut short of the arrow head, as a bezier with the same start.
    pub fn trimmed(&self) -> [Pos2; 4] {
        let e = self.stroke_end();
        [self.p0, self.c0, Pos2::new(e.x - (self.p1.x - self.c1.x), e.y), e]
    }

    /// Distance from `p` to the curve (sampled), for hit testing.
    #[cfg(test)]
    pub fn distance_to(&self, p: Pos2) -> f32 {
        let mut best = f32::MAX;
        let mut prev = self.p0;
        for i in 1..=24 {
            let q = self.point(i as f32 / 24.0);
            best = best.min(dist_to_segment(p, prev, q));
            prev = q;
        }
        best
    }
}

#[cfg(test)]
fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    if len2 < 1e-9 {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

// ----------------------------------------------------------------- overlays

/// Top-left of an overlay of `size` inside `base` (the frame it opens over,
/// both in the frame's coordinate space starting at 0,0). `trigger` is the
/// rect of the layer that fired the action, used by `Manual`.
pub fn overlay_origin(pos: OverlayPosition, offset: Vec2, size: Vec2, base: Vec2, trigger: Rect) -> Vec2 {
    let (cx, cy) = ((base.x - size.x) / 2.0, (base.y - size.y) / 2.0);
    let (rx, by) = (base.x - size.x, base.y - size.y);
    let (x, y) = match pos {
        OverlayPosition::Center => (cx, cy),
        OverlayPosition::TopLeft => (0.0, 0.0),
        OverlayPosition::TopCenter => (cx, 0.0),
        OverlayPosition::TopRight => (rx, 0.0),
        OverlayPosition::BottomLeft => (0.0, by),
        OverlayPosition::BottomCenter => (cx, by),
        OverlayPosition::BottomRight => (rx, by),
        // Anchored to the trigger layer, but kept inside the frame when it fits.
        OverlayPosition::Manual => {
            let (x, y) = (trigger.x + offset.x, trigger.y + offset.y);
            (if rx >= 0.0 { x.clamp(0.0, rx) } else { x }, if by >= 0.0 { y.clamp(0.0, by) } else { y })
        }
    };
    Vec2::new(x, y)
}

// -------------------------------------------------------------- transitions

/// Unit vector of the motion a direction stands for (Left = moving left).
pub fn dir_vec(d: Direction) -> Vec2 {
    match d {
        Direction::Left => Vec2::new(-1.0, 0.0),
        Direction::Right => Vec2::new(1.0, 0.0),
        Direction::Top => Vec2::new(0.0, -1.0),
        Direction::Bottom => Vec2::new(0.0, 1.0),
    }
}

pub fn opposite(d: Direction) -> Direction {
    match d {
        Direction::Left => Direction::Right,
        Direction::Right => Direction::Left,
        Direction::Top => Direction::Bottom,
        Direction::Bottom => Direction::Top,
    }
}

/// The transition played when going back through one.
pub fn reverse(k: TransitionKind) -> TransitionKind {
    match k {
        TransitionKind::Dissolve | TransitionKind::SmartAnimate => k,
        TransitionKind::MoveIn(d) => TransitionKind::MoveOut(opposite(d)),
        TransitionKind::MoveOut(d) => TransitionKind::MoveIn(opposite(d)),
        TransitionKind::Push(d) => TransitionKind::Push(opposite(d)),
        TransitionKind::SlideIn(d) => TransitionKind::SlideOut(opposite(d)),
        TransitionKind::SlideOut(d) => TransitionKind::SlideIn(opposite(d)),
    }
}

/// How one layer of a transition is drawn: offset in viewport px, opacity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub offset: Vec2,
    pub alpha: f32,
}

/// Poses of the outgoing and incoming frame at eased progress `p` (0..1, may
/// overshoot) in a viewport of `size`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Poses {
    pub from: Pose,
    pub to: Pose,
    /// Draw the incoming frame above the outgoing one.
    pub to_on_top: bool,
}

/// Portion of the frame the underlying screen shifts by in Slide transitions.
pub const SLIDE_PARALLAX: f32 = 0.3;

fn mul(v: Vec2, sx: f32, sy: f32) -> Vec2 {
    Vec2::new(v.x * sx, v.y * sy)
}

pub fn poses(kind: TransitionKind, p: f32, size: Vec2) -> Poses {
    let still = Pose { offset: Vec2::ZERO, alpha: 1.0 };
    let at = |v: Vec2| Pose { offset: v, alpha: 1.0 };
    match kind {
        // SmartAnimate is rendered from interpolated layers; this is its fallback.
        TransitionKind::Dissolve | TransitionKind::SmartAnimate => Poses { from: still, to: Pose { offset: Vec2::ZERO, alpha: p.clamp(0.0, 1.0) }, to_on_top: true },
        TransitionKind::MoveIn(d) => {
            let v = dir_vec(d);
            Poses { from: still, to: at(mul(v, -(1.0 - p) * size.x, -(1.0 - p) * size.y)), to_on_top: true }
        }
        TransitionKind::MoveOut(d) => {
            let v = dir_vec(d);
            Poses { from: at(mul(v, p * size.x, p * size.y)), to: still, to_on_top: false }
        }
        TransitionKind::Push(d) => {
            let v = dir_vec(d);
            Poses { from: at(mul(v, p * size.x, p * size.y)), to: at(mul(v, -(1.0 - p) * size.x, -(1.0 - p) * size.y)), to_on_top: true }
        }
        TransitionKind::SlideIn(d) => {
            let v = dir_vec(d);
            Poses {
                from: at(mul(v, p * size.x * SLIDE_PARALLAX, p * size.y * SLIDE_PARALLAX)),
                to: at(mul(v, -(1.0 - p) * size.x, -(1.0 - p) * size.y)),
                to_on_top: true,
            }
        }
        TransitionKind::SlideOut(d) => {
            let v = dir_vec(d);
            Poses {
                from: at(mul(v, p * size.x, p * size.y)),
                to: at(mul(v, -(1.0 - p) * size.x * SLIDE_PARALLAX, -(1.0 - p) * size.y * SLIDE_PARALLAX)),
                to_on_top: false,
            }
        }
    }
}

/// Pose of an overlay at eased progress `p` where 0 is hidden and 1 is shown,
/// for the transition it opens with (closing plays it backwards).
pub fn overlay_pose(kind: TransitionKind, p: f32, overlay: Vec2) -> Pose {
    match kind {
        TransitionKind::MoveIn(d) | TransitionKind::SlideIn(d) | TransitionKind::Push(d) | TransitionKind::MoveOut(d) | TransitionKind::SlideOut(d) => {
            let v = dir_vec(d);
            // Moves in from the side it came from: offset shrinks to zero.
            let off = mul(v, -(1.0 - p) * overlay.x, -(1.0 - p) * overlay.y);
            Pose { offset: off, alpha: 1.0 }
        }
        TransitionKind::Dissolve | TransitionKind::SmartAnimate => Pose { offset: Vec2::ZERO, alpha: p.clamp(0.0, 1.0) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2};

    fn r(x: f32, y: f32, w: f32, h: f32) -> egui::Rect {
        egui::Rect::from_min_size(pos2(x, y), vec2(w, h))
    }

    #[test]
    fn noodle_leaves_right_and_enters_left_edge() {
        let n = noodle(r(100.0, 200.0, 80.0, 40.0), r(400.0, 100.0, 300.0, 600.0));
        assert_eq!(n.p0, pos2(180.0, 220.0));
        assert_eq!(n.p1, pos2(400.0, 220.0), "keeps the source height when inside the frame's span");
        assert!(n.c0.x > n.p0.x && n.c0.y == n.p0.y, "exits horizontally");
        assert!(n.c1.x < n.p1.x && n.c1.y == n.p1.y, "enters horizontally");
        // arrow tip on the frame edge, base to the left of it
        assert_eq!(n.arrow[0], n.p1);
        assert!(n.arrow[1].x < n.p1.x && n.arrow[2].x < n.p1.x);
        assert!((n.arrow[1].y + n.arrow[2].y) / 2.0 - n.p1.y < 1e-4);
    }

    #[test]
    fn noodle_end_is_clamped_into_the_frame() {
        let n = noodle(r(0.0, 0.0, 10.0, 10.0), r(300.0, 500.0, 100.0, 200.0));
        assert_eq!(n.p1.y, 500.0 + 16.0);
        let n = noodle(r(0.0, 900.0, 10.0, 10.0), r(300.0, 500.0, 100.0, 200.0));
        assert_eq!(n.p1.y, 700.0 - 16.0);
    }

    #[test]
    fn noodle_curve_endpoints_and_midpoint() {
        let n = noodle(r(0.0, 0.0, 10.0, 20.0), r(300.0, 0.0, 100.0, 100.0));
        assert_eq!(n.point(0.0), n.p0);
        assert!((n.point(1.0) - n.p1).length() < 1e-4);
        let mid = n.point(0.5);
        assert!(mid.x > n.p0.x && mid.x < n.p1.x);
        assert!(n.distance_to(mid) < 0.5);
        assert!(n.distance_to(pos2(mid.x, mid.y + 50.0)) > 20.0);
    }

    #[test]
    fn backward_noodle_still_starts_right_and_ends_left() {
        // destination to the left of the source: the curve loops but keeps the tangents
        let n = noodle(r(500.0, 100.0, 50.0, 40.0), r(0.0, 0.0, 200.0, 300.0));
        assert!(n.c0.x > n.p0.x);
        assert!(n.c1.x < n.p1.x);
        assert!(n.reach_ok());
    }

    impl Noodle {
        fn reach_ok(&self) -> bool {
            (self.c0.x - self.p0.x) >= 40.0 && (self.p1.x - self.c1.x) >= 40.0
        }
    }

    #[test]
    fn trimmed_curve_stops_before_arrow() {
        let n = noodle(r(0.0, 0.0, 10.0, 20.0), r(300.0, 0.0, 100.0, 100.0));
        let t = n.trimmed();
        assert_eq!(t[0], n.p0);
        assert!(t[3].x < n.p1.x);
    }

    #[test]
    fn overlay_positions_in_a_390x844_frame() {
        let base = Vec2::new(390.0, 844.0);
        let ov = Vec2::new(200.0, 100.0);
        let z = Rect::default();
        let o = |p| overlay_origin(p, Vec2::ZERO, ov, base, z);
        assert_eq!(o(OverlayPosition::Center), Vec2::new(95.0, 372.0));
        assert_eq!(o(OverlayPosition::TopLeft), Vec2::new(0.0, 0.0));
        assert_eq!(o(OverlayPosition::TopCenter), Vec2::new(95.0, 0.0));
        assert_eq!(o(OverlayPosition::TopRight), Vec2::new(190.0, 0.0));
        assert_eq!(o(OverlayPosition::BottomLeft), Vec2::new(0.0, 744.0));
        assert_eq!(o(OverlayPosition::BottomCenter), Vec2::new(95.0, 744.0));
        assert_eq!(o(OverlayPosition::BottomRight), Vec2::new(190.0, 744.0));
    }

    #[test]
    fn manual_overlay_follows_trigger_and_stays_inside() {
        let base = Vec2::new(400.0, 800.0);
        let ov = Vec2::new(100.0, 100.0);
        let trig = Rect::new(50.0, 60.0, 30.0, 30.0);
        assert_eq!(overlay_origin(OverlayPosition::Manual, Vec2::new(10.0, 40.0), ov, base, trig), Vec2::new(60.0, 100.0));
        // would overflow the right/bottom edge: pulled back in
        let far = Rect::new(380.0, 790.0, 10.0, 10.0);
        assert_eq!(overlay_origin(OverlayPosition::Manual, Vec2::ZERO, ov, base, far), Vec2::new(300.0, 700.0));
        // negative offsets cannot leave the top-left
        assert_eq!(overlay_origin(OverlayPosition::Manual, Vec2::new(-500.0, -500.0), ov, base, trig), Vec2::new(0.0, 0.0));
        // an overlay larger than its base is not clamped (no valid range)
        let big = Vec2::new(600.0, 100.0);
        assert_eq!(overlay_origin(OverlayPosition::Manual, Vec2::new(5.0, 5.0), big, base, trig).x, 55.0);
    }

    #[test]
    fn reverse_is_an_involution_and_flips_direction() {
        use Direction::*;
        for k in [
            TransitionKind::Dissolve,
            TransitionKind::SmartAnimate,
            TransitionKind::MoveIn(Left),
            TransitionKind::MoveOut(Top),
            TransitionKind::Push(Right),
            TransitionKind::SlideIn(Bottom),
            TransitionKind::SlideOut(Left),
        ] {
            assert_eq!(reverse(reverse(k)), k);
        }
        assert_eq!(reverse(TransitionKind::MoveIn(Left)), TransitionKind::MoveOut(Right));
        assert_eq!(reverse(TransitionKind::Push(Left)), TransitionKind::Push(Right));
    }

    #[test]
    fn poses_move_in_from_the_right_when_moving_left() {
        let size = Vec2::new(400.0, 800.0);
        let p0 = poses(TransitionKind::MoveIn(Direction::Left), 0.0, size);
        assert_eq!(p0.to.offset, Vec2::new(400.0, 0.0), "incoming starts one frame to the right");
        assert_eq!(p0.from.offset, Vec2::ZERO);
        assert!(p0.to_on_top);
        let p1 = poses(TransitionKind::MoveIn(Direction::Left), 1.0, size);
        assert_eq!(p1.to.offset, Vec2::ZERO);
        let half = poses(TransitionKind::MoveIn(Direction::Left), 0.5, size);
        assert_eq!(half.to.offset.x, 200.0);
    }

    #[test]
    fn poses_move_out_push_and_slide() {
        let size = Vec2::new(400.0, 800.0);
        let out = poses(TransitionKind::MoveOut(Direction::Bottom), 0.5, size);
        assert_eq!(out.from.offset, Vec2::new(0.0, 400.0));
        assert!(!out.to_on_top);
        let push = poses(TransitionKind::Push(Direction::Left), 0.25, size);
        assert_eq!(push.from.offset.x, -100.0);
        assert_eq!(push.to.offset.x, 300.0);
        let slide = poses(TransitionKind::SlideIn(Direction::Left), 1.0, size);
        assert!((slide.from.offset.x + 400.0 * SLIDE_PARALLAX).abs() < 1e-3);
        assert_eq!(slide.to.offset, Vec2::ZERO);
        let d = poses(TransitionKind::Dissolve, 0.3, size);
        assert!((d.to.alpha - 0.3).abs() < 1e-6 && d.from.alpha == 1.0);
        // easing overshoot is tolerated but alpha stays valid
        assert_eq!(poses(TransitionKind::Dissolve, 1.2, size).to.alpha, 1.0);
    }

    #[test]
    fn overlay_pose_slides_in_from_its_side_and_dissolves() {
        let ov = Vec2::new(300.0, 200.0);
        let hidden = overlay_pose(TransitionKind::MoveIn(Direction::Top), 0.0, ov);
        assert_eq!(hidden.offset, Vec2::new(0.0, 200.0), "moving up means it arrives from below");
        assert_eq!(overlay_pose(TransitionKind::MoveIn(Direction::Top), 1.0, ov).offset, Vec2::ZERO);
        assert_eq!(overlay_pose(TransitionKind::Dissolve, 0.5, ov).alpha, 0.5);
    }
}
