//! Text and numeric input fields.

use super::*;
use egui::text::{CCursor, CCursorRange};
use egui::{Align, Margin, TextEdit};

#[derive(Clone, Copy)]
pub enum Prefix {
    None,
    Text(&'static str),
    Icon(Icon),
}

/// Single-line editable text rendered in `rect` (no chrome; see `field_bg`).
/// Returns the committed text when editing ends (Enter / focus loss).
pub fn edit_text(ui: &mut Ui, rect: Rect, id: Id, display: &str, font: FontId, color: Color32, pad_x: f32) -> Option<String> {
    edit_text_hint(ui, rect, id, display, font, color, pad_x, None)
}

/// Like [`edit_text`], with a hint (placeholder or "Mixed") shown while the text is empty.
///
/// Idle text is painted by us, on the exact baseline and elided with an ellipsis so it
/// never overflows; the egui `TextEdit` only draws it while it has focus.
#[allow(clippy::too_many_arguments)]
pub fn edit_text_hint(
    ui: &mut Ui,
    rect: Rect,
    id: Id,
    display: &str,
    font: FontId,
    color: Color32,
    pad_x: f32,
    hint: Option<(&str, Color32)>,
) -> Option<String> {
    let focused = ui.memory(|m| m.has_focus(id));
    let mut buf = if focused { ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| display.to_string()) } else { display.to_string() };
    let te = TextEdit::singleline(&mut buf)
        .id(id)
        .font(font.clone())
        .text_color(if focused { color } else { Color32::TRANSPARENT })
        .frame(egui::Frame::NONE)
        .margin(Margin::symmetric(pad_x as i8, 0))
        .vertical_align(Align::Center)
        .min_size(rect.size())
        .desired_width(rect.width());
    let out = ui.put(rect, te);
    if out.gained_focus() {
        let mut state = TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
        state.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(0), CCursor::new(buf.chars().count()))));
        state.store(ui.ctx(), id);
    }
    if out.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, buf.clone()));
    }
    let p = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    if buf.is_empty() {
        if let Some((h, hc)) = hint {
            super::text_on_baseline(&p, rect.left() + pad_x, super::centered_baseline(rect, &font), h, font.clone(), hc);
        }
    } else if !focused {
        super::field_text(&p, rect, pad_x, pad_x, &buf, font, color);
    }
    if out.lost_focus() {
        ui.data_mut(|d| d.remove_temp::<String>(id));
        let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if !esc && buf != display {
            return Some(buf);
        }
    }
    None
}

pub(crate) fn parse_number(s: &str) -> Option<f32> {
    let t = s.trim().trim_end_matches(['%', '°']).trim_end_matches("px").trim();
    t.parse::<f32>().ok().or_else(|| eval_simple(t))
}

/// Tiny `a+b`, `a-b`, `a*b`, `a/b` evaluator so users can type "100/2".
fn eval_simple(s: &str) -> Option<f32> {
    for (i, ch) in s.char_indices().skip(1) {
        if matches!(ch, '+' | '-' | '*' | '/') {
            let a: f32 = s[..i].trim().parse().ok()?;
            let b: f32 = s[i + 1..].trim().parse().ok()?;
            return Some(match ch {
                '+' => a + b,
                '-' => a - b,
                '*' => a * b,
                _ => {
                    if b == 0.0 {
                        return None;
                    }
                    a / b
                }
            });
        }
    }
    None
}

/// Numeric field with a prefix label/icon. `value == None` shows "Mixed".
///
/// Figma UI3 (`ui-spec.json` `input_fields`): prefix letter or 16px icon at x+8 in
/// `text_secondary`, value at x+24 (x+8 without a prefix), suffix right after the value
/// in `text_secondary`. Integers print without decimals, other values with at most two.
pub fn num_field(ui: &mut Ui, rect: Rect, key: impl crate::widgets::Key, prefix: Prefix, value: Option<f32>, suffix: &str) -> Option<f32> {
    #[cfg(test)]
    ui.ctx().data_mut(|d| d.insert_temp(Id::new(("field-rect", format!("{key:?}"))), rect));
    let id = ui.id().with(("num", key));
    let display = value.map(fmt_num).unwrap_or_default();
    let has_prefix = !matches!(prefix, Prefix::None);
    let pw = if has_prefix { theme::FIELD_PREFIX_W } else { 0.0 };
    field_bg(ui, rect, id);
    let p = ui.painter().clone();
    match prefix {
        Prefix::None => {}
        Prefix::Text(s) => {
            // 8px in; the value starts 24px in.
            super::field_text(
                &p,
                rect,
                theme::FIELD_PREFIX_X,
                rect.width() - theme::FIELD_PREFIX_X - theme::FIELD_PREFIX_ICON,
                s,
                theme::font_body(),
                theme::TEXT_SECONDARY,
            );
        }
        Prefix::Icon(i) => {
            let slot = Rect::from_min_size(
                Pos2::new(rect.left() + theme::FIELD_PREFIX_X, rect.center().y - theme::FIELD_PREFIX_ICON / 2.0),
                Vec2::splat(theme::FIELD_PREFIX_ICON),
            );
            super::paint_icon_in(&p, slot, i, theme::FIELD_PREFIX_ICON, theme::TEXT_SECONDARY);
        }
    }
    let scrubbed = if has_prefix { scrub_prefix(ui, Rect::from_min_size(rect.min, egui::vec2(pw, rect.height())), id, value) } else { None };
    // The suffix is drawn after the value, so the edit area stops short of it.
    let font = theme::font_body();
    let suffix_w = if suffix.is_empty() {
        0.0
    } else {
        super::text_width(&p, suffix.trim_start(), &font) + if suffix.starts_with(' ') { theme::FIELD_SUFFIX_GAP } else { 0.0 }
    };
    let text_rect = Rect::from_min_max(Pos2::new(rect.left() + pw, rect.top()), Pos2::new((rect.right() - suffix_w).max(rect.left() + pw), rect.bottom()));
    let hint = Some(("Mixed", theme::TEXT_SECONDARY));
    let pad = if has_prefix { 0.0 } else { theme::FIELD_PLAIN_X };
    let committed = edit_text_hint(ui, text_rect, id, &display, font.clone(), theme::TEXT, pad, hint);
    if !suffix.is_empty() && !(value.is_none() && !ui.memory(|m| m.has_focus(id))) {
        let shown = if ui.memory(|m| m.has_focus(id)) { ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| display.clone()) } else { display.clone() };
        let x = text_rect.left() + pad + super::text_width(&p, &shown, &font) + if suffix.starts_with(' ') { theme::FIELD_SUFFIX_GAP } else { 0.0 };
        let b = super::centered_baseline(rect, &font);
        if x + suffix_w <= rect.right() + 0.5 {
            super::text_on_baseline(&p.with_clip_rect(rect), x, b, suffix.trim_start(), font, theme::TEXT_SECONDARY);
        }
    }
    committed.and_then(|s| parse_number(&s)).or(scrubbed)
}

/// Context-data key that is true while any numeric label is being scrubbed;
/// the app snapshots the document on its rising edge and folds all edits
/// made during the drag into one undo step on its falling edge.
pub const SCRUB_FLAG: &str = "desigma-scrub-active";

/// Figma-style scrubbing: dragging a field's prefix changes the value by one
/// unit per pixel (x10 with Shift) and shows the horizontal-resize cursor.
fn scrub_prefix(ui: &mut Ui, prefix: Rect, id: Id, value: Option<f32>) -> Option<f32> {
    let sid = id.with("scrub");
    let resp = ui.interact(prefix, sid, Sense::drag());
    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    let flag = Id::new(SCRUB_FLAG);
    if resp.drag_started() {
        ui.ctx().data_mut(|d| {
            d.insert_temp(flag, true);
            d.insert_temp(sid, (value.unwrap_or(0.0), 0.0f32, f32::NAN));
        });
    }
    let mut out = None;
    if resp.dragged() {
        let (dx, shift) = ui.input(|i| (i.pointer.delta().x, i.modifiers.shift));
        let (start, acc, last) = ui.ctx().data(|d| d.get_temp::<(f32, f32, f32)>(sid)).unwrap_or((value.unwrap_or(0.0), 0.0, f32::NAN));
        let acc = acc + dx * if shift { 10.0 } else { 1.0 };
        let v = start + acc.round();
        if v != last && !(last.is_nan() && acc.round() == 0.0) {
            out = Some(v);
        }
        ui.ctx().data_mut(|d| d.insert_temp(sid, (start, acc, if out.is_some() { v } else { last })));
    }
    if resp.drag_stopped() {
        ui.ctx().data_mut(|d| d.insert_temp(flag, false));
    }
    out
}

/// Paints the standard field background with hover/focus border.
pub fn field_bg(ui: &Ui, rect: Rect, id: Id) {
    let focused = ui.memory(|m| m.has_focus(id));
    let hovered = ui.rect_contains_pointer(rect);
    ui.painter().rect_filled(rect, theme::cr(theme::RADIUS_MEDIUM), theme::BG_SECONDARY);
    if focused {
        ui.painter().rect_stroke(rect, theme::cr(theme::RADIUS_MEDIUM), Stroke::new(1.0, theme::BORDER_SELECTED), StrokeKind::Inside);
    } else if hovered {
        ui.painter().rect_stroke(rect, theme::cr(theme::RADIUS_MEDIUM), Stroke::new(1.0, theme::BORDER), StrokeKind::Inside);
    }
}

/// Plain text field with standard background; returns committed text.
pub fn text_input(ui: &mut Ui, rect: Rect, key: impl crate::widgets::Key, display: &str) -> Option<String> {
    text_input_ph(ui, rect, key, display, "")
}

/// [`text_input`] with a placeholder (in `text_tertiary`) shown while empty.
pub fn text_input_ph(ui: &mut Ui, rect: Rect, key: impl crate::widgets::Key, display: &str, placeholder: &str) -> Option<String> {
    let id = ui.id().with(("txt", key));
    field_bg(ui, rect, id);
    let hint = (!placeholder.is_empty()).then_some((placeholder, theme::TEXT_TERTIARY));
    edit_text_hint(ui, rect, id, display, theme::font_body(), theme::TEXT, theme::FIELD_PLAIN_X, hint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_numbers_and_expressions() {
        assert_eq!(parse_number("12"), Some(12.0));
        assert_eq!(parse_number(" 12.5% "), Some(12.5));
        assert_eq!(parse_number("90°"), Some(90.0));
        assert_eq!(parse_number("100/2"), Some(50.0));
        assert_eq!(parse_number("10+5"), Some(15.0));
        assert_eq!(parse_number("-4"), Some(-4.0));
        assert_eq!(parse_number("abc"), None);
        assert_eq!(parse_number("1/0"), None);
    }
}
