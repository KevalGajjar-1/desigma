//! Typed, local document operations shared by the MCP transport and future bridges.
use desigma_doc::{Command, Document, History, Node, NodeId, NodeKind, Rect, TextStyle};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};

pub struct Engine {
    pub doc: Document,
    pub history: History,
}

impl Default for Engine {
    fn default() -> Self {
        let mut doc = Document::new("Untitled");
        doc.add_page("Page 1");
        Self { doc, history: History::default() }
    }
}

/// What a tool call needs from its document owner: the MCP file session or the editor.
pub trait Host {
    fn doc(&self) -> &Document;
    fn exec(&mut self, cmd: Command) -> Result<(), String>;
    fn undo(&mut self) -> Result<bool, String>;
    fn redo(&mut self) -> Result<bool, String>;
    fn can_undo(&self) -> bool;
    fn can_redo(&self) -> bool;
    /// Allocates a fresh node id on the document.
    fn alloc_id(&mut self) -> Result<NodeId, String>;
}

impl Host for Engine {
    fn doc(&self) -> &Document {
        &self.doc
    }
    fn exec(&mut self, cmd: Command) -> Result<(), String> {
        self.history.exec(&mut self.doc, cmd).map_err(|e| e.to_string())
    }
    fn undo(&mut self) -> Result<bool, String> {
        self.history.undo(&mut self.doc).map_err(|e| e.to_string())
    }
    fn redo(&mut self) -> Result<bool, String> {
        self.history.redo(&mut self.doc).map_err(|e| e.to_string())
    }
    fn can_undo(&self) -> bool {
        self.history.can_undo()
    }
    fn can_redo(&self) -> bool {
        self.history.can_redo()
    }
    fn alloc_id(&mut self) -> Result<NodeId, String> {
        self.doc.try_alloc_id().ok_or_else(|| "Document has exhausted node IDs".into())
    }
}

impl Engine {
    pub fn call(&mut self, name: &str, args: Value) -> Result<Value, String> {
        call(self, name, args)
    }
}

pub fn call<H: Host>(host: &mut H, name: &str, args: Value) -> Result<Value, String> {
    match name {
        "desigma_get_document" => {
            let _: Empty = parse(args)?;
            Ok(json!({"name":host.doc().name,"kind":host.doc().kind,
                "pages":host.doc().pages.iter().filter_map(|id|host.doc().get(*id)).map(summary).collect::<Vec<_>>(),
                "node_count":host.doc().nodes.len(),"variable_count":host.doc().variables.len(),
                "can_undo":host.can_undo(),"can_redo":host.can_redo()}))
        }
        "desigma_list_nodes" => {
            let a: ListArgs = parse(args)?;
            if a.limit == 0 || a.limit > 500 {
                return Err("limit must be between 1 and 500".into());
            }
            if let Some(id) = a.parent_id {
                node(host, id)?;
            }
            if let Some(q) = &a.query {
                bounded_text("query", q, 1024)?;
            }
            let query = a.query.unwrap_or_default().to_lowercase();
            let matching: Vec<_> = host
                .doc()
                .nodes
                .values()
                .filter(|n| a.parent_id.is_none_or(|id| n.parent == Some(NodeId(id))) && n.name.to_lowercase().contains(&query))
                .collect();
            let nodes: Vec<_> = matching.iter().skip(a.offset).take(a.limit).map(|n| summary(n)).collect();
            let next = a.offset.saturating_add(nodes.len());
            Ok(json!({"nodes":nodes,"total":matching.len(),"offset":a.offset,
                "next_offset":(next < matching.len()).then_some(next)}))
        }
        "desigma_get_node" => {
            let a: NodeArgs = parse(args)?;
            Ok(json!({"node":node(host, a.node_id)?}))
        }
        "desigma_get_variables" => {
            let a: VariablesArgs = parse(args)?;
            let effective = if let Some(id) = a.node_id {
                let n = node(host, id)?;
                Some(
                    n.variables
                        .bindings
                        .iter()
                        .map(|(property, variable)| {
                            json!({
                                "property":property,"variable_id":variable,
                                "mode_id":host.doc().variables.get(variable).and_then(|v|host.doc().variable_mode(n.id,&v.collection)),
                                "value":host.doc().resolve_variable(n.id,variable)
                            })
                        })
                        .collect::<Vec<_>>(),
                )
            } else {
                None
            };
            Ok(json!({"variables":host.doc().variables,"collections":host.doc().variable_collections,"effective_bindings":effective}))
        }
        "desigma_generate_code" => {
            let a: CodeArgs = parse(args)?;
            let id = node(host, a.node_id)?.id;
            let doc = desigma_doc::apply_bound_variables(host.doc());
            let code = match a.language.as_str() {
                "css" => desigma_export::css_for(&doc, id),
                "tailwind" => desigma_export::tailwind_for(&doc, id),
                "react" => desigma_export::react_for(&doc, id),
                "swiftui" => desigma_export::swiftui_for(&doc, id),
                "compose" => desigma_export::compose_for(&doc, id),
                "svg" => desigma_export::export_svg(&doc, id, &Default::default()),
                _ => return Err("language must be css, tailwind, react, swiftui, compose, or svg".into()),
            };
            Ok(json!({"node_id":id,"language":a.language,"code":code}))
        }
        "desigma_create_node" => create(host, parse(args)?),
        "desigma_update_node" => update(host, parse(args)?),
        "desigma_delete_node" => {
            let a: NodeArgs = parse(args)?;
            let n = node(host, a.node_id)?;
            if n.parent.is_none() {
                return Err("page roots cannot be deleted by this tool".into());
            }
            let id = n.id;
            host.exec(Command::Remove { id })?;
            Ok(json!({"node_id":id,"deleted":true}))
        }
        "desigma_undo" | "desigma_redo" => {
            let _: Empty = parse(args)?;
            let changed = if name == "desigma_undo" { host.undo() } else { host.redo() }?;
            Ok(json!({"changed":changed,"can_undo":host.can_undo(),"can_redo":host.can_redo()}))
        }
        _ => Err(format!("unknown tool: {name}")),
    }
}

fn node<H: Host>(host: &H, id: u64) -> Result<&Node, String> {
    if id == 0 {
        return Err("node IDs must be positive integers".into());
    }
    host.doc().get(NodeId(id)).ok_or_else(|| format!("node {id} not found"))
}

fn create<H: Host>(host: &mut H, a: CreateArgs) -> Result<Value, String> {
    let parent = node(host, a.parent_id)?;
    if !parent.kind.is_container() {
        return Err("parent_id must identify a container".into());
    }
    let index = parent.children.len();
    geometry(a.x, a.y, a.width, a.height)?;
    if let Some(name) = &a.name {
        bounded_text("name", name, 1024)?;
    }
    if let Some(text) = &a.text {
        bounded_text("text", text, 100_000)?;
    }
    if a.kind != "text" && a.text.is_some() {
        return Err("text can only be set on text nodes".into());
    }
    let rect = Rect::new(a.x, a.y, a.width, a.height);
    // Allocate only after every argument has passed validation.
    let constructor: fn(NodeId, Rect) -> Node = match a.kind.as_str() {
        "frame" => Node::frame,
        "rectangle" => Node::rectangle,
        "ellipse" => Node::ellipse,
        "text" => |id, r| Node::text(id, r, "", TextStyle::default()),
        _ => return Err("kind must be frame, rectangle, ellipse, or text".into()),
    };
    // Imported native files may carry a stale allocation counter. Derive a
    // fresh ID from the actual nodes instead of risking an overwrite.
    let id = host.alloc_id()?;
    let mut node = constructor(id, rect);
    if let Some(name) = a.name {
        node.name = name;
    }
    if let (Some(text), NodeKind::Text(props)) = (a.text, &mut node.kind) {
        props.content = text;
    }
    host.exec(Command::Insert { parent: NodeId(a.parent_id), index, nodes: vec![node] })?;
    Ok(json!({"node":host.doc().get(id),"changed":true}))
}

fn update<H: Host>(host: &mut H, a: UpdateArgs) -> Result<Value, String> {
    let mut node = node(host, a.node_id)?.clone();
    if node.parent.is_none() {
        return Err("page roots cannot be updated by this tool".into());
    }
    if let Some(name) = a.name {
        bounded_text("name", &name, 1024)?;
        node.name = name;
    }
    if let Some(x) = a.x {
        node.transform.tx = x;
    }
    if let Some(y) = a.y {
        node.transform.ty = y;
    }
    if let Some(width) = a.width {
        node.size.x = width;
    }
    if let Some(height) = a.height {
        node.size.y = height;
    }
    geometry(node.transform.tx, node.transform.ty, node.size.x, node.size.y)?;
    if let Some(visible) = a.visible {
        node.visible = visible;
    }
    if let Some(opacity) = a.opacity {
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
            return Err("opacity must be between 0 and 1".into());
        }
        node.opacity = opacity;
    }
    if let Some(text) = a.text {
        bounded_text("text", &text, 100_000)?;
        let NodeKind::Text(props) = &mut node.kind else {
            return Err("text can only be set on text nodes".into());
        };
        if props.content != text {
            props.content = text;
            props.runs.clear();
            props.lists.clear();
        }
    }
    let changed = host.doc().get(node.id) != Some(&node);
    if changed {
        host.exec(Command::Update { node: Box::new(node) })?;
    }
    Ok(json!({"node":host.doc().get(NodeId(a.node_id)),"changed":changed}))
}

fn summary(n: &Node) -> Value {
    json!({"node_id":n.id,"name":n.name,"kind":n.kind.default_name(),"parent_id":n.parent,
        "child_count":n.children.len(),"visible":n.visible,"x":n.transform.tx,"y":n.transform.ty,
        "width":n.size.x,"height":n.size.y})
}

fn parse<T: DeserializeOwned>(args: Value) -> Result<T, String> {
    serde_json::from_value(args).map_err(|e| format!("invalid arguments: {e}"))
}
fn bounded_text(field: &str, value: &str, max: usize) -> Result<(), String> {
    if value.len() > max {
        Err(format!("{field} exceeds {max} UTF-8 bytes"))
    } else {
        Ok(())
    }
}
fn geometry(x: f32, y: f32, width: f32, height: f32) -> Result<(), String> {
    if [x, y, width, height].iter().any(|v| !v.is_finite() || v.abs() > 1_000_000.0) {
        return Err("geometry must be finite and within +/-1000000".into());
    }
    if width < 0.0 || height < 0.0 {
        return Err("width and height must be non-negative".into());
    }
    Ok(())
}
fn default_limit() -> usize {
    100
}
fn default_size() -> f32 {
    100.0
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeArgs {
    node_id: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListArgs {
    parent_id: Option<u64>,
    query: Option<String>,
    #[serde(default)]
    offset: usize,
    #[serde(default = "default_limit")]
    limit: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VariablesArgs {
    node_id: Option<u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CodeArgs {
    node_id: u64,
    language: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateArgs {
    parent_id: u64,
    kind: String,
    name: Option<String>,
    text: Option<String>,
    #[serde(default)]
    x: f32,
    #[serde(default)]
    y: f32,
    #[serde(default = "default_size")]
    width: f32,
    #[serde(default = "default_size")]
    height: f32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateArgs {
    node_id: u64,
    name: Option<String>,
    text: Option<String>,
    x: Option<f32>,
    y: Option<f32>,
    width: Option<f32>,
    height: Option<f32>,
    visible: Option<bool>,
    opacity: Option<f32>,
}

/// JSON tool descriptors kept beside the argument validators.
pub fn tools() -> Vec<Value> {
    let id = json!({"type":"integer","minimum":1,"description":"Desigma numeric node ID; obtain from list_nodes/get_document."});
    let coordinate = json!({"type":"number","minimum":-1000000,"maximum":1000000});
    let size = json!({"type":"number","minimum":0,"maximum":1000000});
    let text = json!({"type":"string","maxLength":100000,"description":"Plain text; limited to 100000 UTF-8 bytes."});
    let name = json!({"type":"string","maxLength":1024,"description":"Layer name; limited to 1024 UTF-8 bytes."});
    let mut out = Vec::new();
    let mut add = |tool: &str, description: &str, properties: Value, required: Vec<&str>, read_only: bool, idempotent: bool| {
        out.push(json!({"name":format!("desigma_{tool}"),"description":description,
            "inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},
            "annotations":{"readOnlyHint":read_only,"destructiveHint":!read_only,"idempotentHint":idempotent,"openWorldHint":false}}));
    };
    add("get_document", "Summarize the current in-memory document, pages, counts, and undo state.", json!({}), vec![], true, true);
    add(
        "list_nodes",
        "List node summaries in numeric ID order. Query matches names case-insensitively; parent_id filters direct children. Use next_offset to paginate.",
        json!({"parent_id":id,"query":{"type":"string","maxLength":1024},"offset":{"type":"integer","minimum":0,"default":0},"limit":{"type":"integer","minimum":1,"maximum":500,"default":100}}),
        vec![],
        true,
        true,
    );
    add("get_node", "Read the full native node including geometry, paints, text, bindings, and child IDs.", json!({"node_id":id}), vec!["node_id"], true, true);
    add(
        "get_variables",
        "Read variable definitions and collections. With node_id, also resolve that node's direct bindings using inherited collection modes.",
        json!({"node_id":id}),
        vec![],
        true,
        true,
    );
    add(
        "generate_code",
        "Generate developer handoff code or SVG for a layer using resolved variable values. Returns source text; writes no files.",
        json!({"node_id":id,"language":{"type":"string","enum":["css","tailwind","react","swiftui","compose","svg"]}}),
        vec!["node_id", "language"],
        true,
        true,
    );
    add("create_node","Create one layer in a container; defaults to x/y=0, width/height=100. Supports undo. Text is only valid for text layers. Changes are in memory until saved.",
        json!({"parent_id":id,"kind":{"type":"string","enum":["frame","rectangle","ellipse","text"]},"name":name,"text":text,"x":coordinate,"y":coordinate,"width":size,"height":size}),vec!["parent_id","kind"],false,false);
    add("update_node","Update specified fields of one non-page layer. Coordinates are parent-relative. Replacing text clears text runs and list ranges. Supports undo; changes remain in memory until saved.",
        json!({"node_id":id,"name":name,"text":text,"x":coordinate,"y":coordinate,"width":size,"height":size,"visible":{"type":"boolean"},"opacity":{"type":"number","minimum":0,"maximum":1}}),vec!["node_id"],false,true);
    add(
        "delete_node",
        "Remove a non-page layer and its entire subtree. Supports undo; changes remain in memory until saved.",
        json!({"node_id":id}),
        vec!["node_id"],
        false,
        false,
    );
    add("undo", "Undo the latest successful edit in this session; returns changed=false if the stack is empty.", json!({}), vec![], false, false);
    add("redo", "Redo the most recently undone edit in this session; returns changed=false if the stack is empty.", json!({}), vec![], false, false);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variable_inspection_and_codegen_resolve_inherited_cross_collection_aliases() {
        use desigma_doc::{Variable, VariableCollection, VariableKind, VariableMode, VariableValue};
        let mut e = Engine::default();
        let id = create(&mut e, "rectangle", "Box");
        for collection in ["alias", "size"] {
            e.doc.variable_collections.insert(
                collection.into(),
                VariableCollection {
                    id: collection.into(),
                    name: collection.into(),
                    modes: vec![VariableMode { id: "light".into(), name: "Light".into() }, VariableMode { id: "dark".into(), name: "Dark".into() }],
                    default_mode: "light".into(),
                    variables: vec![],
                    key: None,
                },
            );
        }
        for (id, collection, light, dark) in [
            ("width", "size", VariableValue::Number(100.0), VariableValue::Number(240.0)),
            ("bound", "alias", VariableValue::Alias("width".into()), VariableValue::Alias("width".into())),
        ] {
            e.doc.variables.insert(
                id.into(),
                Variable {
                    id: id.into(),
                    name: id.into(),
                    collection: collection.into(),
                    kind: VariableKind::Number,
                    values: [("light".into(), light), ("dark".into(), dark)].into_iter().collect(),
                    description: String::new(),
                    scopes: vec![],
                    hidden_from_publishing: false,
                    key: None,
                },
            );
        }
        e.doc.get_mut(NodeId(1)).unwrap().variables.modes.insert("size".into(), "dark".into());
        e.doc.get_mut(NodeId(id)).unwrap().variables.bindings.insert("width".into(), "bound".into());
        let before = e.doc.to_json().unwrap();
        let values = e.call("desigma_get_variables", json!({"node_id":id})).unwrap();
        assert_eq!(values["effective_bindings"][0]["value"], json!({"Number":240.0}));
        let svg = e.call("desigma_generate_code", json!({"node_id":id,"language":"svg"})).unwrap();
        assert!(svg["code"].as_str().unwrap().contains("width=\"240\""));
        assert_eq!(e.doc.to_json().unwrap(), before);
    }
    fn create(e: &mut Engine, kind: &str, name: &str) -> u64 {
        e.call("desigma_create_node", json!({"parent_id":1,"kind":kind,"name":name})).unwrap()["node"]["id"].as_u64().unwrap()
    }
    #[test]
    fn crud_round_trips_through_undo_and_redo() {
        let mut e = Engine::default();
        let id = create(&mut e, "text", "Title");
        let before = e.doc.to_json().unwrap();
        e.call("desigma_update_node", json!({"node_id":id,"text":"Hello","x":17,"visible":false,"opacity":0.5})).unwrap();
        assert_eq!(e.doc.get(NodeId(id)).unwrap().transform.tx, 17.0);
        e.call("desigma_undo", json!({})).unwrap();
        assert_eq!(e.doc.to_json().unwrap(), before);
        e.call("desigma_redo", json!({})).unwrap();
        assert_eq!(e.doc.get(NodeId(id)).unwrap().kind.any_text().unwrap().content, "Hello");
        e.call("desigma_delete_node", json!({"node_id":id})).unwrap();
        assert!(e.doc.get(NodeId(id)).is_none());
        e.call("desigma_undo", json!({})).unwrap();
        assert!(e.doc.get(NodeId(id)).is_some());
    }
    #[test]
    fn invalid_mutations_leave_document_and_history_unchanged() {
        let mut e = Engine::default();
        let id = create(&mut e, "rectangle", "Box");
        let before = e.doc.to_json().unwrap();
        for (name, args) in [
            ("desigma_create_node", json!({"parent_id":1,"kind":"bad"})),
            ("desigma_create_node", json!({"parent_id":id,"kind":"frame"})),
            ("desigma_create_node", json!({"parent_id":1,"kind":"text","width":-1})),
            ("desigma_update_node", json!({"node_id":id,"name":"changed","text":"invalid"})),
            ("desigma_update_node", json!({"node_id":id,"x":1e100})),
            ("desigma_update_node", json!({"node_id":id,"opacity":2})),
            ("desigma_update_node", json!({"node_id":id,"unknown":true})),
            ("desigma_delete_node", json!({"node_id":1})),
            ("desigma_delete_node", json!({"node_id":0})),
        ] {
            assert!(e.call(name, args).is_err(), "{name}");
            assert_eq!(e.doc.to_json().unwrap(), before);
        }
        e.call("desigma_undo", json!({})).unwrap();
        assert!(!e.history.can_undo());
        assert!(e.doc.get(NodeId(id)).is_none());
    }
    #[test]
    fn name_filter_pagination_and_code_generation() {
        let mut e = Engine::default();
        let id = create(&mut e, "frame", "Card");
        create(&mut e, "rectangle", "card background");
        create(&mut e, "ellipse", "Icon");
        let result = e.call("desigma_list_nodes", json!({"parent_id":1,"query":"CARD","limit":1})).unwrap();
        assert_eq!(result["total"], 2);
        assert_eq!(result["nodes"][0]["node_id"], id);
        assert_eq!(result["next_offset"], 1);
        for language in ["css", "tailwind", "react", "swiftui", "compose", "svg"] {
            let output = e.call("desigma_generate_code", json!({"node_id":id,"language":language})).unwrap();
            assert!(!output["code"].as_str().unwrap().is_empty());
        }
        assert!(e.call("desigma_list_nodes", json!({"limit":501})).is_err());
        assert!(e.call("desigma_get_node", json!({"node_id":"2"})).is_err());
    }

    /// A host that counts commands proves `call` does not depend on `Engine`'s own history.
    #[test]
    fn free_call_runs_on_any_host() {
        struct Counting {
            e: Engine,
            execs: usize,
        }
        impl Host for Counting {
            fn doc(&self) -> &Document {
                &self.e.doc
            }
            fn exec(&mut self, cmd: Command) -> Result<(), String> {
                self.execs += 1;
                self.e.exec(cmd)
            }
            fn undo(&mut self) -> Result<bool, String> {
                self.e.undo()
            }
            fn redo(&mut self) -> Result<bool, String> {
                self.e.redo()
            }
            fn can_undo(&self) -> bool {
                self.e.can_undo()
            }
            fn can_redo(&self) -> bool {
                self.e.can_redo()
            }
            fn alloc_id(&mut self) -> Result<NodeId, String> {
                self.e.alloc_id()
            }
        }
        let mut h = Counting { e: Engine::default(), execs: 0 };
        let id = call(&mut h, "desigma_create_node", json!({"parent_id":1,"kind":"frame","name":"A"})).unwrap()["node"]["id"].as_u64().unwrap();
        call(&mut h, "desigma_update_node", json!({"node_id":id,"x":10.0})).unwrap();
        assert_eq!(h.execs, 2);
        assert!(call(&mut h, "desigma_undo", json!({})).unwrap()["changed"].as_bool().unwrap());
    }
}
