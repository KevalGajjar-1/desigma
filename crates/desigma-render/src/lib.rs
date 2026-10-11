//! Turns a Desigma page into a vello [`Scene`].

mod text;

use desigma_core::{Color, Node, NodeKind, Page, Paint};
use vello::kurbo::{Affine, Ellipse, Rect, RoundedRect, Shape, Stroke};
use vello::peniko::{self, Fill, Mix};
use vello::{Glyph, Scene};

pub use text::{
    Fonts, PlacedGlyph, TextLayout, inter_medium_bytes, inter_regular_bytes, inter_semibold_bytes,
};

pub fn to_peniko(c: Color) -> peniko::Color {
    peniko::Color::new([c.r, c.g, c.b, c.a])
}

/// Appends `page`'s layers to `scene`. `transform` maps page space to target pixels.
pub fn render_page(scene: &mut Scene, page: &Page, transform: Affine, fonts: &Fonts) {
    for n in &page.children {
        render_node(scene, n, transform, fonts);
    }
}

/// Unbounded clip used for opacity layers whose content may overflow the node.
const EVERYWHERE: Rect = Rect::new(-1.0e6, -1.0e6, 1.0e6, 1.0e6);

fn render_node(scene: &mut Scene, n: &Node, parent: Affine, fonts: &Fonts) {
    if !n.visible || n.opacity <= 0.0 {
        return;
    }
    let xf = parent * Affine::translate((n.x, n.y));
    let bounds = Rect::new(0.0, 0.0, n.width, n.height);
    let layered = n.opacity < 1.0;
    if layered {
        scene.push_layer(
            Fill::NonZero,
            Mix::Normal,
            n.opacity,
            Affine::IDENTITY,
            &EVERYWHERE,
        );
    }

    match &n.kind {
        NodeKind::Frame { clip_content } => {
            let shape = rounded(bounds, n.corner_radius);
            fill_all(scene, xf, &n.fills, &shape);
            if *clip_content {
                scene.push_clip_layer(Fill::NonZero, xf, &shape);
            }
            for c in &n.children {
                render_node(scene, c, xf, fonts);
            }
            if *clip_content {
                scene.pop_layer();
            }
            stroke_inside(scene, xf, n, |inset| {
                rounded(inset, (n.corner_radius - n.stroke_weight / 2.0).max(0.0)).to_path(0.1)
            });
        }
        NodeKind::Group => {
            for c in &n.children {
                render_node(scene, c, xf, fonts);
            }
        }
        NodeKind::Rectangle => {
            let shape = rounded(bounds, n.corner_radius);
            fill_all(scene, xf, &n.fills, &shape);
            stroke_inside(scene, xf, n, |inset| {
                rounded(inset, (n.corner_radius - n.stroke_weight / 2.0).max(0.0)).to_path(0.1)
            });
        }
        NodeKind::Ellipse => {
            let shape = Ellipse::from_rect(bounds);
            fill_all(scene, xf, &n.fills, &shape);
            stroke_inside(scene, xf, n, |inset| Ellipse::from_rect(inset).to_path(0.1));
        }
        NodeKind::Text(t) => {
            let wrap = (!t.auto_width).then_some(n.width);
            let layout = fonts.layout(t, wrap);
            let face = fonts.face(t.font_weight);
            for paint in n.fills.iter().filter(|p| p.is_visible()) {
                let Paint::Solid { color, .. } = paint;
                scene
                    .draw_glyphs(&face.data)
                    .transform(xf)
                    .font_size(t.font_size as f32)
                    .brush(to_peniko(*color))
                    .draw(
                        Fill::NonZero,
                        layout.glyphs.iter().map(|g| Glyph {
                            id: g.id,
                            x: g.x,
                            y: g.y,
                        }),
                    );
            }
        }
    }

    if layered {
        scene.pop_layer();
    }
}

fn rounded(r: Rect, radius: f64) -> RoundedRect {
    let max = (r.width().min(r.height()) / 2.0).max(0.0);
    RoundedRect::from_rect(r, radius.clamp(0.0, max))
}

fn fill_all(scene: &mut Scene, xf: Affine, fills: &[Paint], shape: &impl Shape) {
    for paint in fills.iter().filter(|p| p.is_visible()) {
        let Paint::Solid { color, .. } = paint;
        scene.fill(Fill::NonZero, xf, to_peniko(*color), None, shape);
    }
}

/// Strokes aligned inside the shape, Figma's default for rectangles, ellipses and frames.
fn stroke_inside(
    scene: &mut Scene,
    xf: Affine,
    n: &Node,
    shape_at: impl Fn(Rect) -> vello::kurbo::BezPath,
) {
    if n.stroke_weight <= 0.0 {
        return;
    }
    let half = n.stroke_weight / 2.0;
    let inset = Rect::new(
        half,
        half,
        (n.width - half).max(half),
        (n.height - half).max(half),
    );
    let path = shape_at(inset);
    let style = Stroke::new(n.stroke_weight);
    for paint in n.strokes.iter().filter(|p| p.is_visible()) {
        let Paint::Solid { color, .. } = paint;
        scene.stroke(&style, xf, to_peniko(*color), None, &path);
    }
}
