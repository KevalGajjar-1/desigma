//! Assets panel (rail "Assets"): local components grouped by page and frame,
//! with search. Drag a row onto the canvas to place an instance.

use super::{clipped, scrollbar, wheel_scroll};
use crate::app::App;
use crate::comp_ops::AssetEntry;
use crate::icons::{self, Icon};
use crate::theme;
use crate::widgets;
use desigma_doc::NodeId;
use egui::{Align2, Color32, Id, LayerId, Order, Pos2, Rect, Sense, Stroke, StrokeKind, TextEdit, Ui, Vec2};

#[derive(Default)]
pub struct AssetsState {
    pub query: String,
    pub scroll: f32,
    /// Component being dragged out of the panel.
    pub drag: Option<NodeId>,
}

const ROW_H: f32 = 32.0;
const GROUP_H: f32 = 28.0;
const SEARCH_H: f32 = 40.0;

#[derive(Clone)]
enum Line {
    Page(String),
    Container(String),
    Item(usize),
}

/// Entries matching `query` (case-insensitive on name, frame and page).
pub fn filtered(entries: &[AssetEntry], query: &str) -> Vec<usize> {
    let q = query.trim().to_lowercase();
    entries
        .iter()
        .enumerate()
        .filter(|(_, e)| q.is_empty() || e.name.to_lowercase().contains(&q) || e.container.to_lowercase().contains(&q) || e.page.to_lowercase().contains(&q))
        .map(|(i, _)| i)
        .collect()
}

fn lines(entries: &[AssetEntry], shown: &[usize]) -> Vec<Line> {
    let mut out = Vec::new();
    let (mut page, mut container) = (String::new(), String::new());
    for &i in shown {
        let e = &entries[i];
        if out.is_empty() || e.page != page {
            out.push(Line::Page(e.page.clone()));
            page = e.page.clone();
            container = String::new();
        }
        if e.container != container {
            container = e.container.clone();
            if !container.is_empty() {
                out.push(Line::Container(container.clone()));
            }
        }
        out.push(Line::Item(i));
    }
    out
}

fn line_h(l: &Line) -> f32 {
    match l {
        Line::Item(_) => ROW_H,
        _ => GROUP_H,
    }
}

pub fn show(app: &mut App, ui: &mut Ui, region: Rect) {
    let p = ui.painter().clone();
    // ---- search
    let sr = Rect::from_min_size(Pos2::new(region.left() + 8.0, region.top() + 8.0), Vec2::new(region.width() - 16.0, 24.0));
    p.rect_filled(sr, theme::cr(theme::RADIUS_MEDIUM), theme::BG_SECONDARY);
    icons::paint(&p, Icon::Search, Pos2::new(sr.left() + 14.0, sr.center().y), 14.0, theme::ICON_SECONDARY);
    let mut q = std::mem::take(&mut app.assets.query);
    let te = TextEdit::singleline(&mut q)
        .id(ui.id().with("assets-search"))
        .font(theme::font_body())
        .text_color(theme::TEXT)
        .frame(egui::Frame::NONE)
        .hint_text("Search")
        .margin(egui::Margin::symmetric(0, 0))
        .vertical_align(egui::Align::Center)
        .desired_width(sr.width() - 36.0);
    let inner = Rect::from_min_max(Pos2::new(sr.left() + 28.0, sr.top()), sr.max);
    ui.put(inner, te);
    widgets::record(ui, "assets-search", sr);
    app.assets.query = q;

    // ---- list
    let list = Rect::from_min_max(Pos2::new(region.left(), region.top() + SEARCH_H), region.max);
    let entries = app.local_components();
    let shown = filtered(&entries, &app.assets.query);
    let ls = lines(&entries, &shown);
    let content_h: f32 = ls.iter().map(line_h).sum::<f32>() + 8.0;
    let mut scroll = app.assets.scroll;
    wheel_scroll(ui, list, &mut scroll, content_h);
    app.assets.scroll = scroll;
    if entries.is_empty() {
        p.text(Pos2::new(list.center().x, list.top() + 48.0), Align2::CENTER_CENTER, "No components yet", theme::font_body(), theme::TEXT_TERTIARY);
        p.text(Pos2::new(list.center().x, list.top() + 68.0), Align2::CENTER_CENTER, "Select a frame and press Ctrl+Alt+K", theme::font_small(), theme::TEXT_TERTIARY);
    } else if shown.is_empty() {
        p.text(Pos2::new(list.center().x, list.top() + 48.0), Align2::CENTER_CENTER, "No results", theme::font_body(), theme::TEXT_TERTIARY);
    }

    let mut started: Option<NodeId> = None;
    let mut go_main: Option<NodeId> = None;
    clipped(ui, list, |ui| {
        let p = ui.painter().clone();
        let mut y = list.top() - scroll;
        for l in &ls {
            let h = line_h(l);
            let row = Rect::from_min_size(Pos2::new(list.left(), y), Vec2::new(list.width(), h));
            y += h;
            if row.bottom() < list.top() || row.top() > list.bottom() {
                continue;
            }
            match l {
                Line::Page(name) => {
                    p.text(Pos2::new(row.left() + theme::PANEL_PADDING_X, row.center().y), Align2::LEFT_CENTER, name, theme::font_body_strong(), theme::TEXT);
                }
                Line::Container(name) => {
                    let s = widgets::elide(&p, name, &theme::font_body(), row.width() - 40.0);
                    p.text(Pos2::new(row.left() + theme::PANEL_PADDING_X + 8.0, row.center().y), Align2::LEFT_CENTER, s, theme::font_body(), theme::TEXT_SECONDARY);
                }
                Line::Item(i) => {
                    let e = &entries[*i];
                    let r = ui.interact(row, ui.id().with(("asset", e.id.0)), Sense::click_and_drag());
                    widgets::record(ui, &format!("asset:{}", e.name), row);
                    let inset = row.shrink2(Vec2::new(8.0, 0.0));
                    if r.hovered() || app.assets.drag == Some(e.id) {
                        p.rect_filled(inset, theme::cr(theme::RADIUS_MEDIUM), theme::BG_HOVER);
                    }
                    icons::paint(&p, if e.variants > 0 { Icon::Component } else { Icon::Component }, Pos2::new(inset.left() + 20.0, row.center().y), 16.0, theme::CANVAS_COMPONENT);
                    let label = if e.variants > 0 { format!("{} ({})", e.name, e.variants) } else { e.name.clone() };
                    let s = widgets::elide(&p, &label, &theme::font_body(), inset.width() - 44.0);
                    p.text(Pos2::new(inset.left() + 36.0, row.center().y), Align2::LEFT_CENTER, s, theme::font_body(), theme::TEXT);
                    if r.drag_started() {
                        started = Some(e.id);
                    }
                    if r.double_clicked() {
                        go_main = Some(e.id);
                    }
                }
            }
        }
    });
    scrollbar(ui, list, scroll, content_h);
    if started.is_some() {
        app.assets.drag = started;
    }
    if let Some(c) = go_main {
        app.go_to_main_component(c);
    }
    drag(app, ui);
}

/// Ghost under the pointer while dragging; drops an instance on release.
fn drag(app: &mut App, ui: &mut Ui) {
    let Some(comp) = app.assets.drag else { return };
    let (ptr, released) = ui.input(|i| (i.pointer.interact_pos(), i.pointer.primary_released()));
    let over_canvas = ptr.is_some_and(|p| app.canvas_rect.contains(p));
    if let (Some(pos), Some(c)) = (ptr, app.doc.get(comp).cloned()) {
        let p = ui.ctx().layer_painter(LayerId::new(Order::Tooltip, Id::new("asset-drag")));
        if over_canvas {
            let size = Vec2::new(c.size.x * app.view.zoom, c.size.y * app.view.zoom);
            let r = Rect::from_center_size(pos, size);
            p.rect_filled(r, 0.0, theme::CANVAS_COMPONENT.gamma_multiply(0.18));
            p.rect_stroke(r, 0.0, Stroke::new(1.0, theme::CANVAS_COMPONENT), StrokeKind::Inside);
        } else {
            let font = theme::font_body();
            let w = widgets::text_width(&p, &c.name, &font) + 24.0;
            let r = Rect::from_min_size(pos + Vec2::new(12.0, 12.0), Vec2::new(w, 24.0));
            p.rect_filled(r, theme::cr(theme::RADIUS_MEDIUM), theme::BG_MENU);
            p.text(r.center(), Align2::CENTER_CENTER, &c.name, font, Color32::WHITE);
        }
    }
    if released {
        app.assets.drag = None;
        if let Some(pos) = ptr.filter(|_| over_canvas) {
            let at = app.view.to_page(app.canvas_rect.min, pos);
            app.insert_instance(comp, at);
        }
    }
}
