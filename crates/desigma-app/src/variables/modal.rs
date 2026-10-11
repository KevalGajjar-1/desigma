//! The "Local variables" modal: collections sidebar plus a table of variables
//! grouped by "/" folders with one column per mode.

use super::widgets::{from_c32, hexagon, kind_glyph, kind_name, picker, pill, to_c32, PickItem, PickResult};
use super::{AliasPicker, CellPicker, RowDrag, VarMenu, VarMenuKind};
use crate::app::App;
use crate::icons::{self, Icon};
use crate::panels::{clipped, wheel_scroll};
use crate::theme;
use crate::widgets::field;
use crate::widgets::menu::{popup_menu, MenuItem, MenuResult};
use crate::widgets::{self, fmt_num};
use desigma_doc::*;
use egui::{Align2, Color32, Context, Id, Key, Order, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};

const HEADER_H: f32 = 48.0;
const SIDEBAR_W: f32 = 216.0;
const TOOLBAR_H: f32 = 48.0;
const HEAD_ROW_H: f32 = 32.0;
const ROW_H: f32 = 36.0;
const FOLDER_H: f32 = 28.0;
const NAME_W: f32 = 280.0;
const COL_MIN: f32 = 156.0;
const COL_MAX: f32 = 260.0;
const COLL_ROW_H: f32 = 32.0;

/// Modal rectangle for a window of size `screen`.
pub fn modal_rect(screen: Rect) -> Rect {
    let size = Vec2::new((screen.width() - 96.0).clamp(560.0, 1000.0), (screen.height() - 96.0).clamp(360.0, 600.0));
    Rect::from_center_size(screen.center(), size)
}

/// What one frame of the modal wants to change.
struct Frame<'a> {
    view: &'a VariableStore,
    work: VariableStore,
    removed_vars: Vec<String>,
    removed_colls: Vec<String>,
    /// Variable rows drawn this frame: (id, top, bottom), for drag-and-drop.
    rows: Vec<(String, f32, f32)>,
    close: bool,
}

pub fn show(app: &mut App, ctx: &Context) {
    if !app.vars.open {
        // Pickers opened from the inspector live independently of the modal.
        super::super::panels::var_bind::bind_picker(app, ctx);
        return;
    }
    let screen = ctx.content_rect();
    let m = modal_rect(screen);
    let view = VariableStore::from_doc(&app.doc);
    if !app.vars.collection.as_ref().is_some_and(|c| view.collections.contains_key(c)) {
        app.vars.collection = view.collections.keys().next().cloned();
    }
    let mut fr = Frame { view: &view, work: view.clone(), removed_vars: vec![], removed_colls: vec![], rows: vec![], close: false };
    let popup_open = app.vars.menu.is_some() || app.vars.alias_picker.is_some() || app.vars.cell_picker.is_some();
    let esc = !popup_open && !ctx.egui_wants_keyboard_input() && ctx.input(|i| i.key_pressed(Key::Escape));

    egui::Area::new(Id::new("vars-modal")).order(Order::Foreground).fixed_pos(screen.min).constrain(false).show(ctx, |ui| {
        ui.allocate_rect(screen, Sense::hover());
        let scrim = ui.interact(screen, Id::new("vars-scrim"), Sense::click_and_drag());
        ui.painter().rect_filled(screen, 0.0, Color32::from_black_alpha(120));
        let p = ui.painter().clone();
        widgets::elevation_400(&p, m, theme::MENU_RADIUS);

        p.rect_filled(m, theme::cr(theme::MENU_RADIUS), theme::BG);
        p.rect_stroke(m, theme::cr(theme::MENU_RADIUS), Stroke::new(1.0, theme::ELEVATION_400_RING), StrokeKind::Inside);
        ui.interact(m, Id::new("vars-body"), Sense::click_and_drag());
        draw_header(app, ui, m, &mut fr);
        let body = Rect::from_min_max(Pos2::new(m.left(), m.top() + HEADER_H), m.max);
        draw_sidebar(app, ui, body, &mut fr);
        draw_main(app, ui, body, &mut fr);
        if scrim.clicked() && ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| !m.contains(p)) {
            fr.close = true;
        }
    });

    run_popups(app, ctx, &mut fr);
    let (work, rv, rc, close) = (fr.work, fr.removed_vars, fr.removed_colls, fr.close);
    // Only when something was edited: a live colour-picker session changes the document itself.
    if work != view {
        app.commit_store(work, &rv, &rc);
    }
    if close || esc {
        app.close_variables();
    }
}

fn draw_header(app: &mut App, ui: &mut Ui, m: Rect, fr: &mut Frame) {
    let p = ui.painter().clone();
    p.text(Pos2::new(m.left() + 20.0, m.top() + HEADER_H / 2.0), Align2::LEFT_CENTER, "Local variables", theme::font_large_strong(), theme::TEXT);
    widgets::hline(&p, m.left(), m.right(), m.top() + HEADER_H - 1.0, theme::BORDER);
    // close
    let cb = Rect::from_center_size(Pos2::new(m.right() - 28.0, m.top() + HEADER_H / 2.0), Vec2::splat(28.0));
    let r = ui.interact(cb, Id::new("vars-close"), Sense::click());
    widgets::button_bg(&p, cb, &r, false, theme::cr(theme::RADIUS_MEDIUM));
    let c = cb.center();
    let st = Stroke::new(1.3, theme::ICON);
    p.line_segment([c + Vec2::new(-4.5, -4.5), c + Vec2::new(4.5, 4.5)], st);
    p.line_segment([c + Vec2::new(-4.5, 4.5), c + Vec2::new(4.5, -4.5)], st);
    if r.clicked() {
        fr.close = true;
    }
    // import / export
    let mut x = cb.left() - 8.0;
    for (label, key) in [("Export", "vars-export"), ("Import", "vars-import")] {
        let w = widgets::text_width(&p, label, &theme::font_body()) + 20.0;
        let b = Rect::from_min_max(Pos2::new(x - w, m.top() + 12.0), Pos2::new(x, m.top() + 36.0));
        let r = ui.interact(b, Id::new(key), Sense::click());
        widgets::button_bg(&p, b, &r, false, theme::cr(theme::RADIUS_MEDIUM));
        p.text(b.center(), Align2::CENTER_CENTER, label, theme::font_body(), theme::TEXT_SECONDARY);
        widgets::record(ui, key, b);
        if r.clicked() {
            if key == "vars-import" {
                if let Some(path) = rfd::FileDialog::new().add_filter("Variables JSON", &["json"]).pick_file() {
                    app.import_variables_file(&path);
                }
            } else if let Some(path) = rfd::FileDialog::new().add_filter("Variables JSON", &["json"]).set_file_name("variables.json").save_file() {
                app.export_variables_file(&path);
            }
        }
        x -= w + 4.0;
    }
}

// ------------------------------------------------------------------ sidebar

fn draw_sidebar(app: &mut App, ui: &mut Ui, body: Rect, fr: &mut Frame) {
    let side = Rect::from_min_size(body.min, Vec2::new(SIDEBAR_W, body.height()));
    let p = ui.painter().clone();
    widgets::vline(&p, side.right(), side.top(), side.bottom(), theme::BORDER);
    p.text(Pos2::new(side.left() + 20.0, side.top() + 24.0), Align2::LEFT_CENTER, "Collections", theme::font_body_strong(), theme::TEXT_SECONDARY);
    let add = Rect::from_center_size(Pos2::new(side.right() - 24.0, side.top() + 24.0), Vec2::splat(24.0));
    widgets::record(ui, "vars-add-collection", add);
    if widgets::icon_button(ui, add, "vars-add-coll", Icon::Plus, false, "Create collection").clicked() {
        let n = fr.work.collections.len() + 1;
        let id = fr.work.add_collection(&format!("Collection {n}"));
        app.vars.collection = Some(id.clone());
        app.vars.renaming = Some(('c', id));
        app.vars.rename_frames = 0;
        app.vars.scroll = 0.0;
    }
    let mut y = side.top() + 48.0;
    let ids: Vec<String> = fr.view.collections.keys().cloned().collect();
    for id in ids {
        let c = &fr.view.collections[&id];
        let row = Rect::from_min_size(Pos2::new(side.left() + 8.0, y), Vec2::new(side.width() - 16.0, COLL_ROW_H));
        y += COLL_ROW_H + 2.0;
        widgets::record(ui, &format!("vars-coll:{}", c.name), row);
        let resp = ui.interact(row, Id::new(("vars-coll", &id)), Sense::click());
        let selected = app.vars.collection.as_deref() == Some(id.as_str());
        if selected {
            p.rect_filled(row, theme::cr(theme::RADIUS_MEDIUM), theme::BG_SELECTED);
        } else if resp.hovered() {
            p.rect_filled(row, theme::cr(theme::RADIUS_MEDIUM), theme::BG_HOVER);
        }
        collection_glyph(&p, Pos2::new(row.left() + 18.0, row.center().y));
        let name_rect = Rect::from_min_max(Pos2::new(row.left() + 32.0, row.top()), Pos2::new(row.right() - 36.0, row.bottom()));
        if app.vars.renaming == Some(('c', id.clone())) {
            let eid = Id::new(("vars-coll-name", &id));
            if app.vars.rename_frames == 0 {
                ui.ctx().memory_mut(|mem| mem.request_focus(eid));
            }
            app.vars.rename_frames += 1;
            p.rect_filled(name_rect, theme::cr(theme::RADIUS_MEDIUM), theme::BG_SECONDARY);
            if let Some(s) = field::edit_text(ui, name_rect, eid, &c.name, theme::font_body(), theme::TEXT, 6.0) {
                if !s.trim().is_empty() {
                    fr.work.rename_collection(&id, s.trim());
                }
                app.vars.renaming = None;
            } else if app.vars.rename_frames > 2 && !ui.memory(|mem| mem.has_focus(eid)) {
                app.vars.renaming = None;
            }
        } else {
            let nm = widgets::elide(&p, &c.name, &theme::font_body(), name_rect.width());
            p.text(Pos2::new(name_rect.left() + 6.0, row.center().y), Align2::LEFT_CENTER, nm, theme::font_body(), theme::TEXT);
            p.text(Pos2::new(row.right() - 12.0, row.center().y), Align2::RIGHT_CENTER, c.variables.len().to_string(), theme::font_body(), theme::TEXT_TERTIARY);
        }
        if resp.clicked() && !selected {
            app.vars.collection = Some(id.clone());
            app.vars.scroll = 0.0;
            app.vars.scroll_x = 0.0;
        }
        if resp.double_clicked() {
            app.vars.renaming = Some(('c', id.clone()));
            app.vars.rename_frames = 0;
        }
        if resp.secondary_clicked() {
            let pos = ui.input(|i| i.pointer.interact_pos()).unwrap_or(row.left_bottom());
            app.vars.menu = Some(VarMenu { kind: VarMenuKind::Collection(id.clone()), pos, anchor: Rect::from_min_size(pos, Vec2::ZERO) });
        }
    }
    if fr.view.collections.is_empty() {
        p.text(Pos2::new(side.left() + 20.0, y + 8.0), Align2::LEFT_TOP, "No collections yet.", theme::font_body(), theme::TEXT_TERTIARY);
    }
}

// ------------------------------------------------------------------ main

struct Groups<'a> {
    folder: String,
    vars: Vec<&'a Variable>,
}

fn group_variables<'a>(view: &'a VariableStore, coll: &VariableCollection, query: &str) -> Vec<Groups<'a>> {
    let q = query.to_lowercase();
    let mut out: Vec<Groups> = Vec::new();
    for id in &coll.variables {
        let Some(v) = view.variables.get(id) else { continue };
        if !q.is_empty() && !v.name.to_lowercase().contains(&q) {
            continue;
        }
        let folder = split_name(&v.name).0.to_string();
        match out.iter_mut().find(|g| g.folder == folder) {
            Some(g) => g.vars.push(v),
            None => out.push(Groups { folder, vars: vec![v] }),
        }
    }
    out
}

fn draw_main(app: &mut App, ui: &mut Ui, body: Rect, fr: &mut Frame) {
    let main = Rect::from_min_max(Pos2::new(body.left() + SIDEBAR_W + 1.0, body.top()), body.max);
    let p = ui.painter().clone();
    let Some(cid) = app.vars.collection.clone() else {
        p.text(main.center() - Vec2::new(0.0, 16.0), Align2::CENTER_CENTER, "No variables yet", theme::font_large_strong(), theme::TEXT);
        p.text(main.center() + Vec2::new(0.0, 8.0), Align2::CENTER_CENTER, "Create a collection to start defining variables.", theme::font_body(), theme::TEXT_SECONDARY);
        return;
    };
    let Some(coll) = fr.view.collections.get(&cid).cloned() else { return };

    // ---- toolbar: create variable + search
    let create = Rect::from_min_size(Pos2::new(main.left() + 16.0, main.top() + 12.0), Vec2::new(132.0, 24.0));
    let cr_resp = ui.interact(create, Id::new("vars-create"), Sense::click());
    p.rect_filled(create, theme::cr(theme::RADIUS_MEDIUM), if cr_resp.hovered() { theme::BG_BRAND_HOVER } else { theme::BG_BRAND });
    icons::paint(&p, Icon::Plus, Pos2::new(create.left() + 16.0, create.center().y), 12.0, theme::TEXT);
    p.text(Pos2::new(create.left() + 30.0, create.center().y), Align2::LEFT_CENTER, "Create variable", theme::font_body_strong(), theme::TEXT);
    widgets::record(ui, "vars-create", create);
    if cr_resp.clicked() {
        app.vars.menu = Some(VarMenu { kind: VarMenuKind::Add, pos: create.left_bottom() + Vec2::new(0.0, 4.0), anchor: create });
    }
    let search = Rect::from_min_size(Pos2::new(main.right() - 16.0 - 220.0, main.top() + 12.0), Vec2::new(220.0, 24.0));
    p.rect_filled(search, theme::cr(theme::RADIUS_MEDIUM), theme::BG_SECONDARY);
    icons::paint(&p, Icon::Search, Pos2::new(search.left() + 14.0, search.center().y), 12.0, theme::ICON_TERTIARY);
    let te = egui::TextEdit::singleline(&mut app.vars.search)
        .id(Id::new("vars-search"))
        .hint_text("Search variables")
        .font(theme::font_body())
        .text_color(theme::TEXT)
        .frame(egui::Frame::NONE)
        .margin(egui::Margin::symmetric(8, 0))
        .vertical_align(egui::Align::Center);
    ui.put(Rect::from_min_max(search.min + Vec2::new(24.0, 0.0), search.max), te);
    widgets::hline(&p, main.left(), main.right(), main.top() + TOOLBAR_H, theme::BORDER);

    // ---- column geometry
    let n_modes = coll.modes.len().max(1);
    let avail = main.width() - NAME_W - 48.0;
    let col_w = (avail / n_modes as f32).clamp(COL_MIN, COL_MAX);
    let modes_left = main.left() + NAME_W;
    let modes_rect = Rect::from_min_max(Pos2::new(modes_left, main.top() + TOOLBAR_H + 1.0), main.max);
    let total_w = col_w * n_modes as f32 + 48.0;
    let max_sx = (total_w - modes_rect.width()).max(0.0);
    if ui.rect_contains_pointer(modes_rect) {
        app.vars.scroll_x = (app.vars.scroll_x - ui.input(|i| i.smooth_scroll_delta.x)).clamp(0.0, max_sx);
    }
    app.vars.scroll_x = app.vars.scroll_x.clamp(0.0, max_sx);
    let sx = app.vars.scroll_x;

    // ---- header row
    let head_top = main.top() + TOOLBAR_H + 1.0;
    let head = Rect::from_min_size(Pos2::new(main.left(), head_top), Vec2::new(main.width(), HEAD_ROW_H));
    p.text(Pos2::new(main.left() + 20.0, head.center().y), Align2::LEFT_CENTER, "Name", theme::font_body_strong(), theme::TEXT_SECONDARY);
    widgets::hline(&p, main.left(), main.right(), head.bottom(), theme::BORDER);
    clipped(ui, Rect::from_min_max(Pos2::new(modes_left, head.top()), head.max), |ui| {
        let p = ui.painter().clone();
        for (i, mode) in coll.modes.iter().enumerate() {
            let cell = Rect::from_min_size(Pos2::new(modes_left - sx + i as f32 * col_w, head.top()), Vec2::new(col_w, HEAD_ROW_H));
            widgets::vline(&p, cell.left(), cell.top(), cell.bottom(), theme::BORDER);
            let name_r = Rect::from_min_max(cell.min + Vec2::new(8.0, 4.0), cell.max - Vec2::new(30.0, 4.0));
            let eid = Id::new(("vars-mode-name", &coll.id, &mode.id));
            cell_chrome(ui, name_r, eid);
            widgets::record(ui, &format!("vars-mode:{}", mode.name), name_r);
            if let Some(s) = field::edit_text(ui, name_r, eid, &mode.name, theme::font_body_strong(), theme::TEXT, 6.0) {
                if !s.trim().is_empty() {
                    fr.work.rename_mode(&coll.id, &mode.id, s.trim());
                }
            }
            if coll.default_mode == mode.id && coll.modes.len() > 1 {
                let w = widgets::text_width(&p, &mode.name, &theme::font_body_strong());
                if w + 60.0 < name_r.width() {
                    p.text(Pos2::new(name_r.left() + 6.0 + w + 6.0, name_r.center().y), Align2::LEFT_CENTER, "default", theme::font_small(), theme::TEXT_TERTIARY);
                }
            }
            let more = Rect::from_center_size(Pos2::new(cell.right() - 16.0, cell.center().y), Vec2::splat(20.0));
            let mr = ui.interact(more, Id::new(("vars-mode-more", &coll.id, &mode.id)), Sense::click());
            widgets::button_bg(&p, more, &mr, false, theme::cr(theme::RADIUS_MEDIUM));
            icons::paint(&p, Icon::ChevronDown, more.center(), 12.0, theme::ICON_SECONDARY);
            if mr.clicked() || ui.interact(name_r, eid.with("ctx"), Sense::click()).secondary_clicked() {
                app.vars.menu = Some(VarMenu { kind: VarMenuKind::Mode(coll.id.clone(), mode.id.clone()), pos: more.left_bottom() + Vec2::new(0.0, 4.0), anchor: more });
            }
        }
        // add mode
        let ab = Rect::from_min_size(Pos2::new(modes_left - sx + n_modes as f32 * col_w + 8.0, head.top() + 4.0), Vec2::splat(24.0));
        widgets::record(ui, "vars-add-mode", ab);
        if widgets::icon_button(ui, ab, "vars-add-mode", Icon::Plus, false, "Add mode").clicked() {
            let n = coll.modes.len() + 1;
            fr.work.add_mode(&coll.id, &format!("Mode {n}"));
        }
    });

    // ---- body rows
    let rows_rect = Rect::from_min_max(Pos2::new(main.left(), head.bottom() + 1.0), main.max);
    let groups = group_variables(fr.view, &coll, &app.vars.search);
    let folded_key = |f: &str| format!("{}\0{}", coll.id, f);
    let mut content_h = 8.0;
    for g in &groups {
        if !g.folder.is_empty() {
            content_h += FOLDER_H;
            if app.vars.folded.contains(&folded_key(&g.folder)) {
                continue;
            }
        }
        content_h += ROW_H * g.vars.len() as f32;
    }
    let mut scroll = app.vars.scroll;
    wheel_scroll(ui, rows_rect, &mut scroll, content_h);
    app.vars.scroll = scroll;

    if groups.is_empty() {
        let msg = if app.vars.search.is_empty() { "No variables in this collection" } else { "No matching variables" };
        p.text(rows_rect.center_top() + Vec2::new(0.0, 56.0), Align2::CENTER_CENTER, msg, theme::font_body(), theme::TEXT_TERTIARY);
    }

    clipped(ui, rows_rect, |ui| {
        let mut y = rows_rect.top() + 4.0 - scroll;
        for g in &groups {
            if !g.folder.is_empty() {
                let r = Rect::from_min_size(Pos2::new(main.left(), y), Vec2::new(main.width(), FOLDER_H));
                y += FOLDER_H;
                if r.bottom() >= rows_rect.top() && r.top() <= rows_rect.bottom() {
                    folder_row(app, ui, r, &coll.id, &g.folder, folded_key(&g.folder), fr);
                }
                if app.vars.folded.contains(&folded_key(&g.folder)) {
                    continue;
                }
            }
            for v in &g.vars {
                let r = Rect::from_min_size(Pos2::new(main.left(), y), Vec2::new(main.width(), ROW_H));
                y += ROW_H;
                if r.bottom() < rows_rect.top() || r.top() > rows_rect.bottom() {
                    continue;
                }
                fr.rows.push((v.id.clone(), r.top(), r.bottom()));
                variable_row(app, ui, r, &coll, v, !g.folder.is_empty(), modes_left, sx, col_w, fr);
            }
        }
        // drag indicator + drop
        if let Some(d) = &app.vars.drag {
            let pointer = ui.input(|i| i.pointer.latest_pos());
            if let Some(py) = pointer.map(|p| p.y) {
                let (before, line_y) = drop_target(&fr.rows, py, rows_rect);
                ui.painter().rect_filled(Rect::from_min_size(Pos2::new(main.left() + 8.0, line_y - 1.0), Vec2::new(main.width() - 16.0, 2.0)), 1.0, theme::DROP_INDICATOR);
                if ui.input(|i| i.pointer.any_released()) {
                    fr.work.move_variable(&d.var, before.as_deref());
                    app.vars.drag = None;
                }
            }
        }
    });
    if app.vars.drag.is_some() && !ui.input(|i| i.pointer.any_down()) {
        app.vars.drag = None;
    }
    scrollbar_v(ui, rows_rect, scroll, content_h);
}

/// Stacked-layers glyph of a collection row.
fn collection_glyph(p: &egui::Painter, c: Pos2) {
    let st = Stroke::new(1.2, theme::ICON_SECONDARY);
    p.rect_stroke(Rect::from_center_size(c + Vec2::new(1.5, -1.5), Vec2::new(9.0, 8.0)), 1.5, st, StrokeKind::Inside);
    p.rect_stroke(Rect::from_center_size(c + Vec2::new(-1.5, 1.5), Vec2::new(9.0, 8.0)), 1.5, st, StrokeKind::Inside);
}

fn scrollbar_v(ui: &Ui, rect: Rect, scroll: f32, content_h: f32) {
    crate::panels::scrollbar(ui, rect, scroll, content_h);
}

/// Row the dragged variable lands before (None = end) and the y of the indicator line.
fn drop_target(rows: &[(String, f32, f32)], py: f32, area: Rect) -> (Option<String>, f32) {
    for (id, top, bottom) in rows {
        if py < (top + bottom) / 2.0 {
            return (Some(id.clone()), *top);
        }
    }
    (None, rows.last().map(|r| r.2).unwrap_or(area.top() + 4.0))
}

fn cell_chrome(ui: &Ui, rect: Rect, id: Id) {
    let focused = ui.memory(|m| m.has_focus(id));
    let hovered = ui.rect_contains_pointer(rect);
    let p = ui.painter();
    if focused {
        p.rect_filled(rect, theme::cr(theme::RADIUS_MEDIUM), theme::BG_SECONDARY);
        p.rect_stroke(rect, theme::cr(theme::RADIUS_MEDIUM), Stroke::new(1.0, theme::BORDER_SELECTED), StrokeKind::Inside);
    } else if hovered {
        p.rect_filled(rect, theme::cr(theme::RADIUS_MEDIUM), theme::BG_SECONDARY);
    }
}

fn folder_row(app: &mut App, ui: &mut Ui, r: Rect, coll: &str, folder: &str, key: String, fr: &mut Frame) {
    let p = ui.painter().clone();
    let resp = ui.interact(Rect::from_min_size(r.min, Vec2::new(NAME_W, r.height())), Id::new(("vars-folder", &key)), Sense::click());
    if resp.hovered() {
        p.rect_filled(Rect::from_min_size(r.min, Vec2::new(NAME_W, r.height())), 0.0, theme::BG_HOVER);
    }
    let open = !app.vars.folded.contains(&key);
    icons::paint(&p, if open { Icon::ChevronDown } else { Icon::ChevronRight }, Pos2::new(r.left() + 20.0, r.center().y), 12.0, theme::ICON_TERTIARY);
    let renaming = app.vars.renaming == Some(('f', key.clone()));
    let name_r = Rect::from_min_max(Pos2::new(r.left() + 52.0, r.top() + 2.0), Pos2::new(r.left() + NAME_W - 8.0, r.bottom() - 2.0));
    // folder glyph
    let fc = Pos2::new(r.left() + 38.0, r.center().y);
    p.rect_stroke(Rect::from_center_size(fc, Vec2::new(12.0, 9.0)), 1.5, Stroke::new(1.1, theme::ICON_SECONDARY), StrokeKind::Inside);
    if renaming {
        let eid = Id::new(("vars-folder-name", &key));
        if app.vars.rename_frames == 0 {
            ui.ctx().memory_mut(|mem| mem.request_focus(eid));
        }
        app.vars.rename_frames += 1;
        p.rect_filled(name_r, theme::cr(theme::RADIUS_MEDIUM), theme::BG_SECONDARY);
        if let Some(s) = field::edit_text(ui, name_r, eid, folder, theme::font_body_strong(), theme::TEXT, 6.0) {
            fr.work.rename_folder(coll, folder, &s);
            app.vars.renaming = None;
        } else if app.vars.rename_frames > 2 && !ui.memory(|m| m.has_focus(eid)) {
            app.vars.renaming = None;
        }
    } else {
        p.text(Pos2::new(name_r.left(), r.center().y), Align2::LEFT_CENTER, folder, theme::font_body_strong(), theme::TEXT_SECONDARY);
    }
    if resp.clicked() {
        if open {
            app.vars.folded.insert(key.clone());
        } else {
            app.vars.folded.remove(&key);
        }
    }
    if resp.double_clicked() {
        app.vars.folded.remove(&key);
        app.vars.renaming = Some(('f', key.clone()));
        app.vars.rename_frames = 0;
    }
    if resp.secondary_clicked() {
        let pos = ui.input(|i| i.pointer.interact_pos()).unwrap_or(r.left_bottom());
        app.vars.menu = Some(VarMenu { kind: VarMenuKind::Folder(coll.to_string(), folder.to_string()), pos, anchor: Rect::from_min_size(pos, Vec2::ZERO) });
    }
}

#[allow(clippy::too_many_arguments)]
fn variable_row(app: &mut App, ui: &mut Ui, r: Rect, coll: &VariableCollection, v: &Variable, in_folder: bool, modes_left: f32, sx: f32, col_w: f32, fr: &mut Frame) {
    let p = ui.painter().clone();
    let hovered = ui.rect_contains_pointer(r) || app.vars.drag.as_ref().is_some_and(|d| d.var == v.id);
    if hovered {
        p.rect_filled(r, 0.0, Color32::from_white_alpha(6));
    }
    widgets::hline(&p, r.left(), r.right(), r.bottom() - 1.0, Color32::from_white_alpha(8));

    // ---- name cell (sticky)
    let indent = if in_folder { 20.0 } else { 0.0 };
    let name_cell = Rect::from_min_size(r.min, Vec2::new(NAME_W, r.height()));
    clipped(ui, name_cell, |ui| {
        let p = ui.painter().clone();
        p.rect_filled(name_cell, 0.0, theme::BG);
        if hovered {
            p.rect_filled(name_cell, 0.0, Color32::from_white_alpha(6));
        }
        // drag grip
        let grip = Rect::from_min_size(Pos2::new(r.left() + 4.0, r.top()), Vec2::new(16.0, r.height()));
        let gr = ui.interact(grip, Id::new(("vars-grip", &v.id)), Sense::drag());
        if hovered {
            for dy in [-4.0, 0.0, 4.0] {
                for dx in [-2.0, 2.0] {
                    p.circle_filled(grip.center() + Vec2::new(dx, dy), 1.0, theme::ICON_TERTIARY);
                }
            }
            if gr.hovered() || gr.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
        }
        if gr.drag_started() {
            app.vars.drag = Some(RowDrag { var: v.id.clone() });
        }
        kind_glyph(&p, Pos2::new(r.left() + 32.0 + indent, r.center().y), v.kind, theme::ICON_SECONDARY);
        let leaf = if in_folder { split_name(&v.name).1.to_string() } else { v.name.clone() };
        let name_r = Rect::from_min_max(Pos2::new(r.left() + 46.0 + indent, r.top() + 4.0), Pos2::new(r.left() + NAME_W - 32.0, r.bottom() - 4.0));
        let eid = Id::new(("vars-name", &v.id));
        if app.vars.focus_name.as_deref() == Some(v.id.as_str()) {
            ui.ctx().memory_mut(|m| m.request_focus(eid));
            app.vars.focus_name = None;
        }
        cell_chrome(ui, name_r, eid);
        widgets::record(ui, &format!("vars-name:{}", v.name), name_r);
        if let Some(s) = field::edit_text(ui, name_r, eid, &leaf, theme::font_body(), theme::TEXT, 6.0) {
            let (folder, _) = split_name(&v.name);
            let full = if in_folder && !s.contains('/') { format!("{folder}/{}", s.trim()) } else { s.trim().to_string() };
            fr.work.rename_variable(&v.id, &full);
        }
        // row menu
        let more = Rect::from_center_size(Pos2::new(r.left() + NAME_W - 18.0, r.center().y), Vec2::splat(20.0));
        let ctx_resp = ui.interact(r, Id::new(("vars-row-ctx", &v.id)), Sense::click());
        let mut open_menu = ctx_resp.secondary_clicked();
        if hovered {
            let mr = ui.interact(more, Id::new(("vars-row-more", &v.id)), Sense::click());
            widgets::button_bg(&p, more, &mr, false, theme::cr(theme::RADIUS_MEDIUM));
            for dx in [-4.0, 0.0, 4.0] {
                p.circle_filled(more.center() + Vec2::new(dx, 0.0), 1.1, theme::ICON_SECONDARY);
            }
            open_menu |= mr.clicked();
        }
        if open_menu {
            let pos = ui.input(|i| i.pointer.interact_pos()).filter(|_| ctx_resp.secondary_clicked()).unwrap_or(more.left_bottom());
            app.vars.menu = Some(VarMenu { kind: VarMenuKind::Row(v.id.clone()), pos, anchor: Rect::from_min_size(pos, Vec2::ZERO) });
        }
    });

    // ---- value cells
    let area = Rect::from_min_max(Pos2::new(modes_left, r.top()), r.max);
    clipped(ui, area, |ui| {
        for (i, mode) in coll.modes.iter().enumerate() {
            let cell = Rect::from_min_size(Pos2::new(modes_left - sx + i as f32 * col_w, r.top()), Vec2::new(col_w, r.height()));
            widgets::vline(ui.painter(), cell.left(), cell.top(), cell.bottom(), theme::BORDER);
            value_cell(app, ui, cell.shrink2(Vec2::new(6.0, 4.0)), v, &mode.id, fr);
        }
    });
}

fn value_cell(app: &mut App, ui: &mut Ui, r: Rect, v: &Variable, mode: &str, fr: &mut Frame) {
    let id = Id::new(("vars-cell", &v.id, mode));
    let value = v.values.get(mode).cloned().unwrap_or_else(|| default_value(v.kind));
    let p = ui.painter().clone();
    let hovered = ui.rect_contains_pointer(r);
    widgets::record(ui, &format!("vars-cell:{}:{}", v.name, mode), r);
    if let VariableValue::Alias(target) = &value {
        let tv = fr.view.variables.get(target);
        let swatch = match fr.view.resolve(&v.id, mode) {
            Some(VariableValue::Color(c)) if v.kind == VariableKind::Color => Some(to_c32(c)),
            _ => None,
        };
        let label = tv.map(|t| t.name.clone()).unwrap_or_else(|| "Missing variable".into());
        let pr = pill(ui, r, id, &label, swatch, true);
        if pr.detach {
            fr.work.detach_alias(&v.id, mode);
        } else if pr.body.clicked() {
            app.vars.alias_picker = Some(AliasPicker { var: v.id.clone(), mode: mode.to_string(), anchor: r, query: String::new() });
        }
        return;
    }
    let mut bind_hit = false;
    let bind_rect = Rect::from_center_size(Pos2::new(r.right() - 14.0, r.center().y), Vec2::splat(20.0));
    match &value {
        VariableValue::Color(c) => {
            let sw = Rect::from_center_size(Pos2::new(r.left() + 16.0, r.center().y), Vec2::splat(16.0));
            let hex_r = Rect::from_min_max(Pos2::new(sw.right() + 6.0, r.top()), Pos2::new(r.right() - 26.0, r.bottom()));
            let eid = id.with("hex");
            cell_chrome(ui, hex_r, eid);
            let swr = ui.interact(sw.expand(2.0), id.with("sw"), Sense::click());
            widgets::swatch(&p, sw, to_c32(*c));
            widgets::record(ui, &format!("vars-swatch:{}:{}", v.name, mode), sw);
            widgets::record(ui, &format!("vars-hex:{}:{}", v.name, mode), hex_r);
            if swr.clicked() {
                app.vars.cell_picker = Some(CellPicker { var: v.id.clone(), mode: mode.to_string(), color: to_c32(*c), anchor: sw, before: fr.view.clone() });
            }
            if let Some(s) = field::edit_text(ui, hex_r, eid, &c.to_hex(), theme::font_body(), theme::TEXT, 6.0) {
                if let Some(nc) = Color::from_hex(&s) {
                    fr.work.set_value(&v.id, mode, VariableValue::Color(Color { a: if s.trim_start_matches('#').len() == 8 { nc.a } else { c.a }, ..nc }));
                }
            }
        }
        VariableValue::Number(n) => {
            let eid = id.with("num");
            let edit_r = Rect::from_min_max(r.min, Pos2::new(r.right() - 26.0, r.bottom()));
            cell_chrome(ui, edit_r, eid);
            widgets::record(ui, &format!("vars-num:{}:{}", v.name, mode), edit_r);
            if let Some(s) = field::edit_text(ui, edit_r, eid, &fmt_num(*n), theme::font_body(), theme::TEXT, 8.0) {
                if let Some(x) = field::parse_number(&s) {
                    fr.work.set_value(&v.id, mode, VariableValue::Number(x));
                }
            }
        }
        VariableValue::String(s0) => {
            let eid = id.with("str");
            let edit_r = Rect::from_min_max(r.min, Pos2::new(r.right() - 26.0, r.bottom()));
            cell_chrome(ui, edit_r, eid);
            widgets::record(ui, &format!("vars-str:{}:{}", v.name, mode), edit_r);
            if let Some(s) = field::edit_text(ui, edit_r, eid, s0, theme::font_body(), theme::TEXT, 8.0) {
                fr.work.set_value(&v.id, mode, VariableValue::String(s));
            }
        }
        VariableValue::Boolean(b) => {
            let sw = Rect::from_min_size(Pos2::new(r.left() + 8.0, r.center().y - 8.0), Vec2::new(28.0, 16.0));
            let resp = ui.interact(Rect::from_min_max(r.min, Pos2::new(r.right() - 26.0, r.bottom())), id.with("tog"), Sense::click());
            widgets::record(ui, &format!("vars-bool:{}:{}", v.name, mode), sw);
            p.rect_filled(sw, theme::cr(255), if *b { theme::BG_BRAND } else { theme::BG_TERTIARY });
            let kx = if *b { sw.right() - 8.0 } else { sw.left() + 8.0 };
            p.circle_filled(Pos2::new(kx, sw.center().y), 6.0, Color32::WHITE);
            p.text(Pos2::new(sw.right() + 8.0, r.center().y), Align2::LEFT_CENTER, if *b { "True" } else { "False" }, theme::font_body(), theme::TEXT);
            if resp.clicked() {
                fr.work.set_value(&v.id, mode, VariableValue::Boolean(!b));
            }
        }
        VariableValue::Alias(_) => {}
    }
    if hovered {
        let br = ui.interact(bind_rect, id.with("bind"), Sense::click());
        if br.hovered() {
            p.rect_filled(bind_rect, theme::cr(theme::RADIUS_MEDIUM), theme::BG_PRESSED);
        }
        hexagon(&p, bind_rect.center(), 5.0, theme::ICON_SECONDARY, false);
        widgets::record(ui, &format!("vars-bind:{}:{}", v.name, mode), bind_rect);
        bind_hit = br.clicked();
    }
    if bind_hit {
        app.vars.alias_picker = Some(AliasPicker { var: v.id.clone(), mode: mode.to_string(), anchor: r, query: String::new() });
    }
}

// ------------------------------------------------------------------ popups

fn run_popups(app: &mut App, ctx: &Context, fr: &mut Frame) {
    run_menu(app, ctx, fr);
    run_alias_picker(app, ctx, fr);
    run_cell_picker(app, ctx);
    super::super::panels::var_bind::bind_picker(app, ctx);
}

fn run_menu(app: &mut App, ctx: &Context, fr: &mut Frame) {
    let Some(menu) = app.vars.menu.clone() else { return };
    let cid = app.vars.collection.clone().unwrap_or_default();
    let items: Vec<MenuItem> = match &menu.kind {
        VarMenuKind::Add => ["Color", "Number", "String", "Boolean"].iter().map(|s| MenuItem::new(*s)).collect(),
        VarMenuKind::Row(_) => vec![MenuItem::new("Rename"), MenuItem::new("Duplicate"), MenuItem::new("Delete")],
        VarMenuKind::Mode(coll, mode) => {
            let multi = fr.view.collections.get(coll).is_some_and(|c| c.modes.len() > 1);
            let is_default = fr.view.collections.get(coll).is_some_and(|c| &c.default_mode == mode);
            vec![MenuItem::new("Duplicate mode"), MenuItem::new("Set as default").disabled(is_default), MenuItem::new("Delete mode").disabled(!multi)]
        }
        VarMenuKind::Collection(_) => vec![MenuItem::new("Rename"), MenuItem::new("Duplicate"), MenuItem::new("Delete")],
        VarMenuKind::Folder(..) => vec![MenuItem::new("Rename group")],
    };
    match popup_menu(ctx, Id::new("vars-menu"), menu.pos, 168.0, &items, Some(menu.anchor)) {
        MenuResult::Open => {}
        MenuResult::Closed => app.vars.menu = None,
        MenuResult::Picked(i) => {
            app.vars.menu = None;
            match (&menu.kind, i) {
                (VarMenuKind::Add, i) => {
                    let kind = [VariableKind::Color, VariableKind::Number, VariableKind::String, VariableKind::Boolean][i];
                    if let Some(id) = fr.work.add_variable(&cid, kind_name(kind), kind) {
                        app.vars.focus_name = Some(id);
                        app.vars.scroll = f32::MAX;
                        app.vars.search.clear();
                    }
                }
                (VarMenuKind::Row(id), 0) => app.vars.focus_name = Some(id.clone()),
                (VarMenuKind::Row(id), 1) => {
                    if let Some(n) = fr.work.duplicate_variable(id) {
                        app.vars.focus_name = Some(n);
                    }
                }
                (VarMenuKind::Row(id), _) => {
                    fr.work.delete_variable(id);
                    fr.removed_vars.push(id.clone());
                }
                (VarMenuKind::Mode(c, m), 0) => {
                    fr.work.duplicate_mode(c, m);
                }
                (VarMenuKind::Mode(c, m), 1) => fr.work.set_default_mode(c, m),
                (VarMenuKind::Mode(c, m), _) => {
                    fr.work.delete_mode(c, m);
                }
                (VarMenuKind::Collection(id), 0) => {
                    app.vars.collection = Some(id.clone());
                    app.vars.renaming = Some(('c', id.clone()));
                    app.vars.rename_frames = 0;
                }
                (VarMenuKind::Collection(id), 1) => {
                    if let Some(n) = fr.work.duplicate_collection(id) {
                        app.vars.collection = Some(n);
                    }
                }
                (VarMenuKind::Collection(id), _) => {
                    let gone = fr.work.delete_collection(id);
                    fr.removed_vars.extend(gone);
                    fr.removed_colls.push(id.clone());
                    app.vars.collection = fr.work.collections.keys().next().cloned();
                }
                (VarMenuKind::Folder(c, f), _) => {
                    app.vars.renaming = Some(('f', format!("{c}\0{f}")));
                    app.vars.rename_frames = 0;
                }
            }
        }
    }
}

fn run_alias_picker(app: &mut App, ctx: &Context, fr: &mut Frame) {
    let Some(mut ap) = app.vars.alias_picker.take() else { return };
    let Some(var) = fr.work.variables.get(&ap.var).cloned() else { return };
    let current = match var.values.get(&ap.mode) {
        Some(VariableValue::Alias(t)) => Some(t.clone()),
        _ => None,
    };
    let doc_ro = VariableStoreDoc::new(&fr.work);
    let items: Vec<PickItem> = fr
        .work
        .collections
        .values()
        .flat_map(|c| c.variables.iter())
        .filter_map(|id| fr.work.variables.get(id))
        .filter(|t| fr.work.can_alias(&var.id, &t.id))
        .map(|t| doc_ro.item(t))
        .collect();
    let res = picker(ctx, Id::new("vars-alias-picker"), ap.anchor.left_bottom() + Vec2::new(0.0, 4.0), Some(ap.anchor), "Alias of", &mut ap.query, &items, current.as_deref(), current.is_some());
    match res {
        PickResult::Open => app.vars.alias_picker = Some(ap),
        PickResult::Closed => {}
        PickResult::Picked(t) => {
            fr.work.set_alias(&ap.var, &ap.mode, &t);
        }
        PickResult::Detach => {
            fr.work.detach_alias(&ap.var, &ap.mode);
        }
    }
}

/// Builds picker items straight from a store (previews resolve in each default mode).
struct VariableStoreDoc<'a> {
    store: &'a VariableStore,
}

impl<'a> VariableStoreDoc<'a> {
    fn new(store: &'a VariableStore) -> Self {
        Self { store }
    }
    fn item(&self, v: &Variable) -> PickItem {
        let coll = self.store.collections.get(&v.collection);
        let preview = match (v.kind, coll) {
            (VariableKind::Color, Some(c)) => match self.store.resolve(&v.id, &c.default_mode) {
                Some(VariableValue::Color(col)) => Some(to_c32(col)),
                _ => None,
            },
            _ => None,
        };
        PickItem { id: v.id.clone(), group: coll.map(|c| c.name.clone()).unwrap_or_default(), name: v.name.clone(), kind: v.kind, preview }
    }
}

fn run_cell_picker(app: &mut App, ctx: &Context) {
    let Some(mut pk) = app.vars.cell_picker.take() else { return };
    let screen = ctx.content_rect();
    let pos = Pos2::new(pk.anchor.left(), pk.anchor.bottom() + 6.0);
    let size = Vec2::new(224.0, 250.0);
    let pos = Pos2::new(pos.x.min(screen.right() - size.x - 8.0), pos.y.min(screen.bottom() - size.y - 8.0));
    let mut changed = false;
    let area = egui::Area::new(Id::new("vars-color-picker")).order(Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        widgets::menu_panel(ui.painter(), rect, theme::MENU_RADIUS);
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect.shrink(8.0)), |ui| {
            ui.spacing_mut().slider_width = 180.0;
            changed = egui::color_picker::color_picker_color32(ui, &mut pk.color, egui::color_picker::Alpha::OnlyBlend);
        });
        rect
    });
    if changed {
        let c = from_c32(pk.color);
        if let Some(v) = app.doc.variables.get_mut(&pk.var) {
            v.values.insert(pk.mode.clone(), VariableValue::Color(c));
        }
        app.changed();
    }
    let (pressed, ptr, esc) = ctx.input(|i| (i.pointer.any_pressed(), i.pointer.interact_pos(), i.key_pressed(Key::Escape)));
    let outside = pressed && ptr.is_some_and(|p| !area.inner.contains(p) && !pk.anchor.contains(p));
    if outside || esc {
        app.finish_cell_picker(pk);
    } else {
        app.vars.cell_picker = Some(pk);
    }
}
