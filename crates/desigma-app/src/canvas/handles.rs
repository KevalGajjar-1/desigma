//! Selection handles: positions, hit testing and resize/rotate math.

use crate::util::{add, dist, scale, sub, v2};
use desigma_doc::{Affine, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    TL,
    T,
    TR,
    R,
    BR,
    B,
    BL,
    L,
}

impl Handle {
    pub const ALL: [Handle; 8] = [Handle::TL, Handle::T, Handle::TR, Handle::R, Handle::BR, Handle::B, Handle::BL, Handle::L];

    /// Normalized position on the box (0..1 on each axis).
    pub fn norm(self) -> (f32, f32) {
        match self {
            Handle::TL => (0.0, 0.0),
            Handle::T => (0.5, 0.0),
            Handle::TR => (1.0, 0.0),
            Handle::R => (1.0, 0.5),
            Handle::BR => (1.0, 1.0),
            Handle::B => (0.5, 1.0),
            Handle::BL => (0.0, 1.0),
            Handle::L => (0.0, 0.5),
        }
    }
    pub fn affects_x(self) -> bool {
        self.norm().0 != 0.5
    }
    pub fn affects_y(self) -> bool {
        self.norm().1 != 0.5
    }
    pub fn is_corner(self) -> bool {
        self.affects_x() && self.affects_y()
    }
    pub fn cursor(self) -> egui::CursorIcon {
        match self {
            Handle::TL | Handle::BR => egui::CursorIcon::ResizeNwSe,
            Handle::TR | Handle::BL => egui::CursorIcon::ResizeNeSw,
            Handle::T | Handle::B => egui::CursorIcon::ResizeVertical,
            Handle::L | Handle::R => egui::CursorIcon::ResizeHorizontal,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleHit {
    Resize(Handle),
    /// Rotation zone just outside the given corner (0=TL, 1=TR, 2=BR, 3=BL).
    Rotate(usize),
}

/// Position of a handle on a box given its (tl, tr, br, bl) corners.
pub fn handle_pos(corners: &[Vec2; 4], h: Handle) -> Vec2 {
    let (nx, ny) = h.norm();
    let ex = sub(corners[1], corners[0]);
    let ey = sub(corners[3], corners[0]);
    add(corners[0], add(scale(ex, nx), scale(ey, ny)))
}

/// Hit-tests handles (resize first, then rotation zones just outside corners).
/// All values are in the same space as `corners` (screen pixels).
pub fn hit_handle(corners: &[Vec2; 4], p: Vec2, hit_radius: f32, rotate_zone: f32, allow_mid: bool) -> Option<HandleHit> {
    for h in Handle::ALL {
        if !allow_mid && !h.is_corner() {
            continue;
        }
        if dist(handle_pos(corners, h), p) <= hit_radius {
            return Some(HandleHit::Resize(h));
        }
    }
    for (i, c) in corners.iter().enumerate() {
        let d = dist(*c, p);
        if d > hit_radius && d <= rotate_zone && !inside_quad(corners, p) {
            return Some(HandleHit::Rotate(i));
        }
    }
    None
}

/// Point-in-convex-quad test (corners in order).
pub fn inside_quad(c: &[Vec2; 4], p: Vec2) -> bool {
    let mut sign = 0.0f32;
    for i in 0..4 {
        let a = c[i];
        let b = c[(i + 1) % 4];
        let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        if cross.abs() < 1e-6 {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if sign != cross.signum() {
            return false;
        }
    }
    true
}

/// Resize math in the node's local space.
///
/// `mouse_l` is the pointer in the *start-of-drag* local frame. Returns the new
/// size and the local offset `d` to apply to the node transform
/// (`T' = T0 * translate(d)`) so the anchor stays put in the parent space.
pub fn resize_local(size0: Vec2, h: Handle, mouse_l: Vec2, keep_ratio: bool, from_center: bool) -> (Vec2, Vec2) {
    let (hx, hy) = h.norm();
    let (w0, h0) = (size0.x.max(1e-3), size0.y.max(1e-3));
    let (ax, ay) = if from_center {
        (0.5, 0.5)
    } else {
        (if h.affects_x() { 1.0 - hx } else { 0.5 }, if h.affects_y() { 1.0 - hy } else { 0.5 })
    };
    let anchor = v2(ax * w0, ay * h0);
    let mut nw = w0;
    let mut nh = h0;
    if h.affects_x() {
        nw = if hx == 1.0 { mouse_l.x - anchor.x } else { anchor.x - mouse_l.x };
        if from_center {
            nw = if hx == 1.0 { 2.0 * (mouse_l.x - anchor.x) } else { 2.0 * (anchor.x - mouse_l.x) };
        }
    }
    if h.affects_y() {
        nh = if hy == 1.0 { mouse_l.y - anchor.y } else { anchor.y - mouse_l.y };
        if from_center {
            nh = if hy == 1.0 { 2.0 * (mouse_l.y - anchor.y) } else { 2.0 * (anchor.y - mouse_l.y) };
        }
    }
    nw = nw.max(1.0);
    nh = nh.max(1.0);
    if keep_ratio {
        let ratio = w0 / h0;
        if h.affects_x() && h.affects_y() {
            let s = (nw / w0).max(nh / h0);
            nw = w0 * s;
            nh = h0 * s;
        } else if h.affects_x() {
            nh = nw / ratio;
        } else {
            nw = nh * ratio;
        }
    }
    let d = v2(ax * w0 - ax * nw, ay * h0 - ay * nh);
    (v2(nw, nh), d)
}

/// New parent-space transform after rotating about the node's center.
pub fn rotate_about_center(t0: &Affine, size: Vec2, angle: f32) -> Affine {
    let c = t0.apply(v2(size.x / 2.0, size.y / 2.0));
    Affine::translate(c.x, c.y) * Affine::rotate(angle) * Affine::translate(-size.x / 2.0, -size.y / 2.0)
}

pub fn angle_of(center: Vec2, p: Vec2) -> f32 {
    (p.y - center.y).atan2(p.x - center.x)
}

/// Snaps an angle (radians) to multiples of `step_deg`.
pub fn snap_angle(rad: f32, step_deg: f32) -> f32 {
    let step = step_deg.to_radians();
    (rad / step).round() * step
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_right_handle() {
        let (s, d) = resize_local(v2(100.0, 50.0), Handle::R, v2(150.0, 20.0), false, false);
        assert_eq!((s.x, s.y), (150.0, 50.0));
        assert_eq!((d.x, d.y), (0.0, 0.0));
    }

    #[test]
    fn resize_left_handle_moves_origin() {
        let (s, d) = resize_local(v2(100.0, 50.0), Handle::L, v2(-20.0, 20.0), false, false);
        assert_eq!(s.x, 120.0);
        assert_eq!(d.x, -20.0);
    }

    #[test]
    fn resize_keeps_ratio_on_corner() {
        let (s, _) = resize_local(v2(100.0, 50.0), Handle::BR, v2(200.0, 60.0), true, false);
        assert!((s.x / s.y - 2.0).abs() < 1e-4);
        assert_eq!(s.x, 200.0);
    }

    #[test]
    fn resize_from_center_is_symmetric() {
        let (s, d) = resize_local(v2(100.0, 100.0), Handle::BR, v2(150.0, 150.0), false, true);
        assert_eq!(s.x, 200.0);
        assert_eq!(d.x, -50.0);
    }

    #[test]
    fn resize_clamps_to_min() {
        let (s, _) = resize_local(v2(100.0, 50.0), Handle::R, v2(-400.0, 0.0), false, false);
        assert_eq!(s.x, 1.0);
    }

    #[test]
    fn rotation_preserves_center() {
        let t0 = Affine::translate(10.0, 20.0);
        let size = v2(40.0, 20.0);
        let t = rotate_about_center(&t0, size, 0.8);
        let c0 = t0.apply(v2(20.0, 10.0));
        let c1 = t.apply(v2(20.0, 10.0));
        assert!((c0.x - c1.x).abs() < 1e-4 && (c0.y - c1.y).abs() < 1e-4);
        assert!((t.rotation_rad() - 0.8).abs() < 1e-4);
    }

    #[test]
    fn snap_angle_15() {
        let a = snap_angle(16.0f32.to_radians(), 15.0);
        assert!((a - 15.0f32.to_radians()).abs() < 1e-5);
    }

    #[test]
    fn handle_hit_priorities() {
        let c = [v2(0.0, 0.0), v2(100.0, 0.0), v2(100.0, 100.0), v2(0.0, 100.0)];
        assert_eq!(hit_handle(&c, v2(1.0, 1.0), 6.0, 20.0, true), Some(HandleHit::Resize(Handle::TL)));
        assert_eq!(hit_handle(&c, v2(50.0, 2.0), 6.0, 20.0, true), Some(HandleHit::Resize(Handle::T)));
        assert_eq!(hit_handle(&c, v2(110.0, 110.0), 6.0, 20.0, true), Some(HandleHit::Rotate(2)));
        assert_eq!(hit_handle(&c, v2(50.0, 50.0), 6.0, 20.0, true), None);
        assert_eq!(hit_handle(&c, v2(50.0, 2.0), 6.0, 20.0, false), None);
    }

    #[test]
    fn quad_contains() {
        let c = [v2(0.0, 0.0), v2(10.0, 0.0), v2(10.0, 10.0), v2(0.0, 10.0)];
        assert!(inside_quad(&c, v2(5.0, 5.0)));
        assert!(!inside_quad(&c, v2(15.0, 5.0)));
    }
}
