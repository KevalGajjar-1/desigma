//! In-app clipboard: nodes (with subtrees) serialized as JSON, plus subtree cloning,
//! and SVG markup pasted as text.

use crate::app::App;
use crate::edit::Tx;
use crate::layout;
use desigma_doc::{Affine, Command, Document, Node, NodeId, NodeKind, Vec2};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Pastes `text` if it is SVG markup (`<svg` or `<?xml` prolog): the layers
/// appear centered in the viewport, as one undo step, selected. Returns whether
/// the text was SVG (even if it failed to import, then a toast says why), so
/// the caller can fall back to the in-app clipboard otherwise.
pub fn paste_svg(app: &mut App, text: &str) -> bool {
    if !desigma_svg::looks_like_svg(text) {
        return false;
    }
    let view = app.canvas_rect;
    let center = app.view.to_page(view.min, view.center());
    let parent = app.container_for_insert();
    if let Err(e) = place_svg(app, text, "svg", center, parent) {
        app.show_toast(e, true);
    }
    true
}

/// Places a dropped `.svg` file's markup centered on screen position `at`
/// (the drop point; the viewport center when it is outside the canvas), into
/// the frame under that point. `nth` staggers several files dropped together.
pub fn drop_svg(app: &mut App, svg: &str, name: &str, at: Option<egui::Pos2>, nth: usize) {
    let view = app.canvas_rect;
    let screen = at.filter(|p| view.contains(*p)).unwrap_or(view.center());
    let mut p = app.view.to_page(view.min, screen);
    p.x += 24.0 * nth as f32;
    p.y += 24.0 * nth as f32;
    let parent = crate::canvas::create::container_at(&app.doc, app.page, p, &[]);
    if let Err(e) = place_svg(app, svg, name, p, parent) {
        app.show_toast(e, true);
    }
}

/// Imports SVG markup as a Frame named `name` centered on page point `center`
/// (rounded to whole pixels), appended to `parent`'s children as one undoable
/// step, and selects it.
pub fn place_svg(app: &mut App, svg: &str, name: &str, center: Vec2, parent: NodeId) -> Result<NodeId, String> {
    let mut out = desigma_svg::svg_to_nodes(svg, name, &mut || app.doc.alloc_id()).map_err(|e| format!("Cannot import SVG: {e}"))?;
    let inv = app.doc.world_transform(parent).invert().ok_or("Cannot place into a collapsed layer")?;
    let c = inv.apply(center);
    let size = out.nodes[0].size;
    out.nodes[0].transform = Affine::translate((c.x - size.x / 2.0).round(), (c.y - size.y / 2.0).round());
    app.doc.images.append(&mut out.images);
    let root = out.nodes[0].id;
    let index = app.doc.get(parent).map(|p| p.children.len()).unwrap_or(0);
    let mut tx = Tx::new();
    tx.cmd(&mut app.doc, Command::Insert { parent, index, nodes: out.nodes }).map_err(|e| e.to_string())?;
    layout::relayout_around(&mut app.doc, &mut tx.ed, parent);
    app.run_tx(tx);
    app.selection = vec![root];
    app.fonts.load_for(&app.doc);
    Ok(root)
}

const MAGIC: u32 = 0xDE51_6A01;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Clip {
    magic: u32,
    pub roots: Vec<NodeId>,
    pub nodes: Vec<Node>,
}

/// Serializes `ids` (and their subtrees) from `doc`.
pub fn capture(doc: &Document, ids: &[NodeId]) -> Option<Clip> {
    let mut nodes = Vec::new();
    for id in ids {
        doc.walk(*id, |n, _| nodes.push(n.clone()));
    }
    if nodes.is_empty() {
        return None;
    }
    Some(Clip { magic: MAGIC, roots: ids.to_vec(), nodes })
}

pub fn to_json(clip: &Clip) -> Option<String> {
    serde_json::to_string(clip).ok()
}

pub fn from_json(s: &str) -> Option<Clip> {
    let c: Clip = serde_json::from_str(s).ok()?;
    (c.magic == MAGIC && !c.roots.is_empty()).then_some(c)
}

/// Creates fresh-id copies of the clip: one root-first node list per root.
/// Roots have `parent = None`; the Insert command assigns the new parent.
pub fn instantiate(doc: &mut Document, clip: &Clip) -> Vec<Vec<Node>> {
    let mut map: HashMap<NodeId, NodeId> = HashMap::new();
    for n in &clip.nodes {
        map.insert(n.id, doc.alloc_id());
    }
    let by_id: HashMap<NodeId, &Node> = clip.nodes.iter().map(|n| (n.id, n)).collect();
    let mut out = Vec::new();
    for root in &clip.roots {
        let mut list = Vec::new();
        let mut stack = vec![*root];
        while let Some(id) = stack.pop() {
            let Some(src) = by_id.get(&id) else { continue };
            let mut n = (*src).clone();
            n.id = map[&id];
            n.source_id = None;
            // connectors copied together with their nodes join the copies; a connector copied alone
            // keeps its nodes while they exist, and is left loose (at its last point) otherwise
            if let NodeKind::Connector(c) = &mut n.kind {
                for e in [&mut c.start, &mut c.end] {
                    e.node = e.node.and_then(|t| map.get(&t).copied().or_else(|| doc.get(t).map(|_| t)));
                }
            }
            n.children = src.children.iter().filter_map(|c| map.get(c).copied()).collect();
            n.parent = if id == *root { None } else { src.parent.and_then(|p| map.get(&p).copied()) };
            stack.extend(src.children.iter().rev().copied());
            list.push(n);
        }
        out.push(list);
    }
    out
}

/// Fresh-id deep copy of the subtree rooted at `id` (root first).
pub fn clone_subtree(doc: &mut Document, id: NodeId) -> Vec<Node> {
    let Some(clip) = capture(doc, &[id]) else { return vec![] };
    instantiate(doc, &clip).into_iter().next().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use desigma_doc::Rect;

    fn doc() -> (Document, NodeId, NodeId) {
        let mut d = Document::new("t");
        let page = d.add_page("p");
        let fid = d.alloc_id();
        let f = d.insert(page, Node::frame(fid, Rect::new(0.0, 0.0, 100.0, 100.0))).unwrap();
        let rid = d.alloc_id();
        d.insert(f, Node::rectangle(rid, Rect::new(5.0, 5.0, 10.0, 10.0))).unwrap();
        (d, page, f)
    }

    #[test]
    fn json_roundtrip_and_rejects_foreign_text() {
        let (d, _, f) = doc();
        let clip = capture(&d, &[f]).unwrap();
        let s = to_json(&clip).unwrap();
        let back = from_json(&s).unwrap();
        assert_eq!(back.nodes.len(), 2);
        assert!(from_json("hello").is_none());
        assert!(from_json("{\"a\":1}").is_none());
    }

    #[test]
    fn instantiate_assigns_fresh_ids_and_links() {
        let (mut d, _, f) = doc();
        let copy = clone_subtree(&mut d, f);
        assert_eq!(copy.len(), 2);
        assert_ne!(copy[0].id, f);
        assert!(copy[0].parent.is_none());
        assert_eq!(copy[0].children, vec![copy[1].id]);
        assert_eq!(copy[1].parent, Some(copy[0].id));
        assert!(d.get(copy[0].id).is_none(), "clone is not inserted yet");
    }
}
