//! Variables: the "Local variables" modal, the inspector binding picker and the
//! App operations behind them. Every change goes through `Command::SetVariables`
//! (or ordinary node updates for bindings) so it is one undo step.

pub mod modal;
pub mod widgets;

use crate::app::App;
use crate::edit::Tx;
use crate::layout;
use desigma_doc::*;
use egui::Rect;
use std::collections::HashSet;
use std::path::Path;

/// Which popup menu of the modal is open.
#[derive(Clone, Debug, PartialEq)]
pub enum VarMenuKind {
    Add,
    Row(String),
    Mode(String, String),
    Collection(String),
    Folder(String, String),
}

#[derive(Clone, Debug)]
pub struct VarMenu {
    pub kind: VarMenuKind,
    pub pos: egui::Pos2,
    pub anchor: Rect,
}

/// Colour picker editing one value cell; edits are live and committed on close.
pub struct CellPicker {
    pub var: String,
    pub mode: String,
    pub color: egui::Color32,
    pub anchor: Rect,
    pub before: VariableStore,
}

/// Alias picker for one value cell.
pub struct AliasPicker {
    pub var: String,
    pub mode: String,
    pub anchor: Rect,
    pub query: String,
}

/// Binding picker opened from the inspector.
pub struct BindPicker {
    pub keys: Vec<String>,
    pub kind: VariableKind,
    pub nodes: Vec<NodeId>,
    pub anchor: Rect,
    pub query: String,
}

pub struct RowDrag {
    pub var: String,
}

#[derive(Default)]
pub struct VarsState {
    pub open: bool,
    /// Selected collection id.
    pub collection: Option<String>,
    pub scroll: f32,
    pub scroll_x: f32,
    pub search: String,
    pub folded: HashSet<String>,
    pub menu: Option<VarMenu>,
    pub cell_picker: Option<CellPicker>,
    pub alias_picker: Option<AliasPicker>,
    pub bind_picker: Option<BindPicker>,
    pub drag: Option<RowDrag>,
    /// Variable whose name field should take focus on the next frame.
    pub focus_name: Option<String>,
    /// Collection or folder being renamed inline: ("c", id) / ("f", "coll\0path").
    pub renaming: Option<(char, String)>,
    pub rename_frames: u32,
}

impl App {
    /// The document with variable bindings applied, for exporters that read raw
    /// node properties (borrowed when nothing is bound).
    pub fn export_doc(&self) -> std::borrow::Cow<'_, Document> {
        if self.doc.nodes.values().any(|n| !n.variables.bindings.is_empty()) {
            std::borrow::Cow::Owned(apply_bound_variables(&self.doc))
        } else {
            std::borrow::Cow::Borrowed(&self.doc)
        }
    }

    pub fn open_variables(&mut self) {
        self.vars.open = true;
        let known = self.vars.collection.as_ref().is_some_and(|c| self.doc.variable_collections.contains_key(c));
        if !known {
            self.vars.collection = self.doc.variable_collections.keys().next().cloned();
        }
        self.ui.menu = None;
    }

    pub fn close_variables(&mut self) {
        self.vars.open = false;
        self.vars.menu = None;
        self.vars.alias_picker = None;
        self.vars.renaming = None;
        self.vars.drag = None;
        if let Some(p) = self.vars.cell_picker.take() {
            self.finish_cell_picker(p);
        }
    }

    /// Installs an edited variable store as one undo step. `removed_*` list
    /// variables / collections that disappeared so their bindings are dropped
    /// from nodes in the same step.
    pub fn commit_store(&mut self, store: VariableStore, removed_vars: &[String], removed_colls: &[String]) -> bool {
        if store.matches_doc(&self.doc) {
            return false;
        }
        let cleanup = dangling_binding_cleanup(&self.doc, removed_vars, removed_colls);
        let mut cmds = vec![store.command()];
        for mut n in cleanup {
            let keys: Vec<String> = self.doc.get(n.id).map(|o| o.variables.bindings.iter().filter(|(_, v)| removed_vars.contains(v)).map(|(k, _)| k.clone()).collect()).unwrap_or_default();
            bake_bindings(&self.doc, &mut n, &keys);
            cmds.push(Command::Update { node: Box::new(n) });
        }
        let cmd = if cmds.len() == 1 { cmds.pop().unwrap() } else { Command::Batch(cmds) };
        match self.hist.exec(&mut self.doc, cmd) {
            Ok(()) => {
                self.changed();
                self.sanitize();
                true
            }
            Err(e) => {
                self.show_toast(format!("Variable edit failed: {e}"), true);
                false
            }
        }
    }

    /// Ends a colour-picker session: the live edits become one undo step.
    pub fn finish_cell_picker(&mut self, p: CellPicker) {
        let after = VariableStore::from_doc(&self.doc);
        self.doc.variables = p.before.variables.clone();
        self.doc.variable_collections = p.before.collections.clone();
        self.commit_store(after, &[], &[]);
        self.changed();
    }

    /// Binds `keys` of every node in `nodes` to variable `var` (one undo step).
    /// The raw property takes the variable's current value so detaching keeps the look.
    pub fn bind_variable(&mut self, nodes: &[NodeId], keys: &[&str], var: &str) {
        let mut pre = Vec::new();
        for id in nodes {
            for k in keys {
                if let Some(v) = self.doc.resolve_variable(*id, var) {
                    pre.push((*id, k.to_string(), v));
                }
            }
        }
        let mut tx = Tx::new();
        for (id, key, val) in &pre {
            if let Some(n) = tx.ed.node(&mut self.doc, *id) {
                n.variables.bindings.insert(key.clone(), var.to_string());
                set_bound_property(n, key, val);
            }
            layout::relayout_around(&mut self.doc, &mut tx.ed, *id);
        }
        self.run_tx(tx);
    }

    /// Removes the bindings of `keys`, keeping the values they currently resolve to.
    pub fn detach_variable(&mut self, nodes: &[NodeId], keys: &[&str]) {
        let mut pre = Vec::new();
        for id in nodes {
            for k in keys {
                if let Some(v) = self.doc.bound_value(*id, k) {
                    pre.push((*id, k.to_string(), v));
                }
            }
        }
        let mut tx = Tx::new();
        for id in nodes {
            if let Some(n) = tx.ed.node(&mut self.doc, *id) {
                for k in keys {
                    n.variables.bindings.remove(*k);
                }
            }
        }
        for (id, key, val) in &pre {
            if let Some(n) = tx.ed.node(&mut self.doc, *id) {
                set_bound_property(n, key, val);
            }
        }
        for id in nodes {
            layout::relayout_around(&mut self.doc, &mut tx.ed, *id);
        }
        self.run_tx(tx);
    }

    /// Sets (or clears, with `None`) the explicit mode of `collection` on nodes.
    pub fn set_node_mode(&mut self, nodes: &[NodeId], collection: &str, mode: Option<&str>) {
        let c = collection.to_string();
        let m = mode.map(String::from);
        self.edit_nodes(nodes, |n| match &m {
            Some(m) => {
                n.variables.modes.insert(c.clone(), m.clone());
            }
            None => {
                n.variables.modes.remove(&c);
            }
        });
    }

    /// Merges a `variables/local` JSON payload into the document (one undo step).
    /// Returns how many variables the payload held.
    pub fn import_variables_text(&mut self, text: &str) -> Result<usize, String> {
        let mut tmp = Document::new("import");
        let n = desigma_fig::import_variables_json(&mut tmp, text).map_err(|e| e.to_string())?;
        let mut store = VariableStore::from_doc(&self.doc);
        store.collections.extend(tmp.variable_collections);
        store.variables.extend(tmp.variables);
        self.commit_store(store, &[], &[]);
        if self.vars.collection.is_none() {
            self.vars.collection = self.doc.variable_collections.keys().next().cloned();
        }
        Ok(n)
    }

    pub fn import_variables_file(&mut self, path: &Path) {
        match std::fs::read_to_string(path).map_err(|e| e.to_string()).and_then(|t| self.import_variables_text(&t)) {
            Ok(n) => self.show_toast(format!("Imported {n} variables"), false),
            Err(e) => self.show_toast(format!("Cannot import variables: {e}"), true),
        }
    }

    pub fn export_variables_file(&mut self, path: &Path) {
        let json = desigma_fig::export_variables_json(&self.doc);
        let text = serde_json::to_string_pretty(&json).unwrap_or_default();
        match std::fs::write(path, text) {
            Ok(()) => self.show_toast(format!("Exported {} variables", self.doc.variables.len()), false),
            Err(e) => self.show_toast(format!("Cannot write {}: {e}", path.display()), true),
        }
    }
}

/// True when `text` is a variables-only payload (no design document in it).
pub fn is_variables_payload(text: &str) -> bool {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else { return false };
    if v.get("document").is_some() || v.get("nodes").is_some() || v.get("pages").is_some() {
        return false;
    }
    let mut tmp = Document::new("probe");
    desigma_fig::import_variables_json(&mut tmp, text).is_ok_and(|_| !tmp.variable_collections.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_with_vars() -> App {
        let mut d = Document::new("t");
        let page = d.add_page("P");
        let rid = d.alloc_id();
        d.insert(page, Node::rectangle(rid, desigma_doc::Rect::new(0.0, 0.0, 10.0, 10.0))).unwrap();
        let mut app = App::new(d);
        let mut s = VariableStore::default();
        let c = s.add_collection("Theme");
        let a = s.add_variable(&c, "Brand", VariableKind::Color).unwrap();
        let mode = s.collections[&c].default_mode.clone();
        s.set_value(&a, &mode, VariableValue::Color(Color::rgb(255, 0, 0)));
        let g = s.add_variable(&c, "Gap", VariableKind::Number).unwrap();
        s.set_value(&g, &mode, VariableValue::Number(12.0));
        assert!(app.commit_store(s, &[], &[]));
        app
    }

    #[test]
    fn bind_detach_and_delete_are_single_undo_steps() {
        let mut app = app_with_vars();
        let rect = *app.doc.nodes.keys().last().unwrap();
        let brand = app.doc.variables.values().find(|v| v.name == "Brand").unwrap().id.clone();
        app.bind_variable(&[rect], &["fills/0"], &brand);
        let n = app.doc.get(rect).unwrap();
        assert_eq!(n.variables.bindings["fills/0"], brand);
        assert_eq!(n.fills[0].kind, PaintKind::Solid(Color::rgb(255, 0, 0)));
        app.undo();
        assert!(app.doc.get(rect).unwrap().variables.is_empty());
        app.redo();
        app.detach_variable(&[rect], &["fills/0"]);
        assert!(app.doc.get(rect).unwrap().variables.bindings.is_empty());
        assert_eq!(app.doc.get(rect).unwrap().fills[0].kind, PaintKind::Solid(Color::rgb(255, 0, 0)));

        // deleting a bound variable in the store drops the binding in the same step
        app.bind_variable(&[rect], &["fills/0"], &brand);
        let mut store = VariableStore::from_doc(&app.doc);
        store.delete_variable(&brand);
        assert!(app.commit_store(store, &[brand.clone()], &[]));
        assert!(app.doc.get(rect).unwrap().variables.bindings.is_empty());
        app.undo();
        assert_eq!(app.doc.get(rect).unwrap().variables.bindings["fills/0"], brand);
        assert!(app.doc.variables.contains_key(&brand));
    }

    #[test]
    fn node_modes_and_import_merge() {
        let mut app = app_with_vars();
        let rect = *app.doc.nodes.keys().last().unwrap();
        let coll = app.doc.variable_collections.keys().next().unwrap().clone();
        app.set_node_mode(&[rect], &coll, Some("m"));
        assert_eq!(app.doc.get(rect).unwrap().variables.modes[&coll], "m");
        app.set_node_mode(&[rect], &coll, None);
        assert!(app.doc.get(rect).unwrap().variables.is_empty());

        let before = app.doc.variables.len();
        let json = desigma_fig::export_variables_json(&app.doc).to_string();
        assert!(is_variables_payload(&json));
        assert!(!is_variables_payload("{\"document\": {}}"));
        assert!(!is_variables_payload("nope"));
        let mut other = App::new(Document::new("o"));
        assert_eq!(other.import_variables_text(&json).unwrap(), before);
        assert_eq!(other.doc.variables, app.doc.variables);
        other.undo();
        assert!(other.doc.variables.is_empty());
        assert!(other.import_variables_text("{}").is_err());
    }
}
