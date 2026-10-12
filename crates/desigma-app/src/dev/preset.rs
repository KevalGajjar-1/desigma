//! Screenshot aids: `--dev --dev-demo` and `--panel dev-annotate` / `dev-compare`.

use super::{AnnotEdit, Language};
use crate::app::App;
use desigma_doc::*;

/// A Ready for dev layer with an annotation and a link.
pub fn apply(app: &mut App) {
    let Some(id) = app.selection.first().copied() else { return };
    app.dev_mark_ready(&[id]);
    app.dev_edit(id, |d| {
        d.annotations.push(Annotation {
            text: "Keep 20px padding on every side".into(),
            properties: vec!["Width".into(), "Fill".into(), "Padding".into()],
            category: None,
        });
        d.links.push(DevLink { label: "Storybook".into(), url: "https://storybook.example.com/?path=/docs/card".into() });
    });
    app.toast = None;
}

/// `dev-annotate`: the annotation editor open; `dev-compare`: an edited Ready for dev layer with
/// the Compare changes view open.
pub fn panel(app: &mut App, name: &str) {
    app.ui.dev_mode = true;
    app.ui.right_tab = Some(crate::app::RightTab::Design);
    app.dev.lang = Language::SwiftUi;
    if app.selection.is_empty() {
        if let Some(id) = app.doc.nodes.values().find(|n| n.name == "Card").map(|n| n.id) {
            app.select_only(id);
        }
    }
    let Some(id) = app.selection.first().copied() else { return };
    match name {
        "dev-annotate" => {
            apply(app);
            app.dev.annot = Some(AnnotEdit { node: id, index: None, text: "Use the brand blue".into(), props: vec!["Width".into(), "Fill".into()] });
        }
        _ => {
            app.dev_mark_ready(&[id]);
            let child = app.doc.get(id).and_then(|n| n.children.first().copied());
            app.edit_nodes(&[id], |n| {
                n.size.x += 40.0;
                n.fills = vec![Paint::solid(Color::rgb(0xFF, 0xF1, 0xD6))];
            });
            if let Some(c) = child {
                app.edit_nodes(&[c], |n| {
                    if let NodeKind::Text(t) = &mut n.kind {
                        t.content = "Quarterly summary".into();
                    }
                });
            }
            app.dev_refresh(true);
            crate::dev::compare::open(app, id);
            app.toast = None;
        }
    }
}
