//! Application shell: panels, toolbar, shortcuts, clipboard and file handling.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use desigma_core::{Editor, Node, NodeKind, Rect as DRect, TextData, Vec2 as DVec2, ZOrder, io};
use desigma_render::Fonts;
use egui::{
    Color32, Key, KeyboardShortcut, Modifiers, Rect, RichText, Sense, Stroke, Ui, pos2, vec2,
};
use serde::{Deserialize, Serialize};

use crate::canvas::{Canvas, Tool};
use crate::icons::{self, Icon};
use crate::layers::{LayerAction, LayersPanel};
use crate::{properties, theme};

pub struct DesigmaApp {
    editor: Editor,
    fonts: Fonts,
    canvas: Canvas,
    tool: Tool,
    layers: LayersPanel,
    toast: Option<(String, Instant)>,
    confirm_close: bool,
    allow_close: bool,
    last_title: String,
}

/// What we put on the system clipboard so layers survive copy/paste across files.
#[derive(Serialize, Deserialize)]
struct ClipboardPayload {
    desigma_clipboard: u32,
    nodes: Vec<Node>,
}

impl DesigmaApp {
    pub fn new(cc: &eframe::CreationContext<'_>, path: Option<PathBuf>) -> Self {
        theme::install(&cc.egui_ctx);
        let mut app = Self {
            editor: Editor::default(),
            fonts: Fonts::default(),
            canvas: Canvas::new(cc.wgpu_render_state.as_ref()),
            tool: Tool::Move,
            layers: LayersPanel::default(),
            toast: None,
            confirm_close: false,
            allow_close: false,
            last_title: String::new(),
        };
        if let Some(p) = path {
            app.open_path(p);
        }
        app
    }

    fn notify(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    fn file_name(&self) -> String {
        self.editor
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".into())
    }

    // ---- files ---------------------------------------------------------

    fn new_file(&mut self) {
        self.editor = Editor::default();
        self.canvas.reset();
    }

    fn open_dialog(&mut self) {
        let picked = rfd::FileDialog::new()
            .add_filter("Desigma file", &[io::FILE_EXTENSION])
            .pick_file();
        if let Some(p) = picked {
            self.open_path(p);
        }
    }

    fn open_path(&mut self, path: PathBuf) {
        match io::load(&path) {
            Ok(doc) => {
                self.editor = Editor::from_document(doc, Some(path));
                self.canvas.reset();
            }
            Err(e) => self.notify(format!("Couldn't open {}: {e}", path.display())),
        }
    }

    fn save(&mut self) -> bool {
        match self.editor.path.clone() {
            Some(p) => self.save_to(p),
            None => self.save_as(),
        }
    }

    fn save_as(&mut self) -> bool {
        let picked = rfd::FileDialog::new()
            .add_filter("Desigma file", &[io::FILE_EXTENSION])
            .set_file_name(format!("{}.{}", self.file_name(), io::FILE_EXTENSION))
            .save_file();
        match picked {
            Some(mut p) => {
                if p.extension().is_none() {
                    p.set_extension(io::FILE_EXTENSION);
                }
                self.save_to(p)
            }
            None => false,
        }
    }

    fn save_to(&mut self, path: PathBuf) -> bool {
        match io::save(&self.editor.doc, &path) {
            Ok(()) => {
                self.notify(format!("Saved to {}", path.display()));
                self.editor.path = Some(path);
                self.editor.dirty = false;
                true
            }
            Err(e) => {
                self.notify(format!("Couldn't save: {e}"));
                false
            }
        }
    }

    // ---- clipboard -----------------------------------------------------

    fn copy(&mut self, ctx: &egui::Context) -> bool {
        let page = self.editor.page();
        let nodes: Vec<Node> = self
            .editor
            .selection_roots()
            .iter()
            .filter_map(|id| {
                let mut n = page.find(*id)?.clone();
                let w = page.world_rect(*id)?;
                n.x = w.x;
                n.y = w.y;
                Some(n)
            })
            .collect();
        if nodes.is_empty() {
            return false;
        }
        let payload = ClipboardPayload {
            desigma_clipboard: 1,
            nodes,
        };
        if let Ok(s) = serde_json::to_string(&payload) {
            ctx.copy_text(s);
        }
        true
    }

    fn paste(&mut self, text: &str) {
        // Paste into a selected frame, otherwise onto the page.
        let target = match self.editor.selection.as_slice() {
            [id] => self
                .editor
                .page()
                .find(*id)
                .filter(|n| n.is_frame())
                .map(|n| n.id),
            _ => None,
        };
        let origin = self.editor.page().content_origin(target);
        let nodes = match serde_json::from_str::<ClipboardPayload>(text) {
            Ok(p) => p.nodes,
            Err(_) => {
                // Plain text becomes a text layer, as in Figma.
                let text = text.trim_end();
                if text.is_empty() {
                    return;
                }
                let center = self
                    .canvas
                    .view
                    .to_world(egui::Pos2::ZERO, (self.canvas.size / 2.0).to_pos2());
                let id = self.editor.create_node(
                    NodeKind::Text(TextData::new(text)),
                    DRect::new(center.x.round(), center.y.round(), 10.0, 14.0),
                );
                self.editor.select_only(id);
                return;
            }
        };
        self.editor.checkpoint();
        let mut sel = vec![];
        for n in nodes {
            let mut copy = self.editor.doc.clone_with_new_ids(&n);
            copy.x -= origin.x;
            copy.y -= origin.y;
            sel.push(copy.id);
            let len = self
                .editor
                .page()
                .children_of(target)
                .map_or(0, |c| c.len());
            self.editor.page_mut().insert(target, len, copy);
        }
        self.editor.selection = sel;
    }

    // ---- shortcuts -----------------------------------------------------

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        // Clipboard events arrive even while typing; only act when the canvas has focus.
        let typing = ctx.egui_wants_keyboard_input() || self.canvas.editing_text.is_some();
        let events = ctx.input(|i| i.events.clone());
        if !typing {
            for e in &events {
                match e {
                    egui::Event::Copy => {
                        self.copy(ctx);
                    }
                    egui::Event::Cut => {
                        if self.copy(ctx) {
                            self.editor.delete_selection();
                        }
                    }
                    egui::Event::Paste(text) => self.paste(text),
                    _ => {}
                }
            }
        }
        if typing {
            return;
        }

        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let shift = Modifiers::SHIFT;
        let none = Modifiers::NONE;
        let pressed = |m: Modifiers, k: Key| {
            ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)))
        };

        // File and history.
        if pressed(cmd_shift, Key::S) {
            self.save_as();
        }
        if pressed(cmd, Key::S) {
            self.save();
        }
        if pressed(cmd, Key::O) {
            self.open_dialog();
        }
        if pressed(cmd, Key::N) {
            self.new_file();
        }
        if pressed(cmd_shift, Key::Z) || pressed(cmd, Key::Y) {
            self.editor.redo();
        }
        if pressed(cmd, Key::Z) {
            self.editor.undo();
        }

        // Structure.
        if pressed(cmd, Key::D) {
            self.editor.duplicate_selection(DVec2::ZERO);
        }
        if pressed(cmd_shift, Key::G) {
            self.editor.ungroup_selection();
        }
        if pressed(cmd, Key::G) {
            self.editor.group_selection();
        }
        if pressed(cmd, Key::A) {
            self.editor.select_all();
        }
        if pressed(cmd_shift, Key::H) {
            self.editor
                .edit_selected("toggle-visible", |n| n.visible = !n.visible);
            self.editor.end_coalescing();
        }
        if pressed(cmd_shift, Key::L) {
            self.editor
                .edit_selected("toggle-lock", |n| n.locked = !n.locked);
            self.editor.end_coalescing();
        }
        if pressed(cmd, Key::CloseBracket) {
            self.editor.reorder_selection(ZOrder::Front);
        }
        if pressed(cmd, Key::OpenBracket) {
            self.editor.reorder_selection(ZOrder::Back);
        }
        if pressed(none, Key::CloseBracket) {
            self.editor.reorder_selection(ZOrder::Forward);
        }
        if pressed(none, Key::OpenBracket) {
            self.editor.reorder_selection(ZOrder::Backward);
        }
        if pressed(none, Key::Delete) || pressed(none, Key::Backspace) {
            self.editor.delete_selection();
        }

        // Selection navigation.
        if pressed(shift, Key::Enter) || pressed(none, Key::Escape) {
            let parent = self
                .editor
                .selection
                .first()
                .and_then(|id| self.editor.page().locate(*id))
                .and_then(|l| l.parent);
            match parent {
                Some(p) => self.editor.select_only(p),
                None => self.editor.clear_selection(),
            }
            self.tool = Tool::Move;
        }
        if pressed(none, Key::Enter)
            && let [id] = self.editor.selection.as_slice()
        {
            let id = *id;
            let node = self.editor.page().find(id).cloned();
            match node {
                Some(n) if n.text().is_some() => self.canvas.start_text_edit(id),
                Some(n) if !n.children.is_empty() => {
                    self.editor.selection = n.children.iter().map(|c| c.id).collect();
                }
                _ => {}
            }
        }

        // Nudge.
        for (k, d) in [
            (Key::ArrowLeft, DVec2::new(-1.0, 0.0)),
            (Key::ArrowRight, DVec2::new(1.0, 0.0)),
            (Key::ArrowUp, DVec2::new(0.0, -1.0)),
            (Key::ArrowDown, DVec2::new(0.0, 1.0)),
        ] {
            if pressed(shift, k) {
                self.nudge(d * 10.0);
            }
            if pressed(none, k) {
                self.nudge(d);
            }
        }

        // Zoom.
        if pressed(shift, Key::Num0) {
            self.canvas.set_zoom(1.0);
        }
        if pressed(shift, Key::Num1) {
            self.canvas.zoom_to_fit(&self.editor);
        }
        if pressed(shift, Key::Num2) {
            self.canvas.zoom_to_selection(&self.editor);
        }
        if pressed(cmd, Key::Equals) || pressed(cmd, Key::Plus) {
            self.canvas.zoom_by(2.0);
        }
        if pressed(cmd, Key::Minus) {
            self.canvas.zoom_by(0.5);
        }

        // Tools.
        for (k, t) in [
            (Key::V, Tool::Move),
            (Key::H, Tool::Hand),
            (Key::F, Tool::Frame),
            (Key::A, Tool::Frame),
            (Key::R, Tool::Rectangle),
            (Key::O, Tool::Ellipse),
            (Key::T, Tool::Text),
        ] {
            if pressed(none, k) {
                self.tool = t;
            }
        }
    }

    fn nudge(&mut self, d: DVec2) {
        if self.editor.selection.is_empty() {
            return;
        }
        self.editor.checkpoint_coalesced("nudge");
        self.editor.translate_selection(d);
    }

    /// Keeps text boxes sized to their content ("Auto width" / auto height).
    fn sync_text_sizes(&mut self) {
        let fonts = &self.fonts;
        let page = self.editor.page_mut();
        for n in &mut page.children {
            n.walk_mut(&mut |n| {
                let Some(t) = n.text() else { return };
                if t.auto_width {
                    let (w, h) = fonts.measure(t);
                    n.width = w;
                    n.height = h;
                } else {
                    n.height = fonts.layout(t, Some(n.width)).height;
                }
            });
        }
        page.fit_groups();
    }

    // ---- panels --------------------------------------------------------

    fn left_panel(&mut self, ui: &mut Ui) {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            let menu = ui.menu_button(RichText::new("◆").color(theme::TEXT).size(14.0), |ui| {
                ui.set_min_width(200.0);
                if ui.button("New file                 Ctrl+N").clicked() {
                    self.new_file();
                }
                if ui.button("Open…                    Ctrl+O").clicked() {
                    self.open_dialog();
                }
                ui.separator();
                if ui.button("Save                       Ctrl+S").clicked() {
                    self.save();
                }
                if ui.button("Save as…          Shift+Ctrl+S").clicked() {
                    self.save_as();
                }
                ui.separator();
                if ui
                    .add_enabled(
                        self.editor.can_undo(),
                        egui::Button::new("Undo                       Ctrl+Z"),
                    )
                    .clicked()
                {
                    self.editor.undo();
                }
                if ui
                    .add_enabled(
                        self.editor.can_redo(),
                        egui::Button::new("Redo                Shift+Ctrl+Z"),
                    )
                    .clicked()
                {
                    self.editor.redo();
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            menu.response.on_hover_text("Main menu");
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                let name = if self.editor.dirty {
                    format!("{} •", self.file_name())
                } else {
                    self.file_name()
                };
                ui.label(RichText::new(name).font(theme::font_semibold(13.0)));
                ui.label(RichText::new("Drafts").color(theme::TEXT_SECONDARY));
            });
        });
        ui.add_space(8.0);
        hline(ui);
        ui.add_space(4.0);
        self.layers.pages_ui(ui, &mut self.editor);
        ui.add_space(4.0);
        hline(ui);
        ui.add_space(4.0);
        if let LayerAction::Focus = self.layers.layers_ui(ui, &mut self.editor) {
            self.canvas.zoom_to_selection(&self.editor);
        }
    }

    fn toolbar(&mut self, ctx: &egui::Context, canvas_rect: Rect) {
        let tools = [
            Tool::Move,
            Tool::Frame,
            Tool::Rectangle,
            Tool::Ellipse,
            Tool::Text,
            Tool::Hand,
        ];
        let size = vec2(tools.len() as f32 * 36.0 + 12.0, 48.0);
        let pos = pos2(
            canvas_rect.center().x - size.x / 2.0,
            canvas_rect.bottom() - size.y - 12.0,
        );
        egui::Area::new(egui::Id::new("toolbar"))
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::PANEL_BG)
                    .stroke(Stroke::new(1.0, theme::BORDER))
                    .corner_radius(13)
                    .inner_margin(egui::Margin::same(6))
                    .shadow(egui::Shadow {
                        offset: [0, 2],
                        blur: 8,
                        spread: 0,
                        color: Color32::from_black_alpha(60),
                    })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            for t in tools {
                                let icon = match t {
                                    Tool::Move => Icon::Move,
                                    Tool::Frame => Icon::Frame,
                                    Tool::Rectangle => Icon::Rectangle,
                                    Tool::Ellipse => Icon::Ellipse,
                                    Tool::Text => Icon::Text,
                                    Tool::Hand => Icon::Hand,
                                };
                                let r = icons::icon_button(ui, icon, self.tool == t, 32.0)
                                    .on_hover_text(format!("{}  {}", t.label(), t.shortcut()));
                                if r.clicked() {
                                    self.tool = t;
                                }
                            }
                        });
                    });
            });
    }

    fn toast_ui(&mut self, ctx: &egui::Context, canvas_rect: Rect) {
        let Some((msg, at)) = &self.toast else { return };
        if at.elapsed() > Duration::from_secs(4) {
            self.toast = None;
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(250));
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("toast"),
        ));
        let galley = painter.layout_no_wrap(msg.clone(), theme::font(12.0), Color32::WHITE);
        let size = galley.size() + vec2(24.0, 16.0);
        let r = Rect::from_min_size(
            pos2(
                canvas_rect.center().x - size.x / 2.0,
                canvas_rect.bottom() - 120.0,
            ),
            size,
        );
        painter.rect_filled(r, 6.0, Color32::from_rgb(0x1E, 0x1E, 0x1E));
        painter.galley(r.min + vec2(12.0, 8.0), galley, Color32::WHITE);
    }

    fn close_dialog(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && self.editor.dirty && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_close = true;
        }
        if !self.confirm_close {
            return;
        }
        let modal = egui::Modal::new(egui::Id::new("confirm-close")).show(ctx, |ui| {
            ui.set_width(320.0);
            ui.label(
                RichText::new("Save changes before closing?").font(theme::font_semibold(13.0)),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new("Your changes will be lost if you don't save them.")
                    .color(theme::TEXT_SECONDARY),
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Don't save").clicked() {
                    self.allow_close = true;
                    self.confirm_close = false;
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let save = egui::Button::new(RichText::new("Save").color(Color32::WHITE))
                        .fill(theme::BLUE);
                    if ui.add(save).clicked() && self.save() {
                        self.allow_close = true;
                        self.confirm_close = false;
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if ui.button("Cancel").clicked() {
                        self.confirm_close = false;
                    }
                });
            });
        });
        if modal.should_close() {
            self.confirm_close = false;
        }
    }
}

fn hline(ui: &mut Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().line_segment(
        [r.left_center(), r.right_center()],
        Stroke::new(1.0, theme::BORDER),
    );
}

impl eframe::App for DesigmaApp {
    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_shortcuts(&ctx);
        self.sync_text_sizes();
        self.editor.prune_selection();
        if !ctx.input(|i| i.pointer.any_down())
            && !ctx.egui_wants_keyboard_input()
            && !self.canvas.is_interacting()
        {
            self.editor.end_coalescing();
        }

        let title = format!(
            "{}{} – Desigma",
            self.file_name(),
            if self.editor.dirty { " •" } else { "" }
        );
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }

        let panel_frame = egui::Frame::new().fill(theme::PANEL_BG);
        egui::Panel::left("left-panel")
            .exact_size(theme::PANEL_WIDTH)
            .resizable(false)
            .frame(panel_frame.stroke(Stroke::new(1.0, theme::BORDER)))
            .show(ui, |ui| self.left_panel(ui));
        egui::Panel::right("right-panel")
            .exact_size(theme::PANEL_WIDTH)
            .resizable(false)
            .frame(panel_frame.stroke(Stroke::new(1.0, theme::BORDER)))
            .show(ui, |ui| {
                properties::ui(ui, &mut self.editor, &mut self.canvas)
            });
        let canvas_rect = egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                self.canvas
                    .ui(ui, &mut self.editor, &self.fonts, &mut self.tool, frame);
                ui.min_rect()
            })
            .inner;
        self.toolbar(&ctx, canvas_rect);
        self.toast_ui(&ctx, canvas_rect);
        self.close_dialog(&ctx);
    }
}
