#![cfg(feature = "server")]
use desigma_mcp::server::Session;
use serde_json::json;
use std::{fs, path::PathBuf};

struct Workspace(PathBuf);
impl Workspace {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let p = std::env::temp_dir().join(format!("desigma-mcp-{}-{nonce}", std::process::id()));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn saved_file_roundtrip_and_unsaved_changes_guard() {
    let dir = Workspace::new();
    let mut s = Session::new(&dir.0).unwrap();
    let d = s.call("desigma_get_document", json!({})).unwrap();
    let page = d["pages"][0]["node_id"].clone();
    let node = s.call("desigma_create_node", json!({"parent_id":page,"kind":"text","name":"Heading","text":"Hello MCP"})).unwrap();
    assert!(s.call("desigma_new_document", json!({"name":"Oops"})).is_err());
    s.call("desigma_save_file", json!({"path":"first.desigma"})).unwrap();
    s.call("desigma_update_node", json!({"node_id":node["node"]["id"],"text":"Changed"})).unwrap();
    assert!(s.call("desigma_open_file", json!({"path":"first.desigma"})).is_err());
    s.call("desigma_open_file", json!({"path":"first.desigma","discard_changes":true})).unwrap();
    assert_eq!(s.call("desigma_get_node", json!({"node_id":node["node"]["id"]})).unwrap()["node"], node["node"]);
    let original = fs::read(dir.0.join("first.desigma")).unwrap();
    assert!(s.call("desigma_save_file", json!({"path":"first.desigma"})).is_err());
    assert_eq!(fs::read(dir.0.join("first.desigma")).unwrap(), original);
}

#[test]
fn paths_and_bad_imports_cannot_replace_the_session() {
    let dir = Workspace::new();
    let outside = Workspace::new();
    fs::write(outside.0.join("outside.desigma"), "{}").unwrap();
    let mut s = Session::new(&dir.0).unwrap();
    let before = s.call("desigma_get_document", json!({})).unwrap();
    assert!(s.call("desigma_open_file", json!({"path":outside.0.join("outside.desigma")})).is_err());
    assert!(s.call("desigma_save_file", json!({"path":outside.0.join("escape.desigma")})).is_err());
    assert!(s.call("desigma_save_file", json!({"path":"../escape.desigma"})).is_err());
    assert!(s.call("desigma_save_file", json!({"path":"bad.json"})).is_err());
    fs::write(dir.0.join("bad.desigma"), "not JSON").unwrap();
    assert!(s.call("desigma_open_file", json!({"path":"bad.desigma"})).is_err());
    let mut broken = s.engine.doc.clone();
    let id = broken.pages[0];
    broken.get_mut(id).unwrap().parent = Some(id);
    fs::write(dir.0.join("cycle.desigma"), broken.to_json().unwrap()).unwrap();
    assert!(s.call("desigma_open_file", json!({"path":"cycle.desigma"})).is_err());
    assert_eq!(s.call("desigma_get_document", json!({})).unwrap(), before);
    assert!(!outside.0.join("escape.desigma").exists());
}

#[test]
fn imported_stale_id_counter_does_not_overwrite_existing_nodes() {
    let dir = Workspace::new();
    let mut s = Session::new(&dir.0).unwrap();
    let mut native = serde_json::to_value(&s.engine.doc).unwrap();
    native["next_id"] = json!(0);
    fs::write(dir.0.join("stale.desigma"), native.to_string()).unwrap();
    let doc = s.call("desigma_open_file", json!({"path":"stale.desigma"})).unwrap();
    let page = doc["pages"][0]["node_id"].clone();
    let created = s.call("desigma_create_node", json!({"parent_id":page,"kind":"rectangle"})).unwrap();
    assert_ne!(created["node"]["id"], page);
    assert_eq!(s.call("desigma_get_document", json!({})).unwrap()["node_count"], 2);
    s.call("desigma_delete_node", json!({"node_id":created["node"]["id"]})).unwrap();
    let next = s.call("desigma_create_node", json!({"parent_id":page,"kind":"rectangle"})).unwrap();
    assert!(next["node"]["id"].as_u64().unwrap() > created["node"]["id"].as_u64().unwrap());

    let mut exhausted = serde_json::to_value(&s.engine.doc).unwrap();
    exhausted["next_id"] = json!(u64::MAX);
    fs::write(dir.0.join("exhausted.desigma"), exhausted.to_string()).unwrap();
    s.call("desigma_open_file", json!({"path":"exhausted.desigma","discard_changes":true})).unwrap();
    let before = s.engine.doc.to_json().unwrap();
    assert!(s.call("desigma_create_node", json!({"parent_id":page,"kind":"rectangle"})).unwrap_err().contains("exhausted"));
    assert_eq!(s.engine.doc.to_json().unwrap(), before);
}
