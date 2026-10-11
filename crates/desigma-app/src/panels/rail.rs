//! Far-left icon rail.

use crate::app::{App, MenuKind, RailItem};
use crate::icons::{self, Icon};
use crate::theme;
use crate::widgets;
use egui::{Align2, Pos2, Rect, Sense, Ui, Vec2};

pub fn show(app: &mut App, ui: &mut Ui, rect: Rect) {
    let p = ui.painter().clone();
    p.rect_filled(rect, 0.0, theme::BG);
    widgets::vline(&p, rect.right() - theme::BORDER_WIDTH, rect.top(), rect.bottom(), theme::BORDER);

    // Logo / main menu button.
    let mb = Rect::from_min_size(
        Pos2::new(rect.left() + theme::RAIL_MENU_BUTTON_X, rect.top() + theme::RAIL_MENU_BUTTON_Y),
        Vec2::splat(theme::RAIL_MENU_BUTTON_SIZE),
    );
    let r = ui.interact(mb, ui.id().with("rail-menu"), Sense::click());
    widgets::button_bg(&p, mb, &r, app.ui.menu == Some(MenuKind::Main), theme::cr(theme::RADIUS_MEDIUM));
    icons::paint(&p, Icon::Logo, mb.center(), 20.0, theme::ICON);
    if r.clicked() {
        toggle(app, MenuKind::Main, mb);
    }

    let items: [(RailItem, Icon, &str, bool); 4] = [
        (RailItem::File, Icon::File, "File", false),
        (RailItem::Assets, Icon::Assets, "Assets", false),
        (RailItem::Tools, Icon::Tools, "Tools", false),
        (RailItem::Variables, Icon::Variables, "Variables", true),
    ];
    let mut y = rect.top() + theme::RAIL_FIRST_Y;
    for (item, icon, label, gap_before) in items {
        if gap_before {
            y += theme::RAIL_GROUP_GAP;
        }
        let ir = Rect::from_min_size(Pos2::new(rect.left(), y), Vec2::new(theme::RAIL_WIDTH - theme::BORDER_WIDTH, theme::RAIL_ITEM_SIZE));
        let resp = ui.interact(ir, ui.id().with(("rail", label)), Sense::click());
        let selected = app.ui.rail == Some(item) || (item == RailItem::File && app.ui.menu == Some(MenuKind::FileMenu));
        let col = if selected { theme::ICON_SELECTED } else if resp.hovered() { theme::ICON } else { theme::ICON_SECONDARY };
        let tcol = if selected { theme::TEXT_BRAND } else { theme::TEXT_SECONDARY };
        icons::paint(&p, icon, Pos2::new(ir.center().x, ir.top() + 20.0), theme::RAIL_ICON_SIZE, col);
        p.text(Pos2::new(ir.center().x, ir.top() + 40.0 + theme::RAIL_LABEL_GAP / 2.0), Align2::CENTER_CENTER, label, theme::font_small(), tcol);
        if resp.clicked() {
            if item == RailItem::File {
                toggle(app, MenuKind::FileMenu, ir);
            } else {
                app.ui.rail = if app.ui.rail == Some(item) { None } else { Some(item) };
                app.show_toast(format!("{label} panel is not available in this build"), false);
            }
        }
        y += theme::RAIL_ITEM_SIZE;
    }

    // Bottom: help + account.
    let mut by = rect.bottom() - 8.0;
    for (icon, key, tip) in [(Icon::User, "acct", "Account"), (Icon::Help, "help", "Help")] {
        let br = Rect::from_min_size(Pos2::new(rect.left() + 12.0, by - theme::RAIL_MENU_BUTTON_SIZE), Vec2::splat(theme::RAIL_MENU_BUTTON_SIZE));
        by -= theme::RAIL_MENU_BUTTON_SIZE + 4.0;
        widgets::icon_button(ui, br, key, icon, false, tip);
    }
}

fn toggle(app: &mut App, kind: MenuKind, anchor: Rect) {
    if app.ui.menu == Some(kind) {
        app.ui.menu = None;
    } else {
        app.ui.menu = Some(kind);
        app.ui.menu_anchor = Some(anchor);
    }
}
