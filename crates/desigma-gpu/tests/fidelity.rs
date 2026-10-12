//! GPU renderer vs the CPU reference. Tests print a note and return when the
//! machine has no GPU adapter.

mod common;

use common::*;
use desigma_doc::*;
use desigma_render::Viewport;

#[test]
fn simple_shapes() {
    let Some(mut g) = gpu() else { return };
    let mut t = T::new();
    t.rect(None, Rect::new(10.0, 10.0, 60.0, 40.0), Color::rgb(0x18, 0xA0, 0xFB));
    let e = t.add(None, |id| Node::ellipse(id, Rect::new(90.0, 10.0, 50.0, 50.0)));
    t.node(e).fills = vec![Paint::solid(Color::rgb(0xF2, 0x4E, 0x1E))];
    let c = compare(&mut g, "simple", &t.doc, t.page, Viewport::default(), (160, 80), 8);
    assert!(c.report.ratio < 0.01, "{:?}", c.report);
}

#[test]
fn render_demo() {
    let Some(mut g) = gpu() else { return };
    let (doc, page) = demo::demo_scene();
    let vp = Viewport { offset: Vec2::ZERO, zoom: 1.0, dpr: 2.0 };
    let c = compare(&mut g, "render_demo", &doc, page, vp, (1560, 2000), 8);
    assert!(c.report.ratio < 0.01, "mismatch {:.3}%", c.report.ratio * 100.0);
}
