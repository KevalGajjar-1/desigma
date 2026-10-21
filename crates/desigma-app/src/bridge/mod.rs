//! Agent bridge: desigma-mcp drives the live document through the tool engine.
pub mod server;
pub use server::{Bridge, DISCOVERY_FILE};

use crate::app::App;
use desigma_doc::{Command, Document, NodeId};
use desigma_mcp::engine::{self, Host};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Default)]
pub struct BridgeState {
    pub bridge: Option<Bridge>,
}

/// The editor as a tool host: commands go through `Tx` so component sync and rerouting apply.
struct AppHost<'a>(&'a mut App);
impl Host for AppHost<'_> {
    fn doc(&self) -> &Document {
        &self.0.doc
    }
    fn exec(&mut self, cmd: Command) -> Result<(), String> {
        let mut tx = crate::edit::Tx::new();
        tx.cmd(&mut self.0.doc, cmd).map_err(|e| e.to_string())?;
        if self.0.run_tx(tx) {
            Ok(())
        } else {
            Err("Edit failed".into())
        }
    }
    fn undo(&mut self) -> Result<bool, String> {
        Ok(self.0.undo())
    }
    fn redo(&mut self) -> Result<bool, String> {
        Ok(self.0.redo())
    }
    fn can_undo(&self) -> bool {
        self.0.hist.can_undo()
    }
    fn can_redo(&self) -> bool {
        self.0.hist.can_redo()
    }
    fn alloc_id(&mut self) -> Result<NodeId, String> {
        self.0.doc.try_alloc_id().ok_or_else(|| "Document has exhausted node IDs".into())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectArgs {
    node_ids: Vec<u64>,
    #[serde(default = "yes")]
    reveal: bool,
}
fn yes() -> bool {
    true
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenArgs {
    path: PathBuf,
    #[serde(default)]
    discard_changes: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveArgs {
    path: PathBuf,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewArgs {
    name: String,
    #[serde(default)]
    discard_changes: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportArgs {
    node_id: u64,
    #[serde(default = "one")]
    scale: f32,
    path: PathBuf,
}
fn one() -> f32 {
    1.0
}
fn parse<T: serde::de::DeserializeOwned>(args: Value) -> Result<T, String> {
    serde_json::from_value(args).map_err(|e| format!("invalid arguments: {e}"))
}

const MUTATING: &[&str] = &[
    "desigma_create_node",
    "desigma_update_node",
    "desigma_delete_node",
    "desigma_undo",
    "desigma_redo",
    "desigma_select_nodes",
    "desigma_open_file",
    "desigma_save_file",
    "desigma_new_document",
];

impl App {
    // Called from the Preferences toggle and `--bridge` launch, wired in the next commit.
    #[allow(dead_code)]
    pub fn start_bridge(&mut self, ctx: &egui::Context) -> Result<(), String> {
        if self.bridge.bridge.is_some() {
            return Ok(());
        }
        let root = self.lib.as_ref().map(|l| l.root().to_path_buf());
        if let Some(root) = &root {
            server::remove_stale(root);
            if root.join(DISCOVERY_FILE).exists() {
                return Err("Agent bridge is already on in another window".into());
            }
        }
        let bridge = Bridge::start(root.as_deref(), &self.doc.name, self.file_path.as_deref(), ctx.clone())?;
        self.bridge.bridge = Some(bridge);
        self.show_toast("Agent bridge on — MCP clients can now edit this document", false);
        Ok(())
    }

    // Called from the Preferences toggle and the status pill, wired in the next commit.
    #[allow(dead_code)]
    pub fn stop_bridge(&mut self) {
        self.bridge.bridge = None;
    }

    pub fn poll_bridge(&mut self) {
        // One operation per frame keeps a chatty agent from monopolizing the editor.
        let Some(request) = self.bridge.bridge.as_ref().and_then(Bridge::try_recv) else { return };
        if request.cancelled.load(std::sync::atomic::Ordering::Acquire) {
            return;
        }
        let result = self.bridge_call(&request.name, request.args);
        let _ = request.reply.send(result);
    }

    pub(crate) fn bridge_call(&mut self, name: &str, args: Value) -> Result<Value, String> {
        if self.home.is_some() || self.history.viewing.is_some() {
            return Err("Editor is busy: open an editable document first".into());
        }
        if MUTATING.contains(&name) {
            if self.proto.present.is_some()
                || self.scrub.is_some()
                || self.dialog.is_some()
                || !matches!(self.canvas.inter, crate::canvas::Interaction::None)
                || self.canvas.pen.is_some()
                || self.canvas.builder.is_some()
                || self.canvas.vedit.is_some()
            {
                return Err("Editor is busy: finish the current edit or close the dialog".into());
            }
            crate::canvas::interact::finish_text(self, true);
        }
        match name {
            "desigma_get_document" => {
                let mut v = engine::call(&mut AppHost(self), name, args)?;
                v["session"] = json!("editor");
                v["path"] = json!(self.file_path);
                v["dirty"] = json!(self.hist.can_undo());
                Ok(v)
            }
            "desigma_get_selection" => {
                let _: serde_json::Map<String, Value> = parse(args)?;
                Ok(json!({"nodes": self.selection.iter().filter_map(|id| self.doc.get(*id)).map(engine::summary).collect::<Vec<_>>()}))
            }
            "desigma_select_nodes" => {
                let a: SelectArgs = parse(args)?;
                let ids: Vec<NodeId> = a.node_ids.iter().map(|&i| NodeId(i)).collect();
                if let Some(missing) = ids.iter().find(|id| self.doc.get(**id).is_none_or(|n| n.parent.is_none())) {
                    return Err(format!("node {} not found or is a page", missing.0));
                }
                self.selection = ids;
                if a.reveal {
                    self.fit_selection();
                }
                self.changed();
                Ok(json!({"selected": self.selection.len()}))
            }
            "desigma_export_image" => {
                let a: ExportArgs = parse(args)?;
                if a.path.extension().and_then(|e| e.to_str()) != Some("png") || !a.path.is_absolute() {
                    return Err("Export path must be an absolute .png path".into());
                }
                let (png, width, height) = engine::export_image(&AppHost(self), a.node_id, a.scale)?;
                std::fs::write(&a.path, png).map_err(|e| format!("Cannot write PNG: {e}"))?;
                Ok(json!({"path":a.path,"width":width,"height":height}))
            }
            "desigma_open_file" => {
                let a: OpenArgs = parse(args)?;
                if self.hist.can_undo() && !a.discard_changes {
                    return Err("Unsaved changes in the editor: save them, or set discard_changes=true".into());
                }
                if !a.path.is_absolute() {
                    return Err("Open path must be absolute".into());
                }
                let doc = crate::io::load_path(&a.path)?;
                self.replace_document(doc, Some(a.path));
                engine::call(&mut AppHost(self), "desigma_get_document", json!({})).map(|mut v| {
                    v["session"] = json!("editor");
                    v
                })
            }
            "desigma_save_file" => {
                let a: SaveArgs = parse(args)?;
                if a.path.extension().and_then(|e| e.to_str()) != Some(crate::io::NATIVE_EXT) || !a.path.is_absolute() {
                    return Err("Save path must be an absolute .desigma path".into());
                }
                let bytes = self.doc.to_json().map_err(|e| e.to_string())?;
                let mut f = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&a.path)
                    .map_err(|e| format!("Cannot create file (choose an unused .desigma filename): {e}"))?;
                std::io::Write::write_all(&mut f, bytes.as_bytes()).map_err(|e| format!("Cannot save design: {e}"))?;
                self.show_toast(format!("Saved copy {}", a.path.display()), false);
                Ok(json!({"path":a.path,"saved":true}))
            }
            "desigma_new_document" => {
                let a: NewArgs = parse(args)?;
                if self.hist.can_undo() && !a.discard_changes {
                    return Err("Unsaved changes in the editor: save them, or set discard_changes=true".into());
                }
                let mut d = Document::new(&a.name);
                d.add_page("Page 1");
                self.replace_document(d, None);
                engine::call(&mut AppHost(self), "desigma_get_document", json!({})).map(|mut v| {
                    v["session"] = json!("editor");
                    v
                })
            }
            _ => engine::call(&mut AppHost(self), name, args),
        }
    }
}
