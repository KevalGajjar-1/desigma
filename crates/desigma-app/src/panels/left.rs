//! Left panel: file header, pages list and the layers tree.

use super::{clipped, layers, scrollbar, wheel_scroll};
use crate::app::{App, MenuKind};
use crate::icons::{self, Icon};
use crate::theme;
use crate::widgets::menu::{popup_menu, MenuItem, MenuResult};
use crate::widgets::{self, field};
use egui::{Align2, Pos2, Rect, Sense, Ui, Vec2};

pub fn show(app: &mut App, ui: &mut Ui, rect: Rect) {
    let p = ui.painter().clone();
    p.rect_filled(rect, 0.0, theme::BG);
    widgets::vline(&p, rect.right() - theme::BORDER_WIDTH, rect.top(), rect.bottom(), theme::BORDER);
    let inner = Rect::from_min_max(rect.min, Pos2::new(rect.right() - theme::BORDER_WIDTH, rect.bottom()));
    let x0 = inner.left();
    let mut y = inner.top();

    // ---- header
    let name = if app.doc.name.is_empty() { "Untitled".to_string() } else { app.doc.name.clone() };
    let name_font = theme::font_large_strong();
    let shown = widgets::elide(&p, &name, &name_font, theme::LEFT_PANEL_WIDTH - 16.0 - 24.0 - 40.0 - 24.0);
    let name_rect = p.text(Pos2::new(x0 + theme::PANEL_PADDING_X, y + theme::LEFT_FILE_NAME_Y + 8.0), Align2::LEFT_CENTER, &shown, name_font, theme::TEXT);
    let hit = Rect::from_min_max(Pos2::new(x0 + 8.0, y + 8.0), Pos2::new(name_rect.right() + 28.0, y + 32.0));
    let hr = ui.interact(hit, ui.id().with("file-name"), Sense::click());
    if hr.hovered() {
        p.rect_filled(hit, theme::cr(theme::RADIUS_MEDIUM), theme::BG_HOVER);
        p.text(Pos2::new(x0 + theme::PANEL_PADDING_X, y + theme::LEFT_FILE_NAME_Y + 8.0), Align2::LEFT_CENTER, &shown, theme::font_large_strong(), theme::TEXT);
    }
    icons::paint(&p, Icon::ChevronDown, Pos2::new(name_rect.right() + 10.0, name_rect.center().y), 12.0, theme::ICON_SECONDARY);
    if hr.clicked() {
        app.ui.menu = if app.ui.menu == Some(MenuKind::FileName) { None } else { Some(MenuKind::FileName) };
        app.ui.menu_anchor = Some(hit);
    }
    let sub = p.text(Pos2::new(x0 + theme::PANEL_PADDING_X, y + theme::LEFT_SUBLABEL_Y + 8.0), Align2::LEFT_CENTER, "Drafts", theme::font_body(), theme::TEXT_SECONDARY);
    widgets::badge(&p, Pos2::new(sub.right() + theme::BADGE_GAP, sub.center().y - theme::BADGE_HEIGHT / 2.0), "Free");
    let tb = Rect::from_min_size(Pos2::new(inner.right() - theme::LEFT_SIDEBAR_BTN_RIGHT_PAD - theme::PANEL_ACTION_BUTTON, y + theme::LEFT_ACTION_Y), Vec2::splat(theme::PANEL_ACTION_BUTTON));
    if widgets::icon_button(ui, tb, "sidebar", Icon::Sidebar, false, "Hide sidebar").clicked() {
        app.ui.left_collapsed = true;
    }
    y += theme::LEFT_HEADER_HEIGHT;
    widgets::hline(&p, x0, inner.right(), y - 1.0, theme::BORDER);

    if app.ui.rail == Some(crate::app::RailItem::Assets) {
        super::assets::show(app, ui, Rect::from_min_max(Pos2::new(x0, y), inner.max));
        return;
    }

    // ---- pages
    section_title(ui, Rect::from_min_size(Pos2::new(x0, y), Vec2::new(inner.width(), theme::PANEL_TITLE_ROW_HEIGHT)), "Pages");
    let tr = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(inner.width(), theme::PANEL_TITLE_ROW_HEIGHT));
    let plus = Rect::from_min_size(
        Pos2::new(tr.right() - theme::PANEL_ACTION_RIGHT_PAD - theme::PANEL_ACTION_BUTTON, tr.top() + 8.0),
        Vec2::splat(theme::PANEL_ACTION_BUTTON),
    );
    if widgets::icon_button(ui, plus, "page-add", Icon::Plus, false, "Add page").clicked() {
        app.add_page();
    }
    let search = plus.translate(Vec2::new(-theme::PAGES_SEARCH_DX, 0.0));
    widgets::icon_button(ui, search, "page-search", Icon::Search, false, "Search pages");
    y += theme::PANEL_TITLE_ROW_HEIGHT;

    let n = app.doc.pages.len();
    let visible = n.min(theme::PAGES_MAX_VISIBLE_ROWS);
    let list_h = visible as f32 * theme::PAGE_ROW_HEIGHT;
    let list = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(inner.width(), list_h));
    pages_list(app, ui, list);
    y += list_h + 8.0;
    widgets::hline(&p, x0, inner.right(), y - 1.0, theme::BORDER);

    // ---- layers
    let lt = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(inner.width(), theme::LAYER_HEADER_HEIGHT));
    section_title(ui, lt, "Layers");
    y += theme::LAYER_HEADER_HEIGHT;
    let region = Rect::from_min_max(Pos2::new(x0, y), inner.max);
    layers::show(app, ui, region);
}

fn section_title(ui: &Ui, row: Rect, title: &str) {
    ui.painter().text(Pos2::new(row.left() + theme::PANEL_PADDING_X, row.center().y), Align2::LEFT_CENTER, title, theme::font_body_strong(), theme::TEXT);
}

fn pages_list(app: &mut App, ui: &mut Ui, list: Rect) {
    let n = app.doc.pages.len();
    let content_h = n as f32 * theme::PAGE_ROW_HEIGHT;
    let mut scroll = app.layers.page_scroll;
    wheel_scroll(ui, list, &mut scroll, content_h);
    app.layers.page_scroll = scroll;
    let pages = app.doc.pages.clone();
    clipped(ui, list, |ui| {
        let p = ui.painter().clone();
        for (i, pid) in pages.iter().enumerate() {
            let top = list.top() + i as f32 * theme::PAGE_ROW_HEIGHT - scroll;
            let row = Rect::from_min_size(Pos2::new(list.left() + theme::PAGE_ROW_INSET, top), Vec2::new(list.width() - theme::PAGE_ROW_INSET * 2.0, theme::PAGE_ROW_HEIGHT));
            if row.bottom() < list.top() || row.top() > list.bottom() {
                continue;
            }
            let selected = *pid == app.page;
            let name = app.doc.get(*pid).map(|n| n.name.clone()).unwrap_or_default();
            if app.layers.renaming_page == Some(*pid) {
                let id = ui.id().with(("page-rename", pid.0));
                let txt_rect = Rect::from_min_max(row.min + Vec2::new(theme::PAGE_ROW_TEXT_PAD, 0.0), row.max);
                p.rect_filled(row, theme::cr(theme::RADIUS_MEDIUM), theme::BG_SECONDARY);
                if !app.layers.rename_started {
                    ui.memory_mut(|m| m.request_focus(id));
                    app.layers.rename_started = true;
                    app.layers.rename_seen = false;
                }
                let res = field::edit_text(ui, txt_rect, id, &name, theme::font_body(), theme::TEXT, 0.0);
                let focused = ui.memory(|m| m.has_focus(id));
                if focused {
                    app.layers.rename_seen = true;
                }
                if let Some(new_name) = res {
                    app.rename(*pid, &new_name);
                }
                if !focused && app.layers.rename_seen {
                    app.layers.renaming_page = None;
                    app.layers.rename_started = false;
                    app.layers.rename_seen = false;
                }
                continue;
            }
            let r = ui.interact(row, ui.id().with(("page", pid.0)), Sense::click_and_drag());
            if r.secondary_clicked() {
                let at = ui.input(|i| i.pointer.interact_pos()).unwrap_or(row.left_bottom());
                app.layers.page_menu = Some((*pid, at));
            }
            if r.drag_started() {
                app.layers.page_drag = Some(*pid);
            }
            if selected {
                p.rect_filled(row, theme::cr(theme::RADIUS_MEDIUM), theme::BG_SELECTED);
            } else if r.hovered() && app.layers.page_drag.is_none() {
                p.rect_filled(row, theme::cr(theme::RADIUS_MEDIUM), theme::BG_HOVER);
            }
            let font = if selected { theme::font_body_strong() } else { theme::font_body() };
            let shown = widgets::elide(&p, &name, &font, row.width() - theme::PAGE_ROW_TEXT_PAD * 2.0);
            p.text(Pos2::new(row.left() + theme::PAGE_ROW_TEXT_PAD, row.center().y), Align2::LEFT_CENTER, shown, font, theme::TEXT);
            if r.double_clicked() {
                app.layers.renaming_page = Some(*pid);
                app.layers.rename_started = false;
            } else if r.clicked() && !selected {
                app.switch_page(*pid);
            }
        }
    });
    scrollbar(ui, list, scroll, content_h);
    page_drag(app, ui, list, scroll);
    page_menu(app, ui);
}

/// Drag-to-reorder: shows the insertion line and moves the page on release.
fn page_drag(app: &mut App, ui: &mut Ui, list: Rect, scroll: f32) {
    let Some(dragged) = app.layers.page_drag else { return };
    let n = app.doc.pages.len();
    let (ptr, released) = ui.input(|i| (i.pointer.interact_pos(), i.pointer.primary_released()));
    if let Some(pos) = ptr {
        let slot = ((pos.y - list.top() + scroll) / theme::PAGE_ROW_HEIGHT).round().clamp(0.0, n as f32) as usize;
        app.layers.page_drop = Some(slot);
        let y = list.top() + slot as f32 * theme::PAGE_ROW_HEIGHT - scroll;
        let line = Rect::from_min_max(Pos2::new(list.left() + theme::PAGE_ROW_INSET, y - theme::LAYER_DROP_THICKNESS / 2.0), Pos2::new(list.right() - theme::PAGE_ROW_INSET, y + theme::LAYER_DROP_THICKNESS / 2.0));
        ui.painter().with_clip_rect(list).rect_filled(line, 0.0, theme::DROP_INDICATOR);
    }
    if released {
        if let (Some(slot), Some(from)) = (app.layers.page_drop, app.doc.pages.iter().position(|p| *p == dragged)) {
            app.move_page(dragged, if slot > from { slot - 1 } else { slot });
        }
        app.layers.page_drag = None;
        app.layers.page_drop = None;
    }
}

/// Page row context menu: Rename, Duplicate, Delete.
fn page_menu(app: &mut App, ui: &mut Ui) {
    let Some((pid, at)) = app.layers.page_menu else { return };
    if !app.doc.pages.contains(&pid) {
        app.layers.page_menu = None;
        return;
    }
    let items = [
        MenuItem::new("Rename"),
        MenuItem::new("Duplicate"),
        MenuItem::new("Delete").disabled(app.doc.pages.len() <= 1),
    ];
    match popup_menu(ui.ctx(), egui::Id::new("page-menu"), at, theme::PAGE_MENU_WIDTH, &items, None) {
        MenuResult::Picked(i) => {
            app.layers.page_menu = None;
            match i {
                0 => {
                    app.layers.renaming_page = Some(pid);
                    app.layers.rename_started = false;
                }
                1 => {
                    app.duplicate_page(pid);
                }
                _ => {
                    app.delete_page(pid);
                }
            }
        }
        MenuResult::Closed => app.layers.page_menu = None,
        MenuResult::Open => {}
    }
}
