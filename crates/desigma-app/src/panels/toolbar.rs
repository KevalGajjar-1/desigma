//! Floating bottom-center toolbar.

use crate::app::{App, MenuKind};
use crate::canvas::tools::{Tool, GROUPS};
use crate::icons::{self, Icon};
use crate::theme;
use crate::widgets;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};

pub fn tool_icon(t: Tool) -> Icon {
    match t {
        Tool::Move => Icon::Move,
        Tool::Hand => Icon::Hand,
        Tool::Scale => Icon::Scale,
        Tool::Frame => Icon::Frame,
        Tool::Section => Icon::Section,
        Tool::Slice => Icon::Slice,
        Tool::Rectangle => Icon::Rectangle,
        Tool::Line => Icon::Line,
        Tool::Arrow => Icon::Arrow,
        Tool::Ellipse => Icon::Ellipse,
        Tool::Polygon => Icon::Polygon,
        Tool::Star => Icon::Star,
        Tool::Image => Icon::Image,
        Tool::Pen => Icon::Pen,
        Tool::Pencil => Icon::Pencil,
        Tool::Text => Icon::Text,
        Tool::Comment => Icon::Comment,
    }
}

/// Total toolbar width for the current group set.
pub fn toolbar_width() -> f32 {
    let mut w = theme::TOOLBELT_PADDING * 2.0;
    for (i, g) in GROUPS.iter().enumerate() {
        if i > 0 {
            w += theme::TOOL_GROUP_GAP;
        }
        w += theme::TOOL_BUTTON;
        if g.len() > 1 {
            w += theme::TOOL_CHEVRON_GAP + theme::TOOL_CHEVRON_W;
        }
    }
    w + theme::TOOL_GROUP_GAP + theme::TOOL_BUTTON + theme::TOOL_GROUP_GAP + theme::MODE_SWITCH_W * 2.0
}

pub fn toolbar_rect(screen: Rect) -> Rect {
    let w = toolbar_width();
    Rect::from_min_size(
        Pos2::new(screen.center().x - w / 2.0, screen.bottom() - theme::TOOLBELT_BOTTOM_MARGIN - theme::TOOLBELT_HEIGHT),
        Vec2::new(w, theme::TOOLBELT_HEIGHT),
    )
}

pub fn show(app: &mut App, ui: &mut Ui, screen: Rect) {
    let bar = toolbar_rect(screen);
    let p = ui.painter().clone();
    widgets::shadow(&p, bar, theme::RADIUS_LARGE, Vec2::new(0.0, 3.0), 8.0, theme::SHADOW_COLOR);
    widgets::shadow(&p, bar, theme::RADIUS_LARGE, Vec2::new(0.0, 1.0), 3.0, theme::SHADOW_COLOR_STRONG);
    p.rect_filled(bar, theme::cr(theme::RADIUS_LARGE), theme::BG_TOOLBAR);
    p.rect_stroke(bar.shrink(0.5), theme::cr(theme::RADIUS_LARGE), Stroke::new(1.0, theme::INNER_HIGHLIGHT), egui::StrokeKind::Inside);
    // Block canvas interaction under the bar.
    ui.interact(bar, ui.id().with("toolbar-block"), Sense::click_and_drag());

    let mut x = bar.left() + theme::TOOLBELT_PADDING;
    let y = bar.top() + (bar.height() - theme::TOOL_BUTTON) / 2.0;
    for (gi, group) in GROUPS.iter().enumerate() {
        if gi > 0 {
            x += theme::TOOL_GROUP_GAP;
        }
        let pick = app.group_pick[gi].min(group.len() - 1);
        let tool = group[pick];
        let selected = group.contains(&app.tool);
        let shown = if selected { app.tool } else { tool };
        let br = Rect::from_min_size(Pos2::new(x, y), Vec2::splat(theme::TOOL_BUTTON));
        let resp = ui.interact(br, ui.id().with(("tool", gi)), Sense::click());
        let (radius_btn, has_chev) = if group.len() > 1 {
            (theme::cr_each(theme::RADIUS_MEDIUM, 0, 0, theme::RADIUS_MEDIUM), true)
        } else {
            (theme::cr(theme::RADIUS_MEDIUM), false)
        };
        let fill = if selected { theme::BG_TOOLBAR_SELECTED } else if resp.hovered() { theme::BG_TOOLBAR_HOVER } else { Color32::TRANSPARENT };
        if fill != Color32::TRANSPARENT {
            p.rect_filled(br, radius_btn, fill);
        }
        icons::paint(&p, tool_icon(shown), br.center(), theme::TOOL_ICON_SIZE, if selected { Color32::WHITE } else { theme::ICON });
        if resp.clicked() {
            app.set_tool(tool);
        }
        let tip = if tool.shortcut().is_empty() { tool.label().to_string() } else { format!("{}   {}", tool.label(), tool.shortcut()) };
        resp.on_hover_text(tip);
        x += theme::TOOL_BUTTON;
        if has_chev {
            x += theme::TOOL_CHEVRON_GAP;
            let cr = Rect::from_min_size(Pos2::new(x, y), Vec2::new(theme::TOOL_CHEVRON_W, theme::TOOL_BUTTON));
            let cresp = ui.interact(cr, ui.id().with(("tool-chev", gi)), Sense::click());
            let open = app.ui.menu == Some(MenuKind::ToolGroup(gi));
            let cfill = if open { theme::BG_TOOLBAR_HOVER } else if selected { theme::BG_TOOLBAR_SELECTED } else if cresp.hovered() { theme::BG_TOOLBAR_HOVER } else { Color32::TRANSPARENT };
            if cfill != Color32::TRANSPARENT {
                p.rect_filled(cr, theme::cr_each(0, theme::RADIUS_MEDIUM, theme::RADIUS_MEDIUM, 0), cfill);
            }
            icons::paint(&p, Icon::ChevronUp, cr.center(), 10.0, if selected && !open { Color32::WHITE } else { theme::ICON_SECONDARY });
            if cresp.clicked() {
                app.ui.menu = if open { None } else { Some(MenuKind::ToolGroup(gi)) };
                app.ui.menu_anchor = Some(cr);
            }
            x += theme::TOOL_CHEVRON_W;
        }
    }

    // Actions button.
    x += theme::TOOL_GROUP_GAP;
    let ar = Rect::from_min_size(Pos2::new(x, y), Vec2::splat(theme::TOOL_BUTTON));
    let aresp = ui.interact(ar, ui.id().with("actions"), Sense::click());
    if aresp.hovered() {
        p.rect_filled(ar, theme::cr(theme::RADIUS_MEDIUM), theme::BG_TOOLBAR_HOVER);
    }
    icons::paint(&p, Icon::Sparkle, ar.center(), theme::TOOL_ICON_SIZE, theme::ICON);
    if aresp.on_hover_text("Actions").clicked() {
        app.show_toast("Actions are not available yet", false);
    }
    x += theme::TOOL_BUTTON + theme::TOOL_GROUP_GAP;

    // Design / dev mode switcher.
    let sw = Rect::from_min_size(Pos2::new(x, y), Vec2::new(theme::MODE_SWITCH_W * 2.0, theme::TOOL_BUTTON));
    p.rect_filled(sw, theme::cr(theme::RADIUS_MEDIUM), theme::BG_TOOLBAR_MODE_SWITCHER);
    for (i, (icon, dev)) in [(Icon::Design, false), (Icon::Dev, true)].into_iter().enumerate() {
        let r = Rect::from_min_size(Pos2::new(x + i as f32 * theme::MODE_SWITCH_W, y), Vec2::new(theme::MODE_SWITCH_W, theme::TOOL_BUTTON));
        let resp = ui.interact(r, ui.id().with(("mode", i)), Sense::click());
        let sel = app.ui.dev_mode == dev;
        if sel {
            p.rect_filled(r.shrink(2.0), theme::cr(theme::RADIUS_SMALL + 1), theme::BG_TOOLBAR_SELECTED);
        } else if resp.hovered() {
            p.rect_filled(r.shrink(2.0), theme::cr(theme::RADIUS_SMALL + 1), theme::BG_TOOLBAR_HOVER);
        }
        icons::paint(&p, icon, r.center(), 16.0, if sel { Color32::WHITE } else { theme::ICON_SECONDARY });
        if resp.on_hover_text(if dev { "Dev mode" } else { "Design" }).clicked() {
            app.ui.dev_mode = dev;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolbar_is_centered_and_above_bottom() {
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 1088.0));
        let r = toolbar_rect(screen);
        assert!((r.center().x - 800.0).abs() < 0.5);
        assert_eq!(r.bottom(), 1088.0 - theme::TOOLBELT_BOTTOM_MARGIN);
        assert_eq!(r.height(), theme::TOOLBELT_HEIGHT);
    }

    #[test]
    fn every_tool_has_an_icon() {
        for g in GROUPS {
            for t in *g {
                let _ = tool_icon(*t);
            }
        }
    }
}
