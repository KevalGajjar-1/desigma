//! Left sidebar: file header, pages list and the layer tree.

use std::collections::HashSet;

use desigma_core::{Editor, Node, NodeId, NodeKind};
use egui::{Align2, Id, Rect, Sense, Stroke, Ui, pos2, vec2};

use crate::icons::{self, Icon};
use crate::theme;

#[derive(Default)]
pub struct LayersPanel {
    expanded: HashSet<NodeId>,
    renaming: Option<(NodeId, String)>,
    renaming_page: Option<(usize, String)>,
    drag: Option<NodeId>,
    drop_target: Option<DropTarget>,
    last_selection: Vec<NodeId>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct DropTarget {
    parent: Option<NodeId>,
    index: usize,
    /// Y coordinate of the insertion line, or the row rect for "into".
    line_y: f32,
    indent: f32,
    into: Option<Rect>,
}

pub enum LayerAction {
    None,
    /// The user asked to look at this layer (zoom to it).
    Focus,
}

impl LayersPanel {
    /// Expands ancestors of newly selected layers so they are visible in the tree.
    fn reveal_selection(&mut self, editor: &Editor) {
        if editor.selection == self.last_selection {
            return;
        }
        self.last_selection = editor.selection.clone();
        for id in &editor.selection {
            if let Some(path) = editor.page().path_to(*id) {
                for a in &path[..path.len() - 1] {
                    self.expanded.insert(*a);
                }
            }
        }
    }

    pub fn pages_ui(&mut self, ui: &mut Ui, editor: &mut Editor) {
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Pages").font(theme::font_semibold(11.0)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                if icons::icon_button(ui, Icon::Plus, false, 24.0)
                    .on_hover_text("Add new page")
                    .clicked()
                {
                    editor.add_page();
                }
            });
        });
        let mut switch_to = None;
        let mut delete = None;
        for i in 0..editor.doc.pages.len() {
            let (rect, resp) =
                ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
            let current = i == editor.page;
            let painter = ui.painter();
            let row = rect.shrink2(vec2(8.0, 2.0));
            if current {
                painter.rect_filled(row, theme::RADIUS, theme::HOVER_BG);
            } else if resp.hovered() {
                painter.rect_filled(row, theme::RADIUS, theme::HOVER_BG.gamma_multiply(0.6));
            }
            if let Some((idx, buf)) = &mut self.renaming_page
                && *idx == i
            {
                let r = row.shrink2(vec2(20.0, 4.0));
                let te = ui.put(r, egui::TextEdit::singleline(buf).font(theme::font(11.0)));
                te.request_focus();
                if te.lost_focus() {
                    let name = buf.trim().to_string();
                    if !name.is_empty() && name != editor.doc.pages[i].name {
                        editor.checkpoint();
                        editor.doc.pages[i].name = name;
                    }
                    self.renaming_page = None;
                }
                continue;
            }
            let font = if current {
                theme::font_semibold(11.0)
            } else {
                theme::font(11.0)
            };
            painter.text(
                pos2(row.left() + 24.0, row.center().y),
                Align2::LEFT_CENTER,
                &editor.doc.pages[i].name,
                font,
                theme::TEXT,
            );
            if resp.clicked() {
                switch_to = Some(i);
            }
            if resp.double_clicked() {
                self.renaming_page = Some((i, editor.doc.pages[i].name.clone()));
            }
            resp.context_menu(|ui| {
                if ui.button("Rename").clicked() {
                    self.renaming_page = Some((i, editor.doc.pages[i].name.clone()));
                    ui.close();
                }
                if ui
                    .add_enabled(editor.doc.pages.len() > 1, egui::Button::new("Delete page"))
                    .clicked()
                {
                    delete = Some(i);
                    ui.close();
                }
            });
        }
        if let Some(i) = switch_to {
            editor.set_page(i);
        }
        if let Some(i) = delete {
            editor.delete_page(i);
        }
    }

    pub fn layers_ui(&mut self, ui: &mut Ui, editor: &mut Editor) -> LayerAction {
        self.reveal_selection(editor);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Layers").font(theme::font_semibold(11.0)));
        });
        let mut action = LayerAction::None;
        self.drop_target = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let roots: Vec<Node> = editor.page().children.iter().rev().cloned().collect();
                if roots.is_empty() {
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.add_space(16.0);
                        ui.label(
                            egui::RichText::new("No layers yet. Press F to draw a frame.")
                                .color(theme::TEXT_TERTIARY),
                        );
                    });
                }
                let len = roots.len();
                for (i, n) in roots.iter().enumerate() {
                    // Rows are drawn top-first, which is the reverse of paint order.
                    self.row(ui, editor, n, None, len - 1 - i, 0, false, &mut action);
                }
                // Drop below the last row puts the layer at the very bottom of the page.
                let (rest, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
                if let (Some(_), Some(p)) = (self.drag, ui.ctx().pointer_hover_pos())
                    && rest.contains(p)
                {
                    self.drop_target = Some(DropTarget {
                        parent: None,
                        index: 0,
                        line_y: rest.top(),
                        indent: 0.0,
                        into: None,
                    });
                }
            });

        // Drop indicator and completion.
        if let Some(t) = self.drop_target {
            let painter = ui.ctx().layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                Id::new("layers-dnd"),
            ));
            match t.into {
                Some(r) => {
                    painter.rect_stroke(
                        r,
                        2.0,
                        Stroke::new(2.0, theme::BLUE),
                        egui::StrokeKind::Inside,
                    );
                }
                None => {
                    let x0 = ui.min_rect().left() + 8.0 + t.indent;
                    painter.line_segment(
                        [
                            pos2(x0, t.line_y),
                            pos2(ui.min_rect().right() - 8.0, t.line_y),
                        ],
                        Stroke::new(2.0, theme::BLUE),
                    );
                }
            }
        }
        if ui.input(|i| i.pointer.any_released()) {
            if let (Some(id), Some(t)) = (self.drag.take(), self.drop_target.take()) {
                editor.move_layer(id, t.parent, t.index);
            }
            self.drag = None;
        }
        action
    }

    #[allow(clippy::too_many_arguments)]
    fn row(
        &mut self,
        ui: &mut Ui,
        editor: &mut Editor,
        node: &Node,
        parent: Option<NodeId>,
        index: usize,
        depth: usize,
        ancestor_selected: bool,
        action: &mut LayerAction,
    ) {
        let width = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(
            vec2(width, theme::LAYER_ROW_HEIGHT),
            Sense::click_and_drag(),
        );
        let selected = editor.is_selected(node.id);
        let painter = ui.painter().clone();
        let row = rect.shrink2(vec2(8.0, 0.0));
        let hovered = resp.hovered() || resp.contains_pointer();

        if selected {
            painter.rect_filled(row, theme::RADIUS, theme::SELECTED_ROW);
        } else if ancestor_selected {
            painter.rect_filled(row, 0.0, theme::SELECTED_CHILD_ROW);
        } else if hovered {
            painter.rect_stroke(
                row,
                theme::RADIUS,
                Stroke::new(1.0, theme::BLUE),
                egui::StrokeKind::Inside,
            );
        }

        let indent = depth as f32 * 16.0;
        let mut x = row.left() + 4.0 + indent;
        let has_children = !node.children.is_empty();
        let expanded = self.expanded.contains(&node.id);
        let chevron = Rect::from_min_size(pos2(x, rect.top()), vec2(16.0, rect.height()));
        if has_children {
            icons::paint(
                &painter,
                chevron,
                if expanded {
                    Icon::ChevronDown
                } else {
                    Icon::ChevronRight
                },
                theme::TEXT_SECONDARY,
            );
        }
        x += 16.0;
        let icon_rect = Rect::from_min_size(pos2(x, rect.top()), vec2(16.0, rect.height()));
        let icon = match node.kind {
            NodeKind::Frame { .. } => Icon::Frame,
            NodeKind::Group => Icon::Group,
            NodeKind::Rectangle => Icon::Rectangle,
            NodeKind::Ellipse => Icon::Ellipse,
            NodeKind::Text(_) => Icon::Text,
        };
        let dim = !node.visible;
        let text_color = if dim {
            theme::TEXT_TERTIARY
        } else {
            theme::TEXT
        };
        icons::paint(
            &painter,
            icon_rect,
            icon,
            if dim {
                theme::TEXT_TERTIARY
            } else {
                theme::TEXT_SECONDARY
            },
        );
        x += 24.0;

        // Visibility / lock toggles appear on hover, or stay when active.
        let eye =
            Rect::from_center_size(pos2(row.right() - 16.0, rect.center().y), vec2(24.0, 24.0));
        let lock =
            Rect::from_center_size(pos2(row.right() - 40.0, rect.center().y), vec2(24.0, 24.0));
        let name_right = if hovered || node.locked || !node.visible {
            lock.left()
        } else {
            row.right() - 8.0
        };

        let renaming = matches!(&self.renaming, Some((id, _)) if *id == node.id);
        if renaming {
            let r = Rect::from_min_max(
                pos2(x - 4.0, rect.top() + 4.0),
                pos2(name_right, rect.bottom() - 4.0),
            );
            let mut commit = None;
            if let Some((_, buf)) = &mut self.renaming {
                let te = ui.put(r, egui::TextEdit::singleline(buf).font(theme::font(11.0)));
                te.request_focus();
                if te.lost_focus() {
                    commit = Some(buf.trim().to_string());
                }
            }
            if let Some(name) = commit {
                if !name.is_empty() && name != node.name {
                    editor.edit_node(node.id, "rename", |n| n.name = name);
                    editor.end_coalescing();
                }
                self.renaming = None;
            }
        } else {
            let font = if node.is_frame() && depth == 0 {
                theme::font_semibold(11.0)
            } else {
                theme::font(11.0)
            };
            let galley = painter.layout_no_wrap(node.name.clone(), font, text_color);
            let clip = painter.with_clip_rect(Rect::from_min_max(
                pos2(x, rect.top()),
                pos2(name_right, rect.bottom()),
            ));
            clip.galley(
                pos2(x, rect.center().y - galley.size().y / 2.0),
                galley,
                text_color,
            );
        }

        let mut toggled = false;
        if hovered || node.locked {
            let r = ui.interact(lock, ui.id().with(("lock", node.id.0)), Sense::click());
            icons::paint(
                &painter,
                lock,
                if node.locked {
                    Icon::Lock
                } else {
                    Icon::Unlock
                },
                if node.locked {
                    theme::TEXT
                } else {
                    theme::TEXT_SECONDARY
                },
            );
            if r.clicked() {
                editor.edit_node(node.id, "lock", |n| n.locked = !n.locked);
                editor.end_coalescing();
                toggled = true;
            }
        }
        if hovered || !node.visible {
            let r = ui.interact(eye, ui.id().with(("eye", node.id.0)), Sense::click());
            icons::paint(
                &painter,
                eye,
                if node.visible {
                    Icon::Eye
                } else {
                    Icon::EyeOff
                },
                if node.visible {
                    theme::TEXT_SECONDARY
                } else {
                    theme::TEXT
                },
            );
            if r.clicked() {
                editor.edit_node(node.id, "visible", |n| n.visible = !n.visible);
                editor.end_coalescing();
                toggled = true;
            }
        }

        if !toggled {
            let pointer = ui.ctx().pointer_interact_pos();
            if resp.clicked() {
                if has_children && pointer.is_some_and(|p| chevron.contains(p)) {
                    if !self.expanded.remove(&node.id) {
                        self.expanded.insert(node.id);
                    }
                } else if ui.input(|i| i.modifiers.shift || i.modifiers.command) {
                    editor.toggle_selected(node.id);
                } else {
                    editor.select_only(node.id);
                }
                self.last_selection = editor.selection.clone();
            }
            if resp.double_clicked() {
                self.renaming = Some((node.id, node.name.clone()));
            }
            if resp.drag_started() {
                self.drag = Some(node.id);
            }
        }
        resp.context_menu(|ui| {
            if !editor.is_selected(node.id) {
                editor.select_only(node.id);
            }
            if ui.button("Zoom to layer").clicked() {
                *action = LayerAction::Focus;
                ui.close();
            }
            if ui.button("Rename").clicked() {
                self.renaming = Some((node.id, node.name.clone()));
                ui.close();
            }
            ui.separator();
            if ui.button("Duplicate").clicked() {
                editor.duplicate_selection(desigma_core::Vec2::ZERO);
                ui.close();
            }
            if ui.button("Group selection").clicked() {
                editor.group_selection();
                ui.close();
            }
            if ui.button("Delete").clicked() {
                editor.delete_selection();
                ui.close();
            }
        });

        // Drop target computation while another row is being dragged.
        if let (Some(dragging), Some(p)) = (self.drag, ui.ctx().pointer_hover_pos())
            && dragging != node.id
            && rect.contains(p)
        {
            let t = (p.y - rect.top()) / rect.height();
            let can_nest = node.kind.is_container();
            // Rows show the topmost layer first, so "above" in the panel means a higher index.
            self.drop_target = Some(if can_nest && (0.25..0.75).contains(&t) {
                let len = node.children.len();
                DropTarget {
                    parent: Some(node.id),
                    index: len,
                    line_y: 0.0,
                    indent: 0.0,
                    into: Some(row),
                }
            } else if t < 0.5 {
                DropTarget {
                    parent,
                    index: index + 1,
                    line_y: rect.top(),
                    indent,
                    into: None,
                }
            } else {
                DropTarget {
                    parent,
                    index,
                    line_y: rect.bottom(),
                    indent,
                    into: None,
                }
            });
        }

        if has_children && expanded {
            let len = node.children.len();
            for (i, c) in node.children.iter().rev().enumerate() {
                self.row(
                    ui,
                    editor,
                    c,
                    Some(node.id),
                    len - 1 - i,
                    depth + 1,
                    ancestor_selected || selected,
                    action,
                );
            }
        }
    }
}
