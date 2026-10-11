//! Typography section of the inspector. Edits the selected character range
//! while a text session is open (splitting and merging runs), otherwise every
//! selected text layer. Properties that differ over the range show "Mixed".

use super::properties::{open_picker, row_end, PickTarget};
use super::properties::{segmented, Pane};
use super::var_bind;
use crate::app::App;
use crate::canvas::textsession as ts;
use crate::layout;
use crate::richtext::{self, ListKind, Summary};
use crate::theme;
use crate::widgets::field::{self, Prefix};
use crate::widgets::menu::dropdown;
use crate::widgets::{self};
use desigma_doc::*;
use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

const WEIGHTS: [(&str, u16); 9] = [
    ("Thin", 100),
    ("Extra Light", 200),
    ("Light", 300),
    ("Regular", 400),
    ("Medium", 500),
    ("Semi Bold", 600),
    ("Bold", 700),
    ("Extra Bold", 800),
    ("Black", 900),
];

/// Combines two summaries: properties that differ become mixed.
pub fn combine(a: &Summary, b: &Summary) -> Summary {
    fn m<T: PartialEq + Clone>(x: &Option<T>, y: &Option<T>) -> Option<T> {
        if x == y {
            x.clone()
        } else {
            None
        }
    }
    Summary {
        family: m(&a.family, &b.family),
        weight: m(&a.weight, &b.weight),
        italic: m(&a.italic, &b.italic),
        size: m(&a.size, &b.size),
        line_height: m(&a.line_height, &b.line_height),
        letter_spacing: m(&a.letter_spacing, &b.letter_spacing),
        fills: m(&a.fills, &b.fills),
        decoration: m(&a.decoration, &b.decoration),
        case: m(&a.case, &b.case),
    }
}

/// Summary over whole text layers (not editing).
pub fn nodes_summary(app: &App, sel: &[NodeId]) -> Option<Summary> {
    sel.iter()
        .filter_map(|id| match app.doc.get(*id).map(|n| &n.kind) {
            Some(NodeKind::Text(t)) => Some(richtext::summary(t, 0..t.content.len())),
            _ => None,
        })
        .reduce(|a, b| combine(&a, &b))
}

fn editing(app: &App, sel: &[NodeId]) -> bool {
    app.canvas.text.as_ref().is_some_and(|s| sel.contains(&s.id))
}

/// Applies `f` to the character range being edited, or to every selected text layer.
pub fn set_style(app: &mut App, sel: &[NodeId], f: impl Fn(&mut TextStyle)) {
    if editing(app, sel) {
        ts::style_selection_as(app, &f, crate::textedit::EditKind::Style);
        return;
    }
    app.edit_nodes(sel, |n| {
        if let NodeKind::Text(t) = &mut n.kind {
            richtext::apply_all(t, &f);
            n.size = layout::fit_text(t, n.size);
        }
    });
}

/// Applies `f` to the base style only (paragraph level fields such as alignment).
fn set_base(app: &mut App, sel: &[NodeId], f: impl Fn(&mut TextStyle)) {
    if editing(app, sel) {
        ts::base_style(app, &f);
        return;
    }
    app.edit_nodes(sel, |n| {
        if let NodeKind::Text(t) = &mut n.kind {
            f(&mut t.style);
            n.size = layout::fit_text(t, n.size);
        }
    });
}

/// Segmented control with short text labels instead of icons.
fn seg_text(ui: &mut Ui, pane: &Pane, x: f32, key: &str, labels: &[&str], selected: Option<usize>, w: f32) -> Option<usize> {
    let mut out = None;
    let n = labels.len();
    for (i, label) in labels.iter().enumerate() {
        let r = pane.rect(x + i as f32 * w, 0.0, w, theme::FIELD_HEIGHT);
        let radius = match (i == 0, i + 1 == n) {
            (true, true) => theme::cr(theme::RADIUS_MEDIUM),
            (true, false) => theme::cr_each(theme::RADIUS_MEDIUM, 0, 0, theme::RADIUS_MEDIUM),
            (false, true) => theme::cr_each(0, theme::RADIUS_MEDIUM, theme::RADIUS_MEDIUM, 0),
            _ => theme::cr(0),
        };
        let resp = ui.interact(r, ui.id().with(("segt", key, i)), Sense::click());
        let sel = selected == Some(i);
        let bg = if sel {
            theme::BG_TERTIARY
        } else if resp.hovered() {
            theme::BG_TERTIARY.gamma_multiply(0.6)
        } else {
            theme::BG_SECONDARY
        };
        ui.painter().rect_filled(r, radius, bg);
        ui.painter().text(r.center(), Align2::CENTER_CENTER, *label, theme::font_body_strong(), if sel { theme::ICON } else { theme::ICON_SECONDARY });
        if i > 0 {
            widgets::vline(ui.painter(), r.left(), r.top(), r.bottom(), theme::BG);
        }
        if resp.clicked() {
            out = Some(i);
        }
    }
    out
}

fn weight_name(w: u16) -> &'static str {
    WEIGHTS.iter().min_by_key(|x| (x.1 as i32 - w as i32).abs()).map(|x| x.0).unwrap_or("Regular")
}

fn solid_of(fills: &[Paint]) -> Option<(Color, f32)> {
    fills.iter().find(|p| p.visible).and_then(|p| match &p.kind {
        PaintKind::Solid(c) => Some((*c, p.opacity)),
        _ => None,
    })
}

pub(super) fn section(app: &mut App, ui: &mut Ui, pane: &mut Pane, sel: &[NodeId], t: &TextProps) {
    let in_session = editing(app, sel);
    let sum = if in_session { ts::session_summary(app) } else { nodes_summary(app, sel) };
    let Some(sum) = sum else { return };
    let more_id = ui.id().with("typo-more");
    let more = ui.data(|d| d.get_temp::<bool>(more_id)).unwrap_or(false);

    // "More type settings" toggle sits in the title row (the title was drawn by the caller).
    let mb = pane.rect(theme::SECTION_TRAILING_X, -theme::SECTION_TITLE_HEIGHT + 8.0, theme::PANEL_ACTION_BUTTON, theme::PANEL_ACTION_BUTTON);
    let mr = ui.interact(mb, ui.id().with("typo-more-btn"), Sense::click());
    if mr.hovered() || more {
        ui.painter().rect_filled(mb, theme::cr(theme::RADIUS_MEDIUM), if more { theme::BG_TERTIARY } else { theme::BG_SECONDARY });
    }
    ui.painter().text(mb.center(), Align2::CENTER_CENTER, "\u{2026}", theme::font_body_strong(), if more { theme::ICON } else { theme::ICON_SECONDARY });
    mr.clone().on_hover_text("More type settings");
    if mr.clicked() {
        ui.data_mut(|d| d.insert_temp(more_id, !more));
    }

    // ---- family
    let fam_label = sum.family.clone().unwrap_or_else(|| "Mixed".to_string());
    if let Some(f) = family_picker(ui, pane.full(0.0), &fam_label) {
        set_style(app, sel, |s| s.font_family = f.clone());
        app.fonts.load_for(&app.doc);
    }
    row_end(pane);

    // ---- weight + size
    let names: Vec<String> = WEIGHTS.iter().map(|w| w.0.to_string()).collect();
    let cur = sum.weight.map(weight_name).unwrap_or("Mixed");
    if let Some(i) = dropdown(ui, pane.col(0, 0.0), "font-weight", cur, &names) {
        let w = WEIGHTS[i].1;
        set_style(app, sel, |s| s.font_weight = w);
        app.fonts.load_for(&app.doc);
    }
    let fs_r = pane.col(1, 0.0);
    if let Some(v) = var_bind::numeric(app, ui, fs_r, sel, &["fontSize"], |ui| field::num_field(ui, fs_r, "font-size", Prefix::Text("S"), sum.size, "")) {
        let v = v.max(1.0);
        set_style(app, sel, |s| s.font_size = v);
    }
    row_end(pane);

    // ---- line height + letter spacing
    let lh = match sum.line_height {
        Some(l) => Some(
            richtext::line_height_px(l, sum.size.unwrap_or(t.style.font_size))
                .unwrap_or_else(|| ((sum.size.unwrap_or(t.style.font_size) * 1.21) * 10.0).round() / 10.0),
        ),
        None => None,
    };
    if let Some(v) = field::num_field(ui, pane.col(0, 0.0), "line-height", Prefix::Text("\u{2195}"), lh, "") {
        let v = v.max(1.0);
        set_style(app, sel, |s| s.line_height = LineHeight::Pixels(v));
    }
    if let Some(v) = field::num_field(ui, pane.col(1, 0.0), "letter-spacing", Prefix::Text("\u{2194}"), sum.letter_spacing, "") {
        set_style(app, sel, |s| s.letter_spacing = v);
    }
    row_end(pane);

    // ---- colour
    color_row(app, ui, pane, sel, &sum);

    // ---- alignment (paragraph level: always the base style)
    let cur = match t.style.align_h {
        TextAlignH::Left | TextAlignH::Justified => 0,
        TextAlignH::Center => 1,
        TextAlignH::Right => 2,
    };
    if let Some(i) = segmented(
        ui,
        pane,
        theme::SECTION_CONTENT_X,
        "text-align",
        &[crate::icons::Icon::TextLeft, crate::icons::Icon::TextCenter, crate::icons::Icon::TextRight],
        Some(cur),
    ) {
        let a = [TextAlignH::Left, TextAlignH::Center, TextAlignH::Right][i];
        set_base(app, sel, |s| s.align_h = a);
    }
    // list buttons share the row (emulated with literal prefixes, see richtext)
    if let Some(i) = seg_text(ui, pane, theme::FIELD_COL_X[1], "list", &["\u{2022}", "1."], None, theme::SEGMENT_W) {
        ts::text_command(app, ts::TextCmd::List(if i == 0 { ListKind::Bullet } else { ListKind::Numbered }));
    }
    row_end(pane);

    if more {
        // ---- decoration + case
        let dec = sum.decoration.map(|d| match d {
            TextDecoration::None => 0,
            TextDecoration::Underline => 1,
            TextDecoration::Strikethrough => 2,
        });
        if let Some(i) = seg_text(ui, pane, theme::SECTION_CONTENT_X, "decoration", &["\u{2013}", "U", "S"], dec, theme::SEGMENT_W) {
            let d = [TextDecoration::None, TextDecoration::Underline, TextDecoration::Strikethrough][i];
            set_style(app, sel, |s| s.decoration = d);
        }
        let case = sum.case.and_then(|c| match c {
            TextCase::Original => Some(0),
            TextCase::Upper => Some(1),
            TextCase::Lower => Some(2),
            TextCase::Title => Some(3),
            TextCase::SmallCaps => None,
        });
        if let Some(i) = seg_text(ui, pane, theme::FIELD_COL_X[1], "case", &["\u{2013}", "AG", "ag", "Ag"], case, theme::SEGMENT_W) {
            let c = [TextCase::Original, TextCase::Upper, TextCase::Lower, TextCase::Title][i];
            set_style(app, sel, |s| s.case = c);
        }
        row_end(pane);
        let italic = sum.italic;
        if let Some(i) = seg_text(ui, pane, theme::SECTION_CONTENT_X, "italic", &["Normal", "Italic"], italic.map(|b| b as usize), 48.0) {
            set_style(app, sel, |s| s.italic = i == 1);
            app.fonts.load_for(&app.doc);
        }
        row_end(pane);
    }
}

fn color_row(app: &mut App, ui: &mut Ui, pane: &mut Pane, sel: &[NodeId], sum: &Summary) {
    let sw = pane.rect(theme::SWATCH_ROW_X, 4.0, theme::SWATCH_SIZE, theme::SWATCH_SIZE);
    let hex_r = pane.rect(theme::SWATCH_ROW_X + theme::SWATCH_SIZE + 8.0, 0.0, 96.0, theme::FIELD_HEIGHT);
    let op_r = pane.rect(theme::SWATCH_ROW_X + theme::SWATCH_SIZE + 8.0 + 96.0 + 4.0, 0.0, theme::OPACITY_FIELD_W, theme::FIELD_HEIGHT);
    let solid = sum.fills.as_deref().and_then(solid_of);
    match (&sum.fills, solid) {
        (Some(_), Some((c, op))) => {
            let col = {
                let f = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                Color32::from_rgba_unmultiplied(f(c.r), f(c.g), f(c.b), f(c.a * op))
            };
            widgets::swatch(ui.painter(), sw, col);
            let swr = ui.interact(sw, ui.id().with("typo-sw"), Sense::click());
            if swr.clicked() {
                open_picker(app, PickTarget::TextFill, col, sw, sel.to_vec());
            }
            if let Some(s) = field::text_input(ui, hex_r, "text-hex", &c.to_hex()) {
                if let Some(nc) = Color::from_hex(&s) {
                    set_style(app, sel, |st| set_solid(st, Color { a: c.a, ..nc }, None));
                }
            }
            if let Some(v) = field::num_field(ui, op_r, "text-op", Prefix::None, Some((op * 100.0).round()), "%") {
                let o = (v / 100.0).clamp(0.0, 1.0);
                set_style(app, sel, |st| set_solid(st, c, Some(o)));
            }
        }
        (None, _) => {
            widgets::swatch(ui.painter(), sw, Color32::TRANSPARENT);
            field::field_bg(ui, hex_r, ui.id().with("text-mixed"));
            ui.painter().text(Pos2::new(hex_r.left() + 8.0, hex_r.center().y), Align2::LEFT_CENTER, "Mixed", theme::font_body(), theme::TEXT);
        }
        _ => {
            widgets::swatch(ui.painter(), sw, Color32::TRANSPARENT);
            field::field_bg(ui, hex_r, ui.id().with("text-other"));
            ui.painter().text(Pos2::new(hex_r.left() + 8.0, hex_r.center().y), Align2::LEFT_CENTER, "Gradient / image", theme::font_body(), theme::TEXT);
        }
    }
    pane.y += theme::FIELD_HEIGHT + 8.0;
}

/// Sets the first fill to a solid colour (keeping other paints), optionally changing its opacity.
pub fn set_solid(st: &mut TextStyle, c: Color, opacity: Option<f32>) {
    match st.fills.iter_mut().find(|p| p.visible) {
        Some(p) => {
            p.kind = PaintKind::Solid(c);
            if let Some(o) = opacity {
                p.opacity = o;
            }
        }
        None => {
            let mut p = Paint::solid(c);
            if let Some(o) = opacity {
                p.opacity = o;
            }
            st.fills.push(p);
        }
    }
}

// ------------------------------------------------------------------ font family picker
fn family_picker(ui: &mut Ui, rect: Rect, label: &str) -> Option<String> {
    let id = ui.id().with("font-family");
    #[cfg(test)]
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(("field-rect", format!("{:?}", "font-family"))), rect));
    let open_id = id.with("open");
    let open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    let r = ui.interact(rect, id, Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, theme::cr(theme::RADIUS_MEDIUM), theme::BG_SECONDARY);
    if r.hovered() || open {
        p.rect_stroke(rect, theme::cr(theme::RADIUS_MEDIUM), Stroke::new(1.0, if open { theme::BORDER_SELECTED } else { theme::BORDER }), StrokeKind::Inside);
    }
    let font = theme::font_body();
    let shown = widgets::elide(p, label, &font, rect.width() - 28.0);
    p.text(Pos2::new(rect.left() + 8.0, rect.center().y), Align2::LEFT_CENTER, shown, font, theme::TEXT);
    crate::icons::paint(p, crate::icons::Icon::ChevronDown, Pos2::new(rect.right() - 12.0, rect.center().y), 12.0, theme::ICON_SECONDARY);
    if r.clicked() {
        ui.data_mut(|d| {
            d.insert_temp(open_id, !open);
            d.insert_temp(id.with("q"), String::new());
        });
    }
    if !open {
        return None;
    }
    let ctx = ui.ctx().clone();
    let mut picked = None;
    let mut close = false;
    let pos = Pos2::new(rect.left() - 4.0, rect.bottom() + 2.0);
    let w = rect.width() + 8.0;
    let area = egui::Area::new(id.with("popup")).order(egui::Order::Foreground).fixed_pos(pos).show(&ctx, |ui| {
        let panel = Rect::from_min_size(pos, Vec2::new(w, 296.0));
        ui.allocate_exact_size(panel.size(), Sense::hover());
        widgets::menu_panel(ui.painter(), panel, theme::MENU_RADIUS);
        let search = Rect::from_min_size(panel.min + Vec2::new(8.0, 8.0), Vec2::new(w - 16.0, theme::FIELD_HEIGHT));
        let mut q = ui.data(|d| d.get_temp::<String>(id.with("q"))).unwrap_or_default();
        let qid = id.with("q-edit");
        field::field_bg(ui, search, qid);
        let te = egui::TextEdit::singleline(&mut q)
            .id(qid)
            .frame(egui::Frame::NONE)
            .hint_text("Search fonts")
            .font(theme::font_body())
            .text_color(theme::TEXT)
            .margin(egui::Margin::symmetric(8, 0))
            .vertical_align(egui::Align::Center)
            .desired_width(search.width());
        let resp = ui.put(search, te);
        if !resp.has_focus() && !ui.data(|d| d.get_temp::<bool>(id.with("focused"))).unwrap_or(false) {
            resp.request_focus();
            ui.data_mut(|d| d.insert_temp(id.with("focused"), true));
        }
        ui.data_mut(|d| d.insert_temp(id.with("q"), q.clone()));
        let families = families_cached(&ctx);
        let needle = q.to_lowercase();
        let list: Vec<&String> = families.iter().filter(|f| needle.is_empty() || f.to_lowercase().contains(&needle)).collect();
        let body = Rect::from_min_max(panel.min + Vec2::new(0.0, 8.0 + theme::FIELD_HEIGHT + 4.0), panel.max - Vec2::new(0.0, 8.0));
        let row_h = theme::MENU_ITEM_HEIGHT;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
        child.set_clip_rect(body);
        if resp.has_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            if let Some(f) = list.first() {
                picked = Some((*f).clone());
            }
        }
        egui::ScrollArea::vertical().max_height(body.height()).auto_shrink([false, false]).show_rows(&mut child, row_h, list.len(), |ui, range| {
            for i in range {
                let (row, rr) = ui.allocate_exact_size(Vec2::new(body.width(), row_h), Sense::click());
                let cur = list[i].eq_ignore_ascii_case(label);
                if rr.hovered() {
                    ui.painter().rect_filled(row.shrink2(Vec2::new(theme::MENU_PILL_INSET, 0.0)), theme::cr(theme::MENU_PILL_RADIUS), theme::BG_MENU_SELECTED);
                }
                if cur {
                    crate::icons::paint(
                        ui.painter(),
                        crate::icons::Icon::Check,
                        Pos2::new(row.left() + 20.0, row.center().y),
                        theme::MENU_ICON_SIZE,
                        theme::TEXT,
                    );
                }
                ui.painter().text(Pos2::new(row.left() + 36.0, row.center().y), Align2::LEFT_CENTER, list[i], theme::font_menu(), theme::TEXT);
                if rr.clicked() {
                    picked = Some(list[i].clone());
                }
            }
        });
        if list.is_empty() {
            ui.painter().text(body.left_top() + Vec2::new(16.0, 12.0), Align2::LEFT_CENTER, "No matching fonts", theme::font_menu(), theme::TEXT_SECONDARY);
        }
        panel
    });
    let (pressed, ptr, esc) = ctx.input(|i| (i.pointer.any_pressed(), i.pointer.interact_pos(), i.key_pressed(egui::Key::Escape)));
    if esc || (pressed && ptr.is_some_and(|p| !area.response.rect.contains(p) && !rect.contains(p))) || picked.is_some() {
        close = true;
    }
    if close {
        ui.data_mut(|d| {
            d.insert_temp(open_id, false);
            d.insert_temp(id.with("focused"), false);
        });
    }
    picked
}

/// Installed families, cached per font generation.
fn families_cached(ctx: &egui::Context) -> std::sync::Arc<Vec<String>> {
    let key = egui::Id::new("typo-families");
    let gen = desigma_render::font_generation();
    if let Some((g, v)) = ctx.data(|d| d.get_temp::<(usize, std::sync::Arc<Vec<String>>)>(key)) {
        if g == gen {
            return v;
        }
    }
    let v = std::sync::Arc::new(desigma_render::available_families());
    ctx.data_mut(|d| d.insert_temp(key, (gen, v.clone())));
    v
}
