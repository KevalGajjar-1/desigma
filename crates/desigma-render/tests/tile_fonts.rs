//! Own test binary: registering fonts bumps a process-wide generation that
//! would invalidate caches used by tests running in parallel.

mod common;

use common::*;
use desigma_doc::*;
use desigma_render::tiny_skia::Pixmap;
use desigma_render::{render_page, TileBudget, TileCache, Viewport};

#[test]
fn registered_fonts_reach_worker_threads() {
    let mut t = T::new();
    let style = TextStyle { font_size: 24.0, fills: vec![Paint::solid(Color::BLACK)], ..Default::default() };
    for i in 0..6 {
        t.add(None, |id| Node::text(id, Rect::new(10.0 + i as f32 * 120.0, 20.0 + i as f32 * 90.0, 300.0, 30.0), "Worker thread text", style.clone()));
    }
    let before = desigma_render::font_generation();
    // registering a face the database already has must not break shaping anywhere
    desigma_render::register_font(desigma_fonts::bundled()[0].data.to_vec());
    assert_eq!(desigma_render::font_generation(), before + 1);

    let vp = Viewport::default();
    let mut cache = TileCache::new();
    let f = cache.render_tiles(&t.doc, t.page, &vp, (768, 768), 1, TileBudget::unlimited());
    let mut out = Pixmap::new(768, 768).unwrap();
    for tile in &f.tiles {
        let (x, y) = tile.key.device_origin();
        out.draw_pixmap(x, y, tile.pixmap.as_ref().as_ref(), &Default::default(), Default::default(), None);
    }
    let mut want = Pixmap::new(768, 768).unwrap();
    render_page(&t.doc, t.page, &vp, &mut want);
    let diffs: Vec<usize> = out.data().iter().zip(want.data()).enumerate().filter(|(_, (a, b))| a != b).map(|(i, _)| i / 4).take(5).collect();
    let n = out.data().iter().zip(want.data()).filter(|(a, b)| a != b).count();
    assert!(diffs.is_empty(), "{n} differing bytes, first pixels {:?}", diffs.iter().map(|i| (i % 768, i / 768)).collect::<Vec<_>>());

    // a font registered after the workers started is replayed on them too
    desigma_render::register_font(desigma_fonts::bundled()[0].data.to_vec());
    cache.invalidate_all();
    let f = cache.render_tiles(&t.doc, t.page, &vp, (768, 768), 1, TileBudget::unlimited());
    assert_eq!(f.pending(), 0);
    assert!(f.tiles.iter().any(|t| t.pixmap.data().iter().any(|b| *b != 0xFF)), "text was drawn");
}
