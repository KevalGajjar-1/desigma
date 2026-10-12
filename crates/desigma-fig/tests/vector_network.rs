//! `vectorNetwork` in REST JSON: import, edit-ready model and export round trip.

use desigma_doc::*;
use desigma_fig::{export_rest_json, import_rest_json};
use serde_json::json;

fn file(vector: serde_json::Value) -> String {
    json!({
        "name": "vec",
        "document": {"id": "0:0", "type": "DOCUMENT", "children": [
            {"id": "0:1", "name": "Page", "type": "CANVAS", "children": [vector]}
        ]}
    })
    .to_string()
}

fn triangle_node(with_geometry: bool) -> serde_json::Value {
    let mut v = json!({
        "id": "1:1", "name": "Tri", "type": "VECTOR",
        "size": {"x": 100.0, "y": 100.0},
        "relativeTransform": [[1, 0, 10], [0, 1, 20]],
        "vectorNetwork": {
            "vertices": [
                {"x": 0.0, "y": 100.0, "cornerRadius": 8.0},
                {"x": 50.0, "y": 0.0, "strokeCap": "ROUND", "handleMirroring": "ANGLE_AND_LENGTH"},
                {"x": 100.0, "y": 100.0}
            ],
            "segments": [
                {"start": 0, "end": 1, "tangentStart": {"x": 0.0, "y": 0.0}, "tangentEnd": {"x": 0.0, "y": 0.0}},
                {"start": 1, "end": 2, "tangentStart": {"x": 0.0, "y": 0.0}, "tangentEnd": {"x": 0.0, "y": 0.0}},
                {"start": 2, "end": 0, "tangentStart": {"x": 0.0, "y": 0.0}, "tangentEnd": {"x": 0.0, "y": 0.0}}
            ],
            "regions": [{"windingRule": "NONZERO", "loops": [[0, 1, 2]]}]
        }
    });
    if with_geometry {
        v["fillGeometry"] = json!([{"path": "M0 100 L50 0 L100 100 Z", "windingRule": "NONZERO"}]);
    }
    v
}

fn vector(doc: &Document) -> &Node {
    doc.nodes.values().find(|n| n.name == "Tri").unwrap()
}

#[test]
fn network_is_imported_with_per_vertex_properties() {
    let doc = import_rest_json(&file(triangle_node(false))).unwrap();
    let n = vector(&doc);
    let NodeKind::Vector { paths, network: Some(net) } = &n.kind else { panic!("expected a vector with a network") };
    assert_eq!((net.vertices.len(), net.segments.len(), net.regions.len()), (3, 3, 1));
    assert_eq!(net.vertices[0].corner_radius, 8.0);
    assert_eq!(net.vertices[1].cap, Some(StrokeCap::Round));
    assert_eq!(net.vertices[1].mirror, HandleMirror::AngleAndLength);
    // without Figma's outline the paths are derived from the network (with the corner radius)
    assert_eq!(paths, &net.to_paths());
    assert!(paths[0].cmds.iter().any(|c| matches!(c, PathCmd::CubicTo(..))), "the rounded corner is an arc");
}

#[test]
fn figma_outline_stays_the_rendered_geometry() {
    let doc = import_rest_json(&file(triangle_node(true))).unwrap();
    let n = vector(&doc);
    let NodeKind::Vector { paths, network } = &n.kind else { panic!() };
    assert!(network.is_some());
    assert_eq!(paths.len(), 1);
    assert!(paths[0].cmds.iter().all(|c| !matches!(c, PathCmd::CubicTo(..))), "Figma's own outline, not re-derived");
    assert!(n.fill_geometry.is_some());
}

#[test]
fn malformed_networks_fall_back_to_plain_paths() {
    let mut node = triangle_node(true);
    node["vectorNetwork"]["segments"][0]["end"] = json!(9);
    let doc = import_rest_json(&file(node)).unwrap();
    let NodeKind::Vector { paths, network } = &vector(&doc).kind else { panic!() };
    assert!(network.is_none());
    assert_eq!(paths.len(), 1);
}

#[test]
fn export_and_reimport_keep_the_network() {
    let doc = import_rest_json(&file(triangle_node(false))).unwrap();
    let json = export_rest_json(&doc);
    let again = import_rest_json(&json.to_string()).unwrap();
    let (a, b) = (vector(&doc), vector(&again));
    let (NodeKind::Vector { network: Some(na), .. }, NodeKind::Vector { network: Some(nb), .. }) = (&a.kind, &b.kind) else { panic!() };
    assert_eq!(na, nb);
    assert_eq!(json, export_rest_json(&again));
}
