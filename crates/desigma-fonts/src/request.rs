use desigma_doc::{Document, NodeKind, TextStyle};
use std::collections::BTreeSet;

/// One font face a document needs.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontRequest {
    pub family: String,
    /// CSS weight 100..=900.
    pub weight: u16,
    pub italic: bool,
}

impl FontRequest {
    pub fn new(family: impl Into<String>, weight: u16, italic: bool) -> Self {
        Self { family: family.into(), weight, italic }
    }

    fn of(style: &TextStyle) -> Self {
        Self::new(style.font_family.clone(), style.font_weight, style.italic)
    }
}

/// Every family/weight/italic combination used by text nodes and their runs.
pub fn requests_for(doc: &Document) -> BTreeSet<FontRequest> {
    let mut out = BTreeSet::new();
    for node in doc.nodes.values() {
        if let NodeKind::Text(t) = &node.kind {
            out.insert(FontRequest::of(&t.style));
            for run in &t.runs {
                out.insert(FontRequest::of(&run.style));
            }
        }
    }
    out
}
