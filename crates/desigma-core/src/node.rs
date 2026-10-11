//! Scene graph nodes.

use serde::{Deserialize, Serialize};

use crate::color::{Color, Paint};
use crate::geometry::{Rect, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextData {
    pub content: String,
    pub font_size: f64,
    /// CSS-style weight: 400 regular, 500 medium, 600 semibold.
    pub font_weight: u16,
    /// Line height in pixels. `None` means "Auto" (1.2 × font size), as in Figma.
    #[serde(default)]
    pub line_height: Option<f64>,
    #[serde(default)]
    pub align: TextAlign,
    /// When true the box width follows the content ("Auto width" in Figma).
    #[serde(default = "default_true")]
    pub auto_width: bool,
}

fn default_true() -> bool {
    true
}

impl TextData {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            font_size: 12.0,
            font_weight: 400,
            line_height: None,
            align: TextAlign::Left,
            auto_width: true,
        }
    }

    pub fn resolved_line_height(&self) -> f64 {
        self.line_height.unwrap_or(self.font_size * 1.2).max(1.0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NodeKind {
    Frame {
        #[serde(default = "default_true")]
        clip_content: bool,
    },
    /// Bounds always fit the children; see [`crate::Page::fit_groups`].
    Group,
    Rectangle,
    Ellipse,
    Text(TextData),
}

impl NodeKind {
    pub fn label(&self) -> &'static str {
        match self {
            NodeKind::Frame { .. } => "Frame",
            NodeKind::Group => "Group",
            NodeKind::Rectangle => "Rectangle",
            NodeKind::Ellipse => "Ellipse",
            NodeKind::Text(_) => "Text",
        }
    }

    pub fn is_container(&self) -> bool {
        matches!(self, NodeKind::Frame { .. } | NodeKind::Group)
    }
}

/// A layer. `x`/`y` are relative to the parent node (or the page for
/// top-level layers), matching Figma's relative transforms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub name: String,
    pub kind: NodeKind,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default = "one")]
    pub opacity: f32,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub fills: Vec<Paint>,
    #[serde(default)]
    pub strokes: Vec<Paint>,
    #[serde(default = "one_f64")]
    pub stroke_weight: f64,
    #[serde(default)]
    pub corner_radius: f64,
    #[serde(default)]
    pub children: Vec<Node>,
}

fn one() -> f32 {
    1.0
}

fn one_f64() -> f64 {
    1.0
}

impl Node {
    /// A node with Figma's defaults for its kind.
    pub fn new(id: NodeId, kind: NodeKind, rect: Rect) -> Self {
        let fills = match &kind {
            NodeKind::Frame { .. } => vec![Paint::solid(Color::WHITE)],
            NodeKind::Group => vec![],
            NodeKind::Rectangle | NodeKind::Ellipse => vec![Paint::solid(Color::DEFAULT_SHAPE)],
            NodeKind::Text(_) => vec![Paint::solid(Color::BLACK)],
        };
        Self {
            id,
            name: kind.label().to_string(),
            kind,
            x: rect.x,
            y: rect.y,
            width: rect.w,
            height: rect.h,
            opacity: 1.0,
            visible: true,
            locked: false,
            fills,
            strokes: vec![],
            stroke_weight: 1.0,
            corner_radius: 0.0,
            children: vec![],
        }
    }

    pub fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.width, self.height)
    }

    pub fn set_rect(&mut self, r: Rect) {
        self.x = r.x;
        self.y = r.y;
        self.width = r.w;
        self.height = r.h;
    }

    pub fn position(&self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }

    pub fn text(&self) -> Option<&TextData> {
        match &self.kind {
            NodeKind::Text(t) => Some(t),
            _ => None,
        }
    }

    pub fn text_mut(&mut self) -> Option<&mut TextData> {
        match &mut self.kind {
            NodeKind::Text(t) => Some(t),
            _ => None,
        }
    }

    pub fn is_frame(&self) -> bool {
        matches!(self.kind, NodeKind::Frame { .. })
    }

    pub fn clips_content(&self) -> bool {
        matches!(self.kind, NodeKind::Frame { clip_content: true })
    }

    /// Whether `p` (in this node's parent space) lies on the node's shape.
    pub fn shape_contains(&self, p: Vec2) -> bool {
        match self.kind {
            NodeKind::Ellipse => {
                let rx = self.width / 2.0;
                let ry = self.height / 2.0;
                if rx <= 0.0 || ry <= 0.0 {
                    return false;
                }
                let dx = (p.x - (self.x + rx)) / rx;
                let dy = (p.y - (self.y + ry)) / ry;
                dx * dx + dy * dy <= 1.0
            }
            _ => self.rect().contains(p),
        }
    }

    /// Visits this node and all descendants depth-first.
    pub fn walk<'a>(&'a self, f: &mut impl FnMut(&'a Node)) {
        f(self);
        for c in &self.children {
            c.walk(f);
        }
    }

    pub fn walk_mut(&mut self, f: &mut impl FnMut(&mut Node)) {
        f(self);
        for c in &mut self.children {
            c.walk_mut(f);
        }
    }

    pub fn contains_id(&self, id: NodeId) -> bool {
        let mut found = false;
        self.walk(&mut |n| found |= n.id == id);
        found
    }

    /// Scales this subtree's geometry relative to its own origin.
    pub fn scale_children(&mut self, sx: f64, sy: f64) {
        for c in &mut self.children {
            c.x *= sx;
            c.y *= sy;
            c.width *= sx;
            c.height *= sy;
            c.scale_children(sx, sy);
        }
    }
}
