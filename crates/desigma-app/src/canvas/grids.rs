//! Canvas guides drawn over the page: the pixel grid (from 800% zoom) and the
//! layout grids (`FrameProps::grids`) of frames.

use super::*;
use crate::theme;
use crate::util::*;
use desigma_doc::{Color, LayoutGrid, NodeKind};
use egui::{Color32, Painter, Pos2, Shape, Stroke};

/// Start and length of each stripe of a columns / rows grid along an extent.
/// `count` stripes separated by `gutter`, inset by `offset` on both ends.
pub fn stripes(count: u32, gutter: f32, offset: f32, extent: f32) -> Vec<(f32, f32)> {
    if count == 0 {
        return vec![];
    }
    let avail = extent - 2.0 * offset;
    let w = (avail - gutter * (count - 1) as f32) / count as f32;
    if !(w > 0.0) || !w.is_finite() {
        return vec![];
    }
    (0..count).map(|i| (offset + i as f32 * (w + gutter), w)).collect()
}

/// Positions of the lines of a square grid with cells of `size` inside `extent`.
pub fn grid_lines(size: f32, extent: f32) -> Vec<f32> {
    if !(size >= 1.0) || !extent.is_finite() {
        return vec![];
    }
    let n = (extent / size).floor() as usize;
    (1..=n).map(|i| i as f32 * size).filter(|v| *v <= extent).collect()
}

fn c32(c: &Color) -> Color32 {
    let f = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(f(c.r), f(c.g), f(c.b), f(c.a))
}

pub fn paint(app: &App, ui: &Ui, rect: ERect) {
    let p = ui.painter().with_clip_rect(rect);
    if app.ui.pixel_grid && app.view.zoom >= theme::PIXEL_GRID_MIN_ZOOM {
        pixel_grid(app, &p, rect);
    }
    if !app.ui.hide_layout_grids {
        layout_grids(app, &p, rect);
    }
}

fn page_is_light(app: &App) -> bool {
    match app.doc.get(app.page).map(|n| &n.kind) {
        Some(NodeKind::Page { background }) => {
            let f = |v: f32| (v * 255.0) as u8;
            theme::is_light(Color32::from_rgb(f(background.r), f(background.g), f(background.b)))
        }
        _ => false,
    }
}

fn pixel_grid(app: &App, p: &Painter, rect: ERect) {
    let col = if page_is_light(app) { theme::PIXEL_GRID_DARK_LINES } else { theme::PIXEL_GRID_LIGHT_LINES };
    let stroke = Stroke::new(1.0, col);
    let tl = app.view.to_page(rect.min, rect.min);
    let br = app.view.to_page(rect.min, rect.max);
    let (x0, x1) = (tl.x.floor() as i64, br.x.ceil() as i64);
    let (y0, y1) = (tl.y.floor() as i64, br.y.ceil() as i64);
    for x in x0..=x1 {
        let sx = app.view.to_screen(rect.min, v2(x as f32, 0.0)).x;
        p.line_segment([Pos2::new(sx, rect.top()), Pos2::new(sx, rect.bottom())], stroke);
    }
    for y in y0..=y1 {
        let sy = app.view.to_screen(rect.min, v2(0.0, y as f32)).y;
        p.line_segment([Pos2::new(rect.left(), sy), Pos2::new(rect.right(), sy)], stroke);
    }
}

fn layout_grids(app: &App, p: &Painter, rect: ERect) {
    let z = app.view.zoom;
    let view = Rect::new(app.view.offset.x, app.view.offset.y, rect.width() / z, rect.height() / z);
    let mut stack: Vec<NodeId> = app.doc.get(app.page).map(|n| n.children.clone()).unwrap_or_default();
    while let Some(id) = stack.pop() {
        let Some(n) = app.doc.get(id) else { continue };
        if !n.visible {
            continue;
        }
        stack.extend(n.children.iter().copied());
        let Some(fp) = n.kind.frame_props() else { continue };
        if fp.grids.is_empty() || !world_aabb(&app.doc, id).intersects(&view) {
            continue;
        }
        let w = app.doc.world_transform(id);
        let to_s = |x: f32, y: f32| app.view.to_screen(rect.min, w.apply(v2(x, y)));
        let quad = |x0: f32, y0: f32, x1: f32, y1: f32, col: Color32| {
            p.add(Shape::convex_polygon(vec![to_s(x0, y0), to_s(x1, y0), to_s(x1, y1), to_s(x0, y1)], col, Stroke::NONE));
        };
        for g in &fp.grids {
            match g {
                LayoutGrid::Grid { size, color } => {
                    // too dense to read: skip below 4 screen px per cell
                    if *size * z < 4.0 {
                        continue;
                    }
                    let stroke = Stroke::new(1.0, c32(color));
                    for x in grid_lines(*size, n.size.x) {
                        p.line_segment([to_s(x, 0.0), to_s(x, n.size.y)], stroke);
                    }
                    for y in grid_lines(*size, n.size.y) {
                        p.line_segment([to_s(0.0, y), to_s(n.size.x, y)], stroke);
                    }
                }
                LayoutGrid::Columns { count, gutter, offset, color } => {
                    for (x, cw) in stripes(*count, *gutter, *offset, n.size.x) {
                        quad(x, 0.0, x + cw, n.size.y, c32(color));
                    }
                }
                LayoutGrid::Rows { count, gutter, offset, color } => {
                    for (y, rh) in stripes(*count, *gutter, *offset, n.size.y) {
                        quad(0.0, y, n.size.x, y + rh, c32(color));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_stripes_fill_the_extent() {
        // 4 columns, 20px gutters, 10px margins in 400px: (380 - 60) / 4 = 80 each
        let s = stripes(4, 20.0, 10.0, 400.0);
        assert_eq!(s, vec![(10.0, 80.0), (110.0, 80.0), (210.0, 80.0), (310.0, 80.0)]);
        let last = s.last().unwrap();
        assert_eq!(last.0 + last.1, 390.0);
    }

    #[test]
    fn degenerate_stripes_are_empty() {
        assert!(stripes(0, 10.0, 0.0, 100.0).is_empty());
        assert!(stripes(10, 50.0, 0.0, 100.0).is_empty(), "gutters wider than the extent");
        assert!(stripes(2, 0.0, 60.0, 100.0).is_empty(), "margins eat everything");
    }

    #[test]
    fn square_grid_lines() {
        assert_eq!(grid_lines(10.0, 35.0), vec![10.0, 20.0, 30.0]);
        assert!(grid_lines(0.0, 100.0).is_empty());
        assert!(grid_lines(10.0, 5.0).is_empty());
    }
}
