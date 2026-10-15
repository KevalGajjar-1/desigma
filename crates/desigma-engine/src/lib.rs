//! GUI-independent editing, layout, text measurement, and derived updates.
pub mod components;
pub mod connect;
pub mod edit;
pub mod layout;
mod resize;
pub mod textmeasure;
mod util;
pub mod variables;

pub use resize::apply_resize;
