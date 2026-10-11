//! In-app clipboard: nodes (with subtrees) serialized as JSON, plus subtree cloning.

use desigma_doc::{Document, Node, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
