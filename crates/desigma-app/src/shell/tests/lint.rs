//! Layout lint: runs the app headless in several states and checks that no painted
//! text overflows the widget it sits in and that icons are centered in their square
//! buttons (see `crate::lint`).

use super::*;
use crate::lint;

fn audit(label: &str, opts: Options) -> Vec<lint::Finding> {
    lint::take_icons();
    let mut s = Sim::with(opts);
    s.frame(vec![]);
    lint::take_icons();
    s.frame(vec![]);
    let (texts, boxes) = lint::scan_shapes(&s.shapes);
    // The audit must really see the screen: a lot of text and rects.
    assert!(texts.len() > 15 && boxes.len() > 15, "[{label}] only {} texts / {} boxes painted", texts.len(), boxes.len());
    if label.contains("picker") {
        assert!(texts.iter().any(|t| t.text == "Custom"), "[{label}] picker not painted");
    }
    let mut out = lint::check_text(&texts, &boxes);
    out.extend(lint::check_icons(&lint::take_icons()));
    for f in &mut out {
        f.what = format!("[{label}] {}", f.what);
    }
    out
}

fn states() -> Vec<(&'static str, Options)> {
    let sel = |n: &str| Options { select: Some(n.into()), ..Options::default() };
    vec![
        ("no selection", Options { select: Some("none".into()), ..Options::default() }),
        ("frame", sel("Welcome")),
        ("text", sel("Continue")),
        ("instance", sel("Get started")),
        ("rectangle", sel("Card")),
        ("fill picker", Options { select: Some("Card".into()), panel: Some("picker".into()), ..Options::default() }),
        ("gradient picker", Options { select: Some("Hero".into()), panel: Some("picker-gradient".into()), ..Options::default() }),
        ("effects menu", Options { select: Some("Hero".into()), panel: Some("effects-menu".into()), ..Options::default() }),
        ("stroke settings", Options { select: Some("Hero".into()), panel: Some("stroke-settings".into()), ..Options::default() }),
        ("noise settings", Options { select: Some("Hero".into()), panel: Some("effect-noise".into()), ..Options::default() }),
        ("texture settings", Options { select: Some("Hero".into()), panel: Some("effect-texture".into()), ..Options::default() }),
        ("glass settings", Options { select: Some("Hero".into()), panel: Some("effect-glass".into()), ..Options::default() }),
        ("main menu", Options { menu: Some("main".into()), ..Options::default() }),
        ("shape menu", Options { menu: Some("shape".into()), ..Options::default() }),
    ]
}

#[test]
fn nothing_overflows_and_icons_are_centered() {
    let mut all = Vec::new();
    for (label, opts) in states() {
        all.extend(audit(label, opts));
    }
    for f in &all {
        eprintln!("LINT {} :: {}", f.what, f.detail);
    }
    assert!(all.is_empty(), "{} layout findings", all.len());
}
