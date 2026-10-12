//! GPU renderer vs the CPU reference. Tests print a note and return when the
//! machine has no GPU adapter.

mod common;

use common::*;
use desigma_doc::*;
use desigma_render::Viewport;
use std::sync::Arc;

#[test]
fn simple_shapes() {
    let Some(mut g) = gpu() else { return };
    let mut t = T::new();
    t.rect(None, Rect::new(10.0, 10.0, 60.0, 40.0), Color::rgb(0x18, 0xA0, 0xFB));
    let e = t.add(None, |id| Node::ellipse(id, Rect::new(90.0, 10.0, 50.0, 50.0)));
    t.node(e).fills = vec![Paint::solid(Color::rgb(0xF2, 0x4E, 0x1E))];
    let c = compare(&mut g, "simple", &t.doc, t.page, Viewport::default(), (160, 80), 8);
    // tiny scene: nearly all of its mismatches are edge pixels
    assert!(c.report.ratio < 0.03, "{:?}", c.report);
}

#[test]
fn render_demo() {
    let Some(mut g) = gpu() else { return };
    let (doc, page) = demo::demo_scene();
    let vp = Viewport { offset: Vec2::ZERO, zoom: 1.0, dpr: 2.0 };
    let c = compare(&mut g, "render_demo", &doc, page, vp, (1560, 2000), 8);
    assert!(c.report.ratio < 0.01, "mismatch {:.3}%", c.report.ratio * 100.0);
}

#[test]
fn render_demo_zoomed() {
    let Some(mut g) = gpu() else { return };
    let (doc, page) = demo::demo_scene();
    // zoomed in and panned: nothing is re-rasterised on the GPU, geometry is re-flattened by bucket
    let vp = Viewport { offset: Vec2::new(300.0, 340.0), zoom: 3.0, dpr: 1.0 };
    let c = compare(&mut g, "render_demo_zoom3", &doc, page, vp, (1200, 900), 8);
    assert!(c.report.ratio < 0.02, "mismatch {:.3}%", c.report.ratio * 100.0);
}

fn fx_scene() -> (Document, NodeId) {
    let mut t = T::new();
    t.doc.images.insert("checker".into(), demo::checker());
    // stripes behind glass
    for i in 0..10 {
        t.rect(None, Rect::new(10.0 + i as f32 * 14.0, 10.0, 7.0, 90.0), Color::rgb(0x22, 0x22, 0x22));
    }
    let glass = t.rect(None, Rect::new(30.0, 30.0, 90.0, 50.0), Color::rgba(255, 255, 255, 40));
    t.node(glass).corner_radii = [14.0; 4];
    t.node(glass).effects =
        vec![Effect::Glass(GlassEffect { light_angle: 45.0, light_intensity: 0.7, refraction: 0.5, depth: 10.0, dispersion: 0.3, frost: 4.0, visible: true })];
    // noise (shape fast path) and noise on a group (general path)
    let n = t.rect(None, Rect::new(180.0, 10.0, 80.0, 60.0), Color::rgb(0x44, 0x88, 0xDD));
    t.node(n).effects = vec![Effect::Noise(NoiseEffect {
        mode: NoiseMode::Duo { color: Color::rgb(0, 0, 0), secondary: Color::WHITE },
        size: 1.5,
        density: 0.5,
        opacity: 0.4,
        blend: BlendMode::Normal,
        seed: 7,
        visible: true,
    })];
    let tx = t.rect(None, Rect::new(180.0, 90.0, 80.0, 50.0), Color::rgb(0xE0, 0x50, 0x30));
    t.node(tx).effects = vec![Effect::Texture(TextureEffect { size: 6.0, radius: 4.0, clip_to_shape: false, seed: 3, visible: true })];
    // clipped rounded frame with rotated child, image + blend modes
    let fr = t.add(None, |id| Node::frame(id, Rect::new(10.0, 120.0, 100.0, 70.0)));
    if let NodeKind::Frame(p) = &mut t.node(fr).kind {
        p.clips_content = true;
    }
    t.node(fr).corner_radii = [18.0; 4];
    let ch = t.rect(Some(fr), Rect::new(30.0, 20.0, 90.0, 60.0), Color::rgb(0x22, 0xAA, 0x66));
    t.node(ch).transform = Affine::translate(30.0, 20.0) * Affine::rotate(0.5);
    let im = t.rect(None, Rect::new(130.0, 120.0, 60.0, 60.0), Color::WHITE);
    t.node(im).fills = vec![Paint {
        kind: PaintKind::Image { image_ref: "checker".into(), scale_mode: ImageScaleMode::Fill },
        opacity: 0.9,
        visible: true,
        blend: BlendMode::Multiply,
    }];
    let modes = [BlendMode::Screen, BlendMode::Overlay, BlendMode::Difference, BlendMode::Hue, BlendMode::LinearDodge, BlendMode::SoftLight];
    for (i, m) in modes.iter().enumerate() {
        let r = t.rect(None, Rect::new(200.0 + i as f32 * 10.0, 160.0, 40.0, 30.0), Color::rgb(0xC0, 0x40, 0x90));
        t.node(r).blend = *m;
    }
    (t.doc, t.page)
}

#[test]
fn effects_and_blends() {
    let Some(mut g) = gpu() else { return };
    let (doc, page) = fx_scene();
    let c = compare(&mut g, "fx", &doc, page, Viewport { offset: Vec2::ZERO, zoom: 1.0, dpr: 2.0 }, (600, 420), 8);
    assert!(c.report.ratio < 0.03, "mismatch {:.3}%", c.report.ratio * 100.0);
}

#[test]
fn cpu_fallback_matches_cpu() {
    let Some(mut g) = gpu() else { return };
    let (doc, page) = fx_scene();
    // every node with an effect goes through the CPU-into-texture fallback
    g.compiler.opts.force_fallback = Some(Arc::new(|n: &Node| !n.effects.is_empty()));
    let c = compare(&mut g, "fx_fallback", &doc, page, Viewport { offset: Vec2::ZERO, zoom: 1.0, dpr: 2.0 }, (600, 420), 8);
    let stats = g.compile(&doc, page, 1, &Viewport { offset: Vec2::ZERO, zoom: 1.0, dpr: 2.0 }, (600, 420)).stats();
    assert!(stats.fallback_layers >= 2, "{stats:?}");
    assert!(c.report.ratio < 0.02, "mismatch {:.3}%", c.report.ratio * 100.0);
}
