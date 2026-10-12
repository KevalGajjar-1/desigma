//! Small previews of shapes and stamps for the toolbar popovers, rendered with the real renderer
//! (so a preview always looks like what gets placed) and cached as textures.

use desigma_doc::*;
use egui::{Context, TextureHandle, TextureId, TextureOptions};
use std::collections::HashMap;

#[derive(Default)]
pub struct Previews {
    map: HashMap<String, TextureHandle>,
}

/// Renders `node` (placed at the origin of a scratch document) at `scale`.
fn render(node: Node, scale: f32) -> Option<egui::ColorImage> {
    let mut doc = Document::new("preview");
    let page = doc.add_page("p");
    let id = node.id;
    doc.insert(page, node).ok()?;
    let pm = desigma_render::render_node(&doc, id, scale)?;
    let size = [pm.width() as usize, pm.height() as usize];
    Some(egui::ColorImage::from_rgba_premultiplied(size, pm.data()))
}

impl Previews {
    fn get(&mut self, ctx: &Context, key: String, make: impl FnOnce() -> Option<egui::ColorImage>) -> Option<TextureId> {
        if let Some(t) = self.map.get(&key) {
            return Some(t.id());
        }
        let img = make()?;
        let tex = ctx.load_texture(format!("board-preview-{key}"), img, TextureOptions::LINEAR);
        let id = tex.id();
        self.map.insert(key, tex);
        Some(id)
    }

    /// A shape outline filled in `fill`, `w` x `h` points big (rendered at 3x).
    pub fn shape(&mut self, ctx: &Context, kind: ShapeKind, fill: Color, w: f32, h: f32) -> Option<TextureId> {
        let key = format!("shape-{kind:?}-{}-{w}-{h}", fill.to_hex());
        self.get(ctx, key, || {
            let mut n = Node::shape_with_text(NodeId(1), Rect::new(0.0, 0.0, w, h), kind, "");
            n.fills = vec![Paint::solid(fill)];
            n.stroke.paints.clear();
            n.stroke.weight = 0.0;
            render(n, 3.0)
        })
    }

    /// A stamp `size` points big (rendered at 3x).
    pub fn stamp(&mut self, ctx: &Context, kind: &str, size: f32) -> Option<TextureId> {
        let key = format!("stamp-{kind}-{size}");
        self.get(ctx, key, || render(Node::stamp(NodeId(1), Vec2::ZERO, kind, size), 3.0))
    }
}
