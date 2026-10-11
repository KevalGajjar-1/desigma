//! Core of Desigma: the document model, editing operations, undo history and
//! the `.desigma` file format. Has no GPU or UI dependencies.

pub mod color;
pub mod document;
pub mod editor;
pub mod geometry;
pub mod io;
pub mod node;

pub use color::{Color, Paint};
pub use document::{Document, Location, Page};
pub use editor::{Editor, ZOrder};
pub use geometry::{Rect, Vec2};
pub use node::{Node, NodeId, NodeKind, TextAlign, TextData};
