//! Layout lint (tests only): runs the app headless and checks that nothing painted
//! overflows its container and that icons sit centered in their square buttons.
//!
//! Text is audited from the frame's painted shapes (every `Shape::Text` against the
//! smallest filled/stroked rect that contains its start). Icons are recorded by a
//! hook in `icons::paint`; `widgets::icon_button` and friends declare the box an icon
//! belongs to with [`with_box`].

use egui::Rect;
use std::cell::RefCell;

#[derive(Clone, Debug)]
pub struct IconRec {
    pub name: String,
    pub glyph: Rect,
    /// The button / field box the icon is painted in, when declared.
    pub boxr: Option<Rect>,
}

thread_local! {
    static ICONS: RefCell<Vec<IconRec>> = const { RefCell::new(Vec::new()) };
    static BOX: RefCell<Option<Rect>> = const { RefCell::new(None) };
}

/// Called by `icons::paint` for every glyph.
pub fn note_icon(_p: &egui::Painter, icon: crate::icons::Icon, glyph: Rect) {
    let boxr = BOX.with(|b| *b.borrow());
    ICONS.with(|v| v.borrow_mut().push(IconRec { name: format!("{icon:?}"), glyph, boxr }));
}

/// Declares that icons painted inside `f` belong to the widget rect `boxr`.
pub fn with_box<R>(boxr: Rect, f: impl FnOnce() -> R) -> R {
    let prev = BOX.with(|b| b.replace(Some(boxr)));
    let r = f();
    BOX.with(|b| *b.borrow_mut() = prev);
    r
}

pub fn take_icons() -> Vec<IconRec> {
    ICONS.with(|v| std::mem::take(&mut *v.borrow_mut()))
}

#[derive(Clone, Debug)]
pub struct Finding {
    pub what: String,
    pub detail: String,
}

fn contains_tol(outer: Rect, inner: Rect, tol: f32) -> bool {
    inner.left() >= outer.left() - tol && inner.right() <= outer.right() + tol && inner.top() >= outer.top() - tol && inner.bottom() <= outer.bottom() + tol
}

/// Checks recorded icons: every icon declared inside a square-ish button must be
/// centered within 0.5px and must not leave the box.
pub fn check_icons(recs: &[IconRec]) -> Vec<Finding> {
    let mut out = Vec::new();
    for r in recs {
        let Some(b) = r.boxr else { continue };
        let d = r.glyph.center() - b.center();
        if (b.width() - b.height()).abs() < 0.5 && (d.x.abs() > 0.5 || d.y.abs() > 0.5) {
            out.push(Finding { what: format!("icon {} off-center", r.name), detail: format!("glyph {:?} box {:?} delta {:?}", r.glyph, b, d) });
        }
        if !contains_tol(b, r.glyph, 0.5) {
            out.push(Finding { what: format!("icon {} outside its box", r.name), detail: format!("glyph {:?} box {:?}", r.glyph, b) });
        }
    }
    out
}

/// One painted text run.
#[derive(Clone, Debug)]
pub struct TextRun {
    pub text: String,
    pub rect: Rect,
    pub clip: Rect,
}

/// Collects every text run and every container-like rect from a frame's shapes.
pub fn scan_shapes(shapes: &[egui::epaint::ClippedShape]) -> (Vec<TextRun>, Vec<Rect>) {
    let mut texts = Vec::new();
    let mut boxes = Vec::new();
    fn walk(s: &egui::Shape, clip: Rect, texts: &mut Vec<TextRun>, boxes: &mut Vec<Rect>) {
        match s {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, clip, texts, boxes)),
            egui::Shape::Text(t) => {
                let rect = t.galley.rect.translate(t.pos.to_vec2());
                let text: String = t.galley.rows.iter().map(|r| r.row.glyphs.iter().map(|g| g.chr).collect::<String>()).collect::<Vec<_>>().join("\n");
                if !text.trim().is_empty() {
                    texts.push(TextRun { text, rect, clip });
                }
            }
            egui::Shape::Rect(r) => {
                let filled = r.fill.a() > 0;
                let stroked = !r.stroke.is_empty();
                if (filled || stroked) && r.rect.width() > 1.0 && r.rect.height() > 1.0 {
                    boxes.push(r.rect);
                }
            }
            _ => {}
        }
    }
    for cs in shapes {
        walk(&cs.shape, cs.clip_rect, &mut texts, &mut boxes);
    }
    (texts, boxes)
}

/// Text must stay inside the smallest painted rect containing its centre point
/// (its left edge, vertically centered) and inside its clip rect.
pub fn check_text(texts: &[TextRun], boxes: &[Rect]) -> Vec<Finding> {
    let mut out = Vec::new();
    for t in texts {
        // Fully clipped text is simply scrolled out of view.
        if !t.clip.intersects(t.rect) || t.rect.width() <= 0.0 {
            continue;
        }
        let anchor = t.rect.center();
        let Some(container) = boxes
            .iter()
            .filter(|b| b.shrink(1.5).contains(anchor) && b.height() <= 64.0 && b.width() >= 8.0)
            .min_by(|a, b| (a.width() * a.height()).total_cmp(&(b.width() * b.height())))
        else {
            continue;
        };
        // A value squeezed down to a bare ellipsis means its field is too narrow.
        if container.width() >= 24.0 && matches!(t.text.trim(), "…" | "...") {
            out.push(Finding { what: "value elided to a bare ellipsis".into(), detail: format!("text {:?}", t.rect) });
        }
        if t.rect.right() > container.right() + 0.5 && t.rect.right() <= t.clip.right() + 0.5 {
            out.push(Finding { what: format!("text overflows container: {:?}", t.text), detail: format!("text {:?} container {:?}", t.rect, container) });
        }
        if t.rect.top() < container.top() - 1.5 || t.rect.bottom() > container.bottom() + 1.5 {
            out.push(Finding {
                what: format!("text outside container vertically: {:?}", t.text),
                detail: format!("text {:?} container {:?}", t.rect, container),
            });
        }
    }
    out
}
