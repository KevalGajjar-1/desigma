//! Right panel content: page settings when nothing is selected, otherwise the
//! inspector (alignment, position, layout, appearance, fill, stroke, effects).

use super::{clipped, scrollbar, wheel_scroll};
use crate::app::App;
use crate::edit::Tx;
use crate::icons::{self, Icon};
use crate::layout;
use crate::ops::{apply_resize, Align};
use crate::theme;
use crate::util::v2;
use crate::widgets::field::{self, Prefix};
use crate::widgets::menu::dropdown;
use crate::widgets::{self, fmt_num};
use desigma_doc::*;
use desigma_doc::Vec2 as DVec2;
use egui::{Align2, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickTarget {
    Fill(usize),
    Stroke(usize),
    PageBg,
    Shadow(usize),
}

pub struct Picker {
    pub target: PickTarget,
    pub tx: Tx,
    pub color: Color32,
    pub anchor: Rect,
    pub ids: Vec<NodeId>,
}

#[derive(Default)]
pub struct PropsState {
    pub scroll: f32,
    pub content_h: f32,
    pub picker: Option<Picker>,
}

struct Pane {
    x0: f32,
    oy: f32,
    y: f32,
}

impl Pane {
    fn rect(&self, x: f32, dy: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(Pos2::new(self.x0 + x, self.oy + self.y + dy), Vec2::new(w, h))
    }
    fn col(&self, i: usize, dy: f32) -> Rect {
        self.rect(theme::FIELD_COL_X[i], dy, theme::FIELD_WIDTH_HALF, theme::FIELD_HEIGHT)
    }
    fn full(&self, dy: f32) -> Rect {
        self.rect(theme::SECTION_CONTENT_X, dy, theme::SECTION_CONTENT_W, theme::FIELD_HEIGHT)
    }
}

fn to_c32(c: Color, opacity: f32) -> Color32 {
    let f = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgba_unmultiplied(f(c.r), f(c.g), f(c.b), f(c.a * opacity))
}

fn from_c32(c: Color32) -> Color {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    Color::rgba(r, g, b, a)
}

fn section_sep(ui: &Ui, pane: &mut Pane, first: bool) {
    if !first {
        widgets::hline(ui.painter(), pane.x0, pane.x0 + theme::RIGHT_PANEL_WIDTH, pane.oy + pane.y, theme::BORDER);
        pane.y += 1.0;
    }
}

/// Section title row; returns true when its action button was clicked.
fn title(ui: &mut Ui, pane: &mut Pane, text: &str, action: Option<Icon>, dim: bool) -> bool {
    let row = pane.rect(0.0, 0.0, theme::RIGHT_PANEL_WIDTH - 1.0, theme::SECTION_TITLE_HEIGHT);
    ui.painter().text(
        Pos2::new(row.left() + theme::SECTION_CONTENT_X, row.center().y),
        Align2::LEFT_CENTER,
        text,
        theme::font_body_strong(),
        if dim { theme::TEXT_SECONDARY } else { theme::TEXT },
    );
    let mut clicked = false;
    if let Some(ic) = action {
        let b = pane.rect(theme::SECTION_TRAILING_X, 8.0, theme::PANEL_ACTION_BUTTON, theme::PANEL_ACTION_BUTTON);
        clicked = widgets::icon_button(ui, b, ("title-action", text), ic, false, "").clicked();
    }
    pane.y += theme::SECTION_TITLE_HEIGHT;
    clicked
}

fn sublabel(ui: &Ui, pane: &mut Pane, text: &str) {
    let r = pane.rect(theme::SECTION_CONTENT_X, 0.0, theme::SECTION_CONTENT_W, theme::SUBLABEL_HEIGHT);
    ui.painter().text(Pos2::new(r.left(), r.center().y), Align2::LEFT_CENTER, text, theme::font_body(), theme::TEXT_SECONDARY);
    pane.y += theme::SUBLABEL_HEIGHT + 4.0;
}

/// Row advance after a 24px field row.
fn row_end(pane: &mut Pane) {
    pane.y += theme::FIELD_HEIGHT + 4.0;
}

fn segmented(ui: &mut Ui, pane: &Pane, x: f32, key: &str, items: &[Icon], selected: Option<usize>) -> Option<usize> {
    let mut out = None;
    let n = items.len();
    for (i, icon) in items.iter().enumerate() {
        let r = pane.rect(x + i as f32 * theme::SEGMENT_W, 0.0, theme::SEGMENT_W, theme::FIELD_HEIGHT);
        let radius = match (i == 0, i + 1 == n) {
            (true, true) => theme::cr(theme::RADIUS_MEDIUM),
            (true, false) => theme::cr_each(theme::RADIUS_MEDIUM, 0, 0, theme::RADIUS_MEDIUM),
            (false, true) => theme::cr_each(0, theme::RADIUS_MEDIUM, theme::RADIUS_MEDIUM, 0),
            _ => theme::cr(0),
        };
        let resp = ui.interact(r, ui.id().with(("seg", key, i)), Sense::click());
        let sel = selected == Some(i);
        let bg = if sel { theme::BG_TERTIARY } else if resp.hovered() { theme::BG_TERTIARY.gamma_multiply(0.6) } else { theme::BG_SECONDARY };
        ui.painter().rect_filled(r, radius, bg);
        icons::paint(ui.painter(), *icon, r.center(), 16.0, if sel { theme::ICON } else { theme::ICON_SECONDARY });
        if resp.clicked() {
            out = Some(i);
        }
    }
    out
}

fn checkbox(ui: &mut Ui, pane: &mut Pane, key: &str, label: &str, value: bool) -> Option<bool> {
    let r = pane.rect(theme::SECTION_CONTENT_X, 0.0, theme::SECTION_CONTENT_W, theme::FIELD_HEIGHT);
    let resp = ui.interact(r, ui.id().with(("chk", key)), Sense::click());
    let b = Rect::from_min_size(Pos2::new(r.left(), r.center().y - 7.0), Vec2::splat(14.0));
    let p = ui.painter();
    if value {
        p.rect_filled(b, theme::cr(theme::RADIUS_SMALL), theme::BG_BRAND);
        icons::paint(p, Icon::Check, b.center(), 12.0, theme::TEXT);
    } else {
        p.rect_stroke(b, theme::cr(theme::RADIUS_SMALL), Stroke::new(1.0, theme::TEXT_TERTIARY), StrokeKind::Inside);
    }
    p.text(Pos2::new(b.right() + 8.0, r.center().y), Align2::LEFT_CENTER, label, theme::font_body(), theme::TEXT);
    pane.y += theme::FIELD_HEIGHT + 4.0;
    resp.clicked().then_some(!value)
}

/// Common value of `f` over all selected nodes, or None if they differ.
fn common<T: PartialEq>(app: &App, sel: &[NodeId], f: impl Fn(&Node) -> T) -> Option<T> {
    let mut it = sel.iter().filter_map(|id| app.doc.get(*id)).map(f);
    let first = it.next()?;
    it.all(|v| v == first).then_some(first)
}

pub fn show(app: &mut App, ui: &mut Ui, region: Rect) {
    let mut scroll = app.props.scroll;
    wheel_scroll(ui, region, &mut scroll, app.props.content_h);
    app.props.scroll = scroll;
    let sel = app.selection.clone();
    let end = clipped(ui, region, |ui| {
        // Spec x values are relative to the panel's outer left edge (border at x=0).
        let mut pane = Pane { x0: region.left() - theme::BORDER_WIDTH, oy: region.top() - scroll, y: 0.0 };
        if sel.is_empty() {
            page_section(app, ui, &mut pane);
            styles_section(app, ui, &mut pane);
        } else {
            selection_ui(app, ui, &mut pane, &sel);
        }
        pane.y + 24.0
    });
    app.props.content_h = end;
    scrollbar(ui, region, scroll, end);
    picker_popup(app, ui, region);
}

// ---------------------------------------------------------------- no selection
fn page_section(app: &mut App, ui: &mut Ui, pane: &mut Pane) {
    title(ui, pane, "Page", None, false);
    let bg = match app.doc.get(app.page).map(|n| &n.kind) {
        Some(NodeKind::Page { background }) => *background,
        _ => Color::WHITE,
    };
    let sw = pane.rect(theme::SWATCH_ROW_X, 4.0, theme::SWATCH_SIZE, theme::SWATCH_SIZE);
    let hex_r = pane.rect(theme::SWATCH_ROW_X + theme::SWATCH_SIZE + 8.0, 0.0, theme::HEX_FIELD_W + 24.0, theme::FIELD_HEIGHT);
    let op_r = pane.rect(theme::FIELD_COL_X[1] + 8.0 + 16.0, 0.0, theme::OPACITY_FIELD_W + 24.0, theme::FIELD_HEIGHT);
    let swr = ui.interact(sw, ui.id().with("page-swatch"), Sense::click());
    widgets::swatch(ui.painter(), sw, to_c32(bg, 1.0));
    if swr.clicked() {
        open_picker(app, PickTarget::PageBg, to_c32(bg, 1.0), sw, vec![app.page]);
    }
    if let Some(s) = field::text_input(ui, hex_r, "page-hex", &bg.to_hex()) {
        if let Some(c) = Color::from_hex(&s) {
            let page = app.page;
            app.edit_nodes(&[page], |n| n.kind = NodeKind::Page { background: Color { a: bg.a, ..c } });
        }
    }
    if let Some(v) = field::num_field(ui, op_r, "page-op", Prefix::None, Some((bg.a * 100.0).round()), "%") {
        let page = app.page;
        let a = (v / 100.0).clamp(0.0, 1.0);
        app.edit_nodes(&[page], |n| n.kind = NodeKind::Page { background: Color { a, ..bg } });
    }
    pane.y += 36.0;
    section_sep(ui, pane, false);
}

fn styles_section(app: &mut App, ui: &mut Ui, pane: &mut Pane) {
    title(ui, pane, "Styles", Some(Icon::Plus), false);
    let text_styles: Vec<(String, TextStyle)> = app.doc.styles.iter().filter_map(|(k, s)| if let Style::Text(t) = s { Some((k.clone(), t.clone())) } else { None }).collect();
    let fill_styles: Vec<(String, Vec<Paint>)> = app.doc.styles.iter().filter_map(|(k, s)| if let Style::Fill(f) = s { Some((k.clone(), f.clone())) } else { None }).collect();
    if !text_styles.is_empty() {
        sublabel(ui, pane, "Text styles");
        for (name, t) in &text_styles {
            let r = pane.rect(8.0, 0.0, theme::RIGHT_PANEL_WIDTH - 17.0, 28.0);
            let resp = ui.interact(r, ui.id().with(("tstyle", name)), Sense::hover());
            if resp.hovered() {
                ui.painter().rect_filled(r, theme::cr(theme::RADIUS_MEDIUM), theme::BG_HOVER);
            }
            ui.painter().text(Pos2::new(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, "Ag", theme::font_body_strong(), theme::TEXT);
            let nm = widgets::elide(ui.painter(), name, &theme::font_body(), 110.0);
            ui.painter().text(Pos2::new(r.left() + 36.0, r.center().y), Align2::LEFT_CENTER, nm, theme::font_body(), theme::TEXT);
            let lh = match t.line_height {
                LineHeight::Pixels(p) => fmt_num(p),
                LineHeight::Percent(p) => fmt_num(t.font_size * p / 100.0),
                LineHeight::Auto => fmt_num(t.font_size),
            };
            ui.painter().text(Pos2::new(r.right() - 10.0, r.center().y), Align2::RIGHT_CENTER, format!("{}/{}", fmt_num(t.font_size), lh), theme::font_body(), theme::TEXT_TERTIARY);
            pane.y += 28.0;
        }
        pane.y += 4.0;
    }
    if !fill_styles.is_empty() {
        sublabel(ui, pane, "Color styles");
        for (name, paints) in &fill_styles {
            let r = pane.rect(8.0, 0.0, theme::RIGHT_PANEL_WIDTH - 17.0, 28.0);
            let resp = ui.interact(r, ui.id().with(("cstyle", name)), Sense::hover());
            if resp.hovered() {
                ui.painter().rect_filled(r, theme::cr(theme::RADIUS_MEDIUM), theme::BG_HOVER);
            }
            let sw = Rect::from_center_size(Pos2::new(r.left() + 18.0, r.center().y), Vec2::splat(theme::SWATCH_SIZE));
            let col = paints.first().and_then(|p| if let PaintKind::Solid(c) = &p.kind { Some(to_c32(*c, p.opacity)) } else { None }).unwrap_or(Color32::GRAY);
            widgets::swatch(ui.painter(), sw, col);
            let nm = widgets::elide(ui.painter(), name, &theme::font_body(), 130.0);
            ui.painter().text(Pos2::new(r.left() + 36.0, r.center().y), Align2::LEFT_CENTER, nm, theme::font_body(), theme::TEXT);
            pane.y += 28.0;
        }
        pane.y += 4.0;
    }
    pane.y += 4.0;
    section_sep(ui, pane, false);
    title(ui, pane, "Export", Some(Icon::Plus), true);
}

// ---------------------------------------------------------------- selection
fn selection_ui(app: &mut App, ui: &mut Ui, pane: &mut Pane, sel: &[NodeId]) {
    let Some(n0) = app.doc.get(sel[0]).cloned() else { return };
    let multi = sel.len() > 1;
    let is_text = matches!(n0.kind, NodeKind::Text(_));
    let is_frame = n0.kind.frame_props().is_some();
    let is_line = matches!(n0.kind, NodeKind::Line);
    let parent_al = n0
        .parent
        .and_then(|p| app.doc.get(p))
        .and_then(|p| p.kind.frame_props())
        .map(|f| f.auto_layout.is_some())
        .unwrap_or(false);
    let parent_frame = n0.parent.and_then(|p| app.doc.get(p)).map(|p| p.kind.frame_props().is_some()).unwrap_or(false);

    // ---- header: name + quick toggles
    pane.y += theme::RIGHT_SELECTION_HEADER_Y - theme::RIGHT_CONTENT_TOP;
    let name_r = pane.rect(theme::RIGHT_NAME_FIELD_X - 1.0, 0.0, theme::RIGHT_NAME_FIELD_W, theme::FIELD_HEIGHT);
    if multi {
        ui.painter().text(Pos2::new(name_r.left() + 8.0, name_r.center().y), Align2::LEFT_CENTER, format!("{} layers", sel.len()), theme::font_large(), theme::TEXT);
    } else {
        let id = ui.id().with("sel-name");
        if ui.rect_contains_pointer(name_r) || ui.memory(|m| m.has_focus(id)) {
            let col = if ui.memory(|m| m.has_focus(id)) { theme::BORDER_SELECTED } else { theme::BORDER };
            ui.painter().rect_stroke(name_r, theme::cr(theme::RADIUS_MEDIUM), Stroke::new(1.0, col), StrokeKind::Inside);
        }
        let comp = matches!(n0.kind, NodeKind::Component(_) | NodeKind::Instance { .. });
        if let Some(s) = field::edit_text(ui, name_r, id, &n0.name, theme::font_large(), if comp { theme::TEXT_COMPONENT } else { theme::TEXT }, 8.0) {
            app.rename(sel[0], &s);
        }
    }
    let btns = [
        (Icon::Mask, "Use as mask", n0.is_mask),
        (if n0.locked { Icon::Lock } else { Icon::Unlock }, "Lock", n0.locked),
        (if n0.visible { Icon::Eye } else { Icon::EyeOff }, "Hide / show", !n0.visible),
    ];
    for (i, (icon, tip, on)) in btns.iter().enumerate() {
        let r = pane.rect(theme::RIGHT_NAME_ACTION_X[i] - 1.0, 0.0, theme::PANEL_ACTION_BUTTON, theme::PANEL_ACTION_BUTTON);
        if widgets::icon_button(ui, r, ("hdr", i), *icon, *on, tip).clicked() {
            match i {
                0 => app.edit_nodes(sel, |n| n.is_mask = !on),
                1 => app.edit_nodes(sel, |n| n.locked = !on),
                _ => app.edit_nodes(sel, |n| n.visible = !n.visible),
            }
        }
    }
    pane.y += theme::FIELD_HEIGHT + 8.0;
    section_sep(ui, pane, false);

    // ---- Position
    title(ui, pane, "Position", None, false);
    sublabel(ui, pane, "Alignment");
    let h = [Icon::AlignLeft, Icon::AlignHCenter, Icon::AlignRight];
    let v = [Icon::AlignTop, Icon::AlignVCenter, Icon::AlignBottom];
    if let Some(i) = segmented(ui, pane, theme::SECTION_CONTENT_X, "alignh", &h, None) {
        app.align([Align::Left, Align::HCenter, Align::Right][i]);
    }
    if let Some(i) = segmented(ui, pane, theme::SECTION_CONTENT_X + 3.0 * theme::SEGMENT_W + theme::SEGMENT_GROUP_GAP, "alignv", &v, None) {
        app.align([Align::Top, Align::VCenter, Align::Bottom][i]);
    }
    row_end(pane);
    sublabel(ui, pane, "Position");
    let xs = common(app, sel, |n| n.transform.tx);
    let ys = common(app, sel, |n| n.transform.ty);
    if let Some(x) = field::num_field(ui, pane.col(0, 0.0), "x", Prefix::Text("X"), xs, "") {
        app.edit_nodes(sel, |n| n.transform.tx = x);
    }
    if let Some(y) = field::num_field(ui, pane.col(1, 0.0), "y", Prefix::Text("Y"), ys, "") {
        app.edit_nodes(sel, |n| n.transform.ty = y);
    }
    row_end(pane);
    let rot = common(app, sel, |n| (-n.transform.rotation_rad().to_degrees() * 100.0).round() / 100.0);
    if let Some(deg) = field::num_field(ui, pane.col(0, 0.0), "rot", Prefix::Icon(Icon::Rotation), rot, "°") {
        let rad = -deg.to_radians();
        app.edit_nodes(sel, |n| n.transform = crate::canvas::handles::rotate_about_center(&n.transform, n.size, rad));
    }
    row_end(pane);
    sublabel(ui, pane, "Dimensions");
    let ws = common(app, sel, |n| n.size.x);
    let hs = common(app, sel, |n| n.size.y);
    let w_new = field::num_field(ui, pane.col(0, 0.0), "w", Prefix::Text("W"), ws, "");
    let h_new = if is_line { None } else { field::num_field(ui, pane.col(1, 0.0), "h", Prefix::Text("H"), hs, "") };
    if w_new.is_some() || h_new.is_some() {
        resize_selection(app, sel, w_new, h_new);
    }
    row_end(pane);
    if parent_frame && !parent_al && !multi {
        sublabel(ui, pane, "Constraints");
        let hn = ["Left", "Right", "Left & Right", "Center", "Scale"].map(String::from);
        let vn = ["Top", "Bottom", "Top & Bottom", "Center", "Scale"].map(String::from);
        let order = [Constraint::Min, Constraint::Max, Constraint::Stretch, Constraint::Center, Constraint::Scale];
        let hi = order.iter().position(|c| *c == n0.constraints.horizontal).unwrap_or(0);
        let vi = order.iter().position(|c| *c == n0.constraints.vertical).unwrap_or(0);
        if let Some(i) = dropdown(ui, pane.col(0, 0.0), "con-h", &hn[hi], &hn) {
            app.edit_nodes(sel, |n| n.constraints.horizontal = order[i]);
        }
        if let Some(i) = dropdown(ui, pane.col(1, 0.0), "con-v", &vn[vi], &vn) {
            app.edit_nodes(sel, |n| n.constraints.vertical = order[i]);
        }
        row_end(pane);
    }
    if parent_al || (is_frame && n0.kind.frame_props().map(|f| f.auto_layout.is_some()).unwrap_or(false)) || is_text {
        sublabel(ui, pane, "Resizing");
        let opts = ["Fixed", "Hug", "Fill"].map(String::from);
        let order = [Sizing::Fixed, Sizing::Hug, Sizing::Fill];
        let (hs, vs) = (sizing_of(&n0, true), sizing_of(&n0, false));
        if let Some(i) = dropdown(ui, pane.col(0, 0.0), "rs-h", &format!("W: {}", opts[order.iter().position(|s| *s == hs).unwrap_or(0)]), &opts) {
            set_sizing(app, sel, true, order[i]);
        }
        if let Some(i) = dropdown(ui, pane.col(1, 0.0), "rs-v", &format!("H: {}", opts[order.iter().position(|s| *s == vs).unwrap_or(0)]), &opts) {
            set_sizing(app, sel, false, order[i]);
        }
        row_end(pane);
    }
    pane.y += 4.0;

    // ---- Auto layout
    if is_frame && !multi {
        section_sep(ui, pane, false);
        let al = n0.kind.frame_props().and_then(|f| f.auto_layout);
        if title(ui, pane, "Auto layout", Some(if al.is_some() { Icon::Minus } else { Icon::Plus }), al.is_none()) {
            app.toggle_auto_layout();
        }
        if let Some(a) = al {
            auto_layout_ui(app, ui, pane, sel, a);
        }
        if al.is_none() {
            pane.y -= 0.0;
        }
    }

    // ---- Typography
    if let NodeKind::Text(t) = &n0.kind {
        section_sep(ui, pane, false);
        title(ui, pane, "Typography", None, false);
        text_ui(app, ui, pane, sel, t);
        pane.y += 4.0;
    }

    // ---- Appearance
    section_sep(ui, pane, false);
    title(ui, pane, "Appearance", None, false);
    let op = common(app, sel, |n| (n.opacity * 100.0).round());
    if let Some(v) = field::num_field(ui, pane.col(0, 0.0), "opacity", Prefix::Icon(Icon::Opacity), op, "%") {
        let o = (v / 100.0).clamp(0.0, 1.0);
        app.edit_nodes(sel, |n| n.opacity = o);
    }
    let has_radius = !matches!(n0.kind, NodeKind::Text(_) | NodeKind::Line | NodeKind::Group | NodeKind::Vector { .. });
    if has_radius {
        let rad = common(app, sel, |n| n.corner_radii[0]);
        if let Some(v) = field::num_field(ui, pane.col(1, 0.0), "radius", Prefix::Icon(Icon::Radius), rad, "") {
            let v = v.max(0.0);
            app.edit_nodes(sel, |n| n.corner_radii = [v; 4]);
        }
    }
    row_end(pane);
    if is_frame {
        let clip = n0.kind.frame_props().map(|f| f.clips_content).unwrap_or(false);
        if let Some(v) = checkbox(ui, pane, "clip", "Clip content", clip) {
            app.edit_nodes(sel, |n| {
                if let Some(f) = n.kind.frame_props_mut() {
                    f.clips_content = v;
                }
            });
        }
    }
    pane.y += 4.0;

    // ---- Fill
    if !is_line {
        section_sep(ui, pane, false);
        if title(ui, pane, "Fill", Some(Icon::Plus), n0.fills.is_empty()) {
            let paint = Paint::solid(Color::from_hex(&format!("{:06X}", theme::DEFAULT_SHAPE_FILL)).unwrap_or(Color::WHITE));
            app.edit_nodes(sel, |n| n.fills.push(paint.clone()));
        }
        paint_rows(app, ui, pane, sel, &n0.fills, false);
    }

    // ---- Stroke
    section_sep(ui, pane, false);
    if title(ui, pane, "Stroke", Some(Icon::Plus), n0.stroke.paints.is_empty()) {
        let paint = Paint::solid(Color::from_hex(&format!("{:06X}", theme::DEFAULT_STROKE)).unwrap_or(Color::BLACK));
        app.edit_nodes(sel, |n| n.stroke.paints.push(paint.clone()));
    }
    paint_rows(app, ui, pane, sel, &n0.stroke.paints, true);
    if !n0.stroke.paints.is_empty() {
        if let Some(w) = field::num_field(ui, pane.col(0, 0.0), "stroke-w", Prefix::Text("W"), common(app, sel, |n| n.stroke.weight), "") {
            let w = w.max(0.0);
            app.edit_nodes(sel, |n| n.stroke.weight = w);
        }
        let aligns = ["Inside", "Center", "Outside"].map(String::from);
        let cur = match n0.stroke.align {
            StrokeAlign::Inside => 0,
            StrokeAlign::Center => 1,
            StrokeAlign::Outside => 2,
        };
        if let Some(i) = dropdown(ui, pane.col(1, 0.0), "stroke-align", &aligns[cur], &aligns) {
            let a = [StrokeAlign::Inside, StrokeAlign::Center, StrokeAlign::Outside][i];
            app.edit_nodes(sel, |n| n.stroke.align = a);
        }
        row_end(pane);
        pane.y += 4.0;
    }

    // ---- Effects
    section_sep(ui, pane, false);
    if title(ui, pane, "Effects", Some(Icon::Plus), n0.effects.is_empty()) {
        let e = Effect::DropShadow { color: Color::rgba(0, 0, 0, 64), offset: DVec2::new(0.0, 4.0), radius: 4.0, spread: 0.0, visible: true, show_behind: false };
        app.edit_nodes(sel, |n| n.effects.push(e.clone()));
    }
    effect_rows(app, ui, pane, sel, &n0.effects);

    // ---- Export
    section_sep(ui, pane, false);
    title(ui, pane, "Export", None, false);
    let r = pane.full(0.0);
    let resp = ui.interact(r, ui.id().with("export-btn"), Sense::click());
    ui.painter().rect_filled(r, theme::cr(theme::RADIUS_MEDIUM), if resp.hovered() { theme::BG_TERTIARY } else { theme::BG_SECONDARY });
    icons::paint(ui.painter(), Icon::Export, Pos2::new(r.left() + 16.0, r.center().y), 14.0, theme::ICON_SECONDARY);
    let label = widgets::elide(ui.painter(), &format!("Export {}", n0.name), &theme::font_body(), r.width() - 44.0);
    ui.painter().text(Pos2::new(r.left() + 32.0, r.center().y), Align2::LEFT_CENTER, label, theme::font_body(), theme::TEXT);
    if resp.clicked() {
        app.export_selection();
    }
    pane.y += theme::FIELD_HEIGHT + 12.0;
}

fn sizing_of(n: &Node, horizontal: bool) -> Sizing {
    if let Some(al) = n.kind.frame_props().and_then(|f| f.auto_layout) {
        let primary_h = al.direction == LayoutDirection::Horizontal;
        let s = if horizontal == primary_h { al.primary_sizing } else { al.counter_sizing };
        if s == Sizing::Hug {
            return Sizing::Hug;
        }
    }
    if let NodeKind::Text(t) = &n.kind {
        if t.auto_resize == TextAutoResize::WidthAndHeight || (t.auto_resize == TextAutoResize::Height && !horizontal) {
            return Sizing::Hug;
        }
    }
    if horizontal {
        n.layout_child.horizontal
    } else {
        n.layout_child.vertical
    }
}

fn set_sizing(app: &mut App, sel: &[NodeId], horizontal: bool, s: Sizing) {
    app.edit_nodes(sel, |n| {
        if s != Sizing::Hug {
            if horizontal {
                n.layout_child.horizontal = s;
            } else {
                n.layout_child.vertical = s;
            }
        } else if horizontal {
            n.layout_child.horizontal = Sizing::Fixed;
        } else {
            n.layout_child.vertical = Sizing::Fixed;
        }
        if let Some(al) = n.kind.frame_props_mut().and_then(|f| f.auto_layout.as_mut()) {
            let primary_h = al.direction == LayoutDirection::Horizontal;
            let target = if horizontal == primary_h { &mut al.primary_sizing } else { &mut al.counter_sizing };
            *target = if s == Sizing::Hug { Sizing::Hug } else { Sizing::Fixed };
        }
        if let NodeKind::Text(t) = &mut n.kind {
            if s == Sizing::Hug {
                t.auto_resize = if horizontal { TextAutoResize::WidthAndHeight } else { TextAutoResize::Height };
            } else if t.auto_resize != TextAutoResize::None {
                t.auto_resize = TextAutoResize::None;
            }
        }
    });
}

fn resize_selection(app: &mut App, sel: &[NodeId], w: Option<f32>, h: Option<f32>) {
    let mut tx = Tx::new();
    for id in sel {
        let Some(n) = app.doc.get(*id) else { continue };
        let size = v2(w.unwrap_or(n.size.x).max(1.0), h.unwrap_or(n.size.y).max(if matches!(n.kind, NodeKind::Line) { 0.0 } else { 1.0 }));
        let t = n.transform;
        apply_resize(&mut app.doc, &mut tx.ed, *id, size, t, true);
    }
    app.run_tx(tx);
}

fn auto_layout_ui(app: &mut App, ui: &mut Ui, pane: &mut Pane, sel: &[NodeId], a: AutoLayout) {
    sublabel(ui, pane, "Flow");
    let dir_sel = if a.wrap {
        2
    } else if a.direction == LayoutDirection::Horizontal {
        0
    } else {
        1
    };
    if let Some(i) = segmented(ui, pane, theme::SECTION_CONTENT_X, "al-dir", &[Icon::DirHorizontal, Icon::DirVertical, Icon::Wrap], Some(dir_sel)) {
        update_al(app, sel, |al| match i {
            0 => {
                al.direction = LayoutDirection::Horizontal;
                al.wrap = false;
            }
            1 => {
                al.direction = LayoutDirection::Vertical;
                al.wrap = false;
            }
            _ => {
                al.wrap = true;
                al.primary_sizing = Sizing::Fixed;
            }
        });
    }
    if let Some(v) = field::num_field(ui, pane.col(1, 0.0), "al-gap", Prefix::Icon(Icon::Spacing), Some(a.item_spacing), "") {
        update_al(app, sel, |al| al.item_spacing = v);
    }
    row_end(pane);
    sublabel(ui, pane, "Padding");
    let ph = a.padding[1].max(a.padding[3]);
    let pv = a.padding[0].max(a.padding[2]);
    if let Some(v) = field::num_field(ui, pane.col(0, 0.0), "al-ph", Prefix::Icon(Icon::PaddingH), Some(ph), "") {
        update_al(app, sel, |al| {
            al.padding[1] = v;
            al.padding[3] = v;
        });
    }
    if let Some(v) = field::num_field(ui, pane.col(1, 0.0), "al-pv", Prefix::Icon(Icon::PaddingV), Some(pv), "") {
        update_al(app, sel, |al| {
            al.padding[0] = v;
            al.padding[2] = v;
        });
    }
    row_end(pane);
    sublabel(ui, pane, "Alignment");
    let pi = match a.primary_align {
        LayoutAlign::Start | LayoutAlign::Baseline => 0,
        LayoutAlign::Center => 1,
        LayoutAlign::End => 2,
        LayoutAlign::SpaceBetween => 3,
    };
    let ci = match a.counter_align {
        LayoutAlign::Center => 1,
        LayoutAlign::End => 2,
        _ => 0,
    };
    let al_icons = [Icon::AlignLeft, Icon::AlignHCenter, Icon::AlignRight];
    if let Some(i) = segmented(ui, pane, theme::SECTION_CONTENT_X, "al-main", &al_icons, Some(pi.min(2))) {
        update_al(app, sel, |al| al.primary_align = [LayoutAlign::Start, LayoutAlign::Center, LayoutAlign::End][i]);
    }
    let sb = pane.rect(theme::SECTION_CONTENT_X + 3.0 * theme::SEGMENT_W + 4.0, 0.0, 24.0, theme::FIELD_HEIGHT);
    let sbr = widgets::icon_button(ui, sb, "al-space", Icon::Spacing, pi == 3, "Space between");
    if sbr.clicked() {
        update_al(app, sel, |al| al.primary_align = if pi == 3 { LayoutAlign::Start } else { LayoutAlign::SpaceBetween });
    }
    let v_icons = [Icon::AlignTop, Icon::AlignVCenter, Icon::AlignBottom];
    if let Some(i) = segmented(ui, pane, theme::FIELD_COL_X[1] + 16.0, "al-cross", &v_icons, Some(ci)) {
        update_al(app, sel, |al| al.counter_align = [LayoutAlign::Start, LayoutAlign::Center, LayoutAlign::End][i]);
    }
    row_end(pane);
    pane.y += 4.0;
}

fn update_al(app: &mut App, sel: &[NodeId], f: impl Fn(&mut AutoLayout)) {
    app.edit_nodes(sel, |n| {
        if let Some(al) = n.kind.frame_props_mut().and_then(|f| f.auto_layout.as_mut()) {
            f(al);
        }
    });
}

fn text_ui(app: &mut App, ui: &mut Ui, pane: &mut Pane, sel: &[NodeId], t: &TextProps) {
    let s = &t.style;
    let fam = pane.full(0.0);
    if let Some(v) = field::text_input(ui, fam, "font-family", &s.font_family) {
        app.edit_nodes(sel, |n| {
            if let NodeKind::Text(t) = &mut n.kind {
                t.style.font_family = v.clone();
            }
        });
    }
    row_end(pane);
    let weights = [("Thin", 100u16), ("Extra Light", 200), ("Light", 300), ("Regular", 400), ("Medium", 500), ("Semi Bold", 600), ("Bold", 700), ("Extra Bold", 800), ("Black", 900)];
    let names: Vec<String> = weights.iter().map(|w| w.0.to_string()).collect();
    let cur = weights.iter().min_by_key(|w| (w.1 as i32 - s.font_weight as i32).abs()).map(|w| w.0).unwrap_or("Regular");
    if let Some(i) = dropdown(ui, pane.col(0, 0.0), "font-weight", cur, &names) {
        let w = weights[i].1;
        app.edit_nodes(sel, |n| {
            if let NodeKind::Text(t) = &mut n.kind {
                t.style.font_weight = w;
            }
        });
    }
    if let Some(v) = field::num_field(ui, pane.col(1, 0.0), "font-size", Prefix::Text("S"), Some(s.font_size), "") {
        let v = v.max(1.0);
        app.edit_nodes(sel, |n| {
            if let NodeKind::Text(t) = &mut n.kind {
                t.style.font_size = v;
                if t.auto_resize == TextAutoResize::WidthAndHeight {
                    n.size = layout::estimate_text_size(t);
                }
            }
        });
    }
    row_end(pane);
    let lh = match s.line_height {
        LineHeight::Auto => None,
        LineHeight::Pixels(p) => Some(p),
        LineHeight::Percent(p) => Some(s.font_size * p / 100.0),
    };
    if let Some(v) = field::num_field(ui, pane.col(0, 0.0), "line-height", Prefix::Text("↕"), lh.or(Some((s.font_size * 1.21 * 10.0).round() / 10.0)), "") {
        app.edit_nodes(sel, |n| {
            if let NodeKind::Text(t) = &mut n.kind {
                t.style.line_height = LineHeight::Pixels(v.max(1.0));
            }
        });
    }
    if let Some(v) = field::num_field(ui, pane.col(1, 0.0), "letter-spacing", Prefix::Text("↔"), Some(s.letter_spacing), "") {
        app.edit_nodes(sel, |n| {
            if let NodeKind::Text(t) = &mut n.kind {
                t.style.letter_spacing = v;
            }
        });
    }
    row_end(pane);
    let cur = match s.align_h {
        TextAlignH::Left | TextAlignH::Justified => 0,
        TextAlignH::Center => 1,
        TextAlignH::Right => 2,
    };
    if let Some(i) = segmented(ui, pane, theme::SECTION_CONTENT_X, "text-align", &[Icon::TextLeft, Icon::TextCenter, Icon::TextRight], Some(cur)) {
        let a = [TextAlignH::Left, TextAlignH::Center, TextAlignH::Right][i];
        app.edit_nodes(sel, |n| {
            if let NodeKind::Text(t) = &mut n.kind {
                t.style.align_h = a;
            }
        });
    }
    row_end(pane);
}

// ---------------------------------------------------------------- paints
fn paint_label(p: &Paint) -> String {
    match &p.kind {
        PaintKind::Solid(c) => c.to_hex(),
        PaintKind::LinearGradient { .. } => "Linear".into(),
        PaintKind::RadialGradient { .. } => "Radial".into(),
        PaintKind::AngularGradient { .. } => "Angular".into(),
        PaintKind::DiamondGradient { .. } => "Diamond".into(),
        PaintKind::Image { .. } => "Image".into(),
    }
}

fn paint_preview(p: &Paint) -> Color32 {
    match &p.kind {
        PaintKind::Solid(c) => to_c32(*c, p.opacity),
        PaintKind::LinearGradient { stops, .. } | PaintKind::RadialGradient { stops, .. } | PaintKind::AngularGradient { stops, .. } | PaintKind::DiamondGradient { stops, .. } => {
            stops.first().map(|s| to_c32(s.color, p.opacity)).unwrap_or(Color32::GRAY)
        }
        PaintKind::Image { .. } => Color32::from_gray(150),
    }
}

fn edit_paint_list(app: &mut App, sel: &[NodeId], stroke: bool, f: impl Fn(&mut Vec<Paint>)) {
    app.edit_nodes(sel, |n| {
        if stroke {
            f(&mut n.stroke.paints)
        } else {
            f(&mut n.fills)
        }
    });
}

fn paint_rows(app: &mut App, ui: &mut Ui, pane: &mut Pane, sel: &[NodeId], paints: &[Paint], stroke: bool) {
    for (i, p) in paints.iter().enumerate() {
        let key = (stroke, i);
        let sw = pane.rect(theme::SWATCH_ROW_X, 4.0, theme::SWATCH_SIZE, theme::SWATCH_SIZE);
        let hex_r = pane.rect(theme::SWATCH_ROW_X + theme::SWATCH_SIZE + 8.0, 0.0, 96.0, theme::FIELD_HEIGHT);
        let op_r = pane.rect(theme::SWATCH_ROW_X + theme::SWATCH_SIZE + 8.0 + 96.0 + 4.0, 0.0, theme::OPACITY_FIELD_W, theme::FIELD_HEIGHT);
        let eye_r = pane.rect(theme::RIGHT_NAME_ACTION_X[1] - 1.0 + 4.0, 0.0, theme::PANEL_ACTION_BUTTON, theme::PANEL_ACTION_BUTTON);
        let minus_r = pane.rect(theme::SECTION_TRAILING_X, 0.0, theme::PANEL_ACTION_BUTTON, theme::PANEL_ACTION_BUTTON);
        let swr = ui.interact(sw, ui.id().with(("sw", key)), Sense::click());
        widgets::swatch(ui.painter(), sw, paint_preview(p));
        if swr.clicked() {
            if let PaintKind::Solid(c) = &p.kind {
                let target = if stroke { PickTarget::Stroke(i) } else { PickTarget::Fill(i) };
                open_picker(app, target, to_c32(*c, p.opacity), sw, sel.to_vec());
            }
        }
        match &p.kind {
            PaintKind::Solid(c) => {
                if let Some(s) = field::text_input(ui, hex_r, ("hex", key), &c.to_hex()) {
                    if let Some(nc) = Color::from_hex(&s) {
                        edit_paint_list(app, sel, stroke, |ps| {
                            if let Some(p) = ps.get_mut(i) {
                                p.kind = PaintKind::Solid(Color { a: c.a, ..nc });
                            }
                        });
                    }
                }
            }
            _ => {
                field::field_bg(ui, hex_r, ui.id().with(("ro", key)));
                ui.painter().text(Pos2::new(hex_r.left() + 8.0, hex_r.center().y), Align2::LEFT_CENTER, paint_label(p), theme::font_body(), theme::TEXT);
            }
        }
        if let Some(v) = field::num_field(ui, op_r, ("op", key), Prefix::None, Some((p.opacity * 100.0).round()), "%") {
            let o = (v / 100.0).clamp(0.0, 1.0);
            edit_paint_list(app, sel, stroke, |ps| {
                if let Some(p) = ps.get_mut(i) {
                    p.opacity = o;
                }
            });
        }
        if widgets::icon_button(ui, eye_r, ("peye", key), if p.visible { Icon::Eye } else { Icon::EyeOff }, false, "").clicked() {
            edit_paint_list(app, sel, stroke, |ps| {
                if let Some(p) = ps.get_mut(i) {
                    p.visible = !p.visible;
                }
            });
        }
        if widgets::icon_button(ui, minus_r, ("pminus", key), Icon::Minus, false, "").clicked() {
            edit_paint_list(app, sel, stroke, |ps| {
                if i < ps.len() {
                    ps.remove(i);
                }
            });
        }
        pane.y += theme::FIELD_HEIGHT + 8.0;
    }
    if !paints.is_empty() {
        pane.y += 0.0;
    }
}

fn effect_name(e: &Effect) -> (&'static str, bool) {
    match e {
        Effect::DropShadow { visible, .. } => ("Drop shadow", *visible),
        Effect::InnerShadow { visible, .. } => ("Inner shadow", *visible),
        Effect::LayerBlur { visible, .. } => ("Layer blur", *visible),
        Effect::BackgroundBlur { visible, .. } => ("Background blur", *visible),
    }
}

fn effect_rows(app: &mut App, ui: &mut Ui, pane: &mut Pane, sel: &[NodeId], effects: &[Effect]) {
    let kinds = ["Drop shadow", "Inner shadow", "Layer blur", "Background blur"].map(String::from);
    for (i, e) in effects.iter().enumerate() {
        let (name, visible) = effect_name(e);
        let dd = pane.rect(theme::SECTION_CONTENT_X, 0.0, 128.0, theme::FIELD_HEIGHT);
        if let Some(k) = dropdown(ui, dd, ("fx-kind", i), name, &kinds) {
            let new = convert_effect(e, k);
            app.edit_nodes(sel, |n| {
                if let Some(slot) = n.effects.get_mut(i) {
                    *slot = new.clone();
                }
            });
        }
        let eye_r = pane.rect(theme::RIGHT_NAME_ACTION_X[1] - 1.0 + 4.0, 0.0, theme::PANEL_ACTION_BUTTON, theme::PANEL_ACTION_BUTTON);
        if widgets::icon_button(ui, eye_r, ("fx-eye", i), if visible { Icon::Eye } else { Icon::EyeOff }, false, "").clicked() {
            app.edit_nodes(sel, |n| {
                if let Some(e) = n.effects.get_mut(i) {
                    set_effect_visible(e, !visible);
                }
            });
        }
        let minus_r = pane.rect(theme::SECTION_TRAILING_X, 0.0, theme::PANEL_ACTION_BUTTON, theme::PANEL_ACTION_BUTTON);
        if widgets::icon_button(ui, minus_r, ("fx-minus", i), Icon::Minus, false, "").clicked() {
            app.edit_nodes(sel, |n| {
                if i < n.effects.len() {
                    n.effects.remove(i);
                }
            });
        }
        pane.y += theme::FIELD_HEIGHT + 8.0;
        match e {
            Effect::DropShadow { color, offset, radius, spread, .. } | Effect::InnerShadow { color, offset, radius, spread, .. } => {
                let (ox, oy, r, s, c) = (offset.x, offset.y, *radius, *spread, *color);
                let mut upd: Option<Box<dyn Fn(&mut Effect)>> = None;
                if let Some(v) = field::num_field(ui, pane.col(0, 0.0), ("fx-x", i), Prefix::Text("X"), Some(ox), "") {
                    upd = Some(Box::new(move |e| set_shadow(e, |_, o, _, _| o.x = v)));
                }
                if let Some(v) = field::num_field(ui, pane.col(1, 0.0), ("fx-y", i), Prefix::Text("Y"), Some(oy), "") {
                    upd = Some(Box::new(move |e| set_shadow(e, |_, o, _, _| o.y = v)));
                }
                pane.y += theme::FIELD_HEIGHT + 8.0;
                if let Some(v) = field::num_field(ui, pane.col(0, 0.0), ("fx-b", i), Prefix::Text("B"), Some(r), "") {
                    upd = Some(Box::new(move |e| set_shadow(e, |_, _, rr, _| *rr = v.max(0.0))));
                }
                if let Some(v) = field::num_field(ui, pane.col(1, 0.0), ("fx-s", i), Prefix::Text("S"), Some(s), "") {
                    upd = Some(Box::new(move |e| set_shadow(e, |_, _, _, sp| *sp = v)));
                }
                pane.y += theme::FIELD_HEIGHT + 8.0;
                let sw = pane.rect(theme::SWATCH_ROW_X, 4.0, theme::SWATCH_SIZE, theme::SWATCH_SIZE);
                let hex_r = pane.rect(theme::SWATCH_ROW_X + theme::SWATCH_SIZE + 8.0, 0.0, 96.0, theme::FIELD_HEIGHT);
                let swr = ui.interact(sw, ui.id().with(("fx-sw", i)), Sense::click());
                widgets::swatch(ui.painter(), sw, to_c32(c, 1.0));
                if swr.clicked() {
                    open_picker(app, PickTarget::Shadow(i), to_c32(c, 1.0), sw, sel.to_vec());
                }
                if let Some(s) = field::text_input(ui, hex_r, ("fx-hex", i), &c.to_hex()) {
                    if let Some(nc) = Color::from_hex(&s) {
                        upd = Some(Box::new(move |e| set_shadow(e, |col, _, _, _| *col = Color { a: c.a, ..nc })));
                    }
                }
                pane.y += theme::FIELD_HEIGHT + 8.0;
                if let Some(u) = upd {
                    app.edit_nodes(sel, |n| {
                        if let Some(e) = n.effects.get_mut(i) {
                            u(e);
                        }
                    });
                }
            }
            Effect::LayerBlur { radius, .. } | Effect::BackgroundBlur { radius, .. } => {
                if let Some(v) = field::num_field(ui, pane.col(0, 0.0), ("fx-blur", i), Prefix::Text("B"), Some(*radius), "") {
                    app.edit_nodes(sel, |n| {
                        if let Some(e) = n.effects.get_mut(i) {
                            match e {
                                Effect::LayerBlur { radius, .. } | Effect::BackgroundBlur { radius, .. } => *radius = v.max(0.0),
                                _ => {}
                            }
                        }
                    });
                }
                pane.y += theme::FIELD_HEIGHT + 8.0;
            }
        }
    }
}

fn set_shadow(e: &mut Effect, f: impl FnOnce(&mut Color, &mut DVec2, &mut f32, &mut f32)) {
    match e {
        Effect::DropShadow { color, offset, radius, spread, .. } | Effect::InnerShadow { color, offset, radius, spread, .. } => f(color, offset, radius, spread),
        _ => {}
    }
}

fn set_effect_visible(e: &mut Effect, v: bool) {
    match e {
        Effect::DropShadow { visible, .. } | Effect::InnerShadow { visible, .. } | Effect::LayerBlur { visible, .. } | Effect::BackgroundBlur { visible, .. } => *visible = v,
    }
}

fn convert_effect(e: &Effect, kind: usize) -> Effect {
    let (color, offset, radius, spread, visible) = match e {
        Effect::DropShadow { color, offset, radius, spread, visible, .. } | Effect::InnerShadow { color, offset, radius, spread, visible } => (*color, *offset, *radius, *spread, *visible),
        Effect::LayerBlur { radius, visible } | Effect::BackgroundBlur { radius, visible } => (Color::rgba(0, 0, 0, 64), DVec2::new(0.0, 4.0), *radius, 0.0, *visible),
    };
    match kind {
        0 => Effect::DropShadow { color, offset, radius, spread, visible, show_behind: false },
        1 => Effect::InnerShadow { color, offset, radius, spread, visible },
        2 => Effect::LayerBlur { radius, visible },
        _ => Effect::BackgroundBlur { radius, visible },
    }
}

// ---------------------------------------------------------------- color picker
fn open_picker(app: &mut App, target: PickTarget, color: Color32, anchor: Rect, ids: Vec<NodeId>) {
    app.props.picker = Some(Picker { target, tx: Tx::new(), color, anchor, ids });
}

fn apply_picker(doc: &mut Document, p: &mut Picker) {
    let c = from_c32(p.color);
    for id in &p.ids {
        let Some(n) = p.tx.ed.node(doc, *id) else { continue };
        match p.target {
            PickTarget::Fill(i) => {
                if let Some(pt) = n.fills.get_mut(i) {
                    pt.kind = PaintKind::Solid(Color { a: 1.0, ..c });
                    pt.opacity = c.a;
                }
            }
            PickTarget::Stroke(i) => {
                if let Some(pt) = n.stroke.paints.get_mut(i) {
                    pt.kind = PaintKind::Solid(Color { a: 1.0, ..c });
                    pt.opacity = c.a;
                }
            }
            PickTarget::PageBg => n.kind = NodeKind::Page { background: c },
            PickTarget::Shadow(i) => {
                if let Some(e) = n.effects.get_mut(i) {
                    set_shadow(e, |col, _, _, _| *col = c);
                }
            }
        }
    }
}

fn picker_popup(app: &mut App, ui: &mut Ui, region: Rect) {
    let Some(mut pk) = app.props.picker.take() else { return };
    let ctx = ui.ctx().clone();
    let pos = Pos2::new(region.left() - 232.0, (pk.anchor.top() - 20.0).max(region.top() - 40.0));
    let mut changed = false;
    let area = egui::Area::new(egui::Id::new("color-picker")).order(egui::Order::Foreground).fixed_pos(pos).show(&ctx, |ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(224.0, 250.0), Sense::hover());
        widgets::elevation_shadow(ui.painter(), rect, theme::MENU_RADIUS);
        ui.painter().rect_filled(rect, theme::cr(theme::MENU_RADIUS), theme::BG_MENU);
        ui.painter().rect_stroke(rect, theme::cr(theme::MENU_RADIUS), Stroke::new(1.0, theme::BORDER_MENU), StrokeKind::Inside);
        let inner = rect.shrink(8.0);
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
            ui.spacing_mut().slider_width = 180.0;
            changed = egui::color_picker::color_picker_color32(ui, &mut pk.color, egui::color_picker::Alpha::OnlyBlend);
        });
        rect
    });
    if changed {
        pk.tx.ed.revert(&mut app.doc);
        apply_picker(&mut app.doc, &mut pk);
        app.changed();
    }
    let (pressed, ptr, esc) = ctx.input(|i| (i.pointer.any_pressed(), i.pointer.interact_pos(), i.key_pressed(egui::Key::Escape)));
    let outside = pressed && ptr.map(|p| !area.inner.contains(p) && !pk.anchor.contains(p)).unwrap_or(false);
    if outside || esc {
        let tx = pk.tx;
        app.run_tx(tx);
    } else {
        app.props.picker = Some(pk);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_conversion_roundtrip() {
        let c = Color::rgba(0x12, 0x34, 0x56, 255);
        let back = from_c32(to_c32(c, 1.0));
        assert_eq!(back.to_hex(), c.to_hex());
        let half = to_c32(c, 0.5);
        assert!((half.a() as i32 - 128).abs() <= 1);
    }

    #[test]
    fn effect_conversion_keeps_radius() {
        let e = Effect::DropShadow { color: Color::BLACK, offset: DVec2::new(1.0, 2.0), radius: 7.0, spread: 0.0, visible: true, show_behind: false };
        assert!(matches!(convert_effect(&e, 2), Effect::LayerBlur { radius, .. } if radius == 7.0));
        assert!(matches!(convert_effect(&e, 1), Effect::InnerShadow { radius, .. } if radius == 7.0));
    }

    #[test]
    fn sizing_reflects_hug_frames_and_text() {
        let mut n = Node::frame(NodeId(1), desigma_doc::Rect::new(0.0, 0.0, 10.0, 10.0));
        n.kind = NodeKind::Frame(FrameProps { auto_layout: Some(AutoLayout { primary_sizing: Sizing::Hug, ..Default::default() }), ..Default::default() });
        assert_eq!(sizing_of(&n, true), Sizing::Hug);
        assert_eq!(sizing_of(&n, false), Sizing::Fixed);
    }
}
