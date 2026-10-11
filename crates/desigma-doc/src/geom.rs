use serde::{Deserialize, Serialize};
use std::ops::Mul;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.x && p.y >= self.y && p.x <= self.x + self.w && p.y <= self.y + self.h
    }
    pub fn union(&self, o: &Rect) -> Rect {
        let x0 = self.x.min(o.x);
        let y0 = self.y.min(o.y);
        let x1 = (self.x + self.w).max(o.x + o.w);
        let y1 = (self.y + self.h).max(o.y + o.h);
        Rect::new(x0, y0, x1 - x0, y1 - y0)
    }
    pub fn intersects(&self, o: &Rect) -> bool {
        self.x < o.x + o.w && o.x < self.x + self.w && self.y < o.y + o.h && o.y < self.y + self.h
    }
}

/// 2D affine transform, Figma `relativeTransform` layout:
/// `[[a, c, tx], [b, d, ty]]` maps (x, y) to (a*x + c*y + tx, b*x + d*y + ty).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Affine {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Default for Affine {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Affine {
    pub const IDENTITY: Self = Self { a: 1.0, b: 0.0, c: 0.0, d: 1.0, tx: 0.0, ty: 0.0 };

    pub const fn translate(x: f32, y: f32) -> Self {
        Self { tx: x, ty: y, ..Self::IDENTITY }
    }
    pub fn scale(sx: f32, sy: f32) -> Self {
        Self { a: sx, d: sy, ..Self::IDENTITY }
    }
    /// Rotation in radians, clockwise in y-down space.
    pub fn rotate(rad: f32) -> Self {
        let (s, c) = rad.sin_cos();
        Self { a: c, b: s, c: -s, d: c, tx: 0.0, ty: 0.0 }
    }
    pub fn apply(&self, p: Vec2) -> Vec2 {
        Vec2::new(self.a * p.x + self.c * p.y + self.tx, self.b * p.x + self.d * p.y + self.ty)
    }
    pub fn invert(&self) -> Option<Self> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-12 {
            return None;
        }
        let inv = 1.0 / det;
        Some(Self {
            a: self.d * inv,
            b: -self.b * inv,
            c: -self.c * inv,
            d: self.a * inv,
            tx: (self.c * self.ty - self.d * self.tx) * inv,
            ty: (self.b * self.tx - self.a * self.ty) * inv,
        })
    }
    pub fn translation(&self) -> Vec2 {
        Vec2::new(self.tx, self.ty)
    }
    pub fn rotation_rad(&self) -> f32 {
        self.b.atan2(self.a)
    }
}

impl Mul for Affine {
    type Output = Affine;
    /// `self * rhs` applies `rhs` first, then `self`.
    fn mul(self, r: Affine) -> Affine {
        Affine {
            a: self.a * r.a + self.c * r.b,
            b: self.b * r.a + self.d * r.b,
            c: self.a * r.c + self.c * r.d,
            d: self.b * r.c + self.d * r.d,
            tx: self.a * r.tx + self.c * r.ty + self.tx,
            ty: self.b * r.tx + self.d * r.ty + self.ty,
        }
    }
}

/// Vector path in node-local coordinates.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Path {
    pub cmds: Vec<PathCmd>,
    pub fill_rule: FillRule,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum PathCmd {
    MoveTo(Vec2),
    LineTo(Vec2),
    QuadTo(Vec2, Vec2),
    CubicTo(Vec2, Vec2, Vec2),
    Close,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affine_inverse() {
        let t = Affine::translate(10.0, 5.0) * Affine::rotate(0.7) * Affine::scale(2.0, 3.0);
        let p = Vec2::new(3.0, -4.0);
        let back = t.invert().unwrap().apply(t.apply(p));
        assert!((back.x - p.x).abs() < 1e-4 && (back.y - p.y).abs() < 1e-4);
    }
}
