#![allow(dead_code)]

use desigma_doc::*;
use desigma_render::{render_page, tiny_skia::Pixmap, Viewport};

pub struct T {
    pub doc: Document,
    pub page: NodeId,
}

impl T {
    pub fn new() -> Self {
        let mut doc = Document::new("t");
        let page = doc.add_page("p");
        doc.get_mut(page).unwrap().kind = NodeKind::Page { background: Color::WHITE };
        Self { doc, page }
    }
    pub fn add(&mut self, parent: Option<NodeId>, f: impl FnOnce(NodeId) -> Node) -> NodeId {
        let id = self.doc.alloc_id();
        self.doc.insert(parent.unwrap_or(self.page), f(id)).unwrap()
    }
    pub fn rect(&mut self, parent: Option<NodeId>, r: Rect, c: Color) -> NodeId {
        self.add(parent, |id| {
            let mut n = Node::rectangle(id, r);
            n.fills = vec![Paint::solid(c)];
            n
        })
    }
    pub fn render(&self, w: u32, h: u32) -> Pixmap {
        let mut pm = Pixmap::new(w, h).unwrap();
        render_page(&self.doc, self.page, &Viewport::default(), &mut pm);
        pm
    }
    pub fn node(&mut self, id: NodeId) -> &mut Node {
        self.doc.get_mut(id).unwrap()
    }
}

pub fn px(pm: &Pixmap, x: u32, y: u32) -> [u8; 4] {
    let p = pm.pixel(x, y).unwrap();
    [p.red(), p.green(), p.blue(), p.alpha()]
}

pub fn near(a: [u8; 4], b: [u8; 4], tol: i32) -> bool {
    a.iter().zip(b.iter()).all(|(x, y)| (*x as i32 - *y as i32).abs() <= tol)
}

pub const WHITE: [u8; 4] = [255, 255, 255, 255];
pub const RED: Color = Color { r: 1.0, g: 0.0, b: 0.0, a: 1.0 };
pub const BLUE: Color = Color { r: 0.0, g: 0.0, b: 1.0, a: 1.0 };

pub fn stop(pos: f32, c: Color) -> GradientStop {
    GradientStop { pos, color: c }
}

pub fn paint(kind: PaintKind) -> Paint {
    Paint { kind, opacity: 1.0, visible: true, blend: BlendMode::Normal }
}
