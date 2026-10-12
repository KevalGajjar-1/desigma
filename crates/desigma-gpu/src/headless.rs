//! Compile + execute on a window-less device (tests, benchmarks, screenshots).

use crate::compile::SceneCompiler;
use crate::context::{GpuContext, GpuError};
use crate::exec::Renderer;
use crate::frame::Frame;
use desigma_doc::{Document, NodeId};
use desigma_render::tiny_skia::Pixmap;
use desigma_render::Viewport;

pub struct HeadlessGpu {
    pub ctx: GpuContext,
    pub compiler: SceneCompiler,
    pub renderer: Renderer,
}

impl HeadlessGpu {
    pub fn new() -> Result<Self, GpuError> {
        let ctx = GpuContext::new()?;
        let renderer = Renderer::new(&ctx.device, &ctx.queue)?;
        let mut compiler = SceneCompiler::new();
        compiler.opts.max_texture = renderer.max_texture_dimension();
        Ok(Self { ctx, compiler, renderer })
    }

    /// Compiles `page` as seen through `vp` into a frame of `size` pixels.
    pub fn compile(&mut self, doc: &Document, page: NodeId, rev: u64, vp: &Viewport, size: (u32, u32)) -> Frame {
        self.compiler.compile(doc, page, rev, vp, size)
    }

    /// Renders the page like `desigma_render::render_page` does, on the GPU.
    pub fn render_page(&mut self, doc: &Document, page: NodeId, vp: &Viewport, size: (u32, u32)) -> Pixmap {
        let frame = self.compile(doc, page, 0, vp, size);
        self.render_frame(&frame, size)
    }

    pub fn render_frame(&mut self, frame: &Frame, size: (u32, u32)) -> Pixmap {
        let data = self.renderer.render_to_rgba(frame);
        let mut pm = Pixmap::new(size.0, size.1).expect("pixmap");
        pm.data_mut().copy_from_slice(&data);
        pm
    }
}
