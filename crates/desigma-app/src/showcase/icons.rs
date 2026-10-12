//! The showcase's own stroke icons, drawn on a 24 x 24 grid from simple geometry.
//! None of this artwork comes from an icon set.

use desigma_doc::{FillRule, Path, PathCmd, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Ic {
    Home,
    Card,
    Insights,
    Profile,
    Send,
    Request,
    Swap,
    More,
    Bell,
    Moon,
    Shield,
    Wallet,
    Help,
    Plus,
    ArrowRight,
    ChevronRight,
    ChevronLeft,
    Search,
    Coffee,
    Basket,
    Bus,
    Repeat,
    Bank,
    Gift,
}

pub const ALL: [Ic; 24] = [
    Ic::Home,
    Ic::Card,
    Ic::Insights,
    Ic::Profile,
    Ic::Send,
    Ic::Request,
    Ic::Swap,
    Ic::More,
    Ic::Bell,
    Ic::Moon,
    Ic::Shield,
    Ic::Wallet,
    Ic::Help,
    Ic::Plus,
    Ic::ArrowRight,
    Ic::ChevronRight,
    Ic::ChevronLeft,
    Ic::Search,
    Ic::Coffee,
    Ic::Basket,
    Ic::Bus,
    Ic::Repeat,
    Ic::Bank,
    Ic::Gift,
];

impl Ic {
    pub fn name(self) -> &'static str {
        match self {
            Ic::Home => "Home",
            Ic::Card => "Card",
            Ic::Insights => "Insights",
            Ic::Profile => "Profile",
            Ic::Send => "Send",
            Ic::Request => "Request",
            Ic::Swap => "Swap",
            Ic::More => "More",
            Ic::Bell => "Bell",
            Ic::Moon => "Moon",
            Ic::Shield => "Shield",
            Ic::Wallet => "Wallet",
            Ic::Help => "Help",
            Ic::Plus => "Plus",
            Ic::ArrowRight => "Arrow right",
            Ic::ChevronRight => "Chevron right",
            Ic::ChevronLeft => "Chevron left",
            Ic::Search => "Search",
            Ic::Coffee => "Coffee",
            Ic::Basket => "Basket",
            Ic::Bus => "Bus",
            Ic::Repeat => "Repeat",
            Ic::Bank => "Bank",
            Ic::Gift => "Gift",
        }
    }

    /// Stroke weight on the 24 grid.
    pub fn weight(self) -> f32 {
        match self {
            Ic::Insights => 2.4,
            Ic::More => 1.6,
            _ => 1.75,
        }
    }
}

/// A path under construction (coordinates on the 24 grid).
struct Pb(Vec<PathCmd>);

fn v(x: f32, y: f32) -> Vec2 {
    Vec2::new(x, y)
}

impl Pb {
    fn new() -> Self {
        Pb(Vec::new())
    }
    fn m(mut self, x: f32, y: f32) -> Self {
        self.0.push(PathCmd::MoveTo(v(x, y)));
        self
    }
    fn l(mut self, x: f32, y: f32) -> Self {
        self.0.push(PathCmd::LineTo(v(x, y)));
        self
    }
    fn c(mut self, a: (f32, f32), b: (f32, f32), p: (f32, f32)) -> Self {
        self.0.push(PathCmd::CubicTo(v(a.0, a.1), v(b.0, b.1), v(p.0, p.1)));
        self
    }
    fn z(mut self) -> Self {
        self.0.push(PathCmd::Close);
        self
    }
    fn circle(self, cx: f32, cy: f32, r: f32) -> Self {
        let k = 0.5523 * r;
        self.m(cx + r, cy)
            .c((cx + r, cy + k), (cx + k, cy + r), (cx, cy + r))
            .c((cx - k, cy + r), (cx - r, cy + k), (cx - r, cy))
            .c((cx - r, cy - k), (cx - k, cy - r), (cx, cy - r))
            .c((cx + k, cy - r), (cx + r, cy - k), (cx + r, cy))
            .z()
    }
    fn rrect(self, x: f32, y: f32, w: f32, h: f32, r: f32) -> Self {
        let k = 0.4477 * r;
        let (x1, y1) = (x + w, y + h);
        self.m(x + r, y)
            .l(x1 - r, y)
            .c((x1 - k, y), (x1, y + k), (x1, y + r))
            .l(x1, y1 - r)
            .c((x1, y1 - k), (x1 - k, y1), (x1 - r, y1))
            .l(x + r, y1)
            .c((x + k, y1), (x, y1 - k), (x, y1 - r))
            .l(x, y + r)
            .c((x, y + k), (x + k, y), (x + r, y))
            .z()
    }
    fn done(self, scale: f32) -> Path {
        let s = |p: Vec2| v(p.x * scale, p.y * scale);
        let cmds = self
            .0
            .into_iter()
            .map(|c| match c {
                PathCmd::MoveTo(a) => PathCmd::MoveTo(s(a)),
                PathCmd::LineTo(a) => PathCmd::LineTo(s(a)),
                PathCmd::QuadTo(a, b) => PathCmd::QuadTo(s(a), s(b)),
                PathCmd::CubicTo(a, b, c) => PathCmd::CubicTo(s(a), s(b), s(c)),
                PathCmd::Close => PathCmd::Close,
            })
            .collect();
        Path { cmds, fill_rule: FillRule::NonZero }
    }
}

/// The icon's outline(s) scaled by `scale` (1.0 = the 24 grid).
pub fn paths(ic: Ic, scale: f32) -> Vec<Path> {
    let p = match ic {
        Ic::Home => Pb::new()
            .m(3.5, 10.5)
            .l(12.0, 3.5)
            .l(20.5, 10.5)
            .m(5.5, 9.0)
            .l(5.5, 20.0)
            .l(18.5, 20.0)
            .l(18.5, 9.0)
            .m(10.0, 20.0)
            .l(10.0, 14.5)
            .l(14.0, 14.5)
            .l(14.0, 20.0),
        Ic::Card => Pb::new().rrect(2.5, 5.0, 19.0, 14.0, 3.5).m(2.5, 10.0).l(21.5, 10.0).m(6.0, 15.0).l(10.0, 15.0),
        Ic::Insights => Pb::new().m(6.0, 19.5).l(6.0, 13.0).m(12.0, 19.5).l(12.0, 4.5).m(18.0, 19.5).l(18.0, 9.5),
        Ic::Profile => Pb::new().circle(12.0, 8.0, 3.8).m(4.8, 20.2).c((4.8, 16.6), (8.0, 14.2), (12.0, 14.2)).c((16.0, 14.2), (19.2, 16.6), (19.2, 20.2)),
        Ic::Send => Pb::new().m(7.0, 17.0).l(17.0, 7.0).m(8.5, 7.0).l(17.0, 7.0).l(17.0, 15.5),
        Ic::Request => Pb::new().m(17.0, 7.0).l(7.0, 17.0).m(15.5, 17.0).l(7.0, 17.0).l(7.0, 8.5),
        Ic::Swap => {
            Pb::new().m(4.0, 8.5).l(19.0, 8.5).m(15.0, 4.5).l(19.0, 8.5).l(15.0, 12.5).m(20.0, 15.5).l(5.0, 15.5).m(9.0, 11.5).l(5.0, 15.5).l(9.0, 19.5)
        }
        Ic::More => Pb::new().circle(5.5, 12.0, 1.3).circle(12.0, 12.0, 1.3).circle(18.5, 12.0, 1.3),
        Ic::Bell => Pb::new()
            .m(6.0, 17.0)
            .l(6.0, 11.0)
            .c((6.0, 7.7), (8.7, 5.0), (12.0, 5.0))
            .c((15.3, 5.0), (18.0, 7.7), (18.0, 11.0))
            .l(18.0, 17.0)
            .l(19.5, 18.5)
            .l(4.5, 18.5)
            .z()
            .m(10.0, 21.0)
            .c((10.5, 21.7), (11.2, 22.0), (12.0, 22.0))
            .c((12.8, 22.0), (13.5, 21.7), (14.0, 21.0))
            .m(12.0, 5.0)
            .l(12.0, 3.2),
        Ic::Moon => Pb::new()
            .m(19.8, 14.4)
            .c((18.6, 14.9), (17.3, 15.2), (16.0, 15.2))
            .c((11.9, 15.2), (8.5, 11.8), (8.5, 7.7))
            .c((8.5, 6.4), (8.8, 5.1), (9.4, 4.0))
            .c((5.9, 5.1), (3.5, 8.3), (3.5, 12.0))
            .c((3.5, 16.7), (7.3, 20.5), (12.0, 20.5))
            .c((15.8, 20.5), (19.0, 18.1), (20.0, 14.7))
            .z(),
        Ic::Shield => Pb::new()
            .m(12.0, 3.0)
            .l(19.5, 6.0)
            .l(19.5, 11.5)
            .c((19.5, 16.0), (16.5, 19.5), (12.0, 21.0))
            .c((7.5, 19.5), (4.5, 16.0), (4.5, 11.5))
            .l(4.5, 6.0)
            .z()
            .m(9.0, 12.0)
            .l(11.2, 14.2)
            .l(15.0, 10.0),
        Ic::Wallet => {
            Pb::new().rrect(3.0, 6.5, 18.0, 13.0, 3.0).m(3.0, 9.5).l(3.0, 7.5).c((3.0, 5.8), (4.2, 4.5), (5.8, 4.5)).l(16.0, 4.5).m(16.5, 13.0).l(16.6, 13.0)
        }
        Ic::Help => Pb::new()
            .circle(12.0, 12.0, 9.0)
            .m(9.5, 9.7)
            .c((9.5, 8.2), (10.6, 7.2), (12.0, 7.2))
            .c((13.4, 7.2), (14.5, 8.2), (14.5, 9.5))
            .c((14.5, 11.4), (12.0, 11.6), (12.0, 13.4))
            .m(12.0, 16.4)
            .l(12.0, 16.5),
        Ic::Plus => Pb::new().m(12.0, 5.0).l(12.0, 19.0).m(5.0, 12.0).l(19.0, 12.0),
        Ic::ArrowRight => Pb::new().m(4.5, 12.0).l(19.5, 12.0).m(13.5, 6.0).l(19.5, 12.0).l(13.5, 18.0),
        Ic::ChevronRight => Pb::new().m(9.0, 5.0).l(16.0, 12.0).l(9.0, 19.0),
        Ic::ChevronLeft => Pb::new().m(15.0, 5.0).l(8.0, 12.0).l(15.0, 19.0),
        Ic::Search => Pb::new().circle(10.5, 10.5, 6.5).m(15.5, 15.5).l(20.5, 20.5),
        Ic::Coffee => Pb::new()
            .m(4.5, 9.0)
            .l(16.5, 9.0)
            .l(16.5, 14.5)
            .c((16.5, 17.5), (14.3, 19.5), (11.5, 19.5))
            .l(9.5, 19.5)
            .c((6.7, 19.5), (4.5, 17.5), (4.5, 14.5))
            .z()
            .m(16.5, 10.5)
            .l(18.0, 10.5)
            .c((19.4, 10.5), (20.5, 11.6), (20.5, 13.0))
            .c((20.5, 14.4), (19.4, 15.5), (18.0, 15.5))
            .l(16.5, 15.5)
            .m(8.0, 3.5)
            .c((7.2, 4.6), (8.8, 5.6), (8.0, 6.8))
            .m(12.0, 3.5)
            .c((11.2, 4.6), (12.8, 5.6), (12.0, 6.8)),
        Ic::Basket => Pb::new()
            .m(4.5, 8.5)
            .l(19.5, 8.5)
            .l(18.2, 19.5)
            .l(5.8, 19.5)
            .z()
            .m(8.5, 8.5)
            .c((8.5, 5.9), (10.0, 4.0), (12.0, 4.0))
            .c((14.0, 4.0), (15.5, 5.9), (15.5, 8.5))
            .m(9.0, 12.5)
            .l(9.0, 15.5)
            .m(15.0, 12.5)
            .l(15.0, 15.5),
        Ic::Bus => Pb::new()
            .rrect(4.5, 3.5, 15.0, 14.0, 3.0)
            .m(4.5, 11.0)
            .l(19.5, 11.0)
            .m(7.5, 17.5)
            .l(7.5, 20.5)
            .m(16.5, 17.5)
            .l(16.5, 20.5)
            .m(8.5, 14.3)
            .l(8.6, 14.3)
            .m(15.4, 14.3)
            .l(15.5, 14.3),
        Ic::Repeat => Pb::new()
            .m(4.0, 11.0)
            .l(4.0, 10.0)
            .c((4.0, 7.8), (5.8, 6.0), (8.0, 6.0))
            .l(19.0, 6.0)
            .m(16.0, 3.0)
            .l(19.0, 6.0)
            .l(16.0, 9.0)
            .m(20.0, 13.0)
            .l(20.0, 14.0)
            .c((20.0, 16.2), (18.2, 18.0), (16.0, 18.0))
            .l(5.0, 18.0)
            .m(8.0, 15.0)
            .l(5.0, 18.0)
            .l(8.0, 21.0),
        Ic::Bank => Pb::new()
            .m(3.5, 9.5)
            .l(12.0, 4.0)
            .l(20.5, 9.5)
            .z()
            .m(6.5, 12.5)
            .l(6.5, 17.0)
            .m(10.2, 12.5)
            .l(10.2, 17.0)
            .m(13.8, 12.5)
            .l(13.8, 17.0)
            .m(17.5, 12.5)
            .l(17.5, 17.0)
            .m(3.5, 20.0)
            .l(20.5, 20.0),
        Ic::Gift => Pb::new()
            .rrect(4.0, 10.0, 16.0, 10.0, 2.0)
            .m(3.0, 7.0)
            .l(21.0, 7.0)
            .l(21.0, 10.0)
            .l(3.0, 10.0)
            .z()
            .m(12.0, 7.0)
            .l(12.0, 20.0)
            .m(12.0, 7.0)
            .c((10.0, 7.0), (7.5, 6.2), (7.5, 4.8))
            .c((7.5, 3.6), (9.5, 3.4), (10.8, 4.6))
            .c((11.6, 5.3), (12.0, 6.2), (12.0, 7.0))
            .c((12.0, 6.2), (12.4, 5.3), (13.2, 4.6))
            .c((14.5, 3.4), (16.5, 3.6), (16.5, 4.8))
            .c((16.5, 6.2), (14.0, 7.0), (12.0, 7.0)),
    };
    vec![p.done(scale)]
}

/// A smooth curve through the points (Catmull-Rom), as path commands, for charts.
pub fn smooth_line(points: &[(f32, f32)]) -> Vec<PathCmd> {
    // Catmull-Rom through the points, as cubic segments.
    let mut out = Vec::new();
    let n = points.len();
    if n == 0 {
        return out;
    }
    out.push(PathCmd::MoveTo(v(points[0].0, points[0].1)));
    for i in 0..n.saturating_sub(1) {
        let p0 = points[i.saturating_sub(1)];
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = points[(i + 2).min(n - 1)];
        let c1 = (p1.0 + (p2.0 - p0.0) / 6.0, p1.1 + (p2.1 - p0.1) / 6.0);
        let c2 = (p2.0 - (p3.0 - p1.0) / 6.0, p2.1 - (p3.1 - p1.1) / 6.0);
        out.push(PathCmd::CubicTo(v(c1.0, c1.1), v(c2.0, c2.1), v(p2.0, p2.1)));
    }
    out
}
