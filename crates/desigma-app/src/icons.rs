//! Our own simple vector icons, drawn with the egui painter on a 16x16 grid.
//! (Original artwork; nothing here is copied from Figma.)

use crate::theme;
use egui::{epaint::PathShape, Color32, Painter, Pos2, Rect, Shape, Stroke, StrokeKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    Logo,
    File,
    Assets,
    Tools,
    Variables,
    Help,
    User,
    ChevronDown,
    ChevronRight,
    Plus,
    Minus,
    Search,
    Eye,
    EyeOff,
    Lock,
    Unlock,
    Sidebar,
    Page,
    Check,
    // tools
    Move,
    Hand,
    Scale,
    Frame,
    Section,
    Slice,
    Rectangle,
    Line,
    Arrow,
    Ellipse,
    Polygon,
    Star,
    Image,
    Pen,
    Pencil,
    Text,
    Comment,
    // layer kinds
    Group,
    Vector,
    Component,
    Instance,
    Boolean,
    // property icons
    AlignLeft,
    AlignHCenter,
    AlignRight,
    AlignTop,
    AlignVCenter,
    AlignBottom,
    Rotation,
    Radius,
    Opacity,
    Mask,
    DirHorizontal,
    DirVertical,
    Wrap,
    Spacing,
    PaddingH,
    PaddingV,
    Dev,
    Design,
    Export,
    Sparkle,
    TextLeft,
    TextCenter,
    TextRight,
    Distribute,
    Rotate90,
    FlipH,
    FlipV,
    Flag,
    Play,
}

struct G<'a> {
    p: &'a Painter,
    o: Pos2,
    k: f32,
    col: Color32,
    w: f32,
}

impl G<'_> {
    fn pt(&self, x: f32, y: f32) -> Pos2 {
        Pos2::new(self.o.x + x * self.k, self.o.y + y * self.k)
    }
    fn st(&self) -> Stroke {
        Stroke::new(self.w, self.col)
    }
    fn line(&self, a: (f32, f32), b: (f32, f32)) {
        self.p.line_segment([self.pt(a.0, a.1), self.pt(b.0, b.1)], self.st());
    }
    fn poly(&self, pts: &[(f32, f32)], closed: bool) {
        let v: Vec<Pos2> = pts.iter().map(|(x, y)| self.pt(*x, *y)).collect();
        self.p.add(Shape::Path(PathShape { points: v, closed, fill: Color32::TRANSPARENT, stroke: self.st().into() }));
    }
    fn fill_poly(&self, pts: &[(f32, f32)]) {
        let v: Vec<Pos2> = pts.iter().map(|(x, y)| self.pt(*x, *y)).collect();
        self.p.add(Shape::convex_polygon(v, self.col, Stroke::NONE));
    }
    fn rect(&self, x: f32, y: f32, w: f32, h: f32, r: f32) {
        let rc = Rect::from_min_size(self.pt(x, y), egui::vec2(w * self.k, h * self.k));
        self.p.rect_stroke(rc, r * self.k, self.st(), StrokeKind::Inside);
    }
    fn fill_rect(&self, x: f32, y: f32, w: f32, h: f32, r: f32) {
        let rc = Rect::from_min_size(self.pt(x, y), egui::vec2(w * self.k, h * self.k));
        self.p.rect_filled(rc, r * self.k, self.col);
    }
    fn circle(&self, x: f32, y: f32, r: f32) {
        self.p.circle_stroke(self.pt(x, y), r * self.k, self.st());
    }
    fn dot(&self, x: f32, y: f32, r: f32) {
        self.p.circle_filled(self.pt(x, y), r * self.k, self.col);
    }
    fn arc(&self, cx: f32, cy: f32, r: f32, a0: f32, a1: f32) {
        let n = 16;
        let pts: Vec<(f32, f32)> = (0..=n)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / n as f32;
                (cx + r * a.cos(), cy + r * a.sin())
            })
            .collect();
        self.poly(&pts, false);
    }
}

/// Paints `icon` centered at `center`, fitted in a square of side `size`.
pub fn paint(p: &Painter, icon: Icon, center: Pos2, size: f32, col: Color32) {
    let k = size / 16.0;
    let g = G { p, o: Pos2::new(center.x - size / 2.0, center.y - size / 2.0), k, col, w: (1.15 * k).max(1.0) };
    use std::f32::consts::{FRAC_PI_2, PI, TAU};
    match icon {
        Icon::Logo => {
            g.p.rect_filled(Rect::from_min_size(g.pt(1.0, 1.0), egui::vec2(14.0 * k, 14.0 * k)), 4.0 * k, theme::LOGO_GRADIENT_A);
            g.p.circle_filled(g.pt(6.5, 9.5), 3.2 * k, Color32::WHITE);
            g.p.rect_filled(Rect::from_min_size(g.pt(8.9, 3.5), egui::vec2(1.8 * k, 9.2 * k)), 0.9 * k, Color32::WHITE);
            g.p.circle_filled(g.pt(6.5, 9.5), 1.4 * k, theme::LOGO_GRADIENT_A);
        }
        Icon::File => {
            g.poly(&[(3.5, 1.5), (9.5, 1.5), (12.5, 4.5), (12.5, 14.5), (3.5, 14.5)], true);
            g.poly(&[(9.5, 1.5), (9.5, 4.5), (12.5, 4.5)], false);
        }
        Icon::Assets => {
            g.rect(2.0, 2.0, 5.0, 5.0, 1.0);
            g.rect(9.0, 2.0, 5.0, 5.0, 1.0);
            g.rect(2.0, 9.0, 5.0, 5.0, 1.0);
            g.rect(9.0, 9.0, 5.0, 5.0, 1.0);
        }
        Icon::Tools => {
            g.line((2.0, 5.0), (14.0, 5.0));
            g.line((2.0, 11.0), (14.0, 11.0));
            g.circle(6.0, 5.0, 1.8);
            g.circle(10.0, 11.0, 1.8);
        }
        Icon::Variables => {
            g.circle(5.5, 8.0, 3.5);
            g.circle(10.5, 8.0, 3.5);
        }
        Icon::Help => {
            g.circle(8.0, 8.0, 6.0);
            g.arc(8.0, 6.6, 1.8, PI, TAU + 0.6);
            g.line((8.0, 8.6), (8.0, 9.4));
            g.dot(8.0, 11.4, 0.6);
        }
        Icon::User => {
            g.circle(8.0, 5.5, 2.6);
            g.arc(8.0, 15.0, 5.5, PI, TAU);
        }
        Icon::ChevronDown => g.poly(&[(4.5, 6.5), (8.0, 10.0), (11.5, 6.5)], false),
        Icon::ChevronRight => g.poly(&[(6.0, 4.0), (10.0, 8.0), (6.0, 12.0)], false),
        Icon::Plus => {
            g.line((8.0, 3.0), (8.0, 13.0));
            g.line((3.0, 8.0), (13.0, 8.0));
        }
        Icon::Minus => g.line((3.0, 8.0), (13.0, 8.0)),
        Icon::Search => {
            g.circle(7.0, 7.0, 4.2);
            g.line((10.2, 10.2), (13.5, 13.5));
        }
        Icon::Eye => {
            g.poly(&[(1.5, 8.0), (4.5, 4.5), (8.0, 3.5), (11.5, 4.5), (14.5, 8.0), (11.5, 11.5), (8.0, 12.5), (4.5, 11.5)], true);
            g.circle(8.0, 8.0, 2.2);
        }
        Icon::EyeOff => {
            g.poly(&[(1.5, 8.0), (4.5, 4.5), (8.0, 3.5), (11.5, 4.5), (14.5, 8.0), (11.5, 11.5), (8.0, 12.5), (4.5, 11.5)], true);
            g.line((3.0, 13.0), (13.0, 3.0));
        }
        Icon::Lock => {
            g.rect(3.5, 7.0, 9.0, 7.0, 1.5);
            g.arc(8.0, 7.0, 3.0, PI, TAU);
        }
        Icon::Unlock => {
            g.rect(3.5, 7.0, 9.0, 7.0, 1.5);
            g.arc(8.0, 7.0, 3.0, PI, TAU - FRAC_PI_2 * 0.6);
        }
        Icon::Sidebar => {
            g.rect(2.0, 3.0, 12.0, 10.0, 1.5);
            g.line((6.0, 3.0), (6.0, 13.0));
        }
        Icon::Page => {
            g.poly(&[(4.0, 2.0), (9.5, 2.0), (12.0, 4.5), (12.0, 14.0), (4.0, 14.0)], true);
        }
        Icon::Flag => {
            g.line((4.0, 2.0), (4.0, 14.5));
            g.fill_poly(&[(4.0, 2.5), (12.5, 5.0), (4.0, 8.0)]);
        }
        Icon::Play => g.fill_poly(&[(4.5, 2.5), (13.0, 8.0), (4.5, 13.5)]),
        Icon::Check => g.poly(&[(3.0, 8.5), (6.5, 12.0), (13.0, 4.5)], false),
        Icon::Move => g.fill_poly(&[(3.5, 2.0), (3.5, 12.5), (6.3, 9.8), (8.3, 14.0), (10.2, 13.1), (8.3, 9.0), (12.0, 9.0)]),
        Icon::Hand => {
            g.poly(
                &[
                    (5.0, 8.0),
                    (5.0, 3.5),
                    (6.6, 3.5),
                    (6.6, 7.0),
                    (6.6, 2.5),
                    (8.2, 2.5),
                    (8.2, 7.0),
                    (8.2, 3.2),
                    (9.8, 3.2),
                    (9.8, 7.5),
                    (9.8, 5.0),
                    (11.4, 5.0),
                    (11.4, 10.0),
                    (9.5, 14.0),
                    (6.5, 14.0),
                    (3.5, 10.0),
                    (3.5, 8.8),
                    (5.0, 9.5),
                ],
                false,
            );
        }
        Icon::Scale => {
            g.rect(2.5, 6.5, 7.0, 7.0, 0.5);
            g.line((9.0, 2.5), (13.5, 2.5));
            g.line((13.5, 2.5), (13.5, 7.0));
            g.line((13.5, 2.5), (8.0, 8.0));
        }
        Icon::Frame => {
            g.line((5.0, 1.5), (5.0, 14.5));
            g.line((11.0, 1.5), (11.0, 14.5));
            g.line((1.5, 5.0), (14.5, 5.0));
            g.line((1.5, 11.0), (14.5, 11.0));
        }
        Icon::Section => {
            g.rect(2.0, 4.5, 12.0, 9.0, 1.0);
            g.line((2.0, 2.5), (7.0, 2.5));
        }
        Icon::Slice => {
            g.line((4.5, 1.5), (4.5, 11.5));
            g.line((4.5, 11.5), (14.5, 11.5));
            g.line((11.5, 4.5), (11.5, 14.5));
            g.line((1.5, 4.5), (11.5, 4.5));
        }
        Icon::Rectangle => g.rect(2.5, 3.5, 11.0, 9.0, 1.0),
        Icon::Line => g.line((3.0, 13.0), (13.0, 3.0)),
        Icon::Arrow => {
            g.line((3.0, 13.0), (13.0, 3.0));
            g.poly(&[(7.0, 3.0), (13.0, 3.0), (13.0, 9.0)], false);
        }
        Icon::Ellipse => g.circle(8.0, 8.0, 5.5),
        Icon::Polygon => g.poly(&[(8.0, 2.0), (14.0, 13.0), (2.0, 13.0)], true),
        Icon::Star => {
            let mut pts = Vec::new();
            for i in 0..10 {
                let a = -FRAC_PI_2 + i as f32 * PI / 5.0;
                let r = if i % 2 == 0 { 6.5 } else { 2.8 };
                pts.push((8.0 + r * a.cos(), 8.4 + r * a.sin()));
            }
            g.poly(&pts, true);
        }
        Icon::Image => {
            g.rect(2.0, 3.0, 12.0, 10.0, 1.0);
            g.poly(&[(2.5, 11.5), (6.0, 7.5), (9.0, 10.5), (11.0, 8.5), (13.5, 11.5)], false);
            g.dot(11.0, 6.0, 1.0);
        }
        Icon::Pen => {
            g.poly(&[(8.0, 1.5), (12.0, 7.5), (8.0, 13.0), (4.0, 7.5)], true);
            g.line((8.0, 13.0), (8.0, 14.5));
            g.circle(8.0, 8.0, 1.2);
        }
        Icon::Pencil => {
            g.poly(&[(3.0, 13.0), (3.8, 10.0), (11.0, 2.8), (13.2, 5.0), (6.0, 12.2)], true);
            g.line((9.5, 4.3), (11.7, 6.5));
        }
        Icon::Text => {
            g.line((3.5, 3.5), (12.5, 3.5));
            g.line((8.0, 3.5), (8.0, 13.0));
            g.line((6.5, 13.0), (9.5, 13.0));
        }
        Icon::Comment => g.poly(&[(3.0, 3.0), (13.0, 3.0), (13.0, 11.0), (7.5, 11.0), (4.0, 14.0), (4.0, 11.0), (3.0, 11.0)], true),
        Icon::Group => {
            g.rect(2.5, 2.5, 11.0, 11.0, 1.0);
            g.rect(5.0, 5.0, 6.0, 6.0, 0.5);
        }
        Icon::Vector => {
            g.poly(&[(3.0, 12.0), (6.0, 4.0), (11.0, 9.0), (13.0, 3.5)], false);
            g.fill_rect(2.0, 11.0, 2.5, 2.5, 0.4);
            g.fill_rect(10.0, 8.0, 2.5, 2.5, 0.4);
        }
        Icon::Component => {
            g.fill_poly(&[(8.0, 1.5), (10.5, 4.0), (8.0, 6.5), (5.5, 4.0)]);
            g.fill_poly(&[(8.0, 9.5), (10.5, 12.0), (8.0, 14.5), (5.5, 12.0)]);
            g.fill_poly(&[(1.5, 8.0), (4.0, 5.5), (6.5, 8.0), (4.0, 10.5)]);
            g.fill_poly(&[(9.5, 8.0), (12.0, 5.5), (14.5, 8.0), (12.0, 10.5)]);
        }
        Icon::Instance => g.poly(&[(8.0, 1.5), (14.5, 8.0), (8.0, 14.5), (1.5, 8.0)], true),
        Icon::Boolean => {
            g.rect(2.0, 2.0, 8.0, 8.0, 1.0);
            g.rect(6.0, 6.0, 8.0, 8.0, 1.0);
        }
        Icon::AlignLeft => {
            g.line((2.5, 2.0), (2.5, 14.0));
            g.fill_rect(4.5, 4.0, 8.0, 3.0, 0.5);
            g.fill_rect(4.5, 9.0, 5.0, 3.0, 0.5);
        }
        Icon::AlignHCenter => {
            g.line((8.0, 2.0), (8.0, 14.0));
            g.fill_rect(3.0, 4.0, 10.0, 3.0, 0.5);
            g.fill_rect(5.0, 9.0, 6.0, 3.0, 0.5);
        }
        Icon::AlignRight => {
            g.line((13.5, 2.0), (13.5, 14.0));
            g.fill_rect(3.5, 4.0, 8.0, 3.0, 0.5);
            g.fill_rect(6.5, 9.0, 5.0, 3.0, 0.5);
        }
        Icon::AlignTop => {
            g.line((2.0, 2.5), (14.0, 2.5));
            g.fill_rect(4.0, 4.5, 3.0, 8.0, 0.5);
            g.fill_rect(9.0, 4.5, 3.0, 5.0, 0.5);
        }
        Icon::AlignVCenter => {
            g.line((2.0, 8.0), (14.0, 8.0));
            g.fill_rect(4.0, 3.0, 3.0, 10.0, 0.5);
            g.fill_rect(9.0, 5.0, 3.0, 6.0, 0.5);
        }
        Icon::AlignBottom => {
            g.line((2.0, 13.5), (14.0, 13.5));
            g.fill_rect(4.0, 3.5, 3.0, 8.0, 0.5);
            g.fill_rect(9.0, 6.5, 3.0, 5.0, 0.5);
        }
        Icon::Rotation => {
            g.arc(8.0, 8.0, 5.0, -PI * 0.15, PI * 1.3);
            g.poly(&[(12.5, 4.2), (13.2, 7.4), (10.2, 6.6)], false);
        }
        Icon::Radius => {
            g.arc(13.0, 13.0, 10.0, PI, PI * 1.5);
            g.line((3.0, 13.0), (3.0, 14.0));
        }
        Icon::Opacity => {
            g.circle(8.0, 8.0, 5.5);
            g.fill_poly(&[(8.0, 2.5), (11.0, 3.8), (13.5, 8.0), (11.0, 12.2), (8.0, 13.5)]);
        }
        Icon::Mask => {
            g.circle(6.0, 8.0, 4.0);
            g.rect(7.0, 4.0, 7.0, 8.0, 1.0);
        }
        Icon::DirHorizontal => {
            g.line((2.0, 8.0), (13.5, 8.0));
            g.poly(&[(10.5, 5.0), (13.5, 8.0), (10.5, 11.0)], false);
        }
        Icon::DirVertical => {
            g.line((8.0, 2.0), (8.0, 13.5));
            g.poly(&[(5.0, 10.5), (8.0, 13.5), (11.0, 10.5)], false);
        }
        Icon::Wrap => {
            g.poly(&[(3.0, 4.0), (13.0, 4.0), (13.0, 9.0), (4.0, 9.0)], false);
            g.poly(&[(6.0, 12.0), (3.5, 9.0), (6.0, 6.5)], false);
        }
        Icon::Spacing => {
            g.line((3.0, 2.5), (3.0, 13.5));
            g.line((13.0, 2.5), (13.0, 13.5));
            g.line((5.0, 8.0), (11.0, 8.0));
        }
        Icon::PaddingH => {
            g.rect(2.0, 3.0, 12.0, 10.0, 1.0);
            g.line((5.0, 6.0), (5.0, 10.0));
            g.line((11.0, 6.0), (11.0, 10.0));
        }
        Icon::PaddingV => {
            g.rect(2.0, 3.0, 12.0, 10.0, 1.0);
            g.line((6.0, 6.0), (10.0, 6.0));
            g.line((6.0, 10.0), (10.0, 10.0));
        }
        Icon::Dev => {
            g.poly(&[(5.5, 4.0), (1.5, 8.0), (5.5, 12.0)], false);
            g.poly(&[(10.5, 4.0), (14.5, 8.0), (10.5, 12.0)], false);
        }
        Icon::Design => {
            g.fill_poly(&[(3.5, 2.0), (3.5, 12.5), (6.3, 9.8), (8.3, 14.0), (10.2, 13.1), (8.3, 9.0), (12.0, 9.0)]);
        }
        Icon::Export => {
            g.line((8.0, 2.0), (8.0, 10.0));
            g.poly(&[(5.0, 5.0), (8.0, 2.0), (11.0, 5.0)], false);
            g.poly(&[(3.0, 9.0), (3.0, 13.5), (13.0, 13.5), (13.0, 9.0)], false);
        }
        Icon::Sparkle => {
            g.fill_poly(&[(8.0, 1.5), (9.4, 6.6), (14.5, 8.0), (9.4, 9.4), (8.0, 14.5), (6.6, 9.4), (1.5, 8.0), (6.6, 6.6)]);
        }
        Icon::Distribute => {
            g.line((4.0, 3.5), (12.0, 3.5));
            g.rect(5.0, 7.0, 6.0, 2.0, 0.5);
            g.line((4.0, 12.5), (12.0, 12.5));
        }
        Icon::Rotate90 => {
            g.poly(&[(8.0, 4.5), (12.0, 8.5), (8.0, 12.5), (4.0, 8.5)], true);
            g.poly(&[(5.0, 2.0), (8.0, 3.0), (6.0, 5.5)], false);
        }
        Icon::FlipH => {
            g.line((8.0, 2.0), (8.0, 14.0));
            g.poly(&[(6.0, 5.0), (2.5, 8.0), (6.0, 11.0)], true);
            g.poly(&[(10.0, 5.0), (13.5, 8.0), (10.0, 11.0)], true);
        }
        Icon::FlipV => {
            g.line((2.0, 8.0), (14.0, 8.0));
            g.poly(&[(5.0, 6.0), (8.0, 2.5), (11.0, 6.0)], true);
            g.poly(&[(5.0, 10.0), (8.0, 13.5), (11.0, 10.0)], true);
        }
        Icon::TextLeft => {
            g.line((2.5, 4.0), (13.5, 4.0));
            g.line((2.5, 8.0), (9.0, 8.0));
            g.line((2.5, 12.0), (12.0, 12.0));
        }
        Icon::TextCenter => {
            g.line((2.5, 4.0), (13.5, 4.0));
            g.line((5.0, 8.0), (11.0, 8.0));
            g.line((3.5, 12.0), (12.5, 12.0));
        }
        Icon::TextRight => {
            g.line((2.5, 4.0), (13.5, 4.0));
            g.line((7.0, 8.0), (13.5, 8.0));
            g.line((4.0, 12.0), (13.5, 12.0));
        }
    }
}
