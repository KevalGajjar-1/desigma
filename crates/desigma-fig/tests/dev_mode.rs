//! Dev Mode data (`devStatus`, `annotations`) through the REST importer / exporter.

use desigma_doc::*;
use desigma_fig::{export_rest_json, import_rest_json};

const FILE: &str = r#"{
  "name": "Dev",
  "document": {
    "id": "0:0", "type": "DOCUMENT", "name": "Document",
    "children": [{
      "id": "0:1", "type": "CANVAS", "name": "Page 1",
      "backgroundColor": {"r": 1, "g": 1, "b": 1, "a": 1},
      "children": [{
        "id": "1:1", "type": "FRAME", "name": "Ready",
        "absoluteBoundingBox": {"x": 0, "y": 0, "width": 200, "height": 100},
        "size": {"x": 200, "y": 100},
        "devStatus": {"type": "READY_FOR_DEV"},
        "annotations": [
          {"label": "Use the 8px grid", "properties": [{"type": "width"}, {"type": "fills"}, {"type": "fontFamily"}], "categoryId": "cat-1"},
          {"labelMarkdown": "**Bold** note"}
        ],
        "devResources": [{"name": "Storybook", "url": "https://sb.example.com/button"}],
        "children": [{
          "id": "1:2", "type": "RECTANGLE", "name": "Done",
          "absoluteBoundingBox": {"x": 0, "y": 0, "width": 10, "height": 10},
          "size": {"x": 10, "y": 10},
          "devStatus": {"type": "COMPLETED", "description": "shipped"}
        }, {
          "id": "1:3", "type": "RECTANGLE", "name": "Plain",
          "absoluteBoundingBox": {"x": 0, "y": 0, "width": 10, "height": 10},
          "size": {"x": 10, "y": 10}
        }]
      }]
    }]
  }
}"#;

fn find<'a>(doc: &'a Document, name: &str) -> &'a Node {
    doc.nodes.values().find(|n| n.name == name).unwrap_or_else(|| panic!("no node named {name}"))
}

#[test]
fn dev_status_and_annotations_are_imported() {
    let doc = import_rest_json(FILE).unwrap();
    let ready = find(&doc, "Ready");
    assert_eq!(ready.dev.status, Some(DevStatus::ReadyForDev));
    assert_eq!(
        ready.dev.annotations,
        vec![
            Annotation { text: "Use the 8px grid".into(), properties: vec!["Width".into(), "Fill".into(), "Font".into()], category: Some("cat-1".into()) },
            Annotation { text: "**Bold** note".into(), properties: vec![], category: None },
        ]
    );
    assert_eq!(ready.dev.links, vec![DevLink { label: "Storybook".into(), url: "https://sb.example.com/button".into() }]);
    assert_eq!(find(&doc, "Done").dev.status, Some(DevStatus::Completed));
    assert!(find(&doc, "Plain").dev.is_empty());
}

#[test]
fn dev_data_round_trips() {
    let doc = import_rest_json(FILE).unwrap();
    let text = export_rest_json(&doc).to_string();
    assert!(text.contains(r#""devStatus":{"type":"READY_FOR_DEV"}"#));
    assert!(text.contains(r#""devStatus":{"type":"COMPLETED"}"#));
    assert!(text.contains(r#""properties":[{"type":"width"},{"type":"fills"},{"type":"fontFamily"}]"#));
    let back = import_rest_json(&text).unwrap();
    for name in ["Ready", "Done", "Plain"] {
        assert_eq!(find(&back, name).dev, find(&doc, name).dev, "{name}");
    }
}

#[test]
fn changed_exports_as_ready_for_dev() {
    let mut doc = import_rest_json(FILE).unwrap();
    let id = find(&doc, "Ready").id;
    doc.get_mut(id).unwrap().dev.status = Some(DevStatus::Changed);
    let back = import_rest_json(&export_rest_json(&doc).to_string()).unwrap();
    assert_eq!(find(&back, "Ready").dev.status, Some(DevStatus::ReadyForDev));
}
