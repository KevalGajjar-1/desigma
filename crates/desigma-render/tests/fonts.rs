//! Font selection: bundled Inter, variable weights, substitution.

use desigma_doc::*;
use desigma_render::{render_page, resolve_font, tiny_skia::Pixmap, Viewport};

const SAMPLE: &str = "Hamburgefonstiv";

fn render_text(family: &str, weight: u16, italic: bool) -> Pixmap {
    let mut doc = Document::new("t");
    let page = doc.add_page("p");
    doc.get_mut(page).unwrap().kind = NodeKind::Page { background: Color::WHITE };
    let id = doc.alloc_id();
    let style = TextStyle { font_family: family.into(), font_weight: weight, italic, font_size: 32.0, ..Default::default() };
    let mut node = Node::text(id, Rect::new(4.0, 4.0, 400.0, 48.0), SAMPLE, style);
    if let NodeKind::Text(t) = &mut node.kind {
        t.auto_resize = TextAutoResize::WidthAndHeight;
    }
    doc.insert(page, node).unwrap();
    let mut pm = Pixmap::new(320, 56).unwrap();
    render_page(&doc, page, &Viewport::default(), &mut pm);
    pm
}

/// Total darkness (ink coverage) of a white-background render.
fn ink(pm: &Pixmap) -> u64 {
    pm.pixels().iter().map(|p| 255 - p.red() as u64).sum()
}

#[test]
fn inter_resolves_to_the_bundled_face() {
    for (w, italic) in [(400, false), (700, false), (400, true), (900, true)] {
        let r = resolve_font("Inter", w, italic).expect("Inter resolves");
        assert_eq!(r.family, "Inter");
        assert!(r.bundled, "Inter {w} italic={italic} should be the bundled face");
    }
}

#[test]
fn sf_pro_text_renders_through_inter() {
    let r = resolve_font("SF Pro Text", 400, false).unwrap();
    assert_eq!((r.family.as_str(), r.bundled), ("Inter", true));
    for fam in ["SF Pro Display", "-apple-system", "system-ui"] {
        assert_eq!(resolve_font(fam, 600, false).unwrap().family, "Inter", "{fam}");
    }
    let a = render_text("SF Pro Text", 500, false);
    let b = render_text("Inter", 500, false);
    assert_eq!(a.data(), b.data(), "SF Pro Text must be drawn with Inter");
}

#[test]
fn unknown_family_falls_back_to_inter() {
    let r = resolve_font("Definitely Not A Font", 400, false).unwrap();
    assert_eq!((r.family.as_str(), r.bundled), ("Inter", true));
    assert_eq!(render_text("Definitely Not A Font", 400, false).data(), render_text("Inter", 400, false).data());
}

#[test]
fn variable_weight_axis_is_applied() {
    let (w400, w700, w900) = (ink(&render_text("Inter", 400, false)), ink(&render_text("Inter", 700, false)), ink(&render_text("Inter", 900, false)));
    assert!(w400 > 0);
    assert!(w700 > w400 + w400 / 10, "700 should be clearly heavier: {w400} vs {w700}");
    assert!(w900 > w700, "900 heavier than 700: {w700} vs {w900}");
    let w100 = ink(&render_text("Inter", 100, false));
    assert!(w100 < w400, "100 lighter than 400: {w100} vs {w400}");
}

#[test]
fn italic_uses_the_italic_face() {
    let normal = render_text("Inter", 400, false);
    let italic = render_text("Inter", 400, true);
    assert_ne!(normal.data(), italic.data());
}

#[test]
fn rendering_is_deterministic() {
    assert_eq!(render_text("Inter", 600, false).data(), render_text("Inter", 600, false).data());
}
