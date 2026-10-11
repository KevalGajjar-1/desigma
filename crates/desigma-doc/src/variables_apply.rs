//! Applies variable bindings to nodes: substitutes the value a bound variable
//! resolves to (in the mode in effect at the node) into the node property the
//! binding names. Pure helpers; the document itself keeps the raw values.
//!
//! Binding keys follow Figma's `boundVariables`: `fills/N`, `strokes/N`,
//! `width`, `height`, `topLeftRadius`..., `itemSpacing`, `counterAxisSpacing`,
//! `paddingLeft`..., `opacity`, `visible`, `characters`, `fontSize`,
//! `fontWeight`, `lineHeight`, `letterSpacing`, `strokeWeight` (and the
//! per-side `strokeTopWeight`...). Opacity variables hold percent (0..=100),
//! as in Figma; line height is in pixels.

use crate::*;
use std::borrow::Cow;

/// Kind of variable a binding key accepts, or `None` for unknown keys.
pub fn property_kind(key: &str) -> Option<VariableKind> {
    Some(match key {
        k if k.starts_with("fills/") || k.starts_with("strokes/") => VariableKind::Color,
        "width" | "height" | "topLeftRadius" | "topRightRadius" | "bottomLeftRadius" | "bottomRightRadius" | "itemSpacing" | "counterAxisSpacing"
        | "paddingLeft" | "paddingRight" | "paddingTop" | "paddingBottom" | "opacity" | "fontSize" | "fontWeight" | "lineHeight" | "letterSpacing"
        | "strokeWeight" | "strokeTopWeight" | "strokeRightWeight" | "strokeBottomWeight" | "strokeLeftWeight" => VariableKind::Number,
        "visible" => VariableKind::Boolean,
        "characters" => VariableKind::String,
        _ => return None,
    })
}

/// Whether a variable with these Figma `scopes` may be bound to `key` on a
/// node of `kind`. An empty list (or `ALL_SCOPES`) allows everything.
pub fn scope_allows(scopes: &[String], key: &str, kind: &NodeKind) -> bool {
    if scopes.is_empty() || scopes.iter().any(|s| s == "ALL_SCOPES") {
        return true;
    }
    let has = |names: &[&str]| scopes.iter().any(|s| names.contains(&s.as_str()));
    match key {
        k if k.starts_with("fills/") => {
            let specific = match kind {
                NodeKind::Text(_) => "TEXT_FILL",
                NodeKind::Frame(_) | NodeKind::Component(_) | NodeKind::ComponentSet(_) | NodeKind::Instance { .. } | NodeKind::Section => "FRAME_FILL",
                _ => "SHAPE_FILL",
            };
            has(&["ALL_FILLS", specific])
        }
        k if k.starts_with("strokes/") => has(&["STROKE_COLOR"]),
        "width" | "height" => has(&["WIDTH_HEIGHT"]),
        "topLeftRadius" | "topRightRadius" | "bottomLeftRadius" | "bottomRightRadius" => has(&["CORNER_RADIUS"]),
        "itemSpacing" | "counterAxisSpacing" | "paddingLeft" | "paddingRight" | "paddingTop" | "paddingBottom" => has(&["GAP"]),
        "opacity" => has(&["OPACITY"]),
        "strokeWeight" | "strokeTopWeight" | "strokeRightWeight" | "strokeBottomWeight" | "strokeLeftWeight" => has(&["STROKE_FLOAT"]),
        "fontSize" => has(&["FONT_SIZE"]),
        "fontWeight" => has(&["FONT_WEIGHT"]),
        "lineHeight" => has(&["LINE_HEIGHT"]),
        "letterSpacing" => has(&["LETTER_SPACING"]),
        "characters" => has(&["TEXT_CONTENT"]),
        "visible" => true,
        _ => false,
    }
}

/// Binding keys that make sense for a node (what the inspector can bind).
pub fn bindable_keys(kind: &NodeKind) -> Vec<&'static str> {
    let mut v = vec!["width", "height", "opacity", "visible", "fills/0", "strokes/0", "strokeWeight"];
    if !matches!(kind, NodeKind::Text(_)) {
        v.extend(["topLeftRadius", "topRightRadius", "bottomRightRadius", "bottomLeftRadius"]);
    }
    if kind.frame_props().is_some_and(|f| f.auto_layout.is_some()) {
        v.extend(["itemSpacing", "counterAxisSpacing", "paddingLeft", "paddingRight", "paddingTop", "paddingBottom"]);
    }
    if matches!(kind, NodeKind::Text(_)) {
        v.extend(["characters", "fontSize", "fontWeight", "lineHeight", "letterSpacing"]);
    }
    v
}

fn color_of(v: &VariableValue) -> Option<Color> {
    match v {
        VariableValue::Color(c) => Some(*c),
        _ => None,
    }
}

fn number_of(v: &VariableValue) -> Option<f32> {
    match v {
        VariableValue::Number(n) if n.is_finite() => Some(*n),
        _ => None,
    }
}

fn paint_at<'a>(paints: &'a mut [Paint], key: &str, prefix: &str) -> Option<&'a mut Paint> {
    let i: usize = key.strip_prefix(prefix)?.parse().ok()?;
    paints.get_mut(i)
}

/// Writes `value` into the node property named by `key`; mismatched types and
/// unknown keys are ignored.
pub fn set_bound_property(n: &mut Node, key: &str, value: &VariableValue) {
    if key.starts_with("fills/") {
        let Some(c) = color_of(value) else { return };
        let paints = match &mut n.kind {
            NodeKind::Text(t) => &mut t.style.fills,
            _ => &mut n.fills,
        };
        if let Some(p) = paint_at(paints, key, "fills/") {
            p.kind = PaintKind::Solid(c);
        }
        return;
    }
    if key.starts_with("strokes/") {
        let Some(c) = color_of(value) else { return };
        if let Some(p) = paint_at(&mut n.stroke.paints, key, "strokes/") {
            p.kind = PaintKind::Solid(c);
        }
        return;
    }
    if key == "visible" {
        if let VariableValue::Boolean(b) = value {
            n.visible = *b;
        }
        return;
    }
    if key == "characters" {
        if let (VariableValue::String(s), NodeKind::Text(t)) = (value, &mut n.kind) {
            t.content = s.clone();
            t.runs.clear();
        }
        return;
    }
    let Some(x) = number_of(value) else { return };
    match key {
        "width" => n.size.x = x.max(0.0),
        "height" => n.size.y = x.max(0.0),
        "opacity" => n.opacity = (x / 100.0).clamp(0.0, 1.0),
        "topLeftRadius" => n.corner_radii[0] = x.max(0.0),
        "topRightRadius" => n.corner_radii[1] = x.max(0.0),
        "bottomRightRadius" => n.corner_radii[2] = x.max(0.0),
        "bottomLeftRadius" => n.corner_radii[3] = x.max(0.0),
        "strokeWeight" => n.stroke.weight = x.max(0.0),
        "strokeTopWeight" | "strokeRightWeight" | "strokeBottomWeight" | "strokeLeftWeight" => {
            let w = n.stroke.weight;
            let sw = n.stroke.side_weights.get_or_insert([w; 4]);
            let i = match key {
                "strokeTopWeight" => 0,
                "strokeRightWeight" => 1,
                "strokeBottomWeight" => 2,
                _ => 3,
            };
            sw[i] = x.max(0.0);
        }
        "itemSpacing" | "counterAxisSpacing" | "paddingTop" | "paddingRight" | "paddingBottom" | "paddingLeft" => {
            if let Some(al) = n.kind.frame_props_mut().and_then(|f| f.auto_layout.as_mut()) {
                let x = x.max(0.0);
                match key {
                    "itemSpacing" => al.item_spacing = x,
                    "counterAxisSpacing" => al.counter_spacing = x,
                    "paddingTop" => al.padding[0] = x,
                    "paddingRight" => al.padding[1] = x,
                    "paddingBottom" => al.padding[2] = x,
                    _ => al.padding[3] = x,
                }
            }
        }
        "fontSize" | "fontWeight" | "lineHeight" | "letterSpacing" => {
            if let NodeKind::Text(t) = &mut n.kind {
                let apply = |s: &mut TextStyle| match key {
                    "fontSize" => s.font_size = x.max(1.0),
                    "fontWeight" => s.font_weight = x.round().clamp(1.0, 1000.0) as u16,
                    "lineHeight" => s.line_height = LineHeight::Pixels(x.max(0.0)),
                    _ => s.letter_spacing = x,
                };
                apply(&mut t.style);
                for r in &mut t.runs {
                    apply(&mut r.style);
                }
            }
        }
        _ => {}
    }
}

/// The node with its variable bindings applied. Borrowed when it has none.
pub fn resolve_node<'a>(doc: &Document, node: &'a Node) -> Cow<'a, Node> {
    if node.variables.bindings.is_empty() {
        return Cow::Borrowed(node);
    }
    let mut out = node.clone();
    for (key, var) in &node.variables.bindings {
        if let Some(v) = doc.resolve_variable(node.id, var) {
            set_bound_property(&mut out, key, &v);
        }
    }
    Cow::Owned(out)
}

/// [`resolve_node`] by id.
pub fn resolved_node(doc: &Document, id: NodeId) -> Option<Cow<'_, Node>> {
    doc.get(id).map(|n| resolve_node(doc, n))
}

/// A copy of the document with every binding substituted (for export and
/// tools that read raw node properties).
pub fn apply_bound_variables(doc: &Document) -> Document {
    let mut out = doc.clone();
    for (id, n) in &doc.nodes {
        if !n.variables.bindings.is_empty() {
            if let Cow::Owned(r) = resolve_node(doc, n) {
                out.nodes.insert(*id, r);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(id: &str, kind: VariableKind, light: VariableValue, dark: VariableValue) -> Variable {
        Variable {
            id: id.into(),
            name: id.into(),
            collection: "c".into(),
            kind,
            values: [("light".to_string(), light), ("dark".to_string(), dark)].into_iter().collect(),
            description: String::new(),
            scopes: vec![],
            hidden_from_publishing: false,
            key: None,
        }
    }

    fn setup() -> (Document, NodeId, NodeId, NodeId) {
        let mut d = Document::new("t");
        let page = d.add_page("P");
        let fid = d.alloc_id();
        let mut f = Node::frame(fid, Rect::new(0.0, 0.0, 100.0, 100.0));
        f.kind = NodeKind::Frame(FrameProps { auto_layout: Some(AutoLayout::default()), ..Default::default() });
        let frame = d.insert(page, f).unwrap();
        let tid = d.alloc_id();
        let text = d.insert(frame, Node::text(tid, Rect::new(0.0, 0.0, 50.0, 20.0), "hi", TextStyle::default())).unwrap();
        d.variable_collections.insert(
            "c".into(),
            VariableCollection {
                id: "c".into(),
                name: "C".into(),
                modes: vec![VariableMode { id: "light".into(), name: "Light".into() }, VariableMode { id: "dark".into(), name: "Dark".into() }],
                default_mode: "light".into(),
                variables: vec![],
                key: None,
            },
        );
        let n = |x: f32| VariableValue::Number(x);
        let vars = [
            var("bg", VariableKind::Color, VariableValue::Color(Color::WHITE), VariableValue::Color(Color::BLACK)),
            var("gap", VariableKind::Number, n(8.0), n(16.0)),
            var("w", VariableKind::Number, n(120.0), n(240.0)),
            var("op", VariableKind::Number, n(50.0), n(25.0)),
            var("size", VariableKind::Number, n(14.0), n(20.0)),
            var("txt", VariableKind::String, VariableValue::String("Light".into()), VariableValue::String("Dark".into())),
            var("show", VariableKind::Boolean, VariableValue::Boolean(true), VariableValue::Boolean(false)),
        ];
        for v in vars {
            d.variables.insert(v.id.clone(), v);
        }
        let fr = d.get_mut(frame).unwrap();
        for (k, v) in [("fills/0", "bg"), ("itemSpacing", "gap"), ("paddingLeft", "gap"), ("width", "w"), ("opacity", "op"), ("topLeftRadius", "gap"), ("strokeWeight", "gap")] {
            fr.variables.bindings.insert(k.into(), v.into());
        }
        let tx = d.get_mut(text).unwrap();
        for (k, v) in [("fontSize", "size"), ("characters", "txt"), ("visible", "show"), ("fills/0", "bg"), ("letterSpacing", "gap"), ("fontWeight", "w")] {
            tx.variables.bindings.insert(k.into(), v.into());
        }
        (d, page, frame, text)
    }

    #[test]
    fn substitutes_values_in_default_mode() {
        let (d, _, frame, text) = setup();
        let f = resolved_node(&d, frame).unwrap();
        assert_eq!(f.fills[0].kind, PaintKind::Solid(Color::WHITE));
        assert_eq!(f.size.x, 120.0);
        assert_eq!(f.opacity, 0.5);
        assert_eq!(f.corner_radii[0], 8.0);
        assert_eq!(f.stroke.weight, 8.0);
        let al = f.kind.frame_props().unwrap().auto_layout.unwrap();
        assert_eq!((al.item_spacing, al.padding[3]), (8.0, 8.0));
        let t = resolved_node(&d, text).unwrap();
        let NodeKind::Text(tp) = &t.kind else { panic!() };
        assert_eq!(tp.style.font_size, 14.0);
        assert_eq!(tp.content, "Light");
        assert_eq!(tp.style.letter_spacing, 8.0);
        assert_eq!(tp.style.font_weight, 120);
        assert_eq!(tp.style.fills[0].kind, PaintKind::Solid(Color::WHITE));
        assert!(t.visible);
        // raw node is untouched
        assert_eq!(d.get(frame).unwrap().size.x, 100.0);
    }

    #[test]
    fn mode_on_ancestor_switches_values() {
        let (mut d, _, frame, text) = setup();
        d.get_mut(frame).unwrap().variables.modes.insert("c".into(), "dark".into());
        let f = resolved_node(&d, frame).unwrap();
        assert_eq!(f.fills[0].kind, PaintKind::Solid(Color::BLACK));
        assert_eq!(f.size.x, 240.0);
        let t = resolved_node(&d, text).unwrap();
        let NodeKind::Text(tp) = &t.kind else { panic!() };
        assert_eq!(tp.content, "Dark");
        assert!(!t.visible);
    }

    #[test]
    fn unbound_nodes_are_borrowed_and_bad_bindings_ignored() {
        let (mut d, page, frame, _) = setup();
        assert!(matches!(resolved_node(&d, page).unwrap(), Cow::Borrowed(_)));
        d.get_mut(frame).unwrap().variables.bindings.insert("fills/5".into(), "bg".into());
        d.get_mut(frame).unwrap().variables.bindings.insert("width".into(), "bg".into()); // colour into a number slot
        d.get_mut(frame).unwrap().variables.bindings.insert("height".into(), "missing".into());
        let f = resolved_node(&d, frame).unwrap();
        assert_eq!(f.size.y, 100.0);
        assert_eq!(f.size.x, 100.0);
    }

    #[test]
    fn apply_bound_variables_copies_document() {
        let (mut d, _, frame, _) = setup();
        d.get_mut(frame).unwrap().variables.modes.insert("c".into(), "dark".into());
        let r = apply_bound_variables(&d);
        assert_eq!(r.get(frame).unwrap().size.x, 240.0);
        assert_eq!(d.get(frame).unwrap().size.x, 100.0);
    }

    #[test]
    fn kinds_and_scopes() {
        assert_eq!(property_kind("fills/2"), Some(VariableKind::Color));
        assert_eq!(property_kind("paddingTop"), Some(VariableKind::Number));
        assert_eq!(property_kind("nope"), None);
        let frame = NodeKind::Frame(FrameProps::default());
        let text = NodeKind::Text(TextProps {
            content: String::new(),
            style: TextStyle::default(),
            runs: vec![],
            auto_resize: TextAutoResize::None,
            max_lines: None,
        });
        assert!(scope_allows(&[], "width", &frame));
        assert!(scope_allows(&["GAP".into()], "itemSpacing", &frame));
        assert!(!scope_allows(&["GAP".into()], "width", &frame));
        assert!(scope_allows(&["FRAME_FILL".into()], "fills/0", &frame));
        assert!(!scope_allows(&["FRAME_FILL".into()], "fills/0", &text));
        assert!(scope_allows(&["TEXT_FILL".into()], "fills/0", &text));
        assert!(scope_allows(&["ALL_FILLS".into()], "fills/0", &text));
    }
}
