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
    let focused = ui.memory(|m| m.has_focus(id));
    let mut buf = if focused { ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| display.to_string()) } else { display.to_string() };
    let te = TextEdit::singleline(&mut buf)
        .id(id)
        .font(font)
        .text_color(color)
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
    if out.lost_focus() {
        ui.data_mut(|d| d.remove_temp::<String>(id));
        let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if !esc && buf != display {
            return Some(buf);
        }
    }
    None
}

fn parse_number(s: &str) -> Option<f32> {
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
pub fn num_field(ui: &mut Ui, rect: Rect, key: impl crate::widgets::Key, prefix: Prefix, value: Option<f32>, suffix: &str) -> Option<f32> {
    #[cfg(test)]
    ui.ctx().data_mut(|d| d.insert_temp(Id::new(("field-rect", format!("{key:?}"))), rect));
    let id = ui.id().with(("num", key));
    let display = match value {
        Some(v) => format!("{}{}", fmt_num(v), suffix),
        None => "Mixed".to_string(),
    };
    let pw = match prefix {
        Prefix::None => 0.0,
        _ => theme::FIELD_PREFIX_W,
    };
    field_bg(ui, rect, id);
    let p = ui.painter();
    let c = Pos2::new(rect.left() + pw / 2.0, rect.center().y);
    match prefix {
        Prefix::None => {}
        Prefix::Text(s) => {
            // Left-aligned label, 8px in; the value starts 24px in.
            p.text(Pos2::new(rect.left() + theme::FIELD_PREFIX_X, rect.center().y), Align2::LEFT_CENTER, s, theme::font_body(), theme::TEXT_SECONDARY);
        }
        Prefix::Icon(i) => icons::paint(p, i, c, 12.0, theme::TEXT_SECONDARY),
    }
    let scrubbed = if pw > 0.0 { scrub_prefix(ui, Rect::from_min_size(rect.min, egui::vec2(pw, rect.height())), id, value) } else { None };
    let text_rect = Rect::from_min_max(Pos2::new(rect.left() + pw, rect.top()), rect.max);
    edit_text(ui, text_rect, id, &display, theme::font_body(), theme::TEXT, if pw > 0.0 { 0.0 } else { 8.0 }).and_then(|s| parse_number(&s)).or(scrubbed)
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
    let id = ui.id().with(("txt", key));
    field_bg(ui, rect, id);
    edit_text(ui, rect, id, display, theme::font_body(), theme::TEXT, 8.0)
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
