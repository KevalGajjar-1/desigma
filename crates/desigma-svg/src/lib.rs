//! SVG import for desigma.
//!
//! [`import_svg`] parses SVG markup with `usvg` (which resolves styles, `use`,
//! units, `viewBox`, gradients and so on into a small normalized tree) and maps
//! that tree onto desigma nodes the way Figma does when you paste an SVG:
//!
//! * the root becomes a clipping **Frame** sized to the SVG (`width`/`height`,
//!   else the `viewBox`), named after the file (or `"svg"`);
//! * groups become **Group** nodes with their transform, opacity and blend
//!   mode; groups that only carry a transform (or a single leaf's attributes)
//!   are folded into their child so one `<rect>` is one layer;
//! * paths become **Vector** nodes (`MoveTo/LineTo/QuadTo/CubicTo/Close`, fill
//!   rule, stroke weight/cap/join/miter limit/dashes). `<rect>`, `<circle>` and
//!   `<ellipse>` elements are detected with a `roxmltree` pre-scan of the XML
//!   and become **Rectangle** (with corner radius) / **Ellipse** nodes unless
//!   their transform skews or the radii are elliptical;
//! * solid colors, linear and radial gradients (re-expressed as Figma-style
//!   node-normalized handles), embedded `data:` images (Rectangle with an image
//!   fill, bytes stored in [`Document::images`]);
//! * `<text>` becomes **Text** nodes (see below), `feDropShadow`,
//!   `feGaussianBlur` and the feOffset/feGaussianBlur/feColorMatrix chains that
//!   Figma and desigma itself export become drop/inner shadows and layer blur;
//! * a `clipPath` that is a single rectangle or circle becomes a clipping
//!   Frame, any other `clipPath` and every `<mask>` become mask nodes (a mask
//!   node masks its following siblings in desigma).
//!
//! # Text
//!
//! `usvg` is built with its `text` feature (adds `fontdb`, `rustybuzz` and
//! friends, all pure Rust and already part of the resvg stack). Text is *not*
//! flattened to outlines: the glyph positions that usvg lays out are used to
//! find each line's baseline, while the characters and style (family, size,
//! weight, italic, letter spacing, decoration, fill, `text-anchor`) go into
//! editable Text nodes. Only the bundled Inter face is loaded into usvg's font
//! database (every generic family and every unknown family resolves to it), so
//! import never touches system fonts and is deterministic. Text on a path and
//! per-glyph rotation fall back to the flattened outlines as Vector nodes.
//!
//! # Known gaps
//!
//! Pattern fills other than a single image fall back to the first solid color
//! inside the pattern; spread methods other than `pad` and radial focal points
//! are ignored; nested `<svg>` images are skipped; filters other than the
//! shadow/blur forms above are ignored; a `clipPath` with its own `clip-path`
//! only applies the outer clip; `stroke-dashoffset`, markers and paint order
//! are not represented.

mod convert;
mod effects;
mod paint;
mod scan;
mod text;

use desigma_doc::{Affine, Document, Node, NodeId, Vec2};
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

#[derive(Debug, thiserror::Error)]
pub enum SvgError {
    #[error("not a valid SVG: {0}")]
    Parse(String),
    #[error("the SVG has no drawable size")]
    ZeroSize,
    #[error(transparent)]
    Doc(#[from] desigma_doc::DocError),
}

/// A converted SVG: a root-first subtree plus the images it embeds.
#[derive(Clone, Debug)]
pub struct SvgNodes {
    /// Root first (pre-order). The root has `parent: None`; every other node
    /// already points at its parent and the parents list their `children`.
    /// Suitable for `Command::Insert { nodes, .. }`.
    pub nodes: Vec<Node>,
    /// Image bytes to merge into [`Document::images`], keyed by `image_ref`.
    pub images: BTreeMap<String, Vec<u8>>,
}

impl SvgNodes {
    pub fn root(&self) -> &Node {
        &self.nodes[0]
    }
}

/// Whether clipboard text looks like SVG markup (`<svg ...` or an XML prolog).
pub fn looks_like_svg(text: &str) -> bool {
    let t = text.trim_start_matches('\u{feff}').trim_start();
    if t.starts_with("<svg") {
        return true;
    }
    t.starts_with("<?xml") && t.contains("<svg")
}

/// Converts `svg` into a subtree named `name` (a Frame), allocating node ids
/// from `alloc`. Nothing is inserted anywhere: the caller owns placement, so
/// the app can insert it with one undoable command. The root sits at (0, 0).
pub fn svg_to_nodes(svg: &str, name: &str, alloc: &mut dyn FnMut() -> NodeId) -> Result<SvgNodes, SvgError> {
    let scanned = scan::prescan(svg);
    let opt = options();
    let tree = usvg::Tree::from_str(&scanned.svg, &opt).map_err(|e| SvgError::Parse(e.to_string()))?;
    let size = tree.size();
    if !(size.width() > 0.0 && size.height() > 0.0) {
        return Err(SvgError::ZeroSize);
    }
    Ok(convert::convert_tree(&tree, name, scanned.shapes, alloc))
}

/// Imports `svg` as a Frame named `"svg"` under `parent`, top-left at `at`
/// (in the parent's coordinates). Returns the new root.
pub fn import_svg(svg: &str, doc: &mut Document, parent: NodeId, at: Vec2) -> Result<NodeId, SvgError> {
    import_svg_named(svg, "svg", doc, parent, at)
}

/// Like [`import_svg`] with an explicit layer name (usually the file name).
pub fn import_svg_named(svg: &str, name: &str, doc: &mut Document, parent: NodeId, at: Vec2) -> Result<NodeId, SvgError> {
    let mut out = svg_to_nodes(svg, name, &mut || doc.alloc_id())?;
    if let Some(root) = out.nodes.first_mut() {
        root.transform = Affine::translate(at.x, at.y);
    }
    doc.images.append(&mut out.images);
    let mut nodes = out.nodes.into_iter();
    let root = nodes.next().expect("converter always emits a root");
    let root_id = doc.insert(parent, root)?;
    for n in nodes {
        doc.nodes.insert(n.id, n);
    }
    Ok(root_id)
}

/// usvg options shared by every import: bundled font only, `data:` images only.
fn options() -> usvg::Options<'static> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    let fontdb = FONTS
        .get_or_init(|| {
            let mut db = usvg::fontdb::Database::new();
            for asset in desigma_fonts::bundled() {
                db.load_font_data(asset.data.to_vec());
            }
            db.set_serif_family("Inter");
            db.set_sans_serif_family("Inter");
            db.set_monospace_family("Inter");
            db.set_cursive_family("Inter");
            db.set_fantasy_family("Inter");
            Arc::new(db)
        })
        .clone();
    let mut opt = usvg::Options { fontdb, font_family: "Inter".into(), ..Default::default() };
    // Never read local files named by an untrusted `href`.
    opt.image_href_resolver.resolve_string = Box::new(|_, _| None);
    opt
}
