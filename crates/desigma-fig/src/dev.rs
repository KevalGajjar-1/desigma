//! Dev Mode data in Figma REST JSON: `devStatus`, `annotations` and (as an extension that
//! mirrors the dev resources endpoint) `devResources`.

use crate::rest_import::{farr, fstr};
use desigma_doc::*;
use serde_json::{json, Map, Value};

/// Reads `devStatus`, `annotations` and `devResources` of a node.
///
/// `devStatus.type` is `READY_FOR_DEV` or `COMPLETED`; an annotation's `label` (or
/// `labelMarkdown`), pinned `properties[].type` and `categoryId` map to [`Annotation`].
pub(crate) fn parse_dev(v: &Value) -> DevInfo {
    let mut dev = DevInfo::default();
    if let Some(s) = v.get("devStatus") {
        dev.status = match fstr(s, "type") {
            Some("READY_FOR_DEV") => Some(DevStatus::ReadyForDev),
            Some("COMPLETED") => Some(DevStatus::Completed),
            _ => None,
        };
    }
    for a in farr(v, "annotations") {
        let text = fstr(a, "label").or_else(|| fstr(a, "labelMarkdown")).unwrap_or("").to_string();
        let properties = farr(a, "properties").iter().filter_map(|p| fstr(p, "type")).map(annotation_property_label).collect();
        dev.annotations.push(Annotation { text, properties, category: fstr(a, "categoryId").map(str::to_string) });
    }
    for r in farr(v, "devResources") {
        if let Some(url) = fstr(r, "url") {
            dev.links.push(DevLink { label: fstr(r, "name").unwrap_or("").to_string(), url: url.to_string() });
        }
    }
    dev
}

/// Writes the dev data of `node` into its REST object. *Changed* is written as
/// `READY_FOR_DEV`: the REST API has no such status, Figma derives it from the edit history.
pub(crate) fn export_dev(node: &Node, m: &mut Map<String, Value>) {
    let d = &node.dev;
    match d.status {
        Some(DevStatus::ReadyForDev | DevStatus::Changed) => {
            m.insert("devStatus".into(), json!({"type": "READY_FOR_DEV"}));
        }
        Some(DevStatus::Completed) => {
            m.insert("devStatus".into(), json!({"type": "COMPLETED"}));
        }
        None => {}
    }
    if !d.annotations.is_empty() {
        let list = d
            .annotations
            .iter()
            .map(|a| {
                let mut o = Map::new();
                o.insert("label".into(), a.text.clone().into());
                if !a.properties.is_empty() {
                    o.insert("properties".into(), Value::Array(a.properties.iter().map(|p| json!({"type": annotation_property_type(p)})).collect()));
                }
                if let Some(c) = &a.category {
                    o.insert("categoryId".into(), c.clone().into());
                }
                Value::Object(o)
            })
            .collect();
        m.insert("annotations".into(), Value::Array(list));
    }
    if !d.links.is_empty() {
        m.insert("devResources".into(), Value::Array(d.links.iter().map(|l| json!({"name": l.label, "url": l.url})).collect()));
    }
}
