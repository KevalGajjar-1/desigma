//! Shared styles (grouping, applying) and selection colors.

use crate::app::App;
use crate::edit::Edit;
use crate::layout;
use desigma_doc::*;

/// A style as listed in the UI: display name split at the last "/" into a
/// folder and a leaf, like Figma's style folders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleEntry {
    pub key: String,
    pub folder: String,
    pub leaf: String,
}

pub fn split_name(name: &str) -> (String, String) {
    match name.rsplit_once('/') {
        Some((f, l)) => (f.trim().to_string(), l.trim().to_string()),
        None => (String::new(), name.trim().to_string()),
    }
}

/// Text styles (`text == true`) or paint styles, ordered by folder then name.
pub fn style_entries(doc: &Document, text: bool) -> Vec<StyleEntry> {
    let mut out: Vec<StyleEntry> = doc
        .styles
        .iter()
        .filter(|(_, s)| matches!(s, Style::Text(_)) == text && (text || matches!(s, Style::Fill(_))))
        .map(|(k, _)| {
            let (folder, leaf) = split_name(doc.style_name(k));
            StyleEntry { key: k.clone(), folder, leaf }
        })
        .collect();
    // Root-level styles first, then folders alphabetically.
    out.sort_by(|a, b| {
        (!a.folder.is_empty(), a.folder.to_lowercase(), a.leaf.to_lowercase()).cmp(&(!b.folder.is_empty(), b.folder.to_lowercase(), b.leaf.to_lowercase()))
    });
    out
}

fn property_of(doc: &Document, key: &str) -> StyleProperty {
    doc.style_meta.get(key).map(|m| m.property).unwrap_or(match doc.styles.get(key) {
        Some(Style::Text(_)) => StyleProperty::Text,
        Some(Style::Effect(_)) => StyleProperty::Effect,
        Some(Style::Grid(_)) => StyleProperty::Grid,
        _ => StyleProperty::Fill,
    })
}

impl App {
    /// Applies the shared style `key` to `sel`: records the reference in
    /// `style_refs` and copies the style's values onto the nodes.
    pub fn apply_style(&mut self, sel: &[NodeId], key: &str) {
        let mut tx = crate::edit::Tx::new();
        self.apply_style_in(&mut tx, sel, key);
        self.run_tx(tx);
    }

    /// [`App::apply_style`] inside a caller-owned transaction.
    pub fn apply_style_in(&mut self, tx: &mut crate::edit::Tx, sel: &[NodeId], key: &str) {
        let Some(style) = self.doc.styles.get(key).cloned() else { return };
        let prop = property_of(&self.doc, key);
        let f = |n: &mut Node| match &style {
            Style::Fill(paints) if prop == StyleProperty::Stroke => {
                n.stroke.paints = paints.clone();
                n.style_refs.insert("stroke".into(), key.into());
            }
            Style::Fill(paints) => {
                match &mut n.kind {
                    NodeKind::Text(t) => t.style.fills = paints.clone(),
                    _ => n.fills = paints.clone(),
                }
                n.style_refs.insert("fill".into(), key.into());
            }
            Style::Text(ts) => {
                if let NodeKind::Text(t) = &mut n.kind {
                    // A text style carries no colour: keep the node's own.
                    t.style = TextStyle { fills: t.style.fills.clone(), ..ts.clone() };
                    for r in &mut t.runs {
                        r.style = TextStyle { fills: r.style.fills.clone(), ..ts.clone() };
                    }
                    n.size = layout::fit_text(t, n.size);
                    n.style_refs.insert("text".into(), key.into());
                }
            }
            Style::Effect(fx) => {
                n.effects = fx.clone();
                n.style_refs.insert("effect".into(), key.into());
            }
            Style::Grid(g) => {
                if let Some(f) = n.kind.frame_props_mut() {
                    f.grids = g.clone();
                    n.style_refs.insert("grid".into(), key.into());
                }
            }
        };
        for id in sel {
            if let Some(n) = tx.ed.node(&mut self.doc, *id) {
                f(n);
            }
            layout::relayout_around(&mut self.doc, &mut tx.ed, *id);
        }
    }

    /// Replaces solid colour `old` by `new` in the whole subtree of the selection (one undo step).
    pub fn replace_selection_color(&mut self, sel: &[NodeId], old: Color, new: Color) {
        let mut tx = crate::edit::Tx::new();
        replace_color(&mut self.doc, &mut tx.ed, sel, old, new);
        self.run_tx(tx);
    }
}

fn same_rgb(a: &Color, b: &Color) -> bool {
    a.to_hex() == b.to_hex()
}

fn paint_lists(n: &Node) -> Vec<&Vec<Paint>> {
    let mut v = vec![&n.fills, &n.stroke.paints];
    if let NodeKind::Text(t) = &n.kind {
        v.push(&t.style.fills);
        v.extend(t.runs.iter().map(|r| &r.style.fills));
    }
    v
}

/// Unique solid colours (opaque RGB) used by the subtrees of `roots`, in order of first use.
pub fn selection_colors(doc: &Document, roots: &[NodeId]) -> Vec<Color> {
    let mut out: Vec<Color> = Vec::new();
    for r in roots {
        doc.walk(*r, |n, _| {
            for list in paint_lists(n) {
                for p in list {
                    if let PaintKind::Solid(c) = &p.kind {
                        let c = Color { a: 1.0, ..*c };
                        if !out.iter().any(|o| same_rgb(o, &c)) {
                            out.push(c);
                        }
                    }
                }
            }
        });
    }
    out
}

/// Replaces solid colour `old` with `new` (alpha of each paint is kept) in the subtrees of `roots`.
pub fn replace_color(doc: &mut Document, ed: &mut Edit, roots: &[NodeId], old: Color, new: Color) {
    let mut ids = Vec::new();
    for r in roots {
        doc.walk(*r, |n, _| ids.push(n.id));
    }
    let swap = |list: &mut Vec<Paint>| {
        for p in list {
            if let PaintKind::Solid(c) = &mut p.kind {
                if same_rgb(c, &old) {
                    *c = Color { a: c.a, ..new };
                }
            }
        }
    };
    for id in ids {
        let has =
            doc.get(id).map(|n| paint_lists(n).iter().any(|l| l.iter().any(|p| matches!(&p.kind, PaintKind::Solid(c) if same_rgb(c, &old))))).unwrap_or(false);
        if !has {
            continue;
        }
        if let Some(n) = ed.node(doc, id) {
            swap(&mut n.fills);
            swap(&mut n.stroke.paints);
            if let NodeKind::Text(t) = &mut n.kind {
                swap(&mut t.style.fills);
                for r in &mut t.runs {
                    swap(&mut r.style.fills);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> (App, NodeId, NodeId, NodeId) {
        let mut d = Document::new("t");
        let page = d.add_page("p");
        let fid = d.alloc_id();
        let mut f = Node::frame(fid, Rect::new(0.0, 0.0, 200.0, 200.0));
        f.fills = vec![Paint::solid(Color::rgb(255, 0, 0))];
        let f = d.insert(page, f).unwrap();
        let rid = d.alloc_id();
        let mut r = Node::rectangle(rid, Rect::new(10.0, 10.0, 50.0, 50.0));
        r.fills = vec![Paint::solid(Color::rgb(255, 0, 0)), Paint::solid(Color::rgb(0, 0, 255))];
        r.stroke.paints = vec![Paint::solid(Color::rgb(0, 255, 0))];
        let r = d.insert(f, r).unwrap();
        let tid = d.alloc_id();
        let t = d
            .insert(
                f,
                Node::text(tid, Rect::new(0.0, 100.0, 50.0, 20.0), "Hi", TextStyle { fills: vec![Paint::solid(Color::rgb(0, 0, 255))], ..Default::default() }),
            )
            .unwrap();
        d.styles.insert("Brand/Primary".into(), Style::Fill(vec![Paint::solid(Color::rgb(1, 2, 3))]));
        d.styles.insert("s2".into(), Style::Text(TextStyle { font_size: 40.0, font_weight: 700, ..Default::default() }));
        d.style_meta.insert("s2".into(), StyleMeta { name: "Heading/Large".into(), ..Default::default() });
        (App::new(d), f, r, t)
    }

    #[test]
    fn entries_use_display_names_and_group_by_prefix() {
        let (app, ..) = app();
        let fills = style_entries(&app.doc, false);
        assert_eq!(fills, vec![StyleEntry { key: "Brand/Primary".into(), folder: "Brand".into(), leaf: "Primary".into() }]);
        let texts = style_entries(&app.doc, true);
        assert_eq!(texts[0].key, "s2");
        assert_eq!((texts[0].folder.as_str(), texts[0].leaf.as_str()), ("Heading", "Large"));
        assert_eq!(split_name("Plain"), (String::new(), "Plain".to_string()));
        assert_eq!(split_name("a/b/c"), ("a/b".to_string(), "c".to_string()));
    }

    #[test]
    fn applying_fill_style_sets_ref_and_values_and_undoes() {
        let (mut app, _f, r, t) = app();
        app.apply_style(&[r, t], "Brand/Primary");
        let rn = app.doc.get(r).unwrap();
        assert_eq!(rn.style_refs.get("fill").map(String::as_str), Some("Brand/Primary"));
        assert_eq!(rn.fills, vec![Paint::solid(Color::rgb(1, 2, 3))]);
        match &app.doc.get(t).unwrap().kind {
            NodeKind::Text(tp) => assert_eq!(tp.style.fills, vec![Paint::solid(Color::rgb(1, 2, 3))]),
            _ => unreachable!(),
        }
        app.undo();
        assert!(app.doc.get(r).unwrap().style_refs.is_empty());
        assert_eq!(app.doc.get(r).unwrap().fills.len(), 2);
    }

    #[test]
    fn applying_text_style_keeps_color_and_skips_shapes() {
        let (mut app, _f, r, t) = app();
        app.apply_style(&[r, t], "s2");
        assert!(app.doc.get(r).unwrap().style_refs.is_empty());
        let n = app.doc.get(t).unwrap();
        assert_eq!(n.style_refs.get("text").map(String::as_str), Some("s2"));
        match &n.kind {
            NodeKind::Text(tp) => {
                assert_eq!(tp.style.font_size, 40.0);
                assert_eq!(tp.style.fills, vec![Paint::solid(Color::rgb(0, 0, 255))]);
                assert!(n.size.y >= 40.0, "auto-resized to the new size");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn selection_colors_are_unique_across_subtree() {
        let (app, f, ..) = app();
        let cols: Vec<String> = selection_colors(&app.doc, &[f]).iter().map(|c| c.to_hex()).collect();
        assert_eq!(cols, vec!["FF0000", "0000FF", "00FF00"]);
    }

    #[test]
    fn replacing_a_selection_color_edits_everywhere_in_one_step() {
        let (mut app, f, r, t) = app();
        app.replace_selection_color(&[f], Color::rgb(255, 0, 0), Color::rgb(9, 9, 9));
        assert_eq!(app.doc.get(f).unwrap().fills[0].kind, PaintKind::Solid(Color::rgb(9, 9, 9)));
        assert_eq!(app.doc.get(r).unwrap().fills[0].kind, PaintKind::Solid(Color::rgb(9, 9, 9)));
        assert_eq!(app.doc.get(r).unwrap().fills[1].kind, PaintKind::Solid(Color::rgb(0, 0, 255)));
        app.replace_selection_color(&[f], Color::rgb(0, 0, 255), Color::rgb(7, 7, 7));
        match &app.doc.get(t).unwrap().kind {
            NodeKind::Text(tp) => assert_eq!(tp.style.fills[0].kind, PaintKind::Solid(Color::rgb(7, 7, 7))),
            _ => unreachable!(),
        }
        app.undo();
        app.undo();
        assert_eq!(app.doc.get(f).unwrap().fills[0].kind, PaintKind::Solid(Color::rgb(255, 0, 0)));
        assert!(!app.hist.can_undo());
    }
}
