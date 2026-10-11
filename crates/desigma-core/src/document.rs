//! Documents, pages and tree operations.

use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::geometry::{Rect, Vec2};
use crate::node::{Node, NodeId, NodeKind};

pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub format_version: u32,
    pub pages: Vec<Page>,
    /// Next id handed out by [`Document::alloc_id`]. Ids are unique across pages.
    pub next_id: u64,
}

impl Default for Document {
    fn default() -> Self {
        let mut doc = Self {
            format_version: FORMAT_VERSION,
            pages: vec![],
            next_id: 1,
        };
        doc.add_page("Page 1");
        doc
    }
}

impl Document {
    pub fn alloc_id(&mut self) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        id
    }

    pub fn add_page(&mut self, name: impl Into<String>) -> usize {
        let id = self.alloc_id();
        self.pages.push(Page {
            id,
            name: name.into(),
            background: Color::CANVAS_DARK,
            children: vec![],
        });
        self.pages.len() - 1
    }

    /// Deep copy of `node` with fresh ids for it and every descendant.
    pub fn clone_with_new_ids(&mut self, node: &Node) -> Node {
        let mut copy = node.clone();
        copy.walk_mut(&mut |n| {
            n.id = NodeId(self.next_id);
            self.next_id += 1;
        });
        copy
    }

    /// Repairs `next_id` so it is larger than every id in use. Called after loading.
    pub fn repair_ids(&mut self) {
        let mut max = 0;
        for p in &self.pages {
            max = max.max(p.id.0);
            for n in &p.children {
                n.walk(&mut |n| max = max.max(n.id.0));
            }
        }
        self.next_id = self.next_id.max(max + 1);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub id: NodeId,
    pub name: String,
    pub background: Color,
    pub children: Vec<Node>,
}

/// Where a node lives in the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Location {
    /// `None` for top-level layers.
    pub parent: Option<NodeId>,
    pub index: usize,
}

impl Page {
    pub fn find(&self, id: NodeId) -> Option<&Node> {
        fn rec(nodes: &[Node], id: NodeId) -> Option<&Node> {
            for n in nodes {
                if n.id == id {
                    return Some(n);
                }
                if let Some(f) = rec(&n.children, id) {
                    return Some(f);
                }
            }
            None
        }
        rec(&self.children, id)
    }

    pub fn find_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        fn rec(nodes: &mut [Node], id: NodeId) -> Option<&mut Node> {
            for n in nodes {
                if n.id == id {
                    return Some(n);
                }
                if let Some(f) = rec(&mut n.children, id) {
                    return Some(f);
                }
            }
            None
        }
        rec(&mut self.children, id)
    }

    fn children_of_mut(&mut self, parent: Option<NodeId>) -> Option<&mut Vec<Node>> {
        match parent {
            None => Some(&mut self.children),
            Some(p) => self.find_mut(p).map(|n| &mut n.children),
        }
    }

    pub fn children_of(&self, parent: Option<NodeId>) -> Option<&[Node]> {
        match parent {
            None => Some(&self.children),
            Some(p) => self.find(p).map(|n| n.children.as_slice()),
        }
    }

    pub fn locate(&self, id: NodeId) -> Option<Location> {
        fn rec(nodes: &[Node], parent: Option<NodeId>, id: NodeId) -> Option<Location> {
            for (i, n) in nodes.iter().enumerate() {
                if n.id == id {
                    return Some(Location { parent, index: i });
                }
                if let Some(l) = rec(&n.children, Some(n.id), id) {
                    return Some(l);
                }
            }
            None
        }
        rec(&self.children, None, id)
    }

    /// Ids from the top-level ancestor down to and including `id`.
    pub fn path_to(&self, id: NodeId) -> Option<Vec<NodeId>> {
        fn rec(nodes: &[Node], id: NodeId, path: &mut Vec<NodeId>) -> bool {
            for n in nodes {
                path.push(n.id);
                if n.id == id || rec(&n.children, id, path) {
                    return true;
                }
                path.pop();
            }
            false
        }
        let mut path = vec![];
        rec(&self.children, id, &mut path).then_some(path)
    }

    /// Page-space origin of the coordinate system `id`'s position is expressed in.
    pub fn parent_origin(&self, id: NodeId) -> Vec2 {
        let mut origin = Vec2::ZERO;
        if let Some(path) = self.path_to(id) {
            for anc in &path[..path.len() - 1] {
                if let Some(n) = self.find(*anc) {
                    origin = origin + n.position();
                }
            }
        }
        origin
    }

    /// Page-space origin for children of `parent`.
    pub fn content_origin(&self, parent: Option<NodeId>) -> Vec2 {
        match parent {
            None => Vec2::ZERO,
            Some(p) => self.world_rect(p).map(|r| r.min()).unwrap_or(Vec2::ZERO),
        }
    }

    /// Bounds of `id` in page space.
    pub fn world_rect(&self, id: NodeId) -> Option<Rect> {
        let n = self.find(id)?;
        Some(n.rect().translate(self.parent_origin(id)))
    }

    pub fn selection_bounds(&self, ids: &[NodeId]) -> Option<Rect> {
        ids.iter().filter_map(|id| self.world_rect(*id)).reduce(|a, b| a.union(&b))
    }

    pub fn remove(&mut self, id: NodeId) -> Option<(Node, Location)> {
        let loc = self.locate(id)?;
        let siblings = self.children_of_mut(loc.parent)?;
        Some((siblings.remove(loc.index), loc))
    }

    /// Inserts `node` at `index` (clamped) under `parent`. Returns false if the parent is missing.
    pub fn insert(&mut self, parent: Option<NodeId>, index: usize, node: Node) -> bool {
        match self.children_of_mut(parent) {
            Some(siblings) => {
                let i = index.min(siblings.len());
                siblings.insert(i, node);
                true
            }
            None => false,
        }
    }

    /// Moves `id` under `new_parent` (on top), keeping its on-canvas position.
    pub fn reparent(&mut self, id: NodeId, new_parent: Option<NodeId>) -> bool {
        if let Some(p) = new_parent {
            if self.find(id).is_some_and(|n| n.contains_id(p)) {
                return false;
            }
        }
        let Some(world) = self.world_rect(id) else {
            return false;
        };
        let target_origin = self.content_origin(new_parent);
        let Some((mut node, loc)) = self.remove(id) else {
            return false;
        };
        node.x = world.x - target_origin.x;
        node.y = world.y - target_origin.y;
        let len = self.children_of(new_parent).map_or(0, |c| c.len());
        if self.insert(new_parent, len, node.clone()) {
            true
        } else {
            self.insert(loc.parent, loc.index, node);
            false
        }
    }

    /// The topmost layer under `p` that a click should select, following
    /// Figma's rules: top-level frames that have children are only selected
    /// via their name label, groups are selected as a unit, and hidden or
    /// locked layers are skipped.
    pub fn hit_test(&self, p: Vec2) -> Option<NodeId> {
        fn rec(n: &Node, origin: Vec2, p: Vec2, top_level: bool) -> Option<NodeId> {
            if !n.visible || n.locked {
                return None;
            }
            let local = p - origin;
            match &n.kind {
                NodeKind::Frame { clip_content } => {
                    if *clip_content && !n.rect().contains(local) {
                        return None;
                    }
                    let inner = origin + n.position();
                    for c in n.children.iter().rev() {
                        if let Some(hit) = rec(c, inner, p, false) {
                            return Some(hit);
                        }
                    }
                    if top_level && !n.children.is_empty() {
                        return None;
                    }
                    n.rect().contains(local).then_some(n.id)
                }
                NodeKind::Group => {
                    let inner = origin + n.position();
                    n.children
                        .iter()
                        .rev()
                        .any(|c| rec(c, inner, p, false).is_some())
                        .then_some(n.id)
                }
                _ => n.shape_contains(local).then_some(n.id),
            }
        }
        self.children.iter().rev().find_map(|n| rec(n, Vec2::ZERO, p, true))
    }

    /// The deepest visible, unlocked leaf layer under `p` (Figma's Ctrl/Cmd+click
    /// "deep select"). Containers are returned only when no child is hit.
    pub fn hit_test_deep(&self, p: Vec2) -> Option<NodeId> {
        fn rec(n: &Node, origin: Vec2, p: Vec2) -> Option<NodeId> {
            if !n.visible || n.locked {
                return None;
            }
            let local = p - origin;
            if n.clips_content() && !n.rect().contains(local) {
                return None;
            }
            let inner = origin + n.position();
            if let Some(hit) = n.children.iter().rev().find_map(|c| rec(c, inner, p)) {
                return Some(hit);
            }
            match n.kind {
                NodeKind::Group => None,
                _ => n.shape_contains(local).then_some(n.id),
            }
        }
        self.children.iter().rev().find_map(|n| rec(n, Vec2::ZERO, p))
    }

    /// Layers picked by a marquee drag, following the same rules as [`Page::hit_test`].
    pub fn nodes_in_rect(&self, area: Rect) -> Vec<NodeId> {
        let mut out = vec![];
        for n in &self.children {
            if !n.visible || n.locked {
                continue;
            }
            let r = n.rect();
            if n.is_frame() && !n.children.is_empty() {
                if area.contains_rect(&r) {
                    out.push(n.id);
                    continue;
                }
                for c in &n.children {
                    if c.visible && !c.locked && c.rect().translate(r.min()).intersects(&area) {
                        out.push(c.id);
                    }
                }
            } else if r.intersects(&area) {
                out.push(n.id);
            }
        }
        out
    }

    /// The deepest visible frame containing `p`, ignoring `exclude` and its descendants.
    pub fn frame_at(&self, p: Vec2, exclude: &[NodeId]) -> Option<NodeId> {
        fn rec(nodes: &[Node], origin: Vec2, p: Vec2, exclude: &[NodeId]) -> Option<NodeId> {
            for n in nodes.iter().rev() {
                if !n.visible || exclude.contains(&n.id) || !n.is_frame() {
                    continue;
                }
                let r = n.rect().translate(origin);
                if r.contains(p) {
                    return rec(&n.children, r.min(), p, exclude).or(Some(n.id));
                }
            }
            None
        }
        rec(&self.children, Vec2::ZERO, p, exclude)
    }

    /// Recomputes every group's bounds from its children, as Figma does.
    pub fn fit_groups(&mut self) {
        fn rec(nodes: &mut [Node]) {
            for n in nodes {
                rec(&mut n.children);
                if matches!(n.kind, NodeKind::Group) {
                    let Some(b) = n.children.iter().map(|c| c.rect()).reduce(|a, b| a.union(&b)) else {
                        continue;
                    };
                    for c in &mut n.children {
                        c.x -= b.x;
                        c.y -= b.y;
                    }
                    n.x += b.x;
                    n.y += b.y;
                    n.width = b.w;
                    n.height = b.h;
                }
            }
        }
        rec(&mut self.children);
        // Groups emptied by deletion disappear.
        fn prune(nodes: &mut Vec<Node>) {
            nodes.retain(|n| !(matches!(n.kind, NodeKind::Group) && n.children.is_empty()));
            for n in nodes {
                prune(&mut n.children);
            }
        }
        prune(&mut self.children);
    }

    /// Ids of every layer on the page, in paint order.
    pub fn all_ids(&self) -> Vec<NodeId> {
        let mut out = vec![];
        for n in &self.children {
            n.walk(&mut |n| out.push(n.id));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page_with_frame() -> (Document, NodeId, NodeId) {
        let mut doc = Document::default();
        let fid = doc.alloc_id();
        let rid = doc.alloc_id();
        let mut frame = Node::new(fid, NodeKind::Frame { clip_content: true }, Rect::new(100.0, 100.0, 200.0, 200.0));
        frame.children.push(Node::new(rid, NodeKind::Rectangle, Rect::new(10.0, 10.0, 50.0, 50.0)));
        doc.pages[0].children.push(frame);
        (doc, fid, rid)
    }

    #[test]
    fn world_rect_accumulates_parent_offsets() {
        let (doc, _, rid) = page_with_frame();
        assert_eq!(doc.pages[0].world_rect(rid), Some(Rect::new(110.0, 110.0, 50.0, 50.0)));
    }

    #[test]
    fn hit_test_prefers_children_and_skips_top_level_frame_background() {
        let (doc, fid, rid) = page_with_frame();
        let page = &doc.pages[0];
        assert_eq!(page.hit_test(Vec2::new(120.0, 120.0)), Some(rid));
        assert_eq!(page.hit_test(Vec2::new(250.0, 250.0)), None);
        assert_eq!(page.hit_test(Vec2::new(10.0, 10.0)), None);
        assert_eq!(page.hit_test_deep(Vec2::new(250.0, 250.0)), Some(fid));
    }

    #[test]
    fn empty_frame_is_hit_by_area() {
        let mut doc = Document::default();
        let id = doc.alloc_id();
        doc.pages[0]
            .children
            .push(Node::new(id, NodeKind::Frame { clip_content: true }, Rect::new(0.0, 0.0, 10.0, 10.0)));
        assert_eq!(doc.pages[0].hit_test(Vec2::new(5.0, 5.0)), Some(id));
    }

    #[test]
    fn ellipse_hit_uses_shape() {
        let mut doc = Document::default();
        let id = doc.alloc_id();
        doc.pages[0].children.push(Node::new(id, NodeKind::Ellipse, Rect::new(0.0, 0.0, 100.0, 100.0)));
        assert_eq!(doc.pages[0].hit_test(Vec2::new(50.0, 50.0)), Some(id));
        assert_eq!(doc.pages[0].hit_test(Vec2::new(2.0, 2.0)), None);
    }

    #[test]
    fn reparent_keeps_world_position() {
        let (mut doc, fid, rid) = page_with_frame();
        let page = &mut doc.pages[0];
        assert!(page.reparent(rid, None));
        assert_eq!(page.locate(rid).unwrap().parent, None);
        assert_eq!(page.world_rect(rid), Some(Rect::new(110.0, 110.0, 50.0, 50.0)));
        assert!(page.reparent(rid, Some(fid)));
        assert_eq!(page.find(rid).unwrap().x, 10.0);
        // A frame cannot be moved into its own descendant.
        assert!(!page.reparent(fid, Some(rid)));
    }

    #[test]
    fn groups_fit_children() {
        let mut doc = Document::default();
        let gid = doc.alloc_id();
        let a = doc.alloc_id();
        let b = doc.alloc_id();
        let mut g = Node::new(gid, NodeKind::Group, Rect::default());
        g.children.push(Node::new(a, NodeKind::Rectangle, Rect::new(10.0, 20.0, 10.0, 10.0)));
        g.children.push(Node::new(b, NodeKind::Rectangle, Rect::new(50.0, 40.0, 10.0, 10.0)));
        doc.pages[0].children.push(g);
        doc.pages[0].fit_groups();
        let page = &doc.pages[0];
        assert_eq!(page.find(gid).unwrap().rect(), Rect::new(10.0, 20.0, 50.0, 30.0));
        assert_eq!(page.world_rect(a), Some(Rect::new(10.0, 20.0, 10.0, 10.0)));
        assert_eq!(page.hit_test(Vec2::new(55.0, 45.0)), Some(gid));
    }

    #[test]
    fn marquee_selects_children_of_partially_covered_frame() {
        let (doc, fid, rid) = page_with_frame();
        let page = &doc.pages[0];
        assert_eq!(page.nodes_in_rect(Rect::new(105.0, 105.0, 20.0, 20.0)), vec![rid]);
        assert_eq!(page.nodes_in_rect(Rect::new(0.0, 0.0, 500.0, 500.0)), vec![fid]);
    }

    #[test]
    fn clone_with_new_ids_is_unique() {
        let (mut doc, fid, _) = page_with_frame();
        let frame = doc.pages[0].find(fid).unwrap().clone();
        let copy = doc.clone_with_new_ids(&frame);
        assert_ne!(copy.id, fid);
        assert_ne!(copy.children[0].id, frame.children[0].id);
    }
}
