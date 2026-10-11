//! The canvas: renders the page into a texture, handles viewport gestures,
//! tools and interactions, and paints the selection overlays.

pub mod create;
pub mod draw;
pub mod handles;
pub mod hit;
pub mod interact;
pub mod reorder;
pub mod snap;
pub mod tools;
pub mod vector;

use crate::app::App;
use crate::edit::Tx;
use desigma_doc::{Affine, NodeId, Rect, Vec2 as DV};
use desigma_render::{tiny_skia, Viewport};
use egui::{Context, Rect as ERect, Sense, TextureHandle, TextureOptions, Ui};
use handles::Handle;

pub struct Pending {
    pub node: NodeId,
    pub start_screen: egui::Pos2,
    pub start_page: DV,
    pub shift: bool,
    pub was_selected: bool,
}

pub struct MoveState {
    pub tx: Tx,
    pub ids: Vec<NodeId>,
    pub start: DV,
    pub orig_bbox: Rect,
    pub targets: Vec<Rect>,
    pub moved: bool,
    /// Auto-layout reorder target while dragging a child inside its frame.
    pub reorder: Option<reorder::Insertion>,
}

pub enum ResizeKind {
    Single { id: NodeId, world0: Affine, t0: Affine, size0: DV },
    Multi { bbox0: Rect, nodes: Vec<(NodeId, Rect, Affine, DV)> },
}

pub struct ResizeState {
    pub tx: Tx,
    pub handle: Handle,
    pub kind: ResizeKind,
    pub targets: Vec<Rect>,
}

pub struct RotateState {
    pub tx: Tx,
    pub id: NodeId,
    pub center: DV,
    pub start_angle: f32,
    pub t0: Affine,
    pub size: DV,
    pub rot0: f32,
    /// Multi-selection: every rotated node with its world transform at drag start.
    pub group: Vec<(NodeId, Affine)>,
}

pub struct DrawState {
    pub tx: Tx,
    pub tool: tools::Tool,
    pub id: NodeId,
    pub parent: NodeId,
    pub start: DV,
    pub targets: Vec<Rect>,
    pub dragged: bool,
}

#[derive(Default)]
pub enum Interaction {
    #[default]
    None,
    Pan,
    Pending(Pending),
    Marquee { start: DV, cur: DV, base: Vec<NodeId> },
    Move(MoveState),
    Resize(ResizeState),
    Rotate(RotateState),
    Draw(DrawState),
    Pencil(Vec<DV>),
    PenDrag,
    /// Dragging a vertex/handle in vector edit mode (state lives in `CanvasState::vedit`).
    VectorDrag,
}

pub struct PenState {
    pub pts: Vec<create::PenPoint>,
}

pub struct TextSession {
    pub id: NodeId,
    pub caret: usize,
    pub tx: Tx,
    pub created: bool,
}

#[derive(PartialEq, Clone, Copy)]
struct RenderKey {
    w: u32,
    h: u32,
    ox: u32,
    oy: u32,
    zoom: u32,
    dpr: u32,
    page: NodeId,
}

#[derive(Default)]
pub struct CanvasState {
    pub dirty: bool,
    tex: Option<TextureHandle>,
    pixmap: Option<tiny_skia::Pixmap>,
    last_key: Option<RenderKey>,
    pub inter: Interaction,
    pub guides: Vec<snap::Guide>,
    pub measures: Vec<snap::Measure>,
    pub pen: Option<PenState>,
    pub text: Option<TextSession>,
    /// Point editing of a Vector node.
    pub vedit: Option<vector::VectorEdit>,
    pub pointer_page: Option<DV>,
    pub cursor: Option<egui::CursorIcon>,
    pub last_render_ms: f32,
}

impl CanvasState {
    /// Drops all in-flight state without touching any document (used when the
    /// document itself is being replaced).
    pub fn reset_interaction(&mut self) {
        self.inter = Interaction::None;
        self.guides.clear();
        self.measures.clear();
        self.pen = None;
        self.text = None;
        self.vedit = None;
    }
}

impl App {
    /// Cancels the running interaction, restoring the document.
    pub fn cancel_interaction(&mut self) {
        let inter = std::mem::take(&mut self.canvas.inter);
        match inter {
            Interaction::Move(m) => m.tx.cancel(&mut self.doc),
            Interaction::Resize(r) => r.tx.cancel(&mut self.doc),
            Interaction::Rotate(r) => r.tx.cancel(&mut self.doc),
            Interaction::Draw(d) => d.tx.cancel(&mut self.doc),
            Interaction::VectorDrag => {
                if let Some(d) = self.canvas.vedit.as_mut().and_then(|v| v.drag.take()) {
                    d.tx.cancel(&mut self.doc);
                }
                vector::reload(self);
            }
            _ => {}
        }
        self.canvas.guides.clear();
        self.canvas.measures.clear();
        self.canvas.pen = None;
        if let Some(t) = self.canvas.text.take() {
            t.tx.cancel(&mut self.doc);
        }
        self.changed();
        self.sanitize();
    }

    /// Switches tool, finishing any pen path or text edit in progress.
    pub fn set_tool(&mut self, tool: tools::Tool) {
        interact::finish_pen(self);
        interact::finish_text(self, true);
        self.cancel_interaction();
        self.tool = tool;
    }
}

fn render(app: &mut App, ctx: &Context, rect: ERect) {
    let ppp = ctx.pixels_per_point();
    let w = (rect.width() * ppp).round().max(1.0) as u32;
    let h = (rect.height() * ppp).round().max(1.0) as u32;
    let key = RenderKey {
        w,
        h,
        ox: app.view.offset.x.to_bits(),
        oy: app.view.offset.y.to_bits(),
        zoom: app.view.zoom.to_bits(),
        dpr: ppp.to_bits(),
        page: app.page,
    };
    let c = &mut app.canvas;
    if !c.dirty && c.last_key == Some(key) && c.tex.is_some() {
        return;
    }
    let start = std::time::Instant::now();
    if c.pixmap.as_ref().map(|p| (p.width(), p.height())) != Some((w, h)) {
        c.pixmap = tiny_skia::Pixmap::new(w, h);
    }
    let Some(pm) = c.pixmap.as_mut() else { return };
    let vp = Viewport { offset: app.view.offset, zoom: app.view.zoom, dpr: ppp };
    desigma_render::render_page(&app.doc, app.page, &vp, pm);
    let img = egui::ColorImage::from_rgba_premultiplied([w as usize, h as usize], pm.data());
    match c.tex.as_mut() {
        Some(t) => t.set(img, TextureOptions::NEAREST),
        None => c.tex = Some(ctx.load_texture("canvas", img, TextureOptions::NEAREST)),
    }
    c.last_key = Some(key);
    c.dirty = false;
    c.last_render_ms = start.elapsed().as_secs_f32() * 1000.0;
}

pub fn show(app: &mut App, ui: &mut Ui, rect: ERect) {
    app.canvas_rect = rect;
    if !app.first_fit_done {
        app.first_fit_done = true;
        app.fit_all();
    }
    render(app, ui.ctx(), rect);
    if let Some(tex) = &app.canvas.tex {
        ui.painter().with_clip_rect(rect).image(tex.id(), rect, ERect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
    }
    let resp = ui.interact(rect, ui.id().with("canvas"), Sense::click_and_drag());
    interact::process(app, ui, rect, &resp);
    draw::overlays(app, ui, rect);
    if resp.hovered() || !matches!(app.canvas.inter, Interaction::None) {
        if let Some(c) = app.canvas.cursor {
            ui.ctx().set_cursor_icon(c);
        }
    }
}
