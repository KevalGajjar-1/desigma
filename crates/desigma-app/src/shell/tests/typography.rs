//! The Type settings popover: OpenType features and paragraph settings.

use super::*;
use desigma_doc::{LeadingTrim, TextDirection};

fn popover_sim() -> Sim {
    Sim::with(Options { select: Some("Continue".into()), panel: Some("type-details".into()), ..Options::default() })
}

fn click_rect(s: &mut Sim, r: Rect) {
    s.click(r.center());
}

#[test]
fn feature_checks_toggle_on_the_layer() {
    let mut s = popover_sim();
    let id = s.by_name("Continue");
    assert!(text_of(&s, id).style.features.is_empty());
    let case = s.rect_of("det-case");
    click_rect(&mut s, case);
    assert_eq!(text_of(&s, id).style.features, vec![("case".to_string(), 1)]);
    click_rect(&mut s, case);
    assert!(text_of(&s, id).style.features.is_empty(), "clicking again returns to the font default");
    // ligatures and contextual alternates are on by default: unchecking writes an explicit 0
    let calt = s.rect_of("det-calt");
    click_rect(&mut s, calt);
    assert_eq!(text_of(&s, id).style.features, vec![("calt".to_string(), 0)]);
    click_rect(&mut s, calt);
    assert!(text_of(&s, id).style.features.is_empty());
    // a stylistic set Inter has
    let ss = s.rect_of("det-ss-1");
    click_rect(&mut s, ss);
    assert_eq!(text_of(&s, id).style.features, vec![("ss01".to_string(), 1)]);
    // one undo step per change
    s.cmd_key(Key::Z);
    assert!(text_of(&s, id).style.features.is_empty());
}

#[test]
fn popover_lists_only_the_stylistic_sets_the_font_has() {
    let s = popover_sim();
    for set in 1..=8 {
        s.rect_of(&format!("det-ss-{set}"));
    }
    let nine = s.ctx.data(|d| d.get_temp::<Rect>(egui::Id::new(("test-rect", "det-ss-9".to_string()))));
    assert!(nine.is_none(), "Inter has ss01..ss08 only");
}

#[test]
fn paragraph_settings_apply_to_the_layer() {
    let mut s = popover_sim();
    let id = s.by_name("Continue");
    let r = s.field_rect("det-indent");
    s.click(Pos2::new(r.left() + 30.0, r.center().y));
    s.type_text("12");
    s.key(Key::Enter);
    assert_eq!(text_of(&s, id).paragraph.indent, 12.0);
    let r = s.field_rect("det-lspacing");
    s.click(Pos2::new(r.left() + 30.0, r.center().y));
    s.type_text("6");
    s.key(Key::Enter);
    assert_eq!(text_of(&s, id).paragraph.list_spacing, 6.0);
    let r = s.field_rect("det-pspacing");
    s.click(Pos2::new(r.left() + 30.0, r.center().y));
    s.type_text("9");
    s.key(Key::Enter);
    assert_eq!(text_of(&s, id).style.paragraph_spacing, 9.0);
    let hang = s.rect_of("det-hangp");
    click_rect(&mut s, hang);
    assert!(text_of(&s, id).paragraph.hanging_punctuation);
    // text direction: the segmented control's third cell is RTL
    let dir = s.rect_of("det-dir");
    s.click(Pos2::new(dir.right() - 10.0, dir.center().y));
    assert_eq!(text_of(&s, id).direction, TextDirection::Rtl);
    s.click(Pos2::new(dir.left() + 10.0, dir.center().y));
    assert_eq!(text_of(&s, id).direction, TextDirection::Auto);
    assert_eq!(text_of(&s, id).paragraph.leading_trim, LeadingTrim::None);
}

#[test]
fn features_follow_the_selected_range_while_editing() {
    let mut s = popover_sim();
    let id = s.by_name("Continue");
    // open a text session, select "Con" and set a feature through the popover
    crate::canvas::textsession::begin_text_edit(&mut s.shell.app, id);
    {
        let sess = s.shell.app.canvas.text.as_mut().unwrap();
        sess.caret = 3;
        sess.anchor = 0;
    }
    s.shell.app.props.pop = Some(crate::panels::effects::PopState {
        kind: crate::panels::effects::PopKind::TextDetails,
        anchor: Rect::from_min_size(Pos2::new(1550.0, 330.0), Vec2::splat(24.0)),
        ids: vec![id],
    });
    s.frame(vec![]);
    s.frame(vec![]);
    let case = s.rect_of("det-case");
    click_rect(&mut s, case);
    let t = text_of(&s, id);
    assert!(t.style.features.is_empty(), "the base style is untouched");
    assert_eq!(t.runs.len(), 1);
    assert_eq!((t.runs[0].start, t.runs[0].end), (0, 3));
    assert_eq!(t.runs[0].style.features, vec![("case".to_string(), 1)]);
}
