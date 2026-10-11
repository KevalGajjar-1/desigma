//! Application menus and the shared action dispatcher (menus + shortcuts).

use crate::app::{App, MenuKind};
use crate::canvas::tools::GROUPS;
use crate::theme;
use crate::widgets::menu::{popup_menu, MenuItem, MenuResult};
use egui::{Context, Id, Pos2, Vec2};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    New,
    Open,
    Save,
    SaveAs,
    Export,
    Undo,
    Redo,
    SelectAll,
    Delete,
    Duplicate,
    Group,
    Ungroup,
    FrameSelection,
    AutoLayout,
    FitAll,
    FitSelection,
    Zoom100,
    ZoomIn,
    ZoomOut,
    ZoomTo(f32),
    BringForward,
    SendBackward,
    BringToFront,
    SendToBack,
    ToggleSidebar,
    CreateComponent,
    Detach,
    CombineVariants,
    GoToMain,
    ResetInstance,
}

pub fn run(app: &mut App, ctx: &Context, a: Action) {
    match a {
        Action::New => app.new_document(),
        Action::Open => app.open_dialog(),
        Action::Save => app.save(false),
        Action::SaveAs => app.save(true),
        Action::Export => app.export_selection(),
        Action::Undo => app.undo(),
        Action::Redo => app.redo(),
        Action::SelectAll => {
            let kids = app.doc.get(app.page).map(|p| p.children.clone()).unwrap_or_default();
            app.selection = kids.into_iter().filter(|c| app.doc.get(*c).map(|n| !n.locked && n.visible).unwrap_or(false)).collect();
        }
        Action::Delete => app.delete_selection(),
        Action::Duplicate => app.duplicate_selection(),
        Action::Group => app.group_selection(false),
        Action::Ungroup => app.ungroup_selection(),
        Action::FrameSelection => app.group_selection(true),
        Action::AutoLayout => app.toggle_auto_layout(),
        Action::FitAll => app.fit_all(),
        Action::FitSelection => app.fit_selection(),
        Action::Zoom100 => app.zoom_to(1.0),
        Action::ZoomIn => app.zoom_by(1.25),
        Action::ZoomOut => app.zoom_by(0.8),
        Action::ZoomTo(z) => app.zoom_to(z),
        Action::BringForward => app.reorder(true, false),
        Action::SendBackward => app.reorder(false, false),
        Action::BringToFront => app.reorder(true, true),
        Action::SendToBack => app.reorder(false, true),
        Action::ToggleSidebar => app.ui.left_collapsed = !app.ui.left_collapsed,
        Action::CreateComponent => app.create_component(),
        Action::Detach => app.detach_selection(),
        Action::CombineVariants => app.combine_selection_as_variants(),
        Action::GoToMain => {
            if let Some(i) = app.selected_instances().first().copied() {
                app.go_to_main(i);
            }
        }
        Action::ResetInstance => app.reset_selection(),
    }
    ctx.request_repaint();
}

fn file_items(app: &App) -> Vec<(MenuItem, Option<Action>)> {
    let has_sel = !app.selection.is_empty();
    vec![
        (MenuItem::new("New file"), Some(Action::New)),
        (MenuItem::new("Open…").shortcut("Ctrl+O"), Some(Action::Open)),
        (MenuItem::new("Save").shortcut("Ctrl+S"), Some(Action::Save)),
        (MenuItem::new("Save as…").shortcut("Ctrl+Shift+S").sep(), Some(Action::SaveAs)),
        (MenuItem::new("Undo").shortcut("Ctrl+Z").disabled(!app.hist.can_undo()), Some(Action::Undo)),
        (MenuItem::new("Redo").shortcut("Ctrl+Shift+Z").disabled(!app.hist.can_redo()).sep(), Some(Action::Redo)),
        (MenuItem::new("Select all").shortcut("Ctrl+A"), Some(Action::SelectAll)),
        (MenuItem::new("Group selection").shortcut("Ctrl+G").disabled(!has_sel), Some(Action::Group)),
        (MenuItem::new("Frame selection").shortcut("Ctrl+Alt+G").disabled(!has_sel), Some(Action::FrameSelection)),
        (MenuItem::new("Add auto layout").shortcut("Shift+A").disabled(!has_sel).sep(), Some(Action::AutoLayout)),
        (MenuItem::new("Create component").shortcut("Ctrl+Alt+K").disabled(!has_sel), Some(Action::CreateComponent)),
        (MenuItem::new("Detach instance").shortcut("Ctrl+Alt+B").disabled(!has_sel).sep(), Some(Action::Detach)),
        (MenuItem::new("Export selection as PNG…").disabled(!has_sel), Some(Action::Export)),
    ]
}

fn zoom_items() -> Vec<(MenuItem, Option<Action>)> {
    vec![
        (MenuItem::new("Zoom in").shortcut("Ctrl+="), Some(Action::ZoomIn)),
        (MenuItem::new("Zoom out").shortcut("Ctrl+-").sep(), Some(Action::ZoomOut)),
        (MenuItem::new("Zoom to fit").shortcut("Shift+1"), Some(Action::FitAll)),
        (MenuItem::new("Zoom to selection").shortcut("Shift+2").sep(), Some(Action::FitSelection)),
        (MenuItem::new("Zoom to 50%"), Some(Action::ZoomTo(0.5))),
        (MenuItem::new("Zoom to 100%").shortcut("Shift+0"), Some(Action::Zoom100)),
        (MenuItem::new("Zoom to 200%"), Some(Action::ZoomTo(2.0))),
    ]
}

/// Shows the currently open menu (if any) and executes picked entries.
pub fn show(app: &mut App, ctx: &Context) {
    let Some(kind) = app.ui.menu else { return };
    let anchor = app.ui.menu_anchor;
    let id = Id::new(("app-menu", kind));
    match kind {
        MenuKind::ToolGroup(g) => {
            let tools = GROUPS[g];
            let items: Vec<MenuItem> = tools.iter().map(|t| MenuItem::new(t.label()).shortcut(t.shortcut()).checked(app.tool == *t).icon(super::toolbar::tool_icon(*t))).collect();
            let h = items.len() as f32 * theme::MENU_ITEM_HEIGHT + theme::MENU_PADDING;
            let a = anchor.unwrap_or(egui::Rect::NOTHING);
            let pos = Pos2::new(a.left() - 24.0, a.top() - h - 14.0);
            match popup_menu(ctx, id, pos, 190.0, &items, anchor) {
                MenuResult::Picked(i) => {
                    app.set_tool(tools[i]);
                    app.group_pick[g] = i;
                    app.ui.menu = None;
                }
                MenuResult::Closed => app.ui.menu = None,
                MenuResult::Open => {}
            }
        }
        other => {
            let entries = match other {
                MenuKind::Zoom => zoom_items(),
                _ => file_items(app),
            };
            let items: Vec<MenuItem> = entries.iter().map(|e| e.0.clone()).collect();
            let a = anchor.unwrap_or(egui::Rect::NOTHING);
            let pos = match other {
                MenuKind::Main | MenuKind::FileMenu => a.right_top() + Vec2::new(4.0, 0.0),
                _ => a.left_bottom() + Vec2::new(0.0, 4.0),
            };
            match popup_menu(ctx, id, pos, theme::MENU_WIDTH, &items, anchor) {
                MenuResult::Picked(i) => {
                    app.ui.menu = None;
                    if let Some(act) = entries[i].1 {
                        run(app, ctx, act);
                    }
                }
                MenuResult::Closed => app.ui.menu = None,
                MenuResult::Open => {}
            }
        }
    }
}
