//! Render fidelity: import hand-written SVG fixtures, render the result with
//! `desigma_render`, and compare against `resvg` drawing the original markup.
//!
//! A pixel mismatches when any RGBA channel differs by more than `TOLERANCE`;
//! every fixture must stay under `MAX_MISMATCH`. Run with `--nocapture` for
//! the table.

use desigma_doc::*;
use desigma_svg::import_svg_named;
use std::sync::Arc;

const TOLERANCE: i32 = 8;
const MAX_MISMATCH: f64 = 0.02;

pub fn fonts() -> Arc<usvg::fontdb::Database> {
    let mut db = usvg::fontdb::Database::new();
    for a in desigma_fonts::bundled() {
        db.load_font_data(a.data.to_vec());
    }
    for set in [usvg::fontdb::Database::set_serif_family, usvg::fontdb::Database::set_sans_serif_family, usvg::fontdb::Database::set_monospace_family] {
        set(&mut db, "Inter");
    }
    Arc::new(db)
}

pub fn resvg_render(svg: &str) -> tiny_skia::Pixmap {
    let opt = usvg::Options { fontdb: fonts(), font_family: "Inter".into(), ..Default::default() };
    let tree = usvg::Tree::from_str(svg, &opt).expect("fixture parses");
    let size = tree.size().to_int_size();
    let mut pm = tiny_skia::Pixmap::new(size.width(), size.height()).unwrap();
    resvg::render(&tree, tiny_skia::Transform::identity(), &mut pm.as_mut());
    pm
}

pub fn desigma_render_svg(svg: &str) -> (tiny_skia::Pixmap, Document, NodeId) {
    let mut doc = Document::new("t");
    let page = doc.add_page("p");
    let root = import_svg_named(svg, "fixture", &mut doc, page, Vec2::ZERO).expect("fixture imports");
    let pm = desigma_render::render_node(&doc, root, 1.0).expect("renders");
    (pm, doc, root)
}

pub fn mismatch(a: &tiny_skia::Pixmap, b: &tiny_skia::Pixmap) -> f64 {
    assert_eq!((a.width(), a.height()), (b.width(), b.height()), "render sizes differ");
    let bad = a
        .data()
        .chunks_exact(4)
        .zip(b.data().chunks_exact(4))
        .filter(|(p, q)| p.iter().zip(q.iter()).any(|(x, y)| (*x as i32 - *y as i32).abs() > TOLERANCE))
        .count();
    bad as f64 / (a.width() * a.height()) as f64
}

fn lucide(inner: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96" viewBox="0 0 24 24" fill="none" stroke="#1f2937" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{inner}</svg>"##
    )
}

pub fn fixtures() -> Vec<(&'static str, String)> {
    vec![
        ("icon heart", lucide(r#"<path d="M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z"/>"#)),
        ("icon check-circle", lucide(r#"<circle cx="12" cy="12" r="10"/><path d="m9 12 2 2 4-4"/>"#)),
        ("icon arrow-right", lucide(r#"<path d="M5 12h14"/><path d="m12 5 7 7-7 7"/>"#)),
        ("icon settings-ish", lucide(r#"<rect x="3" y="3" width="18" height="18" rx="4"/><circle cx="12" cy="12" r="3"/><path d="M12 3v3M12 18v3M3 12h3M18 12h3" stroke-dasharray="2 1"/>"#)),
        (
            "logo gradients",
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="160" height="160" viewBox="0 0 160 160">
  <defs>
    <linearGradient id="g1" x1="0" y1="0" x2="160" y2="160" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#7c3aed"/><stop offset="1" stop-color="#06b6d4"/></linearGradient>
    <radialGradient id="g2" cx="0.35" cy="0.3" r="0.8"><stop offset="0" stop-color="#fde68a"/><stop offset="0.6" stop-color="#f59e0b"/><stop offset="1" stop-color="#b45309" stop-opacity="0.9"/></radialGradient>
    <linearGradient id="g3" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#ffffff" stop-opacity="0.9"/><stop offset="1" stop-color="#ffffff" stop-opacity="0.1"/></linearGradient>
  </defs>
  <rect x="8" y="8" width="144" height="144" rx="32" fill="url(#g1)"/>
  <circle cx="80" cy="80" r="44" fill="url(#g2)"/>
  <path d="M80 44 L90 70 L118 72 L96 90 L103 117 L80 102 L57 117 L64 90 L42 72 L70 70 Z" fill="url(#g3)" stroke="#ffffff" stroke-width="2" stroke-linejoin="round"/>
</svg>"##
                .into(),
        ),
        (
            "ui card shadow",
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="240" height="160" viewBox="0 0 240 160" fill="none">
  <g filter="url(#shadow)">
    <rect x="24" y="16" width="192" height="108" rx="14" fill="white"/>
  </g>
  <rect x="44" y="92" width="76" height="20" rx="6" fill="#4F46E5"/>
  <rect x="44" y="34" width="40" height="40" rx="20" fill="#E0E7FF"/>
  <rect x="96" y="40" width="96" height="8" rx="4" fill="#CBD5E1"/>
  <rect x="96" y="58" width="64" height="8" rx="4" fill="#E2E8F0"/>
  <defs>
    <filter id="shadow" x="0" y="0" width="240" height="160" filterUnits="userSpaceOnUse" color-interpolation-filters="sRGB">
      <feFlood flood-opacity="0" result="BackgroundImageFix"/>
      <feColorMatrix in="SourceAlpha" type="matrix" values="0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 127 0" result="hardAlpha"/>
      <feOffset dy="6"/>
      <feGaussianBlur stdDeviation="8"/>
      <feComposite in2="hardAlpha" operator="out"/>
      <feColorMatrix type="matrix" values="0 0 0 0 0.06 0 0 0 0 0.09 0 0 0 0 0.16 0 0 0 0.28 0"/>
      <feBlend mode="normal" in2="BackgroundImageFix" result="effect1_dropShadow"/>
      <feBlend mode="normal" in="SourceGraphic" in2="effect1_dropShadow" result="shape"/>
    </filter>
  </defs>
</svg>"##
                .into(),
        ),
        (
            "text",
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="260" height="120" viewBox="0 0 260 120">
  <rect width="260" height="120" fill="#f8fafc"/>
  <text x="16" y="38" font-family="Inter" font-size="28" fill="#0f172a">Hello, desigma</text>
  <text x="16" y="66" font-family="Inter" font-size="14" fill="#475569">Pixel-perfect vector editor</text>
  <text x="244" y="100" font-family="Inter" font-size="16" text-anchor="end" fill="#4f46e5">Right aligned</text>
</svg>"##
                .into(),
        ),
        (
            "clip and mask",
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="120" viewBox="0 0 120 120">
  <defs>
    <clipPath id="c"><circle cx="40" cy="40" r="30"/></clipPath>
    <clipPath id="t"><path d="M70 60 L110 60 L90 105 Z"/></clipPath>
    <mask id="m"><rect width="120" height="120" fill="#000"/><rect x="0" y="80" width="60" height="30" fill="#fff"/></mask>
  </defs>
  <g clip-path="url(#c)"><rect width="120" height="120" fill="#ef4444"/><rect x="40" width="60" height="120" fill="#3b82f6"/></g>
  <g clip-path="url(#t)"><rect x="60" y="50" width="60" height="60" fill="#10b981"/></g>
  <rect y="70" width="120" height="50" fill="#f59e0b" mask="url(#m)"/>
</svg>"##
                .into(),
        ),
    ]
}

#[test]
fn imported_fixtures_render_like_resvg() {
    let mut rows = Vec::new();
    for (name, svg) in fixtures() {
        let reference = resvg_render(&svg);
        let (ours, _, _) = desigma_render_svg(&svg);
        let ratio = mismatch(&reference, &ours);
        rows.push((name, ratio));
    }
    println!("\n{:<22} {:>10}", "fixture", "mismatch");
    for (name, r) in &rows {
        println!("{name:<22} {:>9.3}%", r * 100.0);
    }
    for (name, r) in rows {
        assert!(r < MAX_MISMATCH, "{name}: {:.3}% of pixels differ", r * 100.0);
    }
}

/// `DUMP_DIR=... cargo test -p desigma-svg --test fidelity dump_pngs -- --ignored`
/// writes reference/ours PNG pairs for eyeballing.
#[test]
#[ignore]
fn dump_pngs() {
    let dir = std::env::var("DUMP_DIR").expect("set DUMP_DIR");
    for (name, svg) in fixtures() {
        let n = name.replace(' ', "_");
        resvg_render(&svg).save_png(format!("{dir}/{n}_ref.png")).unwrap();
        desigma_render_svg(&svg).0.save_png(format!("{dir}/{n}_ours.png")).unwrap();
    }
}
