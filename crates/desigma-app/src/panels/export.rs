//! Inspector "Export" section: per-layer export rows (scale + format), a
//! preview thumbnail, the export button and a "Copy as" context menu.

use crate::app::App;
use crate::icons::{self, Icon};
use crate::theme;
use crate::widgets::field;
use crate::widgets::menu::{dropdown, popup_menu, MenuItem, MenuResult};
use crate::widgets::{self, elide};
use desigma_doc::{Document, NodeId, Rect as DRect};
use desigma_export::{ExportFormat, ExportScale, ExportSetting};
use egui::{Align2, Id, Pos2, Rect, Sense, TextureHandle, TextureOptions, Ui, Vec2};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

const ROW_GAP: f32 = 8.0;
const PREVIEW_H: f32 = 120.0;
const SCALE_W: f32 = 72.0;
const FORMAT_W: f32 = 72.0;
const THUMB_CHECK_SECONDS: f64 = 0.3;

/// What "Copy as" can put on the clipboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyKind {
    Png,
    Svg,
    Css,
}

pub const COPY_ITEMS: [(CopyKind, &str); 3] = [(CopyKind::Png, "Copy as PNG"), (CopyKind::Svg, "Copy as SVG"), (CopyKind::Css, "Copy as CSS")];

#[derive(Clone)]
struct Thumb {
    tex: TextureHandle,
    node: NodeId,
    fingerprint: u64,
    checked: f64,
}

fn rows_id(node: NodeId) -> Id {
    Id::new(("export-rows", node))
}

fn load_rows(ui: &Ui, node: NodeId) -> Vec<ExportSetting> {
    ui.ctx().data(|d| d.get_temp::<Vec<ExportSetting>>(rows_id(node))).unwrap_or_default()
}

fn store_rows(ui: &Ui, node: NodeId, rows: Vec<ExportSetting>) {
    ui.ctx().data_mut(|d| d.insert_temp(rows_id(node), rows));
}

fn fingerprint(doc: &Document, root: NodeId) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    doc.walk(root, |n, _| format!("{n:?}").hash(&mut h));
    for (k, v) in &doc.images {
        k.hash(&mut h);
        v.len().hash(&mut h);
    }
    h.finish()
}

fn thumbnail(app: &App, ui: &Ui, node: NodeId) -> Option<Thumb> {
    let key = Id::new("export-thumb");
    let ctx = ui.ctx();
    let cached = ctx.data(|d| d.get_temp::<Thumb>(key));
    if let Some(t) = &cached {
        if t.node == node && app.now - t.checked < THUMB_CHECK_SECONDS {
            return cached;
        }
    }
    let fp = fingerprint(&app.doc, node);
    if let Some(mut t) = cached {
        if t.node == node && t.fingerprint == fp {
            t.checked = app.now;
            ctx.data_mut(|d| d.insert_temp(key, t.clone()));
            return Some(t);
        }
    }
    let b: DRect = desigma_export::render_bounds(&app.export_doc(), node);
    if b.w <= 0.0 || b.h <= 0.0 {
        return None;
    }
    // Render at 2x the preview box so it stays sharp on hi-dpi screens.
    let scale = (theme::SECTION_CONTENT_W * 2.0 / b.w).min(PREVIEW_H * 2.0 / b.h).clamp(0.01, 2.0);
    let pm = desigma_export::render_export(&app.export_doc(), node, scale)?;
    let img = egui::ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data());
    let tex = ctx.load_texture("export-preview", img, TextureOptions::LINEAR);
    let t = Thumb { tex, node, fingerprint: fp, checked: app.now };
    ctx.data_mut(|d| d.insert_temp(key, t.clone()));
    Some(t)
}

/// Draws the Export section starting at absolute `(x0, top)` and returns the
/// height it used. `x0` is the panel's outer left edge.
pub fn section(app: &mut App, ui: &mut Ui, x0: f32, top: f32, sel: &[NodeId]) -> f32 {
    let Some(&first) = sel.first() else { return 0.0 };
    let name = app.doc.get(first).map(|n| n.name.clone()).unwrap_or_default();
    let mut rows = load_rows(ui, first);
    let at = |x: f32, dy: f32, w: f32, h: f32| Rect::from_min_size(Pos2::new(x0 + x, top + dy), Vec2::new(w, h));
    let mut y = 0.0;

    // Title + add button.
    let title = at(0.0, 0.0, theme::RIGHT_PANEL_WIDTH - 1.0, theme::SECTION_TITLE_HEIGHT);
    ui.painter().text(
        Pos2::new(title.left() + theme::SECTION_CONTENT_X, title.center().y),
        Align2::LEFT_CENTER,
        "Export",
        theme::font_body_strong(),
        theme::TEXT,
    );
    // Created before the button so the button sits on top of it.
    let title_resp = ui.interact(title, ui.id().with("export-title"), Sense::click());
    let plus = at(theme::SECTION_TRAILING_X, 8.0, theme::PANEL_ACTION_BUTTON, theme::PANEL_ACTION_BUTTON);
    if widgets::icon_button(ui, plus, "export-add", Icon::Plus, false, "Add export setting").clicked() {
        rows.push(rows.last().cloned().unwrap_or_default());
        store_rows(ui, first, rows.clone());
    }
    y += theme::SECTION_TITLE_HEIGHT;

    // Rows.
    let mut remove = None;
    let mut changed = false;
    for (i, row) in rows.iter_mut().enumerate() {
        let scale_r = at(theme::SECTION_CONTENT_X, y, SCALE_W, theme::FIELD_HEIGHT);
        let fmt_r = at(theme::SECTION_CONTENT_X + SCALE_W + ROW_GAP, y, FORMAT_W, theme::FIELD_HEIGHT);
        let minus =
            at(theme::SECTION_CONTENT_X + theme::SECTION_CONTENT_W - theme::PANEL_ACTION_BUTTON, y, theme::PANEL_ACTION_BUTTON, theme::PANEL_ACTION_BUTTON);

        // Scale: editable text ("2x", "512w") with a presets menu behind the chevron.
        let text_r = Rect::from_min_max(scale_r.min, Pos2::new(scale_r.right() - 20.0, scale_r.bottom()));
        let chev_r = Rect::from_min_max(Pos2::new(scale_r.right() - 20.0, scale_r.top()), scale_r.max);
        let fid = ui.id().with(("export-scale", first, i));
        field::field_bg(ui, scale_r, fid);
        if let Some(s) = field::edit_text(ui, text_r, fid, &row.scale.to_string(), theme::font_body(), theme::TEXT, 8.0) {
            if let Some(sc) = ExportScale::parse(&s) {
                row.scale = sc;
                changed = true;
            }
        }
        let chev = ui.interact(chev_r, ui.id().with(("export-scale-chev", first, i)), Sense::click());
        icons::paint(ui.painter(), Icon::ChevronDown, chev_r.center(), 12.0, if chev.hovered() { theme::ICON } else { theme::ICON_SECONDARY });
        let menu_id = ui.id().with(("export-scale-menu", first, i));
        let open = ui.data(|d| d.get_temp::<bool>(menu_id)).unwrap_or(false);
        if chev.clicked() {
            ui.data_mut(|d| d.insert_temp(menu_id, !open));
        }
        if open {
            let items: Vec<MenuItem> = ExportScale::PRESETS.iter().map(|s| MenuItem::new(s.to_string()).checked(*s == row.scale)).collect();
            match popup_menu(ui.ctx(), menu_id.with("popup"), scale_r.left_bottom() + Vec2::new(0.0, 2.0), scale_r.width(), &items, Some(scale_r)) {
                MenuResult::Picked(p) => {
                    row.scale = ExportScale::PRESETS[p];
                    changed = true;
                    ui.data_mut(|d| d.insert_temp(menu_id, false));
                }
                MenuResult::Closed => {
                    ui.data_mut(|d| d.insert_temp(menu_id, false));
                }
                MenuResult::Open => {}
            }
        }

        let options: Vec<String> = ExportFormat::ALL.iter().map(|f| f.label().to_string()).collect();
        if let Some(p) = dropdown(ui, fmt_r, ("export-format", first, i), row.format.label(), &options) {
            row.format = ExportFormat::ALL[p];
            changed = true;
        }
        if widgets::icon_button(ui, minus, ("export-remove", i), Icon::Minus, false, "Remove export setting").clicked() {
            remove = Some(i);
        }
        y += theme::FIELD_HEIGHT + ROW_GAP;
    }
    if let Some(i) = remove {
        rows.remove(i);
        changed = true;
    }
    if changed {
        store_rows(ui, first, rows.clone());
    }
    if rows.is_empty() {
        context_menu(app, ui, &title_resp, first);
        return y + 8.0;
    }

    // Preview.
    let prev = at(theme::SECTION_CONTENT_X, y, theme::SECTION_CONTENT_W, PREVIEW_H);
    ui.painter().rect_filled(prev, theme::cr(theme::RADIUS_MEDIUM), theme::BG_SECONDARY);
    if let Some(t) = thumbnail(app, ui, first) {
        // The texture holds up to 2x the logical size; never show it larger than that.
        let sz = t.tex.size_vec2();
        let k = ((prev.width() - 16.0) / sz.x).min((prev.height() - 16.0) / sz.y).min(0.5);
        let draw = Rect::from_center_size(prev.center(), sz * k);
        ui.painter().image(t.tex.id(), draw, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), egui::Color32::WHITE);
    }
    let prev_resp = ui.interact(prev, ui.id().with("export-preview-hit"), Sense::click());
    y += PREVIEW_H + ROW_GAP;

    // Export button.
    let btn = at(theme::SECTION_CONTENT_X, y, theme::SECTION_CONTENT_W, theme::FIELD_HEIGHT);
    let resp = ui.interact(btn, ui.id().with("export-btn"), Sense::click());
    ui.painter().rect_filled(btn, theme::cr(theme::RADIUS_MEDIUM), if resp.hovered() { theme::BG_TERTIARY } else { theme::BG_SECONDARY });
    icons::paint(ui.painter(), Icon::Export, Pos2::new(btn.left() + 16.0, btn.center().y), 14.0, theme::ICON_SECONDARY);
    let label = if sel.len() > 1 { format!("Export {} layers", sel.len()) } else { format!("Export {name}") };
    let label = elide(ui.painter(), &label, &theme::font_body(), btn.width() - 44.0);
    ui.painter().text(Pos2::new(btn.left() + 32.0, btn.center().y), Align2::LEFT_CENTER, label, theme::font_body(), theme::TEXT);
    if resp.clicked() {
        run_export(app, sel, &rows);
    }
    context_menu(app, ui, &resp, first);
    context_menu(app, ui, &prev_resp, first);
    context_menu(app, ui, &title_resp, first);
    y + theme::FIELD_HEIGHT + 12.0
}

/// Opens/draws the "Copy as" menu for right clicks on `anchor`.
fn context_menu(app: &mut App, ui: &mut Ui, anchor: &egui::Response, node: NodeId) {
    let id = Id::new("export-ctx-menu");
    if anchor.secondary_clicked() {
        if let Some(p) = anchor.interact_pointer_pos() {
            ui.data_mut(|d| d.insert_temp(id, p));
        }
    }
    let Some(pos) = ui.data(|d| d.get_temp::<Pos2>(id)) else { return };
    // Only the response that opened it draws it (all three share one position).
    if !anchor.rect.contains(pos) {
        return;
    }
    let items: Vec<MenuItem> = COPY_ITEMS.iter().map(|(_, l)| MenuItem::new(*l)).collect();
    match popup_menu(ui.ctx(), id.with("popup"), pos, 150.0, &items, None) {
        MenuResult::Picked(i) => {
            ui.data_mut(|d| d.remove_temp::<Pos2>(id));
            copy_as(app, node, COPY_ITEMS[i].0);
        }
        MenuResult::Closed => {
            ui.data_mut(|d| d.remove_temp::<Pos2>(id));
        }
        MenuResult::Open => {}
    }
}

/// Puts `node` on the clipboard as PNG (2x), SVG text or CSS text.
pub fn copy_as(app: &mut App, node: NodeId, kind: CopyKind) {
    let result: Result<&str, String> = (|| {
        let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        match kind {
            CopyKind::Png => {
                let pm = desigma_export::render_export(&app.export_doc(), node, 2.0).ok_or("Nothing to copy")?;
                let (w, h) = (pm.width() as usize, pm.height() as usize);
                let bytes = desigma_export::rgba_bytes(&pm);
                cb.set_image(arboard::ImageData { width: w, height: h, bytes: bytes.into() }).map_err(|e| e.to_string())?;
                Ok("Copied as PNG")
            }
            CopyKind::Svg => {
                let svg = desigma_export::export_svg(&app.export_doc(), node, &desigma_export::SvgOptions::default());
                cb.set_text(svg).map_err(|e| e.to_string())?;
                Ok("Copied as SVG")
            }
            CopyKind::Css => {
                cb.set_text(desigma_export::css_for(&app.export_doc(), node)).map_err(|e| e.to_string())?;
                Ok("Copied as CSS")
            }
        }
    })();
    match result {
        Ok(msg) => app.show_toast(msg, false),
        Err(e) => app.show_toast(format!("Copy failed: {e}"), true),
    }
}

/// Writes every selected layer with every export row. One file asks for a
/// path; several ask for a folder and use `name@2x.png` style names.
pub fn run_export(app: &mut App, ids: &[NodeId], rows: &[ExportSetting]) {
    let mut jobs: Vec<(String, NodeId, ExportSetting)> = Vec::new();
    let mut seen: HashMap<String, u32> = HashMap::new();
    for id in ids {
        let Some(n) = app.doc.get(*id) else { continue };
        for s in rows {
            let mut file = s.file_name(&n.name);
            let count = seen.entry(file.clone()).or_insert(0);
            *count += 1;
            if *count > 1 {
                let stem = file.rsplit_once('.').map(|(a, _)| a.to_string()).unwrap_or_else(|| file.clone());
                file = format!("{stem} {count}.{}", s.format.extension());
            }
            jobs.push((file, *id, s.clone()));
        }
    }
    if jobs.is_empty() {
        app.show_toast("Nothing to export", true);
        return;
    }
    let mut written = Vec::new();
    if jobs.len() == 1 {
        let (file, id, s) = &jobs[0];
        let Some(path) = rfd::FileDialog::new().add_filter(s.format.label(), &[s.format.extension()]).set_file_name(file).save_file() else { return };
        let bytes = s.render(&app.doc, *id);
        if bytes.is_empty() {
            app.show_toast("Nothing to export", true);
            return;
        }
        match std::fs::write(&path, bytes) {
            Ok(()) => written.push(path),
            Err(e) => {
                app.show_toast(format!("Export failed: {e}"), true);
                return;
            }
        }
    } else {
        let Some(dir) = rfd::FileDialog::new().set_title("Choose export folder").pick_folder() else { return };
        for (file, id, s) in &jobs {
            let bytes = s.render(&app.doc, *id);
            if bytes.is_empty() {
                continue;
            }
            let path = dir.join(file);
            match std::fs::write(&path, bytes) {
                Ok(()) => written.push(path),
                Err(e) => {
                    app.show_toast(format!("Export failed: {e}"), true);
                    return;
                }
            }
        }
    }
    match written.as_slice() {
        [one] => app.show_toast(format!("Exported {}", one.display()), false),
        many => app.show_toast(format!("Exported {} files", many.len()), false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use desigma_doc::{Node, Rect as R};

    fn frame(ctx: &egui::Context, t: f64, events: Vec<egui::Event>, f: impl FnMut(&mut Ui)) {
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 800.0))), time: Some(t), events, ..Default::default() };
        let _ = ctx.run_ui(raw, f);
    }

    #[test]
    fn plus_adds_a_row_and_shows_preview_and_button() {
        let mut d = Document::new("t");
        let page = d.add_page("p");
        let id = d.alloc_id();
        d.insert(page, Node::rectangle(id, R::new(0.0, 0.0, 40.0, 30.0))).unwrap();
        let mut app = App::new(d);
        app.selection = vec![id];
        let sel = vec![id];
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        let mut h0 = 0.0;
        frame(&ctx, 0.0, vec![], |ui| h0 = section(&mut app, ui, 0.0, 0.0, &sel));
        assert!(h0 < 60.0, "only the title without rows: {h0}");
        // click the "+" at the right end of the title row
        let p = Pos2::new(theme::SECTION_TRAILING_X + 12.0, 20.0);
        let ev = |pressed| egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        frame(&ctx, 0.1, vec![egui::Event::PointerMoved(p)], |ui| {
            section(&mut app, ui, 0.0, 0.0, &sel);
        });
        frame(&ctx, 0.2, vec![ev(true)], |ui| {
            section(&mut app, ui, 0.0, 0.0, &sel);
        });
        frame(&ctx, 0.3, vec![ev(false)], |ui| {
            section(&mut app, ui, 0.0, 0.0, &sel);
        });
        let rows = ctx.data(|d| d.get_temp::<Vec<ExportSetting>>(rows_id(id))).unwrap_or_default();
        assert_eq!(rows, vec![ExportSetting::default()]);
        let mut h1 = 0.0;
        frame(&ctx, 0.4, vec![], |ui| h1 = section(&mut app, ui, 0.0, 0.0, &sel));
        // title + one row + preview + button
        assert!(h1 > h0 + PREVIEW_H, "{h1} vs {h0}");
        // the preview texture was created
        assert!(ctx.data(|d| d.get_temp::<Thumb>(Id::new("export-thumb"))).is_some());
    }

    #[test]
    fn fingerprint_changes_with_the_subtree() {
        let mut d = Document::new("t");
        let page = d.add_page("p");
        let id = d.alloc_id();
        d.insert(page, Node::rectangle(id, R::new(0.0, 0.0, 10.0, 10.0))).unwrap();
        let a = fingerprint(&d, id);
        assert_eq!(a, fingerprint(&d, id));
        d.get_mut(id).unwrap().opacity = 0.5;
        assert_ne!(a, fingerprint(&d, id));
    }
}
