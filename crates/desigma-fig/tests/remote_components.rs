//! Instances of components that live in another file keep working: they get a
//! stand-in main component and a placeholder library.

use desigma_doc::*;
use desigma_fig::import_rest_json;

const JSON: &str = r##"{
  "name": "Consumer",
  "fileKey": "AbC123",
  "document": {
    "id": "0:0", "type": "DOCUMENT",
    "children": [{
      "id": "0:1", "type": "CANVAS", "name": "Page",
      "children": [
        { "id": "1:1", "type": "INSTANCE", "name": "Icon A", "componentId": "9:9",
          "absoluteBoundingBox": {"x":0,"y":0,"width":24,"height":24},
          "children": [
            {"id": "I1:1;9:10", "type": "TEXT", "name": "Label", "characters": "Home",
             "absoluteBoundingBox": {"x":0,"y":0,"width":24,"height":24}}
          ]},
        { "id": "1:2", "type": "INSTANCE", "name": "Icon B", "componentId": "9:9",
          "absoluteBoundingBox": {"x":40,"y":0,"width":24,"height":24},
          "children": [
            {"id": "I1:2;9:10", "type": "TEXT", "name": "Label", "characters": "Search",
             "absoluteBoundingBox": {"x":40,"y":0,"width":24,"height":24}}
          ]}
      ]
    }]
  },
  "components": { "9:9": { "key": "remotekey1", "name": "Icon", "remote": true } }
}"##;

fn find<'a>(doc: &'a Document, name: &str) -> &'a Node {
    doc.nodes.values().find(|n| n.name == name).unwrap_or_else(|| panic!("no node {name}"))
}

#[test]
fn remote_components_become_external_assets_of_a_placeholder_library() {
    let doc = import_rest_json(JSON).unwrap();
    let lp = doc.library_page.expect("hidden library page");
    assert!(!doc.pages.contains(&lp));
    let key = local_key("remote:AbC123", "remotekey1");
    let asset = doc.library.external.get(&key).expect("external asset");
    assert_eq!((asset.kind, asset.version), (LibraryAssetKind::Component, 0));
    assert!(doc.library.enabled.is_empty(), "a placeholder is not an enabled library");
    let comp = asset.node.unwrap();
    assert!(matches!(doc.get(comp).unwrap().kind, NodeKind::Component(_)));
    assert_eq!(doc.get(comp).unwrap().name, "Icon");
    assert!(is_library_node(&doc, comp));

    // Both instances point at the stand-in and are flagged as coming from another file.
    for name in ["Icon A", "Icon B"] {
        let inst = find(&doc, name);
        assert_eq!(instance_component(inst), Some(comp));
        let info = library_instance_info(&doc, inst.id).expect("library info");
        assert!(info.remote);
        assert_eq!(info.library_id, "remote:AbC123");
        assert_eq!(info.library_name, "AbC123");
    }

    // What differs between the instances is an override and survives a sync.
    let b = find(&doc, "Icon B");
    let label_b = doc.get(b.children[0]).unwrap();
    assert!(label_b.component.overrides.contains(&OverrideField::Characters));
    let mut doc = doc;
    let c = sync_instances(&mut doc, comp);
    c.apply(&mut doc).unwrap();
    let NodeKind::Text(t) = &doc.get(doc.get(find(&doc, "Icon B").id).unwrap().children[0]).unwrap().kind else { panic!() };
    assert_eq!(t.content, "Search");
    let NodeKind::Text(t) = &doc.get(doc.get(find(&doc, "Icon A").id).unwrap().children[0]).unwrap().kind else { panic!() };
    assert_eq!(t.content, "Home");

    // Exporting and re-importing keeps the instances linked to a stand-in.
    let again = import_rest_json(&desigma_fig::export_rest_json(&doc).to_string()).unwrap();
    assert_eq!(again.library.external.len(), 1);
    assert!(instance_component(find(&again, "Icon B")).is_some());

    // The state round-trips through JSON.
    let back = Document::from_json(&doc.to_json().unwrap()).unwrap();
    assert_eq!(back.library, doc.library);
    assert_eq!(back.library_page, doc.library_page);
}

#[test]
fn files_without_remote_components_have_no_library_state() {
    let doc = import_rest_json(include_str!("fixtures/rest_file.json")).unwrap();
    for e in doc.library.external.values() {
        assert!(e.library_id.starts_with(REMOTE_LIBRARY_PREFIX));
    }
}
