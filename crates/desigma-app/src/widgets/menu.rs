//! Popup menus and dropdown fields.

use super::*;
use egui::{Area, Order};

#[derive(Clone, Default)]
pub struct MenuItem {
    pub label: String,
    pub shortcut: String,
    pub checked: bool,
    pub disabled: bool,
    pub separator_after: bool,
    pub icon: Option<Icon>,
}

impl MenuItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self { label: label.into(), ..Default::default() }
    }
    pub fn shortcut(mut self, s: &str) -> Self {
        self.shortcut = s.into();
        self
    }
    pub fn checked(mut self, c: bool) -> Self {
        self.checked = c;
        self
    }
    pub fn disabled(mut self, d: bool) -> Self {
        self.disabled = d;
        self
    }
    pub fn sep(mut self) -> Self {
        self.separator_after = true;
        self
    }
    pub fn icon(mut self, i: Icon) -> Self {
        self.icon = Some(i);
        self
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum MenuResult {
    Open,
    Picked(usize),
    Closed,
}

const SEP_H: f32 = 9.0;
const CHECK_W: f32 = 20.0;

/// Shows a floating menu at `pos`. `anchor` is the button that opened it (a
/// press on it does not count as an outside click so the button can toggle).
pub fn popup_menu(ctx: &egui::Context, id: Id, pos: Pos2, min_w: f32, items: &[MenuItem], anchor: Option<Rect>) -> MenuResult {
    let font = theme::font_body();
    let measure = ctx.fonts_mut(|f| {
        items
            .iter()
            .map(|i| {
                let a = f.layout_no_wrap(i.label.clone(), font.clone(), Color32::WHITE).size().x;
                let b = f.layout_no_wrap(i.shortcut.clone(), font.clone(), Color32::WHITE).size().x;
                a + b + if i.shortcut.is_empty() { 0.0 } else { 24.0 }
            })
            .fold(0.0f32, f32::max)
    });
    let w = (measure + CHECK_W + theme::MENU_PADDING * 2.0 + 16.0).max(min_w);
    let h: f32 = items.iter().map(|i| theme::MENU_ITEM_HEIGHT + if i.separator_after { SEP_H } else { 0.0 }).sum::<f32>() + theme::MENU_PADDING;
    let mut result = MenuResult::Open;
    let area = Area::new(id).order(Order::Foreground).fixed_pos(pos).interactable(true).show(ctx, |ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
        let p = ui.painter().clone();
        elevation_shadow(&p, rect, theme::MENU_RADIUS);
        p.rect_filled(rect, theme::cr(theme::MENU_RADIUS), theme::BG_MENU);
        p.rect_stroke(rect, theme::cr(theme::MENU_RADIUS), Stroke::new(1.0, theme::BORDER_MENU), StrokeKind::Inside);
        let mut y = rect.top() + theme::MENU_PADDING / 2.0;
        for (i, item) in items.iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(rect.left() + 4.0, y), Vec2::new(w - 8.0, theme::MENU_ITEM_HEIGHT));
            let resp = ui.interact(r, id.with(i), if item.disabled { Sense::hover() } else { Sense::click() });
            if resp.hovered() && !item.disabled {
                p.rect_filled(r, theme::cr(theme::RADIUS_SMALL), theme::BG_BRAND);
            }
            let col = if item.disabled { theme::TEXT_DISABLED } else { theme::TEXT };
            if item.checked {
                icons::paint(&p, Icon::Check, Pos2::new(r.left() + 10.0, r.center().y), 12.0, col);
            }
            if let Some(ic) = item.icon {
                icons::paint(&p, ic, Pos2::new(r.left() + 10.0, r.center().y), 14.0, col);
            }
            p.text(Pos2::new(r.left() + CHECK_W, r.center().y), Align2::LEFT_CENTER, &item.label, font.clone(), col);
            if !item.shortcut.is_empty() {
                p.text(Pos2::new(r.right() - 8.0, r.center().y), Align2::RIGHT_CENTER, &item.shortcut, font.clone(), if item.disabled { theme::TEXT_DISABLED } else { theme::TEXT_SECONDARY });
            }
            if resp.clicked() && !item.disabled {
                result = MenuResult::Picked(i);
            }
            y += theme::MENU_ITEM_HEIGHT;
            if item.separator_after {
                hline(&p, r.left() + 4.0, r.right() - 4.0, y + SEP_H / 2.0 - 0.5, theme::BORDER_MENU);
                y += SEP_H;
            }
        }
        rect
    });
    if result == MenuResult::Open {
        let (pressed, ptr) = ctx.input(|i| (i.pointer.any_pressed(), i.pointer.interact_pos()));
        let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let outside = pressed && ptr.map(|p| !area.inner.contains(p) && !anchor.map(|a| a.contains(p)).unwrap_or(false)).unwrap_or(false);
        if outside || esc {
            result = MenuResult::Closed;
        }
    }
    result
}

/// A dropdown field: shows `label` with a chevron; returns the picked option.
pub fn dropdown(ui: &mut Ui, rect: Rect, key: impl crate::widgets::Key, label: &str, options: &[String]) -> Option<usize> {
    let id = ui.id().with(("dd", key));
    let r = ui.interact(rect, id, Sense::click());
    let open = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    let p = ui.painter();
    p.rect_filled(rect, theme::cr(theme::RADIUS_MEDIUM), theme::BG_SECONDARY);
    if r.hovered() || open {
        p.rect_stroke(rect, theme::cr(theme::RADIUS_MEDIUM), Stroke::new(1.0, if open { theme::BORDER_SELECTED } else { theme::BORDER }), StrokeKind::Inside);
    }
    let font = theme::font_body();
    let shown = elide(p, label, &font, rect.width() - 28.0);
    p.text(Pos2::new(rect.left() + 8.0, rect.center().y), Align2::LEFT_CENTER, shown, font, theme::TEXT);
    icons::paint(p, Icon::ChevronDown, Pos2::new(rect.right() - 12.0, rect.center().y), 12.0, theme::ICON_SECONDARY);
    if r.clicked() {
        ui.data_mut(|d| d.insert_temp(id, !open));
    }
    let mut picked = None;
    if open {
        let items: Vec<MenuItem> = options.iter().map(|o| MenuItem::new(o.clone()).checked(o == label)).collect();
        match popup_menu(ui.ctx(), id.with("menu"), rect.left_bottom() + Vec2::new(0.0, 2.0), rect.width(), &items, Some(rect)) {
            MenuResult::Picked(i) => {
                picked = Some(i);
                ui.data_mut(|d| d.insert_temp(id, false));
            }
            MenuResult::Closed => {
                ui.data_mut(|d| d.insert_temp(id, false));
            }
            MenuResult::Open => {}
        }
    }
    picked
}
