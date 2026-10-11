//! Paint conversion: colors, gradients (user space -> node-normalized
//! handles), and the pattern fallbacks.

use desigma_doc as d;
use usvg::tiny_skia_path::Transform;

/// The node box paints are expressed against: gradient handles are fractions
/// of `size` measured from `origin`, both in the shape's own user space.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub origin: (f32, f32),
    pub size: (f32, f32),
}

impl Frame {
    /// Node boxes are never thinner than this, so a horizontal line can still
    /// carry a gradient.
    pub const MIN: f32 = 0.01;

    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { origin: (x, y), size: (w.max(Self::MIN), h.max(Self::MIN)) }
    }

    fn norm(&self, p: (f32, f32)) -> d::Vec2 {
        d::Vec2::new((p.0 - self.origin.0) / self.size.0, (p.1 - self.origin.1) / self.size.1)
    }
}

pub fn affine(t: Transform) -> d::Affine {
    d::Affine { a: t.sx, b: t.ky, c: t.kx, d: t.sy, tx: t.tx, ty: t.ty }
}

pub fn color(c: usvg::Color, alpha: f32) -> d::Color {
    d::Color { r: c.red as f32 / 255.0, g: c.green as f32 / 255.0, b: c.blue as f32 / 255.0, a: alpha }
}

fn map(t: &Transform, x: f32, y: f32) -> (f32, f32) {
    (t.sx * x + t.kx * y + t.tx, t.ky * x + t.sy * y + t.ty)
}

fn stops(stops: &[usvg::Stop]) -> Vec<d::GradientStop> {
    stops.iter().map(|s| d::GradientStop { pos: s.offset().get(), color: color(s.color(), s.opacity().get()) }).collect()
}

pub fn linear(g: &usvg::LinearGradient, fr: &Frame) -> d::PaintKind {
    let t = g.transform();
    let (p0, p1) = ((g.x1(), g.y1()), (g.x2(), g.y2()));
    // The width handle is the start handle rotated a quarter turn, like Figma's.
    let p2 = (p0.0 - (p1.1 - p0.1), p0.1 + (p1.0 - p0.0));
    let h = [fr.norm(map(&t, p0.0, p0.1)), fr.norm(map(&t, p1.0, p1.1)), fr.norm(map(&t, p2.0, p2.1))];
    d::PaintKind::LinearGradient { handles: h, stops: stops(g.stops()) }
}

pub fn radial(g: &usvg::RadialGradient, fr: &Frame) -> d::PaintKind {
    let t = g.transform();
    let (cx, cy, r) = (g.cx(), g.cy(), g.r().get());
    let h = [fr.norm(map(&t, cx, cy)), fr.norm(map(&t, cx + r, cy)), fr.norm(map(&t, cx, cy + r))];
    d::PaintKind::RadialGradient { handles: h, stops: stops(g.stops()) }
}

/// First solid color found in a pattern's content (fills, then strokes, then
/// the first stop of a gradient), the fallback for tiled patterns.
pub fn pattern_color(g: &usvg::Group) -> Option<(usvg::Color, f32)> {
    for n in g.children() {
        let found = match n {
            usvg::Node::Group(g) => pattern_color(g),
            usvg::Node::Path(p) => {
                p.fill().and_then(|f| paint_color(f.paint(), f.opacity().get())).or_else(|| p.stroke().and_then(|s| paint_color(s.paint(), s.opacity().get())))
            }
            _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

pub fn paint_color(p: &usvg::Paint, opacity: f32) -> Option<(usvg::Color, f32)> {
    match p {
        usvg::Paint::Color(c) => Some((*c, opacity)),
        usvg::Paint::LinearGradient(g) => g.stops().first().map(|s| (s.color(), s.opacity().get() * opacity)),
        usvg::Paint::RadialGradient(g) => g.stops().first().map(|s| (s.color(), s.opacity().get() * opacity)),
        usvg::Paint::Pattern(p) => pattern_color(p.root()),
    }
}

/// A pattern that is exactly one image: the image, how it is placed in tile
/// space, and the tile.
pub struct PatternImage<'a> {
    pub image: &'a usvg::Image,
    /// Image space -> tile space.
    pub transform: Transform,
}

pub fn pattern_image(p: &usvg::Pattern) -> Option<PatternImage<'_>> {
    let mut g = p.root();
    let mut ts = Transform::identity();
    loop {
        ts = ts.pre_concat(g.transform());
        match g.children() {
            [usvg::Node::Group(inner)] => g = inner,
            [usvg::Node::Image(img)] => return Some(PatternImage { image: img, transform: ts }),
            _ => return None,
        }
    }
}

/// Relative luminance used by SVG luminance masks (sRGB coefficients).
pub fn luminance(c: &d::Color) -> f32 {
    0.2125 * c.r + 0.7154 * c.g + 0.0721 * c.b
}
