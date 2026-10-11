//! The infinite canvas: GPU rendering through vello, viewport navigation,
//! selection, drawing tools, resizing, snapping and on-canvas text editing.

use desigma_core::{Editor, NodeId, NodeKind, Rect as DRect, TextData, Vec2 as DVec2};
use desigma_render::{Fonts, render_page, to_peniko};
use egui::{
    Align2, Color32, CursorIcon, Key, Painter, PointerButton, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui,
    pos2, vec2,
};
use egui_wgpu::wgpu;
use vello::kurbo::Affine;
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};

use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Move,
    Hand,
    Frame,
    Rectangle,
    Ellipse,
    Text,
}

impl Tool {
    pub fn shortcut(self) -> &'static str {
        match self {
            Tool::Move => "V",
            Tool::Hand => "H",
            Tool::Frame => "F",
            Tool::Rectangle => "R",
            Tool::Ellipse => "O",
            Tool::Text => "T",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Tool::Move => "Move",
            Tool::Hand => "Hand tool",
            Tool::Frame => "Frame",
            Tool::Rectangle => "Rectangle",
            Tool::Ellipse => "Ellipse",
            Tool::Text => "Text",
        }
    }
}

/// Maps page space to canvas-local screen points: `screen = pan + page * zoom`.
#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub pan: egui::Vec2,
    pub zoom: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self { pan: egui::Vec2::ZERO, zoom: 1.0 }
    }
}

pub const MIN_ZOOM: f32 = 0.02;
pub const MAX_ZOOM: f32 = 256.0;

impl Viewport {
    pub fn to_screen(&self, origin: Pos2, p: DVec2) -> Pos2 {
        origin + self.pan + vec2(p.x as f32, p.y as f32) * self.zoom
    }

    pub fn to_world(&self, origin: Pos2, p: Pos2) -> DVec2 {
        let v = (p - origin - self.pan) / self.zoom;
        DVec2::new(v.x as f64, v.y as f64)
    }

    pub fn rect_to_screen(&self, origin: Pos2, r: DRect) -> Rect {
        Rect::from_min_max(self.to_screen(origin, r.min()), self.to_screen(origin, r.max()))
    }

    /// Zooms by `factor` keeping the page point under `anchor` (canvas-local) fixed.
    pub fn zoom_around(&mut self, anchor: egui::Vec2, factor: f32) {
        let new_zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let k = new_zoom / self.zoom;
        self.pan = anchor - (anchor - self.pan) * k;
        self.zoom = new_zoom;
    }

    /// Fits `r` into a canvas of `size`, leaving Figma-like padding. Never zooms above 100%
    /// unless `allow_zoom_in`.
    pub fn fit(&mut self, r: DRect, size: egui::Vec2, allow_zoom_in: bool) {
        let pad = 64.0;
        let avail = (size - vec2(pad * 2.0, pad * 2.0)).max(vec2(1.0, 1.0));
        let mut z = (avail.x / r.w.max(1.0) as f32).min(avail.y / r.h.max(1.0) as f32);
        if !allow_zoom_in {
            z = z.min(1.0);
        }
        self.zoom = z.clamp(MIN_ZOOM, MAX_ZOOM);
        let c = r.center();
        self.pan = size / 2.0 - vec2(c.x as f32, c.y as f32) * self.zoom;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Handle {
    N,
    S,
    E,
    W,
    NE,
    NW,
    SE,
    SW,
}

impl Handle {
    fn cursor(self) -> CursorIcon {
        match self {
            Handle::N | Handle::S => CursorIcon::ResizeVertical,
            Handle::E | Handle::W => CursorIcon::ResizeHorizontal,
            Handle::NE | Handle::SW => CursorIcon::ResizeNeSw,
            Handle::NW | Handle::SE => CursorIcon::ResizeNwSe,
        }
    }

    fn is_corner(self) -> bool {
        matches!(self, Handle::NE | Handle::NW | Handle::SE | Handle::SW)
    }

    /// New bounds when this handle is dragged by `d` page units.
    fn apply(self, r: DRect, d: DVec2, keep_aspect: bool) -> DRect {
        let (mut x0, mut y0, mut x1, mut y1) = (r.x, r.y, r.x + r.w, r.y + r.h);
        match self {
            Handle::N => y0 += d.y,
            Handle::S => y1 += d.y,
            Handle::E => x1 += d.x,
            Handle::W => x0 += d.x,
            Handle::NE => {
                x1 += d.x;
                y0 += d.y
            }
            Handle::NW => {
                x0 += d.x;
                y0 += d.y
            }
            Handle::SE => {
                x1 += d.x;
                y1 += d.y
            }
            Handle::SW => {
                x0 += d.x;
                y1 += d.y
            }
        }
        let mut out = DRect::from_points(DVec2::new(x0, y0), DVec2::new(x1, y1));
        if keep_aspect && self.is_corner() && r.w > 0.0 && r.h > 0.0 {
            let s = (out.w / r.w).max(out.h / r.h);
            let (w, h) = (r.w * s, r.h * s);
            // Anchor at the corner opposite the dragged one.
            let ax = if matches!(self, Handle::NW | Handle::SW) { r.x + r.w } else { r.x };
            let ay = if matches!(self, Handle::NW | Handle::NE) { r.y + r.h } else { r.y };
            let fx = if x1 < x0 { -1.0 } else { 1.0 } * if matches!(self, Handle::NW | Handle::SW) { -1.0 } else { 1.0 };
            let fy = if y1 < y0 { -1.0 } else { 1.0 } * if matches!(self, Handle::NW | Handle::NE) { -1.0 } else { 1.0 };
            out = DRect::from_points(DVec2::new(ax, ay), DVec2::new(ax + w * fx, ay + h * fy));
        }
        out
    }
}

/// A snap line drawn while moving, in page space.
#[derive(Debug, Clone, Copy)]
enum Guide {
    Vertical { x: f64, y0: f64, y1: f64 },
    Horizontal { y: f64, x0: f64, x1: f64 },
}

enum Drag {
    None,
    Pan,
    Move {
        start: DVec2,
        applied: DVec2,
        started: bool,
        duplicate: bool,
        orig_bounds: DRect,
        targets: Vec<DRect>,
    },
    Resize {
        handle: Handle,
        start: DVec2,
        from: DRect,
        originals: Vec<(NodeId, DRect)>,
    },
    Create {
        id: NodeId,
        start: DVec2,
        moved: bool,
    },
    Marquee {
        start: DVec2,
        base: Vec<NodeId>,
        current: DVec2,
    },
}

struct GpuTarget {
    texture: wgpu::Texture,
    storage_view: wgpu::TextureView,
    egui_id: egui::TextureId,
    size: [u32; 2],
}

pub struct Canvas {
    pub view: Viewport,
    pub editing_text: Option<NodeId>,
    renderer: Option<Renderer>,
    target: Option<GpuTarget>,
    scene: Scene,
    drag: Drag,
    guides: Vec<Guide>,
    hovered: Option<NodeId>,
    needs_initial_fit: bool,
    pub size: egui::Vec2,
    text_focus_requested: bool,
    pub gpu_error: Option<String>,
}

impl Canvas {
    pub fn new(render_state: Option<&egui_wgpu::RenderState>) -> Self {
        let mut gpu_error = None;
        let renderer = match render_state {
            Some(rs) => match Renderer::new(
                &rs.device,
                RendererOptions {
                    antialiasing_support: AaSupport::area_only(),
                    ..Default::default()
                },
            ) {
                Ok(r) => Some(r),
                Err(e) => {
                    gpu_error = Some(format!("Failed to initialize the GPU renderer: {e}"));
                    None
                }
            },
            None => {
                gpu_error = Some("No wgpu render state; the canvas needs a GPU backend.".into());
                None
            }
        };
        Self {
            view: Viewport::default(),
            editing_text: None,
            renderer,
            target: None,
            scene: Scene::new(),
            drag: Drag::None,
            guides: vec![],
            hovered: None,
            needs_initial_fit: true,
            size: egui::Vec2::ZERO,
            text_focus_requested: false,
            gpu_error,
        }
    }

    pub fn request_fit(&mut self) {
        self.needs_initial_fit = true;
    }

    pub fn zoom_to_fit(&mut self, editor: &Editor) {
        let ids: Vec<NodeId> = editor.page().children.iter().filter(|n| n.visible).map(|n| n.id).collect();
        match editor.page().selection_bounds(&ids) {
            Some(b) => self.view.fit(b, self.size, true),
            None => {
                self.view.zoom = 1.0;
                self.view.pan = self.size / 2.0;
            }
        }
    }

    pub fn zoom_to_selection(&mut self, editor: &Editor) {
        if let Some(b) = editor.selection_bounds() {
            self.view.fit(b, self.size, true);
        }
    }

    pub fn zoom_by(&mut self, factor: f32) {
        self.view.zoom_around(self.size / 2.0, factor);
    }

    pub fn set_zoom(&mut self, zoom: f32) {
        self.view.zoom_around(self.size / 2.0, zoom / self.view.zoom);
    }

    pub fn is_interacting(&self) -> bool {
        !matches!(self.drag, Drag::None)
    }

    // ------------------------------------------------------------------

    pub fn ui(&mut self, ui: &mut Ui, editor: &mut Editor, fonts: &Fonts, tool: &mut Tool, frame: &eframe::Frame) {
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        self.size = rect.size();
        let origin = rect.min;

        if self.needs_initial_fit && rect.width() > 1.0 {
            self.needs_initial_fit = false;
            let ids: Vec<NodeId> = editor.page().children.iter().map(|n| n.id).collect();
            match editor.page().selection_bounds(&ids) {
                Some(b) => self.view.fit(b, self.size, false),
                None => {
                    self.view.zoom = 1.0;
                    self.view.pan = self.size / 2.0 - vec2(400.0, 300.0);
                }
            }
        }

        if self.editing_text.is_some_and(|id| editor.page().find(id).is_none()) {
            self.editing_text = None;
        }

        self.navigate(ui, &resp, rect);
        self.interact(ui, &resp, rect, editor, tool);

        // ---- render ---------------------------------------------------
        let painter = ui.painter_at(rect);
        let bg = editor.page().background;
        self.paint_scene(ui, &painter, rect, editor, fonts, frame, bg);
        self.paint_overlays(&painter, origin, editor, *tool);
        self.text_editor(ui, rect, editor, fonts);
    }

    /// Scroll to pan, Ctrl/Cmd+scroll or pinch to zoom, middle-drag / Space+drag / Hand tool to pan.
    fn navigate(&mut self, ui: &Ui, resp: &egui::Response, rect: Rect) {
        let (scroll, zoom, pointer) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta(), i.pointer.hover_pos()));
        if resp.contains_pointer() {
            if zoom != 1.0 {
                if let Some(p) = pointer {
                    self.view.zoom_around(p - rect.min, zoom);
                }
            } else if scroll != egui::Vec2::ZERO {
                self.view.pan += scroll;
            }
        }
        if let Drag::Pan = self.drag {
            let delta = ui.input(|i| i.pointer.delta());
            self.view.pan += delta;
        }
    }

    fn selection_handles(&self, editor: &Editor, origin: Pos2) -> Vec<(Handle, Rect)> {
        let Some(b) = editor.selection_bounds() else { return vec![] };
        let r = self.view.rect_to_screen(origin, b);
        let hs = 8.0;
        let edge = 6.0;
        let mut out = vec![
            (Handle::NW, Rect::from_center_size(r.left_top(), vec2(hs, hs))),
            (Handle::NE, Rect::from_center_size(r.right_top(), vec2(hs, hs))),
            (Handle::SW, Rect::from_center_size(r.left_bottom(), vec2(hs, hs))),
            (Handle::SE, Rect::from_center_size(r.right_bottom(), vec2(hs, hs))),
        ];
        out.push((Handle::N, Rect::from_min_max(pos2(r.left(), r.top() - edge / 2.0), pos2(r.right(), r.top() + edge / 2.0))));
        out.push((Handle::S, Rect::from_min_max(pos2(r.left(), r.bottom() - edge / 2.0), pos2(r.right(), r.bottom() + edge / 2.0))));
        out.push((Handle::W, Rect::from_min_max(pos2(r.left() - edge / 2.0, r.top()), pos2(r.left() + edge / 2.0, r.bottom()))));
        out.push((Handle::E, Rect::from_min_max(pos2(r.right() - edge / 2.0, r.top()), pos2(r.right() + edge / 2.0, r.bottom()))));
        out
    }

    /// Screen rectangles of top-level frame name labels.
    fn frame_labels(&self, editor: &Editor, origin: Pos2) -> Vec<(NodeId, Rect, String)> {
        let mut out = vec![];
        for n in &editor.page().children {
            if !n.visible || !n.is_frame() {
                continue;
            }
            let r = self.view.rect_to_screen(origin, n.rect());
            let width = r.width().max(24.0);
            let label = Rect::from_min_size(pos2(r.left(), r.top() - 20.0), vec2(width, 18.0));
            out.push((n.id, label, n.name.clone()));
        }
        out
    }

    fn interact(&mut self, ui: &mut Ui, resp: &egui::Response, rect: Rect, editor: &mut Editor, tool: &mut Tool) {
        let origin = rect.min;
        let input = ui.input(|i| i.clone());
        let pointer = input.pointer.interact_pos().or(input.pointer.hover_pos());
        let world = pointer.map(|p| self.view.to_world(origin, p));
        let mods = input.modifiers;
        let space = input.key_down(Key::Space) && !ui.ctx().egui_wants_keyboard_input();

        // Start of a pan gesture.
        if matches!(self.drag, Drag::None) && resp.hovered() {
            let middle = input.pointer.button_pressed(PointerButton::Middle);
            let primary = input.pointer.primary_pressed();
            if middle || (primary && (space || *tool == Tool::Hand)) {
                self.drag = Drag::Pan;
            }
        }

        let primary_pressed = input.pointer.primary_pressed() && resp.hovered() && matches!(self.drag, Drag::None);
        let released = input.pointer.any_released();

        // Clicking outside the text being edited commits it.
        if primary_pressed && self.editing_text.is_some() {
            if let (Some(id), Some(p)) = (self.editing_text, pointer) {
                let inside = editor
                    .page()
                    .world_rect(id)
                    .map(|r| self.view.rect_to_screen(origin, r).expand(4.0).contains(p))
                    .unwrap_or(false);
                if inside {
                    return;
                }
                self.finish_text_edit(editor);
            }
        }

        if primary_pressed && !space {
            if let (Some(w), Some(p)) = (world, pointer) {
                self.on_press(ui, editor, tool, w, p, origin, mods, resp.double_clicked());
            }
        }

        // Double click on a text layer edits it, on a group enters it.
        if resp.double_clicked() && *tool == Tool::Move {
            if let Some(w) = world {
                if let Some(id) = editor.page().hit_test_deep(w) {
                    let node = editor.page().find(id);
                    if node.is_some_and(|n| n.text().is_some()) {
                        editor.select_only(id);
                        self.start_text_edit(id);
                    } else if editor.selection.len() == 1 {
                        // Select the next level down under the cursor.
                        let sel = editor.selection[0];
                        if let Some(path) = editor.page().path_to(id) {
                            if let Some(pos) = path.iter().position(|x| *x == sel) {
                                if let Some(next) = path.get(pos + 1) {
                                    editor.select_only(*next);
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(w) = world {
            self.on_drag(editor, w, mods);
        }

        if released && !matches!(self.drag, Drag::None) {
            self.on_release(editor, tool);
        }

        // Hover + cursor.
        self.hovered = None;
        if matches!(self.drag, Drag::None) && resp.hovered() && *tool == Tool::Move {
            if let (Some(w), Some(p)) = (world, pointer) {
                let handle = self.selection_handles(editor, origin).into_iter().find(|(_, r)| r.contains(p));
                if let Some((h, _)) = handle {
                    ui.ctx().set_cursor_icon(h.cursor());
                } else if let Some((id, _, _)) = self.frame_labels(editor, origin).into_iter().find(|(_, r, _)| r.contains(p)) {
                    self.hovered = Some(id);
                } else {
                    self.hovered = if mods.command { editor.page().hit_test_deep(w) } else { editor.page().hit_test(w) };
                }
            }
        }
        if resp.hovered() || self.is_interacting() {
            let icon = match (&self.drag, *tool) {
                (Drag::Pan, _) => Some(CursorIcon::Grabbing),
                (Drag::Resize { handle, .. }, _) => Some(handle.cursor()),
                (_, Tool::Hand) => Some(CursorIcon::Grab),
                (_, Tool::Frame | Tool::Rectangle | Tool::Ellipse) => Some(CursorIcon::Crosshair),
                (_, Tool::Text) => Some(CursorIcon::Text),
                _ if space => Some(CursorIcon::Grab),
                _ => None,
            };
            if let Some(icon) = icon {
                ui.ctx().set_cursor_icon(icon);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn on_press(
        &mut self,
        ui: &Ui,
        editor: &mut Editor,
        tool: &mut Tool,
        w: DVec2,
        p: Pos2,
        origin: Pos2,
        mods: egui::Modifiers,
        _double: bool,
    ) {
        match *tool {
            Tool::Hand => {}
            Tool::Frame | Tool::Rectangle | Tool::Ellipse => {
                let kind = match *tool {
                    Tool::Frame => NodeKind::Frame { clip_content: true },
                    Tool::Rectangle => NodeKind::Rectangle,
                    _ => NodeKind::Ellipse,
                };
                let start = round(w);
                let id = editor.create_node(kind, DRect::new(start.x, start.y, 0.01, 0.01));
                self.drag = Drag::Create { id, start, moved: false };
            }
            Tool::Text => {
                let start = round(w);
                let id = editor.create_node(NodeKind::Text(TextData::new("")), DRect::new(start.x, start.y, 1.0, 14.0));
                self.start_text_edit(id);
                *tool = Tool::Move;
            }
            Tool::Move => {
                if let Some((handle, _)) = self.selection_handles(editor, origin).into_iter().find(|(_, r)| r.contains(p)) {
                    let roots = editor.selection_roots();
                    let originals = roots
                        .iter()
                        .filter_map(|id| Some((*id, editor.page().world_rect(*id)?)))
                        .collect();
                    if let Some(from) = editor.selection_bounds() {
                        editor.checkpoint();
                        self.drag = Drag::Resize { handle, start: w, from, originals };
                    }
                    return;
                }
                let label_hit = self.frame_labels(editor, origin).into_iter().find(|(_, r, _)| r.contains(p)).map(|x| x.0);
                let hit = label_hit.or_else(|| {
                    if mods.command { editor.page().hit_test_deep(w) } else { editor.page().hit_test(w) }
                });
                // Keep a child selection when clicking inside an already-selected ancestor chain.
                let hit = hit.map(|h| {
                    editor
                        .page()
                        .path_to(h)
                        .and_then(|path| path.into_iter().find(|a| editor.is_selected(*a)))
                        .unwrap_or(h)
                });
                match hit {
                    Some(id) => {
                        if mods.shift {
                            editor.toggle_selected(id);
                        } else if !editor.is_selected(id) {
                            editor.select_only(id);
                        }
                        self.begin_move(editor, w, mods.alt);
                    }
                    None => {
                        let inside_selection = editor
                            .selection
                            .iter()
                            .filter_map(|id| editor.page().world_rect(*id))
                            .any(|r| r.contains(w));
                        if inside_selection && !mods.shift {
                            self.begin_move(editor, w, mods.alt);
                        } else {
                            let base = if mods.shift { editor.selection.clone() } else { vec![] };
                            if !mods.shift {
                                editor.clear_selection();
                            }
                            self.drag = Drag::Marquee { start: w, base, current: w };
                        }
                    }
                }
                let _ = ui;
            }
        }
    }

    fn begin_move(&mut self, editor: &Editor, w: DVec2, duplicate: bool) {
        let roots = editor.selection_roots();
        let Some(orig_bounds) = editor.page().selection_bounds(&roots) else { return };
        // Snap targets: every visible layer outside the moving set.
        let page = editor.page();
        let mut targets = vec![];
        for id in page.all_ids() {
            let Some(path) = page.path_to(id) else { continue };
            if path.iter().any(|a| roots.contains(a)) {
                continue;
            }
            if page.find(id).is_some_and(|n| n.visible) {
                if let Some(r) = page.world_rect(id) {
                    targets.push(r);
                }
            }
        }
        self.drag = Drag::Move {
            start: w,
            applied: DVec2::ZERO,
            started: false,
            duplicate,
            orig_bounds,
            targets,
        };
    }

    fn on_drag(&mut self, editor: &mut Editor, w: DVec2, mods: egui::Modifiers) {
        let zoom = self.view.zoom as f64;
        match &mut self.drag {
            Drag::Move { start, applied, started, duplicate, orig_bounds, targets } => {
                let mut total = w - *start;
                if !*started && (total.x.abs() * zoom < 3.0 && total.y.abs() * zoom < 3.0) {
                    return;
                }
                if mods.shift {
                    if total.x.abs() > total.y.abs() {
                        total.y = 0.0;
                    } else {
                        total.x = 0.0;
                    }
                }
                let (snapped, guides) = snap(*orig_bounds, round(total), targets, 6.0 / zoom, mods.shift);
                self.guides = guides;
                let diff = snapped - *applied;
                if diff.x == 0.0 && diff.y == 0.0 {
                    return;
                }
                if !*started {
                    *started = true;
                    if *duplicate {
                        editor.duplicate_selection(DVec2::ZERO);
                    } else {
                        editor.checkpoint();
                    }
                }
                editor.translate_selection(diff);
                *applied = snapped;
            }
            Drag::Resize { handle, start, from, originals } => {
                let d = round(w - *start);
                let to = handle.apply(*from, d, mods.shift);
                let to = DRect::new(to.x.round(), to.y.round(), to.w.round().max(1.0), to.h.round().max(1.0));
                editor.resize_selection(originals, *from, to);
            }
            Drag::Create { id, start, moved } => {
                let mut cur = round(w);
                if mods.shift {
                    let d = cur - *start;
                    let s = d.x.abs().max(d.y.abs());
                    cur = *start + DVec2::new(s * d.x.signum(), s * d.y.signum());
                }
                let r = DRect::from_points(*start, cur);
                if r.w >= 1.0 || r.h >= 1.0 {
                    *moved = true;
                    editor.set_world_rect(*id, r);
                }
            }
            Drag::Marquee { start, base, current } => {
                *current = w;
                let area = DRect::from_points(*start, w);
                let mut sel = base.clone();
                for id in editor.page().nodes_in_rect(area) {
                    if !sel.contains(&id) {
                        sel.push(id);
                    }
                }
                editor.selection = sel;
            }
            Drag::Pan | Drag::None => {}
        }
    }

    fn on_release(&mut self, editor: &mut Editor, tool: &mut Tool) {
        match std::mem::replace(&mut self.drag, Drag::None) {
            Drag::Move { started: true, .. } => {
                let roots = editor.selection_roots();
                editor.drop_into_frames(&roots);
            }
            Drag::Create { id, start, moved } => {
                if !moved {
                    editor.set_world_rect(id, DRect::new(start.x, start.y, 100.0, 100.0));
                }
                *tool = Tool::Move;
            }
            _ => {}
        }
        self.guides.clear();
    }

    pub fn start_text_edit(&mut self, id: NodeId) {
        self.editing_text = Some(id);
        self.text_focus_requested = false;
    }

    pub fn finish_text_edit(&mut self, editor: &mut Editor) {
        if let Some(id) = self.editing_text.take() {
            let empty = editor
                .page()
                .find(id)
                .and_then(|n| n.text())
                .is_some_and(|t| t.content.trim().is_empty());
            if empty {
                editor.select_only(id);
                editor.delete_selection();
            } else {
                editor.select_only(id);
            }
            editor.end_coalescing();
        }
    }

    // ---- painting -----------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    fn paint_scene(
        &mut self,
        ui: &Ui,
        painter: &Painter,
        rect: Rect,
        editor: &Editor,
        fonts: &Fonts,
        frame: &eframe::Frame,
        bg: desigma_core::Color,
    ) {
        painter.rect_filled(rect, 0.0, theme::to_color32(bg));
        let (Some(rs), Some(renderer)) = (frame.wgpu_render_state(), self.renderer.as_mut()) else {
            if let Some(err) = &self.gpu_error {
                painter.text(rect.center(), Align2::CENTER_CENTER, err, theme::font(13.0), theme::TEXT_SECONDARY);
            }
            return;
        };
        let ppp = ui.ctx().pixels_per_point();
        let w = (rect.width() * ppp).round().max(1.0) as u32;
        let h = (rect.height() * ppp).round().max(1.0) as u32;
        let max = rs.device.limits().max_texture_dimension_2d;
        let (w, h) = (w.min(max), h.min(max));

        self.scene.reset();
        let xf = Affine::scale(ppp as f64)
            * Affine::translate((self.view.pan.x as f64, self.view.pan.y as f64))
            * Affine::scale(self.view.zoom as f64);
        match self.editing_text {
            Some(id) => {
                let mut page = editor.page().clone();
                if let Some(n) = page.find_mut(id) {
                    n.visible = false;
                }
                render_page(&mut self.scene, &page, xf, fonts);
            }
            None => render_page(&mut self.scene, editor.page(), xf, fonts),
        }

        if self.target.as_ref().is_none_or(|t| t.size != [w, h]) {
            let texture = rs.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("desigma-canvas"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
            });
            let storage_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            // vello writes sRGB-encoded values; sampling through an sRGB view hands egui linear color.
            let sample_view = texture.create_view(&wgpu::TextureViewDescriptor {
                format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
                ..Default::default()
            });
            let mut egui_renderer = rs.renderer.write();
            let egui_id = match self.target.take() {
                Some(old) => {
                    egui_renderer.update_egui_texture_from_wgpu_texture(
                        &rs.device,
                        &sample_view,
                        wgpu::FilterMode::Nearest,
                        old.egui_id,
                    );
                    old.texture.destroy();
                    old.egui_id
                }
                None => egui_renderer.register_native_texture(&rs.device, &sample_view, wgpu::FilterMode::Nearest),
            };
            self.target = Some(GpuTarget { texture, storage_view, egui_id, size: [w, h] });
        }
        let target = self.target.as_ref().expect("target created above");
        let params = RenderParams {
            base_color: to_peniko(bg),
            width: w,
            height: h,
            antialiasing_method: AaConfig::Area,
        };
        if let Err(e) = renderer.render_to_texture(&rs.device, &rs.queue, &self.scene, &target.storage_view, &params) {
            log::error!("vello render failed: {e}");
            return;
        }
        painter.image(
            target.egui_id,
            rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }

    fn paint_overlays(&self, painter: &Painter, origin: Pos2, editor: &Editor, tool: Tool) {
        let page = editor.page();
        let blue = theme::BLUE;

        // Frame name labels.
        for (id, r, name) in self.frame_labels(editor, origin) {
            let active = editor.is_selected(id) || self.hovered == Some(id);
            let color = if active { blue } else { theme::FRAME_LABEL };
            let galley = painter.layout_no_wrap(name, theme::font(11.0), color);
            let clip = painter.with_clip_rect(r.intersect(painter.clip_rect()));
            clip.galley(pos2(r.left(), r.bottom() - galley.size().y - 2.0), galley, color);
        }

        // Hover outline.
        if let Some(id) = self.hovered {
            if !editor.is_selected(id) {
                if let Some(r) = page.world_rect(id) {
                    let sr = self.view.rect_to_screen(origin, r);
                    if page.find(id).is_some_and(|n| matches!(n.kind, NodeKind::Ellipse)) {
                        painter.add(Shape::ellipse_stroke(sr.center(), sr.size() / 2.0, Stroke::new(2.0, blue)));
                    } else {
                        painter.rect_stroke(sr, 0.0, Stroke::new(2.0, blue), StrokeKind::Middle);
                    }
                }
            }
        }

        // Selection.
        if self.editing_text.is_none() {
            for id in &editor.selection {
                if let Some(r) = page.world_rect(*id) {
                    painter.rect_stroke(self.view.rect_to_screen(origin, r), 0.0, Stroke::new(1.0, blue), StrokeKind::Middle);
                }
            }
            if let Some(b) = editor.selection_bounds() {
                let sr = self.view.rect_to_screen(origin, b);
                painter.rect_stroke(sr, 0.0, Stroke::new(1.0, blue), StrokeKind::Middle);
                if tool == Tool::Move && !matches!(self.drag, Drag::Move { started: true, .. }) {
                    for (h, r) in self.selection_handles(editor, origin) {
                        if h.is_corner() {
                            painter.rect_filled(r.shrink(0.5), 1.0, Color32::WHITE);
                            painter.rect_stroke(r.shrink(0.5), 1.0, Stroke::new(1.0, blue), StrokeKind::Middle);
                        }
                    }
                }
                // Size badge.
                let text = format!("{} × {}", fmt_num(b.w), fmt_num(b.h));
                let galley = painter.layout_no_wrap(text, theme::font_medium(11.0), Color32::WHITE);
                let size = galley.size() + vec2(8.0, 4.0);
                let badge = Rect::from_min_size(pos2(sr.center().x - size.x / 2.0, sr.bottom() + 6.0), size);
                painter.rect_filled(badge, 2.0, blue);
                painter.galley(badge.min + vec2(4.0, 2.0), galley, Color32::WHITE);
            }
        } else if let Some(id) = self.editing_text {
            if let Some(r) = page.world_rect(id) {
                painter.rect_stroke(self.view.rect_to_screen(origin, r), 0.0, Stroke::new(1.0, blue), StrokeKind::Middle);
            }
        }

        // Snap guides, in Figma's red.
        let guide = Stroke::new(1.0, Color32::from_rgb(0xF2, 0x48, 0x22));
        for g in &self.guides {
            match *g {
                Guide::Vertical { x, y0, y1 } => {
                    let a = self.view.to_screen(origin, DVec2::new(x, y0));
                    let b = self.view.to_screen(origin, DVec2::new(x, y1));
                    painter.line_segment([a, b], guide);
                }
                Guide::Horizontal { y, x0, x1 } => {
                    let a = self.view.to_screen(origin, DVec2::new(x0, y));
                    let b = self.view.to_screen(origin, DVec2::new(x1, y));
                    painter.line_segment([a, b], guide);
                }
            }
        }

        // Marquee.
        if let Drag::Marquee { start, current, .. } = &self.drag {
            let r = self.view.rect_to_screen(origin, DRect::from_points(*start, *current));
            painter.rect_filled(r, 0.0, blue.gamma_multiply(0.1));
            painter.rect_stroke(r, 0.0, Stroke::new(1.0, blue), StrokeKind::Middle);
        }
    }

    /// On-canvas text editing with an egui text field drawn in the same font.
    fn text_editor(&mut self, ui: &mut Ui, rect: Rect, editor: &mut Editor, _fonts: &Fonts) {
        let Some(id) = self.editing_text else { return };
        let Some(node) = editor.page().find(id) else { return };
        let Some(text) = node.text().cloned() else { return };
        let Some(world) = editor.page().world_rect(id) else { return };
        let color = node
            .fills
            .iter()
            .find(|p| p.is_visible())
            .map(|desigma_core::Paint::Solid { color, .. }| theme::to_color32(*color))
            .unwrap_or(Color32::BLACK);
        let zoom = self.view.zoom;
        let sr = self.view.rect_to_screen(rect.min, world);
        let font = egui::FontId::new(text.font_size as f32 * zoom, theme::family_for_weight(text.font_weight));
        let mut content = text.content.clone();
        let width = if text.auto_width { f32::INFINITY } else { sr.width() };
        let align = match text.align {
            desigma_core::TextAlign::Left => egui::Align::Min,
            desigma_core::TextAlign::Center => egui::Align::Center,
            desigma_core::TextAlign::Right => egui::Align::Max,
        };
        let edit = egui::TextEdit::multiline(&mut content)
            .id_salt(("canvas-text", id.0))
            .font(font)
            .text_color(color)
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::ZERO)
            .desired_rows(1)
            .desired_width(width)
            .horizontal_align(align);
        let area = Rect::from_min_size(sr.min, vec2(sr.width().max(4.0), sr.height().max(4.0)));
        let put_rect = if text.auto_width { Rect::from_min_size(area.min, vec2(rect.right() - area.left() + 4000.0, area.height())) } else { area };
        let resp = ui
            .scope_builder(egui::UiBuilder::new().max_rect(put_rect), |ui| {
                ui.set_clip_rect(rect);
                ui.put(put_rect, edit)
            })
            .inner;
        if !self.text_focus_requested {
            resp.request_focus();
            self.text_focus_requested = true;
        }
        if resp.changed() {
            editor.edit_node(id, &format!("text:{}", id.0), |n| {
                let first_line: String = content.lines().next().unwrap_or("").trim().chars().take(40).collect();
                let old_first: String = n.text().map(|t| t.content.lines().next().unwrap_or("").trim().chars().take(40).collect()).unwrap_or_default();
                if n.name == old_first || n.name == "Text" {
                    n.name = if first_line.is_empty() { "Text".into() } else { first_line };
                }
                if let Some(t) = n.text_mut() {
                    t.content = content.clone();
                }
            });
        }
        if resp.lost_focus() || ui.input(|i| i.key_pressed(Key::Escape)) {
            self.finish_text_edit(editor);
        }
    }
}

fn round(v: DVec2) -> DVec2 {
    DVec2::new(v.x.round(), v.y.round())
}

pub fn fmt_num(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if (r - r.round()).abs() < 1e-9 {
        format!("{}", r.round() as i64)
    } else {
        format!("{r}")
    }
}

/// Snaps a moved bounding box's edges and center to nearby layer edges and
/// centers. Returns the adjusted delta and the guides to draw.
fn snap(orig: DRect, delta: DVec2, targets: &[DRect], threshold: f64, axis_locked: bool) -> (DVec2, Vec<Guide>) {
    let moved = orig.translate(delta);
    let xs = [moved.x, moved.x + moved.w / 2.0, moved.x + moved.w];
    let ys = [moved.y, moved.y + moved.h / 2.0, moved.y + moved.h];
    let mut best_x: Option<(f64, f64)> = None; // (adjust, line)
    let mut best_y: Option<(f64, f64)> = None;
    for t in targets {
        let txs = [t.x, t.x + t.w / 2.0, t.x + t.w];
        let tys = [t.y, t.y + t.h / 2.0, t.y + t.h];
        for &a in &xs {
            for &b in &txs {
                let d = b - a;
                if d.abs() <= threshold && best_x.is_none_or(|(bd, _)| d.abs() < bd.abs()) {
                    best_x = Some((d, b));
                }
            }
        }
        for &a in &ys {
            for &b in &tys {
                let d = b - a;
                if d.abs() <= threshold && best_y.is_none_or(|(bd, _)| d.abs() < bd.abs()) {
                    best_y = Some((d, b));
                }
            }
        }
    }
    let mut out = delta;
    if axis_locked {
        if delta.x == 0.0 {
            best_x = None;
        }
        if delta.y == 0.0 {
            best_y = None;
        }
    }
    if let Some((d, _)) = best_x {
        out.x += d;
    }
    if let Some((d, _)) = best_y {
        out.y += d;
    }
    let snapped = orig.translate(out);
    let mut guides = vec![];
    if let Some((_, line)) = best_x {
        let (mut y0, mut y1) = (snapped.y, snapped.y + snapped.h);
        for t in targets {
            if [t.x, t.x + t.w / 2.0, t.x + t.w].iter().any(|v| (v - line).abs() < 1e-6) {
                y0 = y0.min(t.y);
                y1 = y1.max(t.y + t.h);
            }
        }
        guides.push(Guide::Vertical { x: line, y0, y1 });
    }
    if let Some((_, line)) = best_y {
        let (mut x0, mut x1) = (snapped.x, snapped.x + snapped.w);
        for t in targets {
            if [t.y, t.y + t.h / 2.0, t.y + t.h].iter().any(|v| (v - line).abs() < 1e-6) {
                x0 = x0.min(t.x);
                x1 = x1.max(t.x + t.w);
            }
        }
        guides.push(Guide::Horizontal { y: line, x0, x1 });
    }
    (out, guides)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_roundtrip() {
        let v = Viewport { pan: vec2(10.0, 20.0), zoom: 2.0 };
        let o = pos2(5.0, 5.0);
        let p = DVec2::new(3.0, 4.0);
        let s = v.to_screen(o, p);
        assert_eq!(s, pos2(21.0, 33.0));
        let back = v.to_world(o, s);
        assert!((back.x - 3.0).abs() < 1e-6 && (back.y - 4.0).abs() < 1e-6);
    }

    #[test]
    fn zoom_keeps_anchor_fixed() {
        let mut v = Viewport { pan: vec2(0.0, 0.0), zoom: 1.0 };
        let anchor = vec2(100.0, 100.0);
        let before = v.to_world(Pos2::ZERO, anchor.to_pos2());
        v.zoom_around(anchor, 2.0);
        let after = v.to_world(Pos2::ZERO, anchor.to_pos2());
        assert!((before.x - after.x).abs() < 1e-4);
    }

    #[test]
    fn handle_resize_and_flip() {
        let r = DRect::new(0.0, 0.0, 100.0, 50.0);
        assert_eq!(Handle::SE.apply(r, DVec2::new(10.0, 10.0), false), DRect::new(0.0, 0.0, 110.0, 60.0));
        assert_eq!(Handle::W.apply(r, DVec2::new(150.0, 0.0), false), DRect::new(100.0, 0.0, 50.0, 50.0));
        let a = Handle::SE.apply(r, DVec2::new(100.0, 0.0), true);
        assert_eq!((a.w, a.h), (200.0, 100.0));
    }

    #[test]
    fn snapping_aligns_edges() {
        let orig = DRect::new(0.0, 0.0, 10.0, 10.0);
        let targets = [DRect::new(100.0, 50.0, 20.0, 20.0)];
        let (d, guides) = snap(orig, DVec2::new(87.0, 0.0), &targets, 5.0, false);
        assert_eq!(d.x, 90.0); // right edge meets the target's left edge
        assert_eq!(guides.len(), 1);
    }

    #[test]
    fn number_format() {
        assert_eq!(fmt_num(100.0), "100");
        assert_eq!(fmt_num(12.345), "12.35");
    }
}
