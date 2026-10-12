//! Board (whiteboard) mode: a file whose [`DocumentKind`] is `Board` gets its own bottom toolbar,
//! a floating contextual toolbar instead of the right panel, a dotted canvas, stickies, shapes with
//! text, connectors that follow their nodes, and stamps. The items themselves are document kinds
//! (`desigma_doc::board`); everything here is editor behaviour.

pub mod bar;
pub mod canvas;
pub mod connect;
pub mod create;
pub mod demo;
pub mod preview;
pub mod toolbar;

use crate::app::App;
use desigma_doc::{Color, DocumentKind, NodeId, NodeKind, ShapeKind};

/// Which popover of the bottom toolbar is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolMenu {
    Brush,
    Shape,
    Stamp,
}

/// Which popover of the contextual toolbar is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarMenu {
    Shape,
    StartCap,
    EndCap,
}

/// Editor state of board mode.
pub struct BoardState {
    /// Colour of the next sticky note.
    pub color: Color,
    /// Shape the Shape tool draws.
    pub shape: ShapeKind,
    /// Stamp the Stamp tool places.
    pub stamp: String,
    /// Marker and connector ink.
    pub ink: Color,
    /// Marker or highlighter: the brush the toolbar button shows.
    pub brush: crate::canvas::tools::Tool,
    /// Open toolbar popover and the button it hangs off.
    pub tool_menu: Option<(ToolMenu, egui::Rect)>,
    /// Open contextual toolbar popover and its anchor.
    pub bar_menu: Option<(BarMenu, egui::Rect)>,
    /// Node whose magnet dots are shown.
    pub dots: Option<NodeId>,
    /// Node a connector being dragged would attach to.
    pub target: Option<NodeId>,
    /// Rendered previews of shapes and stamps for the popovers.
    pub previews: preview::Previews,
}

impl Default for BoardState {
    fn default() -> Self {
        Self {
            color: desigma_doc::STICKY_YELLOW,
            shape: ShapeKind::RoundedRect,
            stamp: "thumbs_up".into(),
            ink: color_of(crate::theme::BOARD_INK[0]),
            brush: crate::canvas::tools::Tool::Marker,
            tool_menu: None,
            bar_menu: None,
            dots: None,
            target: None,
            previews: Default::default(),
        }
    }
}

/// `0xRRGGBB` as a document colour.
pub fn color_of(c: u32) -> Color {
    Color::rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

/// A document colour as `0xRRGGBB` (alpha dropped).
pub fn rgb_of(c: Color) -> u32 {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    (b(c.r) << 16) | (b(c.g) << 8) | b(c.b)
}

pub fn color32(c: Color) -> egui::Color32 {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    egui::Color32::from_rgb(b(c.r), b(c.g), b(c.b))
}

impl App {
    /// The open file is a board.
    pub fn is_board(&self) -> bool {
        self.doc.kind == DocumentKind::Board
    }
}

/// Kinds a connector can attach to.
pub fn connectable(kind: &NodeKind) -> bool {
    matches!(kind, NodeKind::Sticky(_) | NodeKind::ShapeWithText(_) | NodeKind::Stamp { .. } | NodeKind::Text(_) | NodeKind::Section)
}
