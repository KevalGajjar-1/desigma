//! File thumbnails: rendering the first frame to a PNG, and the texture cache the home screen draws from.

use desigma_doc::{Document, NodeId};
use desigma_library::Library;
use std::collections::HashMap;
use std::time::SystemTime;

/// Longest edge of a stored thumbnail in pixels.
pub const THUMB_PX: f32 = 480.0;

/// The node a thumbnail shows: the first top-level frame of the first page, else its first layer.
pub fn thumbnail_node(doc: &Document) -> Option<NodeId> {
    let page = doc.get(*doc.pages.first()?)?;
    let kids: Vec<_> = page.children.iter().filter_map(|c| doc.get(*c)).filter(|n| n.visible).collect();
    kids.iter().find(|n| n.kind.frame_props().is_some() && !matches!(n.kind, desigma_doc::NodeKind::Instance { .. })).or(kids.first()).map(|n| n.id)
}

/// PNG bytes of the document's thumbnail, scaled to fit [`THUMB_PX`]; `None` for an empty document.
pub fn thumbnail_png(doc: &Document) -> Option<Vec<u8>> {
    let id = thumbnail_node(doc)?;
    let size = doc.get(id)?.size;
    let longest = size.x.max(size.y);
    if !longest.is_finite() || longest < 1.0 {
        return None;
    }
    let scale = (THUMB_PX / longest).clamp(0.01, 2.0);
    let pm = desigma_render::render_node(doc, id, scale)?;
    pm.encode_png().ok()
}

/// Renders and stores the thumbnail now.
pub fn refresh(lib: &Library, file: &str, doc: &Document) -> bool {
    match thumbnail_png(doc) {
        Some(png) => lib.set_thumbnail(file, &png).is_ok(),
        None => false,
    }
}

/// Renders and stores the thumbnail on a background thread (saving must not stutter).
pub fn refresh_in_background(lib: Library, file: String, doc: Document) {
    std::thread::spawn(move || {
        refresh(&lib, &file, &doc);
    });
}

/// Decoded thumbnails as GPU textures, reloaded when the PNG changes on disk.
#[derive(Default)]
pub struct Textures {
    map: HashMap<String, (Option<SystemTime>, Option<egui::TextureHandle>)>,
}

impl Textures {
    /// The texture for `file`'s thumbnail, loading it if new or changed. At most
    /// `budget` decodes happen per call so a big grid does not stall a frame.
    pub fn get(&mut self, ctx: &egui::Context, lib: &Library, entry: &desigma_library::FileEntry, budget: &mut u32) -> Option<(egui::TextureId, egui::Vec2)> {
        let path = lib.thumbnail_path(entry)?;
        let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if let Some((seen, tex)) = self.map.get(&entry.id) {
            if *seen == mtime {
                return tex.as_ref().map(|t| (t.id(), t.size_vec2()));
            }
        }
        if *budget == 0 {
            ctx.request_repaint();
            return self.map.get(&entry.id).and_then(|(_, t)| t.as_ref().map(|t| (t.id(), t.size_vec2())));
        }
        *budget -= 1;
        let tex = std::fs::read(&path).ok().and_then(|bytes| image::load_from_memory(&bytes).ok()).map(|img| {
            let rgba = img.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            ctx.load_texture(format!("thumb-{}", entry.id), egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()), egui::TextureOptions::LINEAR)
        });
        let out = tex.as_ref().map(|t| (t.id(), t.size_vec2()));
        self.map.insert(entry.id.clone(), (mtime, tex));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use desigma_doc::{Node, NodeKind, Rect, Vec2};

    fn doc_with_frame() -> Document {
        let mut d = Document::new("t");
        let page = d.add_page("P");
        let rect = d.alloc_id();
        d.insert(page, Node::rectangle(rect, Rect::new(0.0, 0.0, 5.0, 5.0))).unwrap();
        let fid = d.alloc_id();
        let mut f = Node::new(fid, NodeKind::Frame(Default::default()));
        f.size = Vec2::new(960.0, 540.0);
        d.insert(page, f).unwrap();
        d
    }

    #[test]
    fn picks_the_first_frame_over_loose_layers() {
        let d = doc_with_frame();
        let id = thumbnail_node(&d).unwrap();
        assert!(matches!(d.get(id).unwrap().kind, NodeKind::Frame(_)));
    }

    #[test]
    fn png_fits_the_thumbnail_size() {
        let d = doc_with_frame();
        let png = thumbnail_png(&d).unwrap();
        let img = image::load_from_memory(&png).unwrap();
        assert_eq!(img.width(), THUMB_PX as u32);
        assert!((img.height() as f32 - 270.0).abs() <= 1.0);
    }

    #[test]
    fn empty_documents_have_no_thumbnail() {
        let mut d = Document::new("e");
        d.add_page("P");
        assert!(thumbnail_node(&d).is_none());
        assert!(thumbnail_png(&d).is_none());
    }
}
