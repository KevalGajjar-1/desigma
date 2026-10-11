//! Ruler guides and export presets through the REST importer / exporter.

use desigma_doc::*;
use desigma_fig::{export_rest_json, import_rest_json};

const FILE: &str = r#"{
  "name": "Settings",
  "document": {
    "id": "0:0", "type": "DOCUMENT", "name": "Document",
    "children": [{
      "id": "0:1", "type": "CANVAS", "name": "Page 1",
      "backgroundColor": {"r": 0.9, "g": 0.9, "b": 0.9, "a": 1},
      "guides": [{"axis": "X", "offset": 120}, {"axis": "Y", "offset": 64.5}],
      "children": [{
        "id": "1:1", "type": "FRAME", "name": "Card",
        "absoluteBoundingBox": {"x": 0, "y": 0, "width": 200, "height": 100},
        "size": {"x": 200, "y": 100},
        "guides": [{"axis": "Y", "offset": 8}],
        "exportSettings": [
          {"suffix": "@2x", "format": "PNG", "constraint": {"type": "SCALE", "value": 2}},
          {"suffix": "", "format": "SVG", "constraint": {"type": "WIDTH", "value": 512}},
          {"suffix": "-thumb", "format": "JPG", "constraint": {"type": "HEIGHT", "value": 64}},
          {"suffix": "", "format": "PDF", "constraint": {"type": "SCALE", "value": 1}}
        ],
        "children": [{
          "id": "1:2", "type": "RECTANGLE", "name": "Plain",
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
fn export_settings_and_guides_are_imported() {
    let doc = import_rest_json(FILE).unwrap();
    let card = find(&doc, "Card");
    assert_eq!(
        card.export_settings,
        vec![
            ExportPreset { format: ExportFileFormat::Png, constraint: ExportConstraint::Scale(2.0), suffix: "@2x".into() },
            ExportPreset { format: ExportFileFormat::Svg, constraint: ExportConstraint::Width(512.0), suffix: String::new() },
            ExportPreset { format: ExportFileFormat::Jpg, constraint: ExportConstraint::Height(64.0), suffix: "-thumb".into() },
            ExportPreset { format: ExportFileFormat::Pdf, constraint: ExportConstraint::Scale(1.0), suffix: String::new() },
        ]
    );
    assert!(find(&doc, "Plain").export_settings.is_empty());

    let page = doc.pages[0];
    assert_eq!(
        doc.guides[&page],
        vec![
            Guide { axis: GuideAxis::X, offset: 120.0, frame: None },
            Guide { axis: GuideAxis::Y, offset: 64.5, frame: None },
            Guide { axis: GuideAxis::Y, offset: 8.0, frame: Some(card.id) },
        ]
    );
}

#[test]
fn export_settings_and_guides_round_trip() {
    let doc = import_rest_json(FILE).unwrap();
    let json = export_rest_json(&doc);
    let text = json.to_string();
    assert!(text.contains("\"exportSettings\"") && text.contains("\"guides\""));
    let again = import_rest_json(&text).unwrap();
    assert_eq!(find(&again, "Card").export_settings, find(&doc, "Card").export_settings);
    assert_eq!(again.guides.values().next(), doc.guides.values().next());
    assert_eq!(again.guides.len(), 1);
    // and the export is a fixed point
    assert_eq!(json, export_rest_json(&again));
}

#[test]
fn documents_without_settings_export_none() {
    let doc = import_rest_json(r#"{"document": {"type": "DOCUMENT", "children": [{"id": "0:1", "type": "CANVAS", "name": "P", "children": []}]}}"#).unwrap();
    assert!(doc.guides.is_empty());
    let text = export_rest_json(&doc).to_string();
    assert!(!text.contains("exportSettings") && !text.contains("guides"));
}
