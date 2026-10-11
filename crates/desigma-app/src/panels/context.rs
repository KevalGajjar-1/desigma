//! Right-click context menu for the selection (canvas and layers).

use super::menus::{self, Action};
use crate::app::App;
use crate::widgets::menu::{popup_menu, MenuItem, MenuResult};
use desigma_doc::NodeKind;
use egui::{Context, Id};

/// Items with the action each one runs, given the current selection.
fn items(app: &App) -> Vec<(MenuItem, Action)> {
    let has = !app.selection.is_empty();
    let insts = app.selected_instances();
    let has_inst = !insts.is_empty();
    let any_container =
        app.selection.iter().any(|id| app.doc.get(*id).is_some_and(|n| matches!(n.kind, NodeKind::Group | NodeKind::Frame(_) | NodeKind::Section)));
    let can_create = app
        .selection
        .iter()
        .any(|id| app.doc.get(*id).is_some_and(|n| !matches!(n.kind, NodeKind::Component(_) | NodeKind::ComponentSet(_) | NodeKind::Page { .. })));
    vec![
        (MenuItem::new("Duplicate").shortcut("Ctrl+D").disabled(!has), Action::Duplicate),
        (MenuItem::new("Group selection").shortcut("Ctrl+G").disabled(!has), Action::Group),
        (MenuItem::new("Frame selection").shortcut("Ctrl+Alt+G").disabled(!has), Action::FrameSelection),
        (MenuItem::new("Ungroup").shortcut("Ctrl+Shift+G").disabled(!any_container), Action::Ungroup),
        (MenuItem::new("Add auto layout").shortcut("Shift+A").disabled(!has).sep(), Action::AutoLayout),
        (MenuItem::new("Create component").shortcut("Ctrl+Alt+K").disabled(!can_create), Action::CreateComponent),
        (MenuItem::new("Combine as variants").shortcut("Ctrl+Shift+K").disabled(!has), Action::CombineVariants),
        (MenuItem::new("Detach instance").shortcut("Ctrl+Alt+B").disabled(!has_inst), Action::Detach),
        (MenuItem::new("Go to main component").disabled(!has_inst), Action::GoToMain),
        (MenuItem::new("Reset all changes").disabled(!has_inst).sep(), Action::ResetInstance),
        (MenuItem::new("Bring to front").shortcut("Ctrl+Shift+]").disabled(!has), Action::BringToFront),
        (MenuItem::new("Send to back").shortcut("Ctrl+Shift+[").disabled(!has).sep(), Action::SendToBack),
        (MenuItem::new("Delete").shortcut("Del").disabled(!has), Action::Delete),
    ]
}

pub fn show(app: &mut App, ctx: &Context) {
    let Some(at) = app.ui.ctx_menu else { return };
    let entries = items(app);
    let list: Vec<MenuItem> = entries.iter().map(|e| e.0.clone()).collect();
    match popup_menu(ctx, Id::new("selection-menu"), at, 230.0, &list, None) {
        MenuResult::Picked(i) => {
            app.ui.ctx_menu = None;
            menus::run(app, ctx, entries[i].1);
        }
        MenuResult::Closed => app.ui.ctx_menu = None,
        MenuResult::Open => {}
    }
}

#[cfg(test)]
pub fn labels(app: &App) -> Vec<String> {
    items(app).into_iter().map(|e| e.0.label).collect()
}
