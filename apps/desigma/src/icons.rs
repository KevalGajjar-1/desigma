//! Vector icons drawn with the egui painter, modeled on Figma's 24px icon set.

use egui::{Color32, Painter, Rect, Shape, Stroke, StrokeKind, pos2, vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Move,
    Frame,
    Rectangle,
    Ellipse,
    Text,
    Hand,
    Group,
    Eye,
    EyeOff,
    Lock,
    Unlock,
    Plus,
    Minus,
    ChevronRight,
    ChevronDown,
    AlignLeft,
    AlignCenter,
    AlignRight,
    Opacity,
    Radius,
    StrokeWeight,
}

/// Draws `icon` centered in `rect`, scaled to a 16px glyph.
pub fn paint(p: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let c = rect.center();
    let s = 1.0;
    let st = Stroke::new(1.0, color);
    let at = |x: f32, y: f32| pos2(c.x + x * s, c.y + y * s);
    match icon {
        Icon::Move => {
            let pts = vec![
                at(-4.5, -6.5),
                at(4.5, 1.5),
                at(0.5, 1.8),
                at(-1.8, 6.0),
                at(-4.5, -6.5),
            ];
            p.add(Shape::convex_polygon(pts.clone(), Color32::TRANSPARENT, st));
            p.add(Shape::line(pts, st));
        }
        Icon::Frame => {
            for x in [-3.0, 3.0] {
                p.line_segment([at(x, -7.0), at(x, 7.0)], st);
            }
            for y in [-3.0, 3.0] {
                p.line_segment([at(-7.0, y), at(7.0, y)], st);
            }
        }
        Icon::Rectangle => {
            p.rect_stroke(
                Rect::from_center_size(c, vec2(12.0, 12.0)),
                1.0,
                st,
                StrokeKind::Middle,
            );
        }
        Icon::Ellipse => {
            p.circle_stroke(c, 6.5, st);
        }
        Icon::Text => {
            p.line_segment([at(-5.5, -6.0), at(5.5, -6.0)], Stroke::new(1.2, color));
            p.line_segment([at(0.0, -6.0), at(0.0, 6.5)], Stroke::new(1.2, color));
            p.line_segment([at(-5.5, -6.0), at(-5.5, -4.0)], st);
            p.line_segment([at(5.5, -6.0), at(5.5, -4.0)], st);
            p.line_segment([at(-2.0, 6.5), at(2.0, 6.5)], st);
        }
        Icon::Hand => {
            let r = Rect::from_center_size(at(0.0, 2.0), vec2(9.0, 8.0));
            p.rect_stroke(r, 3.0, st, StrokeKind::Middle);
            for (i, h) in [4.5f32, 6.5, 6.0, 4.5].iter().enumerate() {
                let x = r.left() + 1.2 + i as f32 * 2.2;
                p.line_segment([pos2(x, r.top()), pos2(x, r.top() - h)], st);
            }
        }
        Icon::Group => {
            let r = Rect::from_center_size(c, vec2(12.0, 12.0));
            dashed_rect(p, r, color);
        }
        Icon::Eye | Icon::EyeOff => {
            let n = 16;
            let mut top = vec![];
            let mut bottom = vec![];
            for i in 0..=n {
                let t = i as f32 / n as f32;
                let x = -7.0 + 14.0 * t;
                let y = 4.5 * (std::f32::consts::PI * t).sin();
                top.push(at(x, -y));
                bottom.push(at(x, y));
            }
            p.add(Shape::line(top, st));
            p.add(Shape::line(bottom, st));
            p.circle_stroke(c, 2.0, st);
            if icon == Icon::EyeOff {
                p.line_segment([at(-6.0, 6.0), at(6.0, -6.0)], st);
            }
        }
        Icon::Lock | Icon::Unlock => {
            let body = Rect::from_min_max(at(-4.5, -1.0), at(4.5, 6.0));
            p.rect_stroke(body, 1.0, st, StrokeKind::Middle);
            let (x0, x1) = if icon == Icon::Lock {
                (-2.8, 2.8)
            } else {
                (0.2, 5.8)
            };
            let mut arc = vec![];
            for i in 0..=10 {
                let t = std::f32::consts::PI * (1.0 + i as f32 / 10.0);
                arc.push(at((x0 + x1) / 2.0 + 2.8 * t.cos(), -3.0 + 3.0 * t.sin()));
            }
            p.add(Shape::line(arc, st));
            p.line_segment([at(x0, -3.0), at(x0, -1.0)], st);
            if icon == Icon::Lock {
                p.line_segment([at(x1, -3.0), at(x1, -1.0)], st);
            }
        }
        Icon::Plus => {
            p.line_segment([at(-5.5, 0.0), at(5.5, 0.0)], st);
            p.line_segment([at(0.0, -5.5), at(0.0, 5.5)], st);
        }
        Icon::Minus => {
            p.line_segment([at(-5.5, 0.0), at(5.5, 0.0)], st);
        }
        Icon::ChevronRight => {
            p.add(Shape::convex_polygon(
                vec![at(-1.5, -3.0), at(2.0, 0.0), at(-1.5, 3.0)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::ChevronDown => {
            p.add(Shape::convex_polygon(
                vec![at(-3.0, -1.5), at(3.0, -1.5), at(0.0, 2.0)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Opacity => {
            let r = Rect::from_center_size(c, vec2(10.0, 10.0));
            p.rect_stroke(r, 1.0, st, StrokeKind::Middle);
            p.rect_filled(
                Rect::from_min_max(r.center_top(), r.right_bottom()),
                0.0,
                color,
            );
        }
        Icon::Radius => {
            let mut arc = vec![at(-4.5, 5.0)];
            for i in 0..=8 {
                let t = std::f32::consts::FRAC_PI_2 * (2.0 + i as f32 / 8.0);
                arc.push(at(1.5 + 6.0 * t.cos(), 1.0 + 6.0 * t.sin()).max(at(-4.5, -5.0)));
            }
            arc.push(at(5.0, -5.0));
            p.add(Shape::line(arc, st));
        }
        Icon::StrokeWeight => {
            for (i, w) in [1.0f32, 1.5, 2.5].iter().enumerate() {
                let y = -4.0 + i as f32 * 4.0;
                p.line_segment([at(-5.5, y), at(5.5, y)], Stroke::new(*w, color));
            }
        }
        Icon::AlignLeft | Icon::AlignCenter | Icon::AlignRight => {
            for (i, w) in [12.0f32, 8.0, 12.0, 8.0].iter().enumerate() {
                let y = -4.5 + i as f32 * 3.0;
                let x0 = match icon {
                    Icon::AlignLeft => -6.0,
                    Icon::AlignCenter => -w / 2.0,
                    _ => 6.0 - w,
                };
                p.line_segment([at(x0, y), at(x0 + w, y)], st);
            }
        }
    }
}

pub fn dashed_rect(p: &Painter, r: Rect, color: Color32) {
    let corners = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.left_top(),
    ];
    for w in corners.windows(2) {
        p.extend(Shape::dashed_line(
            &[w[0], w[1]],
            Stroke::new(1.0, color),
            2.0,
            2.0,
        ));
    }
}

/// A square icon button in Figma style. Returns the response.
pub fn icon_button(ui: &mut egui::Ui, icon: Icon, selected: bool, size: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), egui::Sense::click());
    let painter = ui.painter();
    let bg = if selected {
        crate::theme::BLUE
    } else if resp.hovered() {
        crate::theme::HOVER_BG
    } else {
        Color32::TRANSPARENT
    };
    painter.rect_filled(rect, crate::theme::RADIUS, bg);
    paint(painter, rect, icon, crate::theme::TEXT);
    resp
}
