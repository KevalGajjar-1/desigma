//! Pure data behind the Inspect panel: box model, variables used, component properties and the
//! values pinned by annotations.

use crate::widgets::fmt_num;
use desigma_doc::*;

// ------------------------------------------------------------------ box model

/// Padding, size and the space to neighbouring siblings ("margin") of a layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxModel {
    pub size: (f32, f32),
    /// top, right, bottom, left (own auto layout padding)
    pub padding: [f32; 4],
    /// top, right, bottom, left: space to the nearest sibling in that direction
    pub margin: [f32; 4],
}

pub fn box_model(doc: &Document, id: NodeId) -> Option<BoxModel> {
    let n = doc.get(id)?;
    let padding = n.kind.frame_props().and_then(|f| f.auto_layout).map(|a| a.padding).unwrap_or([0.0; 4]);
    let mut margin = [0.0f32; 4];
    if let Some(parent) = n.parent.and_then(|p| doc.get(p)).filter(|p| !matches!(p.kind, NodeKind::Page { .. })) {
        let rect = |m: &Node| (m.transform.tx, m.transform.ty, m.transform.tx + m.size.x, m.transform.ty + m.size.y);
        let (x0, y0, x1, y1) = rect(n);
        let overlap = |a0: f32, a1: f32, b0: f32, b1: f32| a1.min(b1) - a0.max(b0) > 0.0;
        let mut best = [f32::MAX; 4];
        for s in parent.children.iter().filter(|c| **c != id).filter_map(|c| doc.get(*c)).filter(|c| c.visible) {
            let (sx0, sy0, sx1, sy1) = rect(s);
            if overlap(x0, x1, sx0, sx1) {
                if sy1 <= y0 + 0.001 {
                    best[0] = best[0].min(y0 - sy1);
                }
                if sy0 >= y1 - 0.001 {
                    best[2] = best[2].min(sy0 - y1);
                }
            }
            if overlap(y0, y1, sy0, sy1) {
                if sx0 >= x1 - 0.001 {
                    best[1] = best[1].min(sx0 - x1);
                }
                if sx1 <= x0 + 0.001 {
                    best[3] = best[3].min(x0 - sx1);
                }
            }
        }
        for i in 0..4 {
            if best[i] < f32::MAX {
                margin[i] = best[i].max(0.0);
            }
        }
    }
    Some(BoxModel { size: (n.size.x, n.size.y), padding, margin })
}

// ------------------------------------------------------------------ variables

/// One variable used by a layer or its descendants, with its value in every mode of its collection.
#[derive(Clone, Debug, PartialEq)]
pub struct VarUse {
    pub id: String,
    pub name: String,
    pub kind: VariableKind,
    pub collection: String,
    /// `(mode name, value)` in the order of the collection's modes.
    pub modes: Vec<(String, Option<VariableValue>)>,
    /// Properties bound on the inspected layer tree, e.g. "fills/0".
    pub props: Vec<String>,
}

/// Resolves `variable` in a fixed mode, following aliases (an alias into another collection
/// uses that collection's default mode).
pub fn resolve_in_mode(doc: &Document, variable: &str, mode: &str) -> Option<VariableValue> {
    let mut id = variable.to_string();
    let mut mode = mode.to_string();
    for _ in 0..16 {
        let v = doc.variables.get(&id)?;
        let coll = doc.variable_collections.get(&v.collection)?;
        let value = v.values.get(&mode).or_else(|| v.values.get(&coll.default_mode))?;
        match value {
            VariableValue::Alias(next) => {
                let target = doc.variables.get(next)?;
                if target.collection != v.collection {
                    mode = doc.variable_collections.get(&target.collection)?.default_mode.clone();
                }
                id = next.clone();
            }
            other => return Some(other.clone()),
        }
    }
    None
}

pub fn variables_used(doc: &Document, root: NodeId) -> Vec<VarUse> {
    let mut out: Vec<VarUse> = Vec::new();
    doc.walk(root, |n, _| {
        for (prop, var) in &n.variables.bindings {
            let Some(v) = doc.variables.get(var) else { continue };
            if let Some(u) = out.iter_mut().find(|u| u.id == v.id) {
                if !u.props.contains(prop) {
                    u.props.push(prop.clone());
                }
                continue;
            }
            let modes = doc
                .variable_collections
                .get(&v.collection)
                .map(|c| c.modes.iter().map(|m| (m.name.clone(), resolve_in_mode(doc, &v.id, &m.id))).collect())
                .unwrap_or_default();
            out.push(VarUse {
                id: v.id.clone(),
                name: v.name.clone(),
                kind: v.kind,
                collection: doc.variable_collections.get(&v.collection).map(|c| c.name.clone()).unwrap_or_default(),
                modes,
                props: vec![prop.clone()],
            });
        }
    });
    out
}

pub fn format_value(v: &VariableValue) -> String {
    match v {
        VariableValue::Color(c) => {
            if c.a < 0.999 {
                format!("#{} {}%", c.to_hex(), fmt_num((c.a * 100.0).round()))
            } else {
                format!("#{}", c.to_hex())
            }
        }
        VariableValue::Number(n) => fmt_num(*n),
        VariableValue::String(s) => format!("\"{s}\""),
        VariableValue::Boolean(b) => b.to_string(),
        VariableValue::Alias(a) => a.clone(),
    }
}

// ------------------------------------------------------------------ component properties

/// `(property, value)` rows for an instance; empty for other layers.
pub fn component_rows(doc: &Document, id: NodeId) -> Vec<(String, String)> {
    let Some(n) = doc.get(id) else { return Vec::new() };
    let Some(comp) = instance_component(n) else { return Vec::new() };
    let mut rows = vec![("Component".to_string(), doc.get(comp).map(|c| c.name.clone()).unwrap_or_default())];
    for def in prop_defs(doc, comp) {
        let value = match prop_value(doc, id, &def) {
            PropValue::Bool(b) => b.to_string(),
            PropValue::Text(t) => t,
            PropValue::Variant(v) => v,
            PropValue::Instance(c) => doc.get(c).map(|c| c.name.clone()).unwrap_or_default(),
        };
        rows.push((def.name.split('#').next().unwrap_or(&def.name).to_string(), value));
    }
    rows
}

// ------------------------------------------------------------------ annotations

fn paints_hex(p: &[Paint]) -> String {
    let v: Vec<String> = p
        .iter()
        .filter(|p| p.visible)
        .map(|p| match &p.kind {
            PaintKind::Solid(c) => format!("#{}", c.to_hex()),
            PaintKind::Image { .. } => "Image".into(),
            _ => "Gradient".into(),
        })
        .collect();
    if v.is_empty() {
        "None".into()
    } else {
        v.join(", ")
    }
}

/// Labels from [`ANNOTATION_PROPERTIES`] that make sense for `n`.
pub fn applicable_properties(n: &Node) -> Vec<&'static str> {
    let text = matches!(n.kind, NodeKind::Text(_));
    let al = n.kind.frame_props().is_some_and(|f| f.auto_layout.is_some());
    ANNOTATION_PROPERTIES
        .iter()
        .map(|(l, _)| *l)
        .filter(|l| match *l {
            "Font" | "Font style" | "Font size" | "Font weight" | "Line height" | "Letter spacing" | "Text align" | "Text style" => text,
            "Gap" | "Padding" => al,
            "Corner radius" => !text,
            _ => true,
        })
        .collect()
}

/// Current value of a pinned property, as printed in the callout.
pub fn property_value(doc: &Document, n: &Node, label: &str) -> Option<String> {
    let px = |v: f32| format!("{}px", fmt_num(v));
    let lc = &n.layout_child;
    let al = n.kind.frame_props().and_then(|f| f.auto_layout);
    let ts = match &n.kind {
        NodeKind::Text(t) => Some(&t.style),
        _ => None,
    };
    Some(match label {
        "Width" => px(n.size.x),
        "Height" => px(n.size.y),
        "Min width" => px(lc.min_w?),
        "Max width" => px(lc.max_w?),
        "Min height" => px(lc.min_h?),
        "Max height" => px(lc.max_h?),
        "Fill" => match &n.kind {
            NodeKind::Text(t) => paints_hex(&t.style.fills),
            _ => paints_hex(&n.fills),
        },
        "Stroke" => paints_hex(&n.stroke.paints),
        "Stroke weight" => px(n.stroke.weight),
        "Effects" => format!("{} effect{}", n.effects.len(), if n.effects.len() == 1 { "" } else { "s" }),
        "Corner radius" => {
            let r = n.corner_radii;
            if r.iter().all(|v| *v == r[0]) {
                px(r[0])
            } else {
                r.iter().map(|v| fmt_num(*v)).collect::<Vec<_>>().join(" ")
            }
        }
        "Text style" => doc.style_name(n.style_refs.get("text")?).to_string(),
        "Text align" => format!("{:?}", ts?.align_h),
        "Font" => ts?.font_family.clone(),
        "Font style" => format!("{}{}", crate::panels::inspect::weight_name(ts?.font_weight), if ts?.italic { " Italic" } else { "" }),
        "Font size" => px(ts?.font_size),
        "Font weight" => ts?.font_weight.to_string(),
        "Line height" => match ts?.line_height {
            LineHeight::Auto => "Auto".into(),
            LineHeight::Pixels(p) => px(p),
            LineHeight::Percent(p) => format!("{}%", fmt_num(p)),
        },
        "Letter spacing" => px(ts?.letter_spacing),
        "Gap" => px(al?.item_spacing),
        "Padding" => al?.padding.iter().map(|v| fmt_num(*v)).collect::<Vec<_>>().join(" "),
        "Opacity" => format!("{}%", fmt_num((n.opacity * 100.0).round())),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_with_siblings() -> (Document, NodeId, NodeId, NodeId) {
        let mut d = Document::new("t");
        let page = d.add_page("p");
        let f = d.alloc_id();
        let mut fr = Node::frame(f, Rect::new(0.0, 0.0, 300.0, 300.0));
        if let NodeKind::Frame(fp) = &mut fr.kind {
            fp.auto_layout = Some(AutoLayout { padding: [4.0, 8.0, 4.0, 8.0], ..Default::default() });
        }
        let f = d.insert(page, fr).unwrap();
        let mk = |d: &mut Document, r: Rect| {
            let i = d.alloc_id();
            d.insert(f, Node::rectangle(i, r)).unwrap()
        };
        let a = mk(&mut d, Rect::new(0.0, 0.0, 50.0, 40.0));
        let b = mk(&mut d, Rect::new(70.0, 0.0, 50.0, 40.0));
        let c = mk(&mut d, Rect::new(0.0, 60.0, 50.0, 40.0));
        (d, a, b, c)
    }

    #[test]
    fn margins_are_the_gaps_to_neighbouring_siblings() {
        let (d, a, b, c) = doc_with_siblings();
        let ma = box_model(&d, a).unwrap();
        assert_eq!(ma.margin, [0.0, 20.0, 20.0, 0.0], "right neighbour b at 20, below neighbour c at 20");
        assert_eq!(box_model(&d, b).unwrap().margin, [0.0, 0.0, 0.0, 20.0]);
        assert_eq!(box_model(&d, c).unwrap().margin, [20.0, 0.0, 0.0, 0.0]);
        assert!(ma.padding.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn padding_comes_from_the_layers_own_auto_layout() {
        let (d, a, _, _) = doc_with_siblings();
        let parent = d.get(a).unwrap().parent.unwrap();
        let m = box_model(&d, parent).unwrap();
        assert_eq!(m.padding, [4.0, 8.0, 4.0, 8.0]);
        assert_eq!(m.margin, [0.0; 4], "top-level frames have no siblings");
        assert_eq!(m.size, (300.0, 300.0));
    }

    fn var_doc() -> (Document, NodeId) {
        let mut d = Document::new("t");
        let page = d.add_page("p");
        let f = d.alloc_id();
        let f = d.insert(page, Node::frame(f, Rect::new(0.0, 0.0, 10.0, 10.0))).unwrap();
        d.variable_collections.insert(
            "c".into(),
            VariableCollection {
                id: "c".into(),
                name: "Theme".into(),
                modes: vec![VariableMode { id: "l".into(), name: "Light".into() }, VariableMode { id: "d".into(), name: "Dark".into() }],
                default_mode: "l".into(),
                variables: vec!["base".into(), "bg".into()],
                key: None,
            },
        );
        let var = |id: &str, vals: Vec<(&str, VariableValue)>| Variable {
            id: id.into(),
            name: format!("Color/{id}"),
            collection: "c".into(),
            kind: VariableKind::Color,
            values: vals.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
            description: String::new(),
            scopes: vec![],
            hidden_from_publishing: false,
            key: None,
        };
        d.variables.insert("base".into(), var("base", vec![("l", VariableValue::Color(Color::WHITE)), ("d", VariableValue::Color(Color::BLACK))]));
        d.variables.insert("bg".into(), var("bg", vec![("l", VariableValue::Alias("base".into())), ("d", VariableValue::Alias("base".into()))]));
        d.get_mut(f).unwrap().variables.bindings.insert("fills/0".into(), "bg".into());
        d.get_mut(f).unwrap().variables.bindings.insert("strokes/0".into(), "bg".into());
        (d, f)
    }

    #[test]
    fn variables_used_lists_every_mode_through_aliases() {
        let (d, f) = var_doc();
        let u = variables_used(&d, f);
        assert_eq!(u.len(), 1);
        assert_eq!(u[0].name, "Color/bg");
        assert_eq!(u[0].collection, "Theme");
        assert_eq!(u[0].props, vec!["fills/0".to_string(), "strokes/0".to_string()]);
        assert_eq!(
            u[0].modes,
            vec![("Light".to_string(), Some(VariableValue::Color(Color::WHITE))), ("Dark".to_string(), Some(VariableValue::Color(Color::BLACK)))]
        );
        assert_eq!(format_value(&VariableValue::Color(Color::WHITE)), "#FFFFFF");
        assert_eq!(format_value(&VariableValue::Number(8.0)), "8");
    }

    #[test]
    fn pinned_property_values() {
        let (d, a, _, _) = doc_with_siblings();
        let n = d.get(a).unwrap();
        assert_eq!(property_value(&d, n, "Width").as_deref(), Some("50px"));
        assert_eq!(property_value(&d, n, "Fill").as_deref(), Some("#D9D9D9"));
        assert_eq!(property_value(&d, n, "Font"), None);
        assert!(applicable_properties(n).contains(&"Width"));
        assert!(!applicable_properties(n).contains(&"Font"));
        let parent = d.get(d.get(a).unwrap().parent.unwrap()).unwrap();
        assert!(applicable_properties(parent).contains(&"Padding"));
        assert_eq!(property_value(&d, parent, "Padding").as_deref(), Some("4 8 4 8"));
    }
}
