//! Right panel shell: avatar, present, share, tabs, zoom dropdown.

use super::properties;
use crate::app::{App, MenuKind, RightTab};
use crate::icons::{self, Icon};
use crate::theme;
use crate::widgets;
use egui::{Align2, Color32, Pos2, Rect, Sense, Ui, Vec2};

pub fn show(app: &mut App, ui: &mut Ui, rect: Rect) {
    let p = ui.painter().clone();
    p.rect_filled(rect, 0.0, theme::BG);
    widgets::vline(&p, rect.left(), rect.top(), rect.bottom(), theme::BORDER);
    let right = rect.right();
    let left = rect.left();

    // Avatar pill.
    let av = Rect::from_min_size(Pos2::new(left + theme::RIGHT_AVATAR_X, rect.top() + theme::RIGHT_AVATAR_Y), Vec2::new(theme::RIGHT_AVATAR_W, theme::RIGHT_AVATAR_H));
    let ar = ui.interact(av, ui.id().with("avatar"), Sense::click());
    // 24px user-colour circle + 16px chevron inside a 50x24 pill that shows a hover background.
    if ar.hovered() {
        p.rect_filled(av, theme::cr(theme::RADIUS_FULL), theme::BG_HOVER);
    }
    let r_av = theme::RIGHT_AVATAR_CIRCLE / 2.0;
    p.circle_filled(Pos2::new(av.left() + r_av, av.center().y), r_av, theme::AVATAR_BG);
    p.text(Pos2::new(av.left() + r_av, av.center().y), Align2::CENTER_CENTER, "K", theme::font_body_strong(), theme::AVATAR_FG);
    icons::paint(&p, Icon::ChevronDown, Pos2::new(av.left() + theme::RIGHT_AVATAR_CIRCLE + 9.0, av.center().y), theme::CHEVRON_SMALL, theme::ICON_SECONDARY);

    // Present button + chevron.
    let pb = Rect::from_min_size(Pos2::new(right - theme::RIGHT_PRESENT_FROM_RIGHT, rect.top() + theme::RIGHT_BTN_Y), Vec2::splat(theme::RIGHT_BTN_SIZE));
    let pr = ui.interact(pb, ui.id().with("present"), Sense::click());
    widgets::button_bg(&p, pb, &pr, false, theme::cr_each(theme::RADIUS_MEDIUM, 0, 0, theme::RADIUS_MEDIUM));
    // Outlined play triangle, ~16px tall.
    let (cx, cy) = (pb.center().x, pb.center().y);
    let (w, h) = (theme::PRESENT_ICON_W, theme::PRESENT_ICON_H);
    let x0 = cx - w / 2.0 + 0.5;
    p.add(egui::Shape::closed_line(
        vec![Pos2::new(x0, cy - h / 2.0), Pos2::new(x0 + w, cy), Pos2::new(x0, cy + h / 2.0)],
        egui::Stroke::new(theme::PRESENT_ICON_STROKE, theme::ICON),
    ));
    let cb = Rect::from_min_size(pb.right_top(), Vec2::new(theme::RIGHT_PRESENT_CHEVRON_W, theme::RIGHT_BTN_SIZE));
    let cr = ui.interact(cb, ui.id().with("present-chev"), Sense::click());
    widgets::button_bg(&p, cb, &cr, false, theme::cr_each(0, theme::RADIUS_MEDIUM, theme::RADIUS_MEDIUM, 0));
    icons::paint(&p, Icon::ChevronDown, cb.center(), theme::CHEVRON_SMALL, theme::ICON_SECONDARY);
    crate::widgets::record(ui, "present-button", pb);
    if pr.clicked() || cr.clicked() {
        app.start_present();
    }

    // Share button.
    let sb = Rect::from_min_size(Pos2::new(right - theme::RIGHT_SHARE_FROM_RIGHT, rect.top() + theme::RIGHT_BTN_Y), Vec2::new(theme::RIGHT_SHARE_W, theme::RIGHT_BTN_SIZE));
    let sr = ui.interact(sb, ui.id().with("share"), Sense::click());
    let bg = if sr.is_pointer_button_down_on() {
        theme::BG_BRAND_PRESSED
    } else if sr.hovered() {
        theme::BG_BRAND_HOVER
    } else {
        theme::BG_BRAND
    };
    p.rect_filled(sb, theme::cr(theme::RADIUS_MEDIUM), bg);
    p.text(sb.center(), Align2::CENTER_CENTER, "Share", theme::font_body(), Color32::WHITE);
    if sr.clicked() {
        app.show_toast("Sharing is not available in the offline editor", false);
    }

    // Tabs.
    let mut x = left + theme::RIGHT_TABS_X;
    let ty = rect.top() + theme::RIGHT_TABS_Y;
    let active = app.ui.right_tab.unwrap_or(RightTab::Design);
    for (tab, label) in [(RightTab::Design, "Design"), (RightTab::Prototype, "Prototype")] {
        let sel = tab == active;
        let font = if sel { theme::font_body_strong() } else { theme::font_body() };
        let w = widgets::text_width(&p, label, &font) + theme::RIGHT_TAB_PAD_X * 2.0;
        let r = Rect::from_min_size(Pos2::new(x, ty), Vec2::new(w, theme::RIGHT_TABS_H));
        let resp = ui.interact(r, ui.id().with(("tab", label)), Sense::click());
        if sel {
            p.rect_filled(r, theme::cr(theme::RADIUS_MEDIUM), theme::BG_SECONDARY);
        } else if resp.hovered() {
            p.rect_filled(r, theme::cr(theme::RADIUS_MEDIUM), theme::BG_HOVER);
        }
        p.text(r.center(), Align2::CENTER_CENTER, label, font, if sel { theme::TEXT } else { theme::TEXT_SECONDARY });
        if resp.clicked() {
            app.ui.right_tab = Some(tab);
        }
        x += w + 4.0;
    }

    // Zoom dropdown.
    let zr = Rect::from_min_size(Pos2::new(right - theme::RIGHT_ZOOM_FROM_RIGHT, ty), Vec2::new(theme::RIGHT_ZOOM_W, theme::RIGHT_TABS_H));
    let zresp = ui.interact(zr, ui.id().with("zoom"), Sense::click());
    let open = app.ui.menu == Some(MenuKind::Zoom);
    widgets::button_bg(&p, zr, &zresp, open, theme::cr(theme::RADIUS_MEDIUM));
    p.text(Pos2::new(zr.left() + 8.0, zr.center().y), Align2::LEFT_CENTER, format!("{}%", (app.view.zoom * 100.0).round()), theme::font_body(), theme::TEXT);
    icons::paint(&p, Icon::ChevronDown, Pos2::new(zr.right() - 10.0, zr.center().y), theme::CHEVRON_SMALL, theme::ICON_SECONDARY);
    if zresp.clicked() {
        app.ui.menu = if open { None } else { Some(MenuKind::Zoom) };
        app.ui.menu_anchor = Some(zr);
    }

    let content_top = rect.top() + theme::RIGHT_CONTENT_TOP - 1.0;
    widgets::hline(&p, rect.left() + 1.0, right, content_top, theme::BORDER);
    let region = Rect::from_min_max(Pos2::new(rect.left() + 1.0, content_top + 1.0), rect.max);
    if active == RightTab::Prototype {
        super::prototype::show(app, ui, region);
    } else if app.ui.dev_mode {
        super::inspect::show(app, ui, region);
    } else {
        properties::show(app, ui, region);
    }
}
