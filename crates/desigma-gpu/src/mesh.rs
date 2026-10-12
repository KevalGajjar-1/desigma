//! Triangle meshes: tiny-skia paths tessellated with lyon.

use desigma_render::tiny_skia as sk;
use lyon_tessellation::geom::point;
use lyon_tessellation::path::Path as LPath;
use lyon_tessellation::{BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, VertexBuffers};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_MESH_ID: AtomicU64 = AtomicU64::new(1);

/// An indexed triangle list in node-local space. Immutable once built; `id` is
/// unique per mesh so the executor can cache its upload.
#[derive(Debug)]
pub struct Mesh {
    pub id: u64,
    pub verts: Vec<[f32; 2]>,
    pub idx: Vec<u32>,
    /// Local-space bounds (x0, y0, x1, y1).
    pub bounds: (f32, f32, f32, f32),
}

impl Mesh {
    pub fn new(verts: Vec<[f32; 2]>, idx: Vec<u32>) -> Self {
        let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for v in &verts {
            b = (b.0.min(v[0]), b.1.min(v[1]), b.2.max(v[0]), b.3.max(v[1]));
        }
        Self { id: NEXT_MESH_ID.fetch_add(1, Ordering::Relaxed), verts, idx, bounds: b }
    }

    pub fn is_empty(&self) -> bool {
        self.idx.is_empty()
    }

    /// Heap bytes held (for cache budgets).
    pub fn bytes(&self) -> usize {
        self.verts.len() * 8 + self.idx.len() * 4
    }

    /// Unit square scaled to `w` x `h`.
    pub fn rect(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self::new(vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]], vec![0, 1, 2, 0, 2, 3])
    }
}

fn to_lyon(path: &sk::Path) -> LPath {
    let mut b = LPath::builder();
    let mut open = false;
    for seg in path.segments() {
        match seg {
            sk::PathSegment::MoveTo(p) => {
                if open {
                    b.end(false);
                }
                b.begin(point(p.x, p.y));
                open = true;
            }
            sk::PathSegment::LineTo(p) => {
                if !open {
                    b.begin(point(p.x, p.y));
                    open = true;
                } else {
                    b.line_to(point(p.x, p.y));
                }
            }
            sk::PathSegment::QuadTo(c, p) => {
                if open {
                    b.quadratic_bezier_to(point(c.x, c.y), point(p.x, p.y));
                }
            }
            sk::PathSegment::CubicTo(c1, c2, p) => {
                if open {
                    b.cubic_bezier_to(point(c1.x, c1.y), point(c2.x, c2.y), point(p.x, p.y));
                }
            }
            sk::PathSegment::Close => {
                if open {
                    b.end(true);
                    open = false;
                }
            }
        }
    }
    if open {
        b.end(false);
    }
    b.build()
}

/// Tessellates each path with its fill rule into one mesh; `tol` is the maximum
/// curve flattening error in local units. Returns `None` if nothing was produced.
pub fn tessellate(paths: &[(sk::Path, sk::FillRule)], tol: f32) -> Option<Mesh> {
    let mut buffers: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
    let mut tess = FillTessellator::new();
    for (path, rule) in paths {
        let rule = match rule {
            sk::FillRule::Winding => FillRule::NonZero,
            sk::FillRule::EvenOdd => FillRule::EvenOdd,
        };
        let opts = FillOptions::tolerance(tol.max(1e-4)).with_fill_rule(rule);
        let lp = to_lyon(path);
        let _ = tess.tessellate_path(&lp, &opts, &mut BuffersBuilder::new(&mut buffers, |v: FillVertex| v.position().to_array()));
    }
    (!buffers.indices.is_empty()).then(|| Mesh::new(buffers.vertices, buffers.indices))
}
