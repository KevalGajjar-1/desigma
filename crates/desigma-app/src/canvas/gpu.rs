//! Canvas rendering on the GPU (`desigma-gpu`): the visible page is compiled
//! to a frame while the UI is painted and drawn by an egui paint callback.
//! Pan and zoom only change the view transform: geometry is cached per node,
//! nothing is re-rasterised on the CPU.
//!
//! The CPU tile path (`tiles.rs`) stays the fallback: it is used when the GPU
//! cannot be initialised, when "Use GPU rendering" is off, and for
//! `--screenshot` unless `--gpu` is given (pixel diffs must be deterministic).

use crate::app::App;
use desigma_gpu::{Canvas, Frame, SceneCompiler};
use desigma_render::Viewport;
use eframe::egui_wgpu;
use egui::{Rect as ERect, Ui};
use std::sync::Arc;

/// App-side GPU state.
pub struct GpuView {
    /// Preferences -> "Use GPU rendering".
    pub pref: bool,
    /// The device and renderer initialised.
    pub available: bool,
    max_texture: u32,
    compiler: SceneCompiler,
    /// Milliseconds the last frame took to compile (CPU side).
    pub last_compile_ms: f32,
    pub last_stats: Option<desigma_gpu::FrameStats>,
}

impl Default for GpuView {
    fn default() -> Self {
        let mut compiler = SceneCompiler::new();
        // re-flattening curves for a new zoom level may take at most this long per frame
        compiler.geom.budget = Some(std::time::Duration::from_millis(6));
        Self { pref: true, available: false, max_texture: 0, compiler, last_compile_ms: 0.0, last_stats: None }
    }
}

impl GpuView {
    /// Whether the canvas is drawn by the GPU right now.
    pub fn active(&self) -> bool {
        self.pref && self.available
    }

    /// Forgets cached geometry (the document was replaced).
    pub fn reset(&mut self) {
        self.compiler.geom.clear();
    }

    /// Largest texture edge the device supports.
    pub fn set_max_texture(&mut self, max: u32) {
        self.compiler.opts.max_texture = max;
        self.max_texture = max;
    }
}

struct Cb {
    frame: Arc<Frame>,
}

impl egui_wgpu::CallbackTrait for Cb {
    fn prepare(
        &self,
        device: &egui_wgpu::wgpu::Device,
        _queue: &egui_wgpu::wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        encoder: &mut egui_wgpu::wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<egui_wgpu::wgpu::CommandBuffer> {
        if let Some(c) = resources.get_mut::<Canvas>() {
            c.prepare(device, encoder, &self.frame);
        }
        Vec::new()
    }

    fn paint(&self, _info: egui::PaintCallbackInfo, pass: &mut egui_wgpu::wgpu::RenderPass<'static>, resources: &egui_wgpu::CallbackResources) {
        if let Some(c) = resources.get::<Canvas>() {
            c.paint(pass);
        }
    }
}

impl App {
    /// Preferences -> "Use GPU rendering": switches between the GPU canvas and the CPU tiles.
    pub fn toggle_gpu(&mut self) {
        if !self.canvas.gpu.available {
            self.show_toast("GPU rendering is not available on this system", true);
            return;
        }
        self.canvas.gpu.pref = !self.canvas.gpu.pref;
        let on = self.canvas.gpu.pref;
        if let Some(lib) = &self.lib {
            if let Err(e) = crate::comments::settings::save_flag(lib.root(), "use_gpu", on) {
                self.show_toast(e, true);
            }
        }
        // the tile cache may be stale after a long GPU session
        self.canvas.tiles.invalidate_all();
    }
}

/// Creates the GPU renderer on eframe's device. Returns false (and the app keeps
/// using the CPU tiles) when that is not possible.
pub fn init(rs: &egui_wgpu::RenderState) -> Option<u32> {
    match Canvas::new(&rs.device, &rs.queue, rs.target_format, 1) {
        Ok(c) => {
            let max = c.max_texture_dimension();
            rs.renderer.write().callback_resources.insert(c);
            Some(max)
        }
        Err(e) => {
            eprintln!("desigma: GPU rendering unavailable, using the CPU renderer ({e})");
            None
        }
    }
}

/// Paints the page with the GPU into `rect`; false when the CPU path must do it.
pub fn paint(app: &mut App, ui: &Ui, rect: ERect) -> bool {
    if !app.canvas.gpu.active() {
        return false;
    }
    let ppp = ui.ctx().pixels_per_point();
    let size = ((rect.width() * ppp).round().max(1.0) as u32, (rect.height() * ppp).round().max(1.0) as u32);
    if size.0 > app.canvas.gpu.max_texture.max(1) || size.1 > app.canvas.gpu.max_texture.max(1) {
        return false;
    }
    let rev = super::tiles::take_rev(app);
    let start = std::time::Instant::now();
    let vp = Viewport { offset: app.view.offset, zoom: app.view.zoom, dpr: ppp };
    let page = app.page;
    let c = &mut app.canvas;
    let doc = c.motion.document(app.motion.enabled, &app.doc);
    let g = &mut c.gpu;
    let frame = g.compiler.compile(doc, page, rev, &vp, size);
    g.last_stats = Some(frame.stats());
    g.last_compile_ms = start.elapsed().as_secs_f32() * 1000.0;
    if g.compiler.geom.stale_hits > 0 {
        // some meshes still have the previous zoom's flattening: refine next frame
        ui.ctx().request_repaint();
    }
    ui.painter().with_clip_rect(rect).add(egui_wgpu::Callback::new_paint_callback(rect, Cb { frame: Arc::new(frame) }));
    c.last_render_ms = start.elapsed().as_secs_f32() * 1000.0;
    true
}
