//! Renders a [`desigma_doc::Document`] page into a `tiny_skia::Pixmap`.
//! CONTRACT: keep `Viewport` and `render_page` signatures stable; the app and
//! the diff harness depend on them.

use desigma_doc::{Document, NodeId, Vec2};
pub use tiny_skia;

/// Maps page space to device pixels: device = (page - offset) * zoom * dpr.
#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    /// Page-space point shown at the top-left of the pixmap.
    pub offset: Vec2,
    pub zoom: f32,
    /// Device pixel ratio (2.0 on hi-dpi).
    pub dpr: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self { offset: Vec2::ZERO, zoom: 1.0, dpr: 1.0 }
    }
}

/// Draws `page` (background + all visible descendants) into `target`.
pub fn render_page(doc: &Document, page: NodeId, vp: &Viewport, target: &mut tiny_skia::Pixmap) {
    let _ = (doc, page, vp);
    target.fill(tiny_skia::Color::from_rgba8(0xF5, 0xF5, 0xF5, 0xFF));
}

/// Draws a single node subtree with its own top-left at (0,0); used for
/// exports and per-frame diffs.
pub fn render_node(doc: &Document, node: NodeId, scale: f32) -> Option<tiny_skia::Pixmap> {
    let n = doc.get(node)?;
    let w = (n.size.x * scale).ceil().max(1.0) as u32;
    let h = (n.size.y * scale).ceil().max(1.0) as u32;
    tiny_skia::Pixmap::new(w, h)
}
