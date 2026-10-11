//! Editing session: document, selection and undo history. All mutations the
//! UI performs go through here so they can be tested without a window.

use std::path::PathBuf;

use crate::document::{Document, Page};
use crate::geometry::{Rect, Vec2};
use crate::node::{Node, NodeId, NodeKind};

#[derive(Clone)]
struct Snapshot {
    doc: Document,
    page: usize,
    selection: Vec<NodeId>,
}

const MAX_UNDO: usize = 200;

pub struct Editor {
    pub doc: Document,
    pub page: usize,
    pub selection: Vec<NodeId>,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    coalesce_key: Option<String>,
}

impl Default for Editor {
    fn default() -> Self {
        Self::from_document(Document::default(), None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZOrder {
    Forward,
    Backward,
    Front,
    Back,
}

impl Editor {
    pub fn from_document(mut doc: Document, path: Option<PathBuf>) -> Self {
        doc.repair_ids();
        if doc.pages.is_empty() {
            doc.add_page("Page 1");
        }
        Self {
            doc,
            page: 0,
            selection: vec![],
            path,
            dirty: false,
            undo: vec![],
            redo: vec![],
            coalesce_key: None,
        }
    }

    pub fn page(&self) -> &Page {
        &self.doc.pages[self.page]
    }

    pub fn page_mut(&mut self) -> &mut Page {
        &mut self.doc.pages[self.page]
    }

    // ---- history -------------------------------------------------------

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            doc: self.doc.clone(),
            page: self.page,
            selection: self.selection.clone(),
        }
    }

    fn restore(&mut self, s: Snapshot) {
        self.doc = s.doc;
        self.page = s.page.min(self.doc.pages.len() - 1);
        self.selection = s.selection;
        self.dirty = true;
    }

    /// Records the current state as an undo step. Call before mutating.
    pub fn checkpoint(&mut self) {
        self.coalesce_key = None;
        self.push_undo();
    }

    /// Like [`Editor::checkpoint`], but consecutive calls with the same key
    /// (e.g. while dragging a number field) form a single undo step.
    pub fn checkpoint_coalesced(&mut self, key: &str) {
        if self.coalesce_key.as_deref() == Some(key) {
            self.dirty = true;
            return;
        }
        self.coalesce_key = Some(key.to_owned());
        self.push_undo();
    }

    /// Ends the current coalescing run so the next edit opens a new undo step.
    pub fn end_coalescing(&mut self) {
        self.coalesce_key = None;
    }

    fn push_undo(&mut self) {
        self.undo.push(self.snapshot());
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) {
        self.coalesce_key = None;
        if let Some(s) = self.undo.pop() {
            self.redo.push(self.snapshot());
            self.restore(s);
        }
    }

    pub fn redo(&mut self) {
        self.coalesce_key = None;
        if let Some(s) = self.redo.pop() {
            self.undo.push(self.snapshot());
            self.restore(s);
        }
    }

    // ---- pages ---------------------------------------------------------

    pub fn set_page(&mut self, index: usize) {
        if index < self.doc.pages.len() && index != self.page {
            self.page = index;
            self.selection.clear();
        }
    }

    pub fn add_page(&mut self) {
        self.checkpoint();
        let name = format!("Page {}", self.doc.pages.len() + 1);
        self.page = self.doc.add_page(name);
        self.selection.clear();
    }

    pub fn delete_page(&mut self, index: usize) {
        if self.doc.pages.len() <= 1 || index >= self.doc.pages.len() {
            return;
        }
        self.checkpoint();
        self.doc.pages.remove(index);
        self.page = self.page.min(self.doc.pages.len() - 1);
        self.selection.clear();
    }

    // ---- selection -----------------------------------------------------

    pub fn is_selected(&self, id: NodeId) -> bool {
        self.selection.contains(&id)
    }

    pub fn select_only(&mut self, id: NodeId) {
        self.selection = vec![id];
    }

    pub fn toggle_selected(&mut self, id: NodeId) {
        if let Some(i) = self.selection.iter().position(|s| *s == id) {
            self.selection.remove(i);
        } else {
            self.selection.push(id);
        }
    }

    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }

    /// Selects every sibling of the current selection, or all top-level layers.
    pub fn select_all(&mut self) {
        let parent = self
            .selection
            .first()
            .and_then(|id| self.page().locate(*id))
            .and_then(|l| l.parent);
        let ids = self
            .page()
            .children_of(parent)
            .map(|c| {
                c.iter()
                    .filter(|n| n.visible && !n.locked)
                    .map(|n| n.id)
                    .collect()
            })
            .unwrap_or_default();
        self.selection = ids;
    }

    /// Selection with any node whose ancestor is also selected removed, so
    /// moves and deletes apply once per subtree.
    pub fn selection_roots(&self) -> Vec<NodeId> {
        let page = self.page();
        self.selection
            .iter()
            .copied()
            .filter(|id| {
                page.path_to(*id).is_some_and(|path| {
                    !path[..path.len() - 1]
                        .iter()
                        .any(|a| self.selection.contains(a))
                })
            })
            .collect()
    }

    pub fn selection_bounds(&self) -> Option<Rect> {
        self.page().selection_bounds(&self.selection)
    }

    // ---- creation ------------------------------------------------------

    fn next_name(&self, base: &str) -> String {
        let mut max = 0;
        for n in &self.page().children {
            n.walk(&mut |n| {
                if let Some(num) = n
                    .name
                    .strip_prefix(base)
                    .and_then(|r| r.trim().parse::<u32>().ok())
                {
                    max = max.max(num);
                }
            });
        }
        format!("{base} {}", max + 1)
    }

    /// Creates a layer covering `world` (page space). It is placed inside the
    /// deepest frame under its top-left corner, like drawing in Figma.
    pub fn create_node(&mut self, kind: NodeKind, world: Rect) -> NodeId {
        self.checkpoint();
        let parent = self.page().frame_at(world.min(), &[]);
        let origin = self.page().content_origin(parent);
        let id = self.doc.alloc_id();
        let mut node = Node::new(id, kind, world.translate(Vec2::ZERO - origin));
        node.name = match &node.kind {
            NodeKind::Text(t) => {
                let first_line = t.content.lines().next().unwrap_or("").trim();
                if first_line.is_empty() {
                    "Text".into()
                } else {
                    first_line.chars().take(40).collect()
                }
            }
            k => self.next_name(k.label()),
        };
        let len = self.page().children_of(parent).map_or(0, |c| c.len());
        self.page_mut().insert(parent, len, node);
        self.selection = vec![id];
        id
    }

    // ---- geometry ------------------------------------------------------

    /// Moves the selection by `delta` page units. The caller is responsible
    /// for checkpointing once per gesture.
    pub fn translate_selection(&mut self, delta: Vec2) {
        for id in self.selection_roots() {
            if let Some(n) = self.page_mut().find_mut(id)
                && !n.locked
            {
                n.x += delta.x;
                n.y += delta.y;
            }
        }
        self.page_mut().fit_groups();
    }

    /// Sets `id`'s page-space bounds. Group children scale with the group.
    pub fn set_world_rect(&mut self, id: NodeId, world: Rect) {
        let origin = self.page().parent_origin(id);
        if let Some(n) = self.page_mut().find_mut(id) {
            let w = world.w.max(0.01);
            let h = world.h.max(0.01);
            if matches!(n.kind, NodeKind::Group) && n.width > 0.0 && n.height > 0.0 {
                n.scale_children(w / n.width, h / n.height);
            }
            let width_changed = (w - n.width).abs() > f64::EPSILON;
            if let Some(t) = n.text_mut()
                && width_changed
            {
                t.auto_width = false;
            }
            n.x = world.x - origin.x;
            n.y = world.y - origin.y;
            n.width = w;
            n.height = h;
        }
        self.page_mut().fit_groups();
    }

    /// Scales every selected root from `from` to `to` (both page space),
    /// as when dragging a handle of a multi-selection.
    pub fn resize_selection(&mut self, originals: &[(NodeId, Rect)], from: Rect, to: Rect) {
        let sx = if from.w.abs() > f64::EPSILON {
            to.w / from.w
        } else {
            1.0
        };
        let sy = if from.h.abs() > f64::EPSILON {
            to.h / from.h
        } else {
            1.0
        };
        for (id, r) in originals {
            let nr = Rect::new(
                to.x + (r.x - from.x) * sx,
                to.y + (r.y - from.y) * sy,
                r.w * sx,
                r.h * sy,
            );
            self.set_world_rect(*id, nr);
        }
    }

    /// After a drag, moves each dragged layer into the frame under its center
    /// (or out to the page), keeping its on-canvas position.
    pub fn drop_into_frames(&mut self, ids: &[NodeId]) {
        for id in ids {
            let Some(r) = self.page().world_rect(*id) else {
                continue;
            };
            let Some(loc) = self.page().locate(*id) else {
                continue;
            };
            // Children of groups stay with their group.
            if let Some(p) = loc.parent
                && self
                    .page()
                    .find(p)
                    .is_some_and(|n| matches!(n.kind, NodeKind::Group))
            {
                continue;
            }
            let target = self.page().frame_at(r.center(), &[*id]);
            if target != loc.parent {
                self.page_mut().reparent(*id, target);
            }
        }
        self.page_mut().fit_groups();
    }

    // ---- structure -----------------------------------------------------

    pub fn delete_selection(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        self.checkpoint();
        for id in self.selection_roots() {
            self.page_mut().remove(id);
        }
        self.selection.clear();
        self.page_mut().fit_groups();
    }

    /// Duplicates the selection in place (above each original), offset by `offset`.
    pub fn duplicate_selection(&mut self, offset: Vec2) {
        let roots = self.selection_roots();
        if roots.is_empty() {
            return;
        }
        self.checkpoint();
        let mut new_sel = vec![];
        for id in roots {
            let Some(loc) = self.page().locate(id) else {
                continue;
            };
            let Some(orig) = self.page().find(id).cloned() else {
                continue;
            };
            let mut copy = self.doc.clone_with_new_ids(&orig);
            copy.x += offset.x;
            copy.y += offset.y;
            new_sel.push(copy.id);
            self.page_mut().insert(loc.parent, loc.index + 1, copy);
        }
        self.selection = new_sel;
        self.page_mut().fit_groups();
    }

    /// Wraps the selection in a group placed where the topmost selected layer was.
    pub fn group_selection(&mut self) -> Option<NodeId> {
        let roots = self.selection_roots();
        let first = *roots.first()?;
        let parent = self.page().locate(first)?.parent;
        // Only siblings can be grouped together, like Figma's common-parent rule.
        let mut members: Vec<(usize, NodeId)> = roots
            .iter()
            .filter_map(|id| {
                let l = self.page().locate(*id)?;
                (l.parent == parent).then_some((l.index, *id))
            })
            .collect();
        members.sort();
        let insert_at = members.last()?.0 + 1 - members.len();
        self.checkpoint();
        let gid = self.doc.alloc_id();
        let mut group = Node::new(gid, NodeKind::Group, Rect::default());
        group.name = self.next_name("Group");
        for (_, id) in &members {
            if let Some((n, _)) = self.page_mut().remove(*id) {
                group.children.push(n);
            }
        }
        self.page_mut().insert(parent, insert_at, group);
        self.page_mut().fit_groups();
        self.selection = vec![gid];
        Some(gid)
    }

    /// Dissolves selected groups (and frames, like Figma's Shift+Ctrl+G),
    /// moving their children to the group's parent.
    pub fn ungroup_selection(&mut self) {
        let targets: Vec<NodeId> = self
            .selection
            .iter()
            .copied()
            .filter(|id| self.page().find(*id).is_some_and(|n| n.kind.is_container()))
            .collect();
        if targets.is_empty() {
            return;
        }
        self.checkpoint();
        let mut new_sel = vec![];
        for gid in targets {
            let Some((group, loc)) = self.page_mut().remove(gid) else {
                continue;
            };
            for (i, mut c) in group.children.into_iter().enumerate() {
                c.x += group.x;
                c.y += group.y;
                new_sel.push(c.id);
                self.page_mut().insert(loc.parent, loc.index + i, c);
            }
        }
        self.selection = new_sel;
        self.page_mut().fit_groups();
    }

    pub fn reorder_selection(&mut self, order: ZOrder) {
        let roots = self.selection_roots();
        if roots.is_empty() {
            return;
        }
        self.checkpoint();
        for id in roots {
            let Some((node, loc)) = self.page_mut().remove(id) else {
                continue;
            };
            let len = self.page().children_of(loc.parent).map_or(0, |c| c.len());
            let idx = match order {
                ZOrder::Forward => (loc.index + 1).min(len),
                ZOrder::Backward => loc.index.saturating_sub(1),
                ZOrder::Front => len,
                ZOrder::Back => 0,
            };
            self.page_mut().insert(loc.parent, idx, node);
        }
    }

    /// Moves `id` to `index` within `parent` (used by layer-panel drag and drop).
    pub fn move_layer(&mut self, id: NodeId, parent: Option<NodeId>, index: usize) {
        if let Some(p) = parent
            && self.page().find(id).is_some_and(|n| n.contains_id(p))
        {
            return;
        }
        self.checkpoint();
        let world = self.page().world_rect(id);
        let Some((mut node, loc)) = self.page_mut().remove(id) else {
            return;
        };
        let idx = if loc.parent == parent && loc.index < index {
            index - 1
        } else {
            index
        };
        if let Some(w) = world {
            let origin = self.page().content_origin(parent);
            node.x = w.x - origin.x;
            node.y = w.y - origin.y;
        }
        self.page_mut().insert(parent, idx, node);
        self.page_mut().fit_groups();
    }

    /// Applies `f` to every selected node as one undo step.
    pub fn edit_selected(&mut self, key: &str, mut f: impl FnMut(&mut Node)) {
        if self.selection.is_empty() {
            return;
        }
        self.checkpoint_coalesced(key);
        for id in self.selection.clone() {
            if let Some(n) = self.page_mut().find_mut(id) {
                f(n);
            }
        }
        self.page_mut().fit_groups();
    }

    pub fn edit_node(&mut self, id: NodeId, key: &str, f: impl FnOnce(&mut Node)) {
        if self.page().find(id).is_none() {
            return;
        }
        self.checkpoint_coalesced(key);
        if let Some(n) = self.page_mut().find_mut(id) {
            f(n);
        }
        self.page_mut().fit_groups();
    }

    /// Drops selected ids that no longer exist (after undo, page switch, ...).
    pub fn prune_selection(&mut self) {
        let page = &self.doc.pages[self.page];
        self.selection.retain(|id| page.find(*id).is_some());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;

    fn editor_with_frame() -> (Editor, NodeId) {
        let mut ed = Editor::default();
        let f = ed.create_node(
            NodeKind::Frame { clip_content: true },
            Rect::new(0.0, 0.0, 400.0, 300.0),
        );
        (ed, f)
    }

    #[test]
    fn drawing_inside_frame_nests_and_names() {
        let (mut ed, f) = editor_with_frame();
        let r = ed.create_node(NodeKind::Rectangle, Rect::new(50.0, 50.0, 10.0, 10.0));
        let r2 = ed.create_node(NodeKind::Rectangle, Rect::new(500.0, 50.0, 10.0, 10.0));
        let page = ed.page();
        assert_eq!(page.locate(r).unwrap().parent, Some(f));
        assert_eq!(page.locate(r2).unwrap().parent, None);
        assert_eq!(page.find(f).unwrap().name, "Frame 1");
        assert_eq!(page.find(r).unwrap().name, "Rectangle 1");
        assert_eq!(page.find(r2).unwrap().name, "Rectangle 2");
    }

    #[test]
    fn undo_redo_roundtrip() {
        let (mut ed, f) = editor_with_frame();
        ed.select_only(f);
        ed.delete_selection();
        assert!(ed.page().find(f).is_none());
        ed.undo();
        assert!(ed.page().find(f).is_some());
        assert_eq!(ed.selection, vec![f]);
        ed.redo();
        assert!(ed.page().find(f).is_none());
    }

    #[test]
    fn coalesced_edits_are_one_undo_step() {
        let (mut ed, f) = editor_with_frame();
        ed.select_only(f);
        for i in 0..5 {
            ed.edit_selected("x", |n| n.x = i as f64);
        }
        ed.end_coalescing();
        assert_eq!(ed.page().find(f).unwrap().x, 4.0);
        ed.undo();
        assert_eq!(ed.page().find(f).unwrap().x, 0.0);
    }

    #[test]
    fn drag_out_of_frame_reparents() {
        let (mut ed, _f) = editor_with_frame();
        let r = ed.create_node(NodeKind::Rectangle, Rect::new(10.0, 10.0, 20.0, 20.0));
        ed.translate_selection(Vec2::new(600.0, 0.0));
        ed.drop_into_frames(&[r]);
        assert_eq!(ed.page().locate(r).unwrap().parent, None);
        assert_eq!(ed.page().world_rect(r).unwrap().x, 610.0);
    }

    #[test]
    fn group_and_ungroup() {
        let mut ed = Editor::default();
        let a = ed.create_node(NodeKind::Rectangle, Rect::new(0.0, 0.0, 10.0, 10.0));
        let b = ed.create_node(NodeKind::Ellipse, Rect::new(20.0, 20.0, 10.0, 10.0));
        ed.selection = vec![a, b];
        let g = ed.group_selection().unwrap();
        assert_eq!(ed.page().children.len(), 1);
        assert_eq!(
            ed.page().find(g).unwrap().rect(),
            Rect::new(0.0, 0.0, 30.0, 30.0)
        );
        ed.ungroup_selection();
        assert_eq!(ed.page().children.len(), 2);
        assert_eq!(
            ed.page().world_rect(b).unwrap(),
            Rect::new(20.0, 20.0, 10.0, 10.0)
        );
    }

    #[test]
    fn duplicate_places_copy_above() {
        let mut ed = Editor::default();
        let a = ed.create_node(NodeKind::Rectangle, Rect::new(0.0, 0.0, 10.0, 10.0));
        ed.edit_selected("fill", |n| {
            n.fills = vec![crate::Paint::solid(Color::BLACK)]
        });
        ed.duplicate_selection(Vec2::new(10.0, 0.0));
        let page = ed.page();
        assert_eq!(page.children.len(), 2);
        assert_eq!(page.children[0].id, a);
        assert_eq!(page.children[1].x, 10.0);
        assert_eq!(page.children[1].fills, page.children[0].fills);
    }

    #[test]
    fn reorder_layers() {
        let mut ed = Editor::default();
        let a = ed.create_node(NodeKind::Rectangle, Rect::new(0.0, 0.0, 10.0, 10.0));
        let b = ed.create_node(NodeKind::Rectangle, Rect::new(0.0, 0.0, 10.0, 10.0));
        ed.select_only(a);
        ed.reorder_selection(ZOrder::Front);
        assert_eq!(
            ed.page().children.iter().map(|n| n.id).collect::<Vec<_>>(),
            vec![b, a]
        );
        ed.reorder_selection(ZOrder::Back);
        assert_eq!(ed.page().children[0].id, a);
    }

    #[test]
    fn resizing_group_scales_children() {
        let mut ed = Editor::default();
        let a = ed.create_node(NodeKind::Rectangle, Rect::new(0.0, 0.0, 10.0, 10.0));
        let b = ed.create_node(NodeKind::Rectangle, Rect::new(10.0, 10.0, 10.0, 10.0));
        ed.selection = vec![a, b];
        let g = ed.group_selection().unwrap();
        ed.set_world_rect(g, Rect::new(0.0, 0.0, 40.0, 40.0));
        assert_eq!(
            ed.page().world_rect(b).unwrap(),
            Rect::new(20.0, 20.0, 20.0, 20.0)
        );
    }
}
